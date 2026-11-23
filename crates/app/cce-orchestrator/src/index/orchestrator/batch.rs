//! Per-batch processing for full index runs.
//!
//! The full-index batch loop drives deterministic batch boundaries from the
//! `FileIndexer`, processes each batch with path-aware output modes, persists
//! entities/vectors/BM25/summaries, and records progress. Processing a single
//! batch concurrently with bounded concurrency lives in `process_batch_with_mode`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use crate::CheckpointManager;
use crate::index::storage_coordinator::bm25::StagedBm25Write;
use crate::index::vector_store::VectorBatchWriter;
use crate::operation::checkpoint::{ParsedCheckpointPayload, decode_parsed_checkpoint};
use cce_parser::ast_to_nl::chunker::ChunkedResult;
use cce_scanner::FileEntry;
use cce_types::error::ParseError;
use cce_types::{OutputMode, ParsedFile};

use super::checkpoint::persist_parsed_checkpoint;
use super::{FullIndexContext, IndexOrchestrator};
use crate::error::OrchestratorError;

use crate::index::options::IndexOptions;
use crate::index_state::{IndexPhase, ModuleType, ModuleUpdateState, TrackerFailure};

/// Documents accumulated before the next Tantivy commit.
const BM25_COMMIT_DOCUMENTS: usize = 2_048;

impl IndexOrchestrator {
    /// Publish all writes staged across orchestration batches.
    ///
    /// Vector and fulltext work units become `Committed` only after their
    /// respective backends accept the data. The batch checkpoint advances
    /// past the aggregated range only after both commits succeed.
    #[allow(clippy::too_many_arguments)]
    async fn commit_deferred_writes(
        &self,
        ctx: &mut FullIndexContext,
        checkpoint_manager: &Option<Arc<CheckpointManager>>,
        operation_id: &str,
        vector_writer: Option<&mut VectorBatchWriter>,
        vector_hashes: &mut Vec<String>,
        staged_bm25: &mut StagedBm25Write,
        pending_batches: &mut Vec<usize>,
        pending_vector_paths: &mut Vec<PathBuf>,
        pending_bm25_paths: &mut Vec<PathBuf>,
        pending_completed_paths: &mut Vec<PathBuf>,
        advance_through: Option<usize>,
    ) -> Result<(), OrchestratorError> {
        let started = Instant::now();
        if let Some(writer) = vector_writer {
            self.storage
                .commit_staged_vectors(writer, vector_hashes)
                .await?;
            vector_hashes.clear();
        }
        if !staged_bm25.is_empty() {
            self.storage.commit_staged_bm25(staged_bm25).await?;
            *staged_bm25 = StagedBm25Write::default();
        }

        let vector_paths: HashSet<_> = pending_vector_paths.drain(..).collect();
        for path in vector_paths {
            self.state_tracker
                .mark_success(Path::new(&path), ModuleType::Embedding)
                .await
                .unwrap_or_else(|e| tracing::warn!(error = %e, "State tracking operation failed"));
            self.state_tracker
                .mark_phase_complete(&[path], IndexPhase::Embedding)
                .await
                .unwrap_or_else(|e| tracing::warn!(error = %e, "State tracking operation failed"));
        }
        let bm25_paths: HashSet<_> = pending_bm25_paths.drain(..).collect();
        for path in bm25_paths {
            self.state_tracker
                .mark_success(Path::new(&path), ModuleType::Bm25)
                .await
                .unwrap_or_else(|e| tracing::warn!(error = %e, "State tracking operation failed"));
        }
        let completed_paths: HashSet<_> = pending_completed_paths.drain(..).collect();
        for path in completed_paths {
            self.state_tracker
                .mark_phase_complete(&[path], IndexPhase::Completed)
                .await
                .unwrap_or_else(|e| tracing::warn!(error = %e, "State tracking operation failed"));
        }

        if let (Some(cm), Some(target)) = (checkpoint_manager.as_ref(), advance_through) {
            let last = pending_batches.iter().copied().max().unwrap_or(target);
            cm.update_current_batch_index(operation_id, (last + 1) as u32)
                .await
                .map_err(|e| {
                    OrchestratorError::index(
                        "checkpoint_batch_index",
                        format!("Failed to advance batch checkpoint: {e}"),
                    )
                })?;
        }
        pending_batches.clear();
        ctx.stages.write_commit_ms += started.elapsed().as_millis() as u64;
        Ok(())
    }

    /// Process all remaining batches in the deterministic batch loop.
    ///
    /// Results are stored immediately per batch so memory usage stays bounded
    /// by the batch size. Chunks and summaries are accumulated into `ctx` so
    /// the NL document export can run once after the relation index is
    /// finalized.
    pub(super) async fn run_batch_loop(
        &mut self,
        ctx: &mut FullIndexContext,
        options: &IndexOptions,
        output_mode: OutputMode,
    ) -> Result<(), OrchestratorError> {
        let checkpoint_manager = self.checkpoint_manager.clone();
        let operation_id = ctx.operation_id.clone();
        let batch_size = self.batch_config.scan_batch_size;
        let root_dir = options.root_dir.to_string_lossy().to_string();
        let batch_loop_started = Instant::now();
        let mut vector_writer = self.storage.vector().cloned().map(VectorBatchWriter::new);
        let mut vector_hashes: Vec<String> = Vec::new();
        let mut staged_bm25 = StagedBm25Write::default();
        let mut pending_batches: Vec<usize> = Vec::new();
        let mut pending_vector_paths: Vec<PathBuf> = Vec::new();
        let mut pending_bm25_paths: Vec<PathBuf> = Vec::new();
        let mut pending_completed_paths: Vec<PathBuf> = Vec::new();
        let mut source_entity_ids: std::collections::HashMap<(String, i64), i64> =
            std::collections::HashMap::new();
        let mut max_entity_id = 0i64;

        for batch_idx in ctx.start_batch..ctx.total_batches {
            ctx.stages.batches += 1;
            let state_started = Instant::now();
            self.state_tracker.flush_deferred_persistence().await;
            ctx.stages.state_projection_ms += state_started.elapsed().as_millis() as u64;
            let setup_started = Instant::now();
            let batch = match ctx.file_indexer.get_batch(batch_idx) {
                Ok(b) => b,
                Err(e) => {
                    tracing::error!("Failed to get batch {}: {}", batch_idx, e);
                    ctx.errors
                        .push(format!("Failed to get batch {}: {}", batch_idx, e));
                    ctx.all_batches_completed = false;
                    break;
                }
            };
            let batch_paths: Vec<_> = batch.iter().map(|f| f.path.clone()).collect();
            let batch_num = batch_idx + 1;
            self.storage.begin_batch(batch_num as i64).await?;
            ctx.stages.batch_dispatch_ms += setup_started.elapsed().as_millis() as u64;

            // Create state tracking for this batch
            let state_started = Instant::now();
            self.state_tracker
                .create_full_index_batch(
                    &batch_paths,
                    batch_idx,
                    ctx.total_batches,
                    batch_size,
                    root_dir.clone(),
                )
                .await;
            ctx.stages.state_projection_ms += state_started.elapsed().as_millis() as u64;

            // Create batch checkpoint for foreign key constraint
            let checkpoint_started = Instant::now();
            if let Some(ref cm) = checkpoint_manager {
                let first_file = batch
                    .first()
                    .map(|f| f.path.to_string_lossy().to_string())
                    .unwrap_or_default();
                let last_file = batch
                    .last()
                    .map(|f| f.path.to_string_lossy().to_string())
                    .unwrap_or_default();
                if let Err(e) = cm
                    .create_batch_checkpoint(
                        &operation_id,
                        batch_idx as u32,
                        &first_file,
                        &last_file,
                        batch.len() as u32,
                    )
                    .await
                {
                    ctx.errors.push(format!(
                        "Failed to create batch checkpoint for batch {}: {}",
                        batch_idx, e
                    ));
                    ctx.all_batches_completed = false;
                    break;
                }
            }

            // Process files in this batch concurrently with path-aware mode
            let recovered_parsed: HashMap<String, ParsedFile> =
                if let Some(cm) = checkpoint_manager.as_ref() {
                    let checkpoint_files =
                        match cm.get_batch_files(&operation_id, batch_idx as u32).await {
                            Ok(files) => files,
                            Err(e) => {
                                ctx.errors.push(format!(
                                    "Failed to load checkpoint files for batch {}: {}",
                                    batch_idx, e
                                ));
                                ctx.all_batches_completed = false;
                                break;
                            }
                        };
                    checkpoint_files
                        .into_iter()
                        .filter_map(|record| {
                            let entry = batch.iter().find(|entry| {
                                entry.path.to_string_lossy().as_ref() == record.file_path
                            })?;
                            if record.content_hash != entry.content_hash {
                                return None;
                            }
                            let payload = decode_parsed_checkpoint(record.parsed_data.as_deref()?)?;
                            if !payload.is_compatible() {
                                return None;
                            }
                            let ParsedCheckpointPayload::Parsed(envelope) = payload else {
                                return None;
                            };
                            Some((record.file_path, envelope.parsed_file))
                        })
                        .collect()
                } else {
                    HashMap::new()
                };
            ctx.stages.checkpoint_ms += checkpoint_started.elapsed().as_millis() as u64;
            ctx.stages.batch_setup_ms += setup_started.elapsed().as_millis() as u64;

            let stage_started = Instant::now();
            let batch_result = self
                .process_batch_with_mode(
                    batch,
                    batch_idx,
                    options,
                    output_mode,
                    &recovered_parsed,
                    checkpoint_manager.clone(),
                    &operation_id,
                )
                .await;
            ctx.stages.batch_process_ms += stage_started.elapsed().as_millis() as u64;
            ctx.errors.extend(batch_result.errors.iter().cloned());
            ctx.total_skipped_permanent += batch_result.skipped_permanent.len();
            // Baseline for batch-level (infrastructure) errors: per-file
            // processing failures are merged above and must not abort the loop.
            let errors_before_batch = ctx.errors.len();

            // Per-file state projections must cover only the files that
            // actually produced results; failed files stay at their initial
            // state so a resume can retry them.
            let succeeded: HashSet<&str> = batch_result
                .parsed_files
                .iter()
                .map(|pf| pf.path.as_str())
                .collect();
            let success_paths: Vec<_> = batch_paths
                .iter()
                .filter(|p| succeeded.contains(p.to_string_lossy().as_ref()))
                .cloned()
                .collect();
            ctx.total_indexed += batch_result.success_count;
            ctx.total_failed += batch_result.failed_count;
            ctx.total_degraded += batch_result.degraded_files;
            ctx.total_entities += batch_result.entity_count;
            let spool_started = Instant::now();
            if let Some(spool) = ctx.relation_spool.as_mut() {
                for parsed in &batch_result.parsed_files {
                    // Build-config files are injected later as synthetic `Module`
                    // nodes with `config -> dependency` edges. Their document-pipeline
                    // placeholder carries no entities and would create a duplicate
                    // spool entry that is later overwritten by path key, inflating
                    // replay counts. Skip them here and let `build_and_publish_relations`
                    // supply the single canonical entry.
                    let file_name = cce_types::path::file_name_str(&parsed.path);
                    if cce_types::path::is_build_config_name(file_name) {
                        continue;
                    }
                    let write_started = Instant::now();
                    spool.append(parsed).map_err(|error| {
                        OrchestratorError::index(
                            "relation_build_spool",
                            format!("Failed to store relation input: {error}"),
                        )
                    })?;
                    ctx.stages.relation_spool_write_ms +=
                        write_started.elapsed().as_millis() as u64;
                }
            }
            ctx.stages.relation_spool_ms += spool_started.elapsed().as_millis() as u64;

            // Mark batch as completing Parsing phase (after actual processing)
            let state_started = Instant::now();
            self.state_tracker
                .mark_phase_complete(&success_paths, IndexPhase::Parsing)
                .await
                .unwrap_or_else(|e| {
                    tracing::warn!(error = %e, "State tracking operation failed");
                });
            ctx.stages.state_projection_ms += state_started.elapsed().as_millis() as u64;

            let progress_started = Instant::now();
            self.progress.set_total(ctx.total_files);
            ctx.stages.progress_ms += progress_started.elapsed().as_millis() as u64;

            // Store results immediately. Entity rows must commit before any
            // derived write: vectors and BM25 resolve their entity links from
            // these rows, so continuing after an entity failure only amplifies
            // one fault into entity-mapping errors downstream.
            let mut entity_store_ok = true;
            let stage_started = Instant::now();
            if !batch_result.parsed_files.is_empty() {
                ctx.stages.entity_store_calls += 1;
                if let Err(error) = self
                    .storage
                    .store_parsed_files(&batch_result.parsed_files)
                    .await
                {
                    entity_store_ok = false;
                    tracing::error!(
                        error = %error,
                        batch = batch_num,
                        "Failed to persist ordinary entity generation; skipping vector/BM25 for this batch"
                    );
                    ctx.errors.push(format!(
                        "Entity storage failed for batch {}: {}",
                        batch_num, error
                    ));
                }
            }
            ctx.stages.entity_store_ms += stage_started.elapsed().as_millis() as u64;
            if entity_store_ok {
                self.storage
                    .extend_source_entity_ids(&mut source_entity_ids, &mut max_entity_id)
                    .await?;
            }

            let mut vector_deferred = false;
            let mut bm25_deferred = false;
            if !batch_result.chunks.is_empty() && entity_store_ok {
                let mut embedding_store_ok = true;
                let mut bm25_store_ok = true;

                if options.store_vectors {
                    // Mark all files as updating Embedding module
                    let state_started = Instant::now();
                    for path in &success_paths {
                        self.state_tracker
                            .update_module_state(
                                path,
                                ModuleType::Embedding,
                                ModuleUpdateState::Updating,
                            )
                            .await
                            .unwrap_or_else(|e| {
                                tracing::warn!(error = %e, "State tracking operation failed");
                            });
                    }
                    ctx.stages.state_projection_ms += state_started.elapsed().as_millis() as u64;

                    // Only send Embedding-path chunks to vector store
                    let embedding_chunks: Vec<_> = batch_result
                        .chunks
                        .iter()
                        .filter(|c| c.path == cce_parser::ast_to_nl::chunker::ChunkPath::Embedding)
                        .cloned()
                        .collect();
                    let hash_count_before = vector_hashes.len();
                    let stage_started = Instant::now();
                    let vector_result = if let Some(writer) = vector_writer.as_mut() {
                        self.storage
                            .stage_vectors_batched(
                                &embedding_chunks,
                                self.batch_config.embedding_batch_size,
                                self.batch_config.embedding_batch_delay_ms,
                                Some(writer),
                                &mut vector_hashes,
                                Some(&source_entity_ids),
                            )
                            .await
                    } else {
                        self.storage
                            .store_vectors_batched(
                                &embedding_chunks,
                                self.batch_config.embedding_batch_size,
                                self.batch_config.embedding_batch_delay_ms,
                            )
                            .await
                    };
                    ctx.stages.vector_store_ms += stage_started.elapsed().as_millis() as u64;
                    ctx.stages.vector_store_calls += 1;
                    match vector_result {
                        Ok(stored) => {
                            ctx.total_vectors += stored;
                            vector_deferred = vector_hashes.len() > hash_count_before
                                || vector_writer
                                    .as_ref()
                                    .is_some_and(|writer| writer.pending_len() > 0);
                            if vector_deferred {
                                pending_vector_paths.extend(success_paths.iter().cloned());
                            } else {
                                let state_started = Instant::now();
                                for path in &success_paths {
                                    self.state_tracker
                                        .mark_success(path, ModuleType::Embedding)
                                        .await
                                        .unwrap_or_else(|e| {
                                            tracing::warn!(error = %e, "State tracking operation failed");
                                        });
                                }
                                ctx.stages.state_projection_ms +=
                                    state_started.elapsed().as_millis() as u64;
                            }
                        }
                        Err(e) => {
                            embedding_store_ok = false;
                            ctx.circuit_open |= e.is_circuit_open();
                            tracing::error!(error = %e, "Failed to store vectors for batch {}", batch_num);
                            ctx.errors.push(format!(
                                "Vector storage failed for batch {}: {}",
                                batch_num, e
                            ));
                            let failure = e.as_module_failure();
                            let state_started = Instant::now();
                            for path in &success_paths {
                                self.state_tracker
                                    .mark_failed(path, ModuleType::Embedding, failure.clone())
                                    .await
                                    .unwrap_or_else(|e| {
                                        tracing::warn!(error = %e, "State tracking operation failed");
                                    });
                            }
                            ctx.stages.state_projection_ms +=
                                state_started.elapsed().as_millis() as u64;
                        }
                    }
                    if let Some(writer) = vector_writer.as_ref() {
                        ctx.stages.vector_pending_points_max = ctx
                            .stages
                            .vector_pending_points_max
                            .max(writer.pending_len() as u64);
                    }
                    let writer_full = vector_writer.as_ref().is_some_and(|w| w.is_full());
                    if writer_full
                        && let Err(e) = self
                            .commit_deferred_writes(
                                ctx,
                                &checkpoint_manager,
                                &operation_id,
                                vector_writer.as_mut(),
                                &mut vector_hashes,
                                &mut staged_bm25,
                                &mut pending_batches,
                                &mut pending_vector_paths,
                                &mut pending_bm25_paths,
                                &mut pending_completed_paths,
                                None,
                            )
                            .await
                    {
                        ctx.errors
                            .push(format!("Deferred write commit failed: {e}"));
                        ctx.all_batches_completed = false;
                        break;
                    }
                    if writer_full {
                        vector_deferred = false;
                    }
                } else {
                    // Vector storage disabled for this run: chunk rows and
                    // entity mappings are still persisted (metadata only) so
                    // downstream incremental flows keep their identity
                    // guarantees across epochs. Bm25-only runs produce no
                    // Embedding-path chunks, so every chunk of the batch is
                    // persisted here.
                    let all_chunks: Vec<_> = batch_result.chunks.to_vec();
                    if let Err(e) = self
                        .storage
                        .persist_chunks_without_vectors(&all_chunks.iter().collect::<Vec<_>>())
                        .await
                    {
                        embedding_store_ok = false;
                        tracing::error!(
                            error = %e,
                            "Failed to persist chunk metadata for batch {}",
                            batch_num
                        );
                        ctx.errors.push(format!(
                            "Chunk metadata persistence failed for batch {}: {}",
                            batch_num, e
                        ));
                    }
                }

                if options.store_bm25 {
                    let state_started = Instant::now();
                    for path in &success_paths {
                        self.state_tracker
                            .update_module_state(
                                path,
                                ModuleType::Bm25,
                                ModuleUpdateState::Updating,
                            )
                            .await
                            .unwrap_or_else(|e| {
                                tracing::warn!(error = %e, "State tracking operation failed");
                            });
                    }
                    ctx.stages.state_projection_ms += state_started.elapsed().as_millis() as u64;

                    let bm25_chunks: Vec<_> = batch_result
                        .chunks
                        .iter()
                        .filter(|c| c.path == cce_parser::ast_to_nl::chunker::ChunkPath::Bm25)
                        .cloned()
                        .collect();
                    let documents_before = staged_bm25.documents;
                    let stage_started = Instant::now();
                    let bm25_result = self
                        .storage
                        .stage_bm25(&bm25_chunks, 100, Some(&source_entity_ids))
                        .await;
                    ctx.stages.bm25_store_ms += stage_started.elapsed().as_millis() as u64;
                    ctx.stages.bm25_store_calls += 1;
                    match bm25_result {
                        Ok(staged) => {
                            staged_bm25.merge(staged);
                            bm25_deferred = staged_bm25.documents > documents_before;
                            if bm25_deferred {
                                pending_bm25_paths.extend(success_paths.iter().cloned());
                            } else {
                                let state_started = Instant::now();
                                for path in &success_paths {
                                    self.state_tracker
                                        .mark_success(path, ModuleType::Bm25)
                                        .await
                                        .unwrap_or_else(|e| {
                                            tracing::warn!(error = %e, "State tracking operation failed");
                                        });
                                }
                                ctx.stages.state_projection_ms +=
                                    state_started.elapsed().as_millis() as u64;
                            }
                        }
                        Err(e) => {
                            bm25_store_ok = false;
                            tracing::error!(error = %e, "Failed to stage BM25 for batch {}", batch_num);
                            ctx.errors.push(format!(
                                "BM25 storage failed for batch {}: {}",
                                batch_num, e
                            ));
                            let failure = e.as_module_failure();
                            let state_started = Instant::now();
                            for path in &success_paths {
                                self.state_tracker
                                    .mark_failed(path, ModuleType::Bm25, failure.clone())
                                    .await
                                    .unwrap_or_else(|e| {
                                        tracing::warn!(error = %e, "State tracking operation failed");
                                    });
                            }
                            ctx.stages.state_projection_ms +=
                                state_started.elapsed().as_millis() as u64;
                        }
                    }
                    ctx.stages.bm25_pending_documents_max = ctx
                        .stages
                        .bm25_pending_documents_max
                        .max(staged_bm25.documents as u64);
                    if staged_bm25.documents >= BM25_COMMIT_DOCUMENTS
                        && let Err(e) = self
                            .commit_deferred_writes(
                                ctx,
                                &checkpoint_manager,
                                &operation_id,
                                vector_writer.as_mut(),
                                &mut vector_hashes,
                                &mut staged_bm25,
                                &mut pending_batches,
                                &mut pending_vector_paths,
                                &mut pending_bm25_paths,
                                &mut pending_completed_paths,
                                Some(batch_idx),
                            )
                            .await
                    {
                        ctx.errors.push(format!("Deferred BM25 commit failed: {e}"));
                        ctx.all_batches_completed = false;
                        break;
                    }
                    if bm25_deferred && staged_bm25.is_empty() {
                        bm25_deferred = false;
                    }
                }

                if (vector_deferred || bm25_deferred) && pending_batches.last() != Some(&batch_idx)
                {
                    pending_batches.push(batch_idx);
                }

                // The Embedding phase is only complete once every attempted
                // store of the batch actually succeeded: marking it after a
                // storage failure would let a resume trust a phase whose
                // data was lost.
                if embedding_store_ok && bm25_store_ok {
                    if vector_deferred || bm25_deferred {
                        pending_completed_paths.extend(success_paths.iter().cloned());
                    } else {
                        let state_started = Instant::now();
                        self.state_tracker
                            .mark_phase_complete(&success_paths, IndexPhase::Embedding)
                            .await
                            .unwrap_or_else(|e| {
                                tracing::warn!(error = %e, "State tracking operation failed");
                            });
                        ctx.stages.state_projection_ms +=
                            state_started.elapsed().as_millis() as u64;
                    }
                }

                // Checkpoint progress is now tracked by CheckpointManager
                // after file processing completion
            }
            // Relation resolution needs the complete project symbol set. The
            // final build below consumes the retained parsed inputs, so avoid
            // building provisional per-batch graphs that are discarded.
            // relation phase completion/success is NOT marked here — the
            // relation graph is only built after every batch finishes
            // (`build_and_publish_relations`), and state must reflect the
            // build result, not its anticipation.

            // Build file_path -> summary map for NL document export
            let mut file_summary_map: HashMap<String, cce_parser::summary::FileSummary> =
                HashMap::new();

            // Generate summaries for this batch as NL document export input.
            // Skipped without an exporter (no consumer remains) and when
            // entity rows did not commit: the batch stops below and a resume
            // replays it, so spending LLM calls on summaries that cannot be
            // linked yet only wastes budget.
            if self.nl_exporter.is_some()
                && options.store_summaries
                && entity_store_ok
                && !batch_result.parsed_files.is_empty()
            {
                // Mark phase as SummaryGenerating
                self.state_tracker
                    .mark_phase_complete(&success_paths, IndexPhase::SummaryGenerating)
                    .await
                    .unwrap_or_else(|e| {
                        tracing::warn!(error = %e, "State tracking operation failed");
                    });

                // Mark all files as updating Summary module
                let state_started = Instant::now();
                for path in &success_paths {
                    self.state_tracker
                        .update_module_state(path, ModuleType::Summary, ModuleUpdateState::Updating)
                        .await
                        .unwrap_or_else(|e| {
                            tracing::warn!(error = %e, "State tracking operation failed");
                        });
                }
                ctx.stages.state_projection_ms += state_started.elapsed().as_millis() as u64;

                let stage_started = Instant::now();
                let summaries: Vec<cce_parser::summary::FileSummary> =
                    if batch_result.processing_results.len() == batch_result.parsed_files.len() {
                        self.summary_generator
                            .generate_batch_with_groups(
                                &batch_result.parsed_files,
                                &batch_result.processing_results,
                            )
                            .await
                    } else {
                        self.summary_generator
                            .generate_batch(&batch_result.parsed_files)
                            .await
                    };
                ctx.stages.summary_ms += stage_started.elapsed().as_millis() as u64;

                // Populate summary map for NL document export
                for (pf, summary) in batch_result.parsed_files.iter().zip(summaries.iter()) {
                    file_summary_map.insert(pf.path.clone(), summary.clone());
                }

                // Persist the generated summaries into the file checkpoints so a
                // resumed run reuses them for the final NL document export
                // instead of regenerating them.
                let summary_config_fingerprint = self
                    .summary_config
                    .as_ref()
                    .map(cce_export::config_fingerprint);
                if let Some(cm) = &checkpoint_manager {
                    let entries: Vec<(String, cce_parser::summary::FileSummary, Option<String>)> =
                        batch_result
                            .parsed_files
                            .iter()
                            .zip(summaries.iter())
                            .map(|(pf, summary)| {
                                (
                                    pf.path.clone(),
                                    summary.clone(),
                                    crate::operation::checkpoint::plugin_fingerprint_for(
                                        pf.language,
                                    ),
                                )
                            })
                            .collect();
                    crate::operation::checkpoint::persist_summaries_to_checkpoints(
                        cm,
                        &operation_id,
                        batch_idx as u32,
                        &entries,
                        summary_config_fingerprint,
                    )
                    .await
                    .map_err(OrchestratorError::Storage)?;
                }

                // A permanent model failure keeps the rule-based text
                // (already in the export map above) but must not masquerade
                // as a clean Summary success: those files dead-letter with
                // zero retries so a resume re-picks them after the
                // config/key is fixed. Transient fallbacks and clean
                // generations mark success; degraded counting happens
                // in the per-file loop.
                let state_started = Instant::now();
                for path in &success_paths {
                    let outcome = file_summary_map
                        .get(path.to_string_lossy().as_ref())
                        .map(|s| s.outcome.clone());
                    match outcome {
                        // A permanent model failure keeps the
                        // rule-based text (already in the export map) but
                        // must not masquerade as a clean Summary
                        // success: the module dead-letters with zero
                        // retries so a resume re-picks the file once
                        // the LLM configuration is fixed.
                        Some(cce_parser::summary::SummaryOutcome::Failed { code, message }) => {
                            let failure = TrackerFailure {
                                message,
                                code: Some(code),
                                retryable: false,
                            };
                            self.state_tracker
                                .mark_failed(path, ModuleType::Summary, failure)
                                .await
                                .unwrap_or_else(|e| {
                                    tracing::warn!(error = %e, "State tracking operation failed");
                                });
                        }
                        _ => {
                            // A transient fallback produced usable but
                            // rule-only content: the file is degraded.
                            if matches!(
                                outcome,
                                Some(cce_parser::summary::SummaryOutcome::Fallback { .. })
                            ) {
                                ctx.total_degraded += 1;
                            }
                            self.state_tracker
                                .mark_success(path, ModuleType::Summary)
                                .await
                                .unwrap_or_else(|e| {
                                    tracing::warn!(error = %e, "State tracking operation failed");
                                });
                        }
                    }
                }
                ctx.stages.state_projection_ms += state_started.elapsed().as_millis() as u64;
            }

            // Accumulate chunks and summaries for the final (post-finalize)
            // NL document export. Documents are exported once all batches
            // complete and the relation index is finalized, so relation
            // enhancement is active.
            let export_spool_started = Instant::now();
            if self.nl_exporter.is_some() {
                let Some(spool) = ctx.export_spool.as_mut() else {
                    return Err(OrchestratorError::index(
                        "export_spool",
                        "export spool missing for batch chunk accumulation",
                    ));
                };
                let mut chunks_by_file: HashMap<String, Vec<_>> = HashMap::new();
                for chunk in &batch_result.chunks {
                    chunks_by_file
                        .entry(chunk.metadata.file_path.clone())
                        .or_default()
                        .push(chunk.clone());
                }
                for (file_path, chunks) in chunks_by_file {
                    spool.append(&file_path, chunks).map_err(|error| {
                        OrchestratorError::index(
                            "export_spool",
                            format!("Failed to spool export chunks for {file_path}: {error}"),
                        )
                    })?;
                }
                for (file_path, summary) in &file_summary_map {
                    ctx.export_summaries_by_file
                        .entry(file_path.clone())
                        .or_insert_with(|| summary.clone());
                }
            }
            ctx.stages.export_spool_ms += export_spool_started.elapsed().as_millis() as u64;

            // Mark phase as Completed once this batch's vector and fulltext
            // writes are durable; deferred batches carry the marker to their
            // aggregation commit boundary.
            let state_started = Instant::now();
            if vector_deferred || bm25_deferred || !pending_completed_paths.is_empty() {
                pending_completed_paths.extend(success_paths.iter().cloned());
            } else {
                self.state_tracker
                    .mark_phase_complete(&success_paths, IndexPhase::Completed)
                    .await
                    .unwrap_or_else(|e| {
                        tracing::warn!(error = %e, "State tracking operation failed");
                    });
            }
            ctx.stages.state_projection_ms += state_started.elapsed().as_millis() as u64;

            match classify_batch_action(
                ctx.errors.len() - errors_before_batch,
                batch_result.failed_count,
            ) {
                BatchAction::Stop => {
                    // Batch-level infrastructure failure (entity/vector/BM25/
                    // summary storage): every remaining batch would pile up the
                    // same error, so stop and stay resumable. Earlier staged
                    // data may still be committed, but the batch boundary must
                    // not move past the failed batch.
                    if !pending_batches.is_empty()
                        && let Err(e) = self
                            .commit_deferred_writes(
                                ctx,
                                &checkpoint_manager,
                                &operation_id,
                                vector_writer.as_mut(),
                                &mut vector_hashes,
                                &mut staged_bm25,
                                &mut pending_batches,
                                &mut pending_vector_paths,
                                &mut pending_bm25_paths,
                                &mut pending_completed_paths,
                                None,
                            )
                            .await
                    {
                        ctx.errors
                            .push(format!("Deferred write commit failed: {e}"));
                    }
                    ctx.all_batches_completed = false;
                    tracing::warn!(
                        batch = batch_num,
                        "Batch failed; checkpoint remains at the previous completed boundary"
                    );
                    break;
                }
                BatchAction::Continue => {
                    // File-level failures only affect their own files: later
                    // batches still make progress. The batch boundary is not
                    // advanced here, and failed files have no file-level
                    // checkpoint record, so a resume re-enters this batch and
                    // retries them.
                    if !pending_batches.is_empty()
                        && let Err(e) = self
                            .commit_deferred_writes(
                                ctx,
                                &checkpoint_manager,
                                &operation_id,
                                vector_writer.as_mut(),
                                &mut vector_hashes,
                                &mut staged_bm25,
                                &mut pending_batches,
                                &mut pending_vector_paths,
                                &mut pending_bm25_paths,
                                &mut pending_completed_paths,
                                None,
                            )
                            .await
                    {
                        ctx.errors
                            .push(format!("Deferred write commit failed: {e}"));
                    }
                    ctx.all_batches_completed = false;
                    tracing::warn!(
                        batch = batch_num,
                        failed = batch_result.failed_count,
                        "Files failed in batch; continuing remaining batches without advancing the checkpoint boundary"
                    );
                    continue;
                }
                BatchAction::Proceed => {}
            }

            // Persist batch checkpoint progress. While derived writes are
            // staged, the boundary stays at the last committed batch.
            let checkpoint_started = Instant::now();
            if pending_batches.is_empty()
                && let Some(cm) = checkpoint_manager.as_ref()
                && let Err(e) = cm
                    .update_current_batch_index(&operation_id, batch_idx as u32 + 1)
                    .await
            {
                tracing::error!(
                    error = %e,
                    "Failed to update batch checkpoint index for batch {}",
                    batch_num
                );
                ctx.errors.push(format!(
                    "Failed to persist checkpoint for batch {}: {}",
                    batch_idx, e
                ));
                ctx.all_batches_completed = false;
                break;
            }
            ctx.stages.checkpoint_ms += checkpoint_started.elapsed().as_millis() as u64;

            ctx.stages.rss_peak_batch_kb = ctx
                .stages
                .rss_peak_batch_kb
                .max(cce_utils::rss_kb().unwrap_or(0));
        }

        if !pending_batches.is_empty()
            && let Err(e) = self
                .commit_deferred_writes(
                    ctx,
                    &checkpoint_manager,
                    &operation_id,
                    vector_writer.as_mut(),
                    &mut vector_hashes,
                    &mut staged_bm25,
                    &mut pending_batches,
                    &mut pending_vector_paths,
                    &mut pending_bm25_paths,
                    &mut pending_completed_paths,
                    ctx.total_batches.checked_sub(1),
                )
                .await
        {
            ctx.errors
                .push(format!("Final deferred write commit failed: {e}"));
            ctx.all_batches_completed = false;
        }
        if let Err(e) = self.storage.finish_fulltext_writes().await {
            ctx.errors.push(format!("BM25 merge drain failed: {e}"));
        }

        let state_started = Instant::now();
        self.state_tracker.flush_deferred_persistence().await;
        ctx.stages.state_projection_ms += state_started.elapsed().as_millis() as u64;

        let timed_ms = ctx.stages.batch_process_ms
            + ctx.stages.batch_setup_ms
            + ctx.stages.entity_store_ms
            + ctx.stages.vector_store_ms
            + ctx.stages.bm25_store_ms
            + ctx.stages.summary_ms
            + ctx.stages.relation_spool_ms
            + ctx.stages.export_spool_ms
            + ctx.stages.state_projection_ms
            + ctx.stages.checkpoint_ms
            + ctx.stages.progress_ms
            + ctx.stages.batch_dispatch_ms
            + ctx.stages.write_commit_ms;
        let loop_ms = batch_loop_started.elapsed().as_millis() as u64;
        ctx.stages.batch_overhead_ms = loop_ms.saturating_sub(timed_ms);

        Ok(())
    }

    /// Process a batch of files with path-aware output mode
    ///
    /// Uses the specified output mode to optimize conversion by skipping
    /// unnecessary text generation based on configured storage backends.
    #[allow(clippy::too_many_arguments)]
    async fn process_batch_with_mode(
        &mut self,
        batch: &[FileEntry],
        batch_idx: usize,
        options: &IndexOptions,
        output_mode: OutputMode,
        recovered_parsed: &HashMap<String, ParsedFile>,
        checkpoint_manager: Option<Arc<CheckpointManager>>,
        operation_id: &str,
    ) -> BatchResult {
        let mut batch_result = BatchResult::default();
        let _batch_num = batch_idx + 1;

        // Process files concurrently using futures with bounded concurrency.
        // The semaphore is acquired INSIDE each spawned task, so at most
        // `max_concurrency` tasks execute file work at any moment. Previously
        // every file in the batch was spawned and ran immediately, and the
        // chunked await below only serialized result collection — memory/handle
        // peaks scaled with the batch size, not concurrency.
        let max_concurrency = options.max_concurrency.min(batch.len()).max(1);
        let semaphore = Arc::new(tokio::sync::Semaphore::new(max_concurrency));

        // Create futures for all files
        let progress = self.progress.clone();
        let mut join_handles = Vec::with_capacity(batch.len());
        for file_entry in batch.iter() {
            let path_str = file_entry.path.to_string_lossy().to_string();

            // Clone necessary data for the async task
            let file_entry_clone = file_entry.clone();
            let mut processor = self.file_processor.clone();
            let recovered = recovered_parsed.get(&path_str).cloned();
            let file_checkpoint_manager = checkpoint_manager.clone();
            let file_operation_id = operation_id.to_string();
            let file_progress = progress.clone();
            let file_semaphore = semaphore.clone();
            let handle_path = path_str.clone();

            let handle = tokio::spawn(async move {
                // The semaphore is never closed in this scope; acquisition can
                // only fail after the orchestrator is dropped, in which case
                // surfacing an error is correct.
                let _permit = match file_semaphore.acquire_owned().await {
                    Ok(permit) => permit,
                    Err(_) => {
                        return Err((
                            path_str.clone(),
                            OrchestratorError::index(
                                "batch_concurrency",
                                "concurrency semaphore closed during batch processing",
                            ),
                        ));
                    }
                };
                file_progress.increment_scanned();
                file_progress.set_current_file(&file_entry_clone.path);

                let result = async {
                    let mut content_lossy = false;
                    // Entries without a scan-phase content hash are oversized
                    // files skipped by the scanner: fail fast with a distinct
                    // permanent reason instead of reading the full content
                    // only to report a misleading missing-language error.
                    if recovered.is_none() && file_entry_clone.content_hash.is_none() {
                        return Err((
                            path_str.clone(),
                            OrchestratorError::Parse(ParseError::unsupported_language(
                                format!(
                                    "skipped oversized file without content hash: {path_str} (size {} bytes)",
                                    file_entry_clone.size
                                ),
                            )),
                        ));
                    }
                    let process_result = if let Some(parsed) = recovered {
                        let content_route = cce_types::ContentRoute::detect_from_path(&path_str);
                        processor
                            .process_parsed_file_complete(&parsed, content_route, output_mode)
                            .await
                            .map_err(|error| (path_str.clone(), error))?
                    } else {
                        // Fresh files run the read on the workers and the
                        // CPU-bound parse/group/convert/chunk stage on the
                        // blocking pool, so CPU-heavy batches cannot starve
                        // progress reporting, checkpoints and storage IO.
                        let (complete, lossy) = processor
                            .process_entry_blocking(file_entry_clone.clone(), output_mode)
                            .await
                            .map_err(|error| (path_str.clone(), error))?;
                        content_lossy = lossy;
                        complete
                    };

                    persist_parsed_checkpoint(
                        &file_checkpoint_manager,
                        &file_operation_id,
                        batch_idx as u32,
                        &file_entry_clone,
                        &process_result.parsed_file,
                    )
                    .await
                    .map_err(|error| (path_str.clone(), error))?;

                    Ok((process_result, content_lossy))
                }
                .await;

                match result {
                    Ok(complete) => {
                        file_progress.increment_processed();
                        file_progress.clear_current_file();
                        Ok(complete)
                    }
                    Err(e) => {
                        file_progress.increment_error();
                        file_progress.clear_current_file();
                        Err(e)
                    }
                }
            });
            join_handles.push((handle_path, handle));
        }

        // Await all handles; the semaphore already bounds concurrent work.
        let mut completed_handles = Vec::with_capacity(join_handles.len());
        for (path, handle) in join_handles {
            match handle.await {
                Ok(result) => completed_handles.push(result),
                // A JoinError means the task panicked or was cancelled: keep
                // the file identity so the failure is attributable.
                Err(e) => completed_handles.push(Err((
                    path,
                    OrchestratorError::index("file_task_join", e.to_string()),
                ))),
            }
        }

        // Process completed results
        for result in completed_handles {
            match result {
                Ok((process_result, content_lossy)) => {
                    let pf = &process_result.parsed_file;
                    batch_result.entity_count += pf.entities.len();
                    batch_result.parsed_files.push(pf.clone());
                    if let Some(pr) = &process_result.processing_result {
                        batch_result.processing_results.push(pr.clone());
                    }
                    if let Some(metrics) = &self.quality_metrics {
                        metrics.record_vocabulary(
                            process_result.vocabulary.injected,
                            process_result.vocabulary.truncated,
                        );
                    }
                    let produced_chunks = !process_result.chunks.is_empty();
                    batch_result.chunks.extend(process_result.chunks);
                    batch_result.success_count += 1;
                    // A file counts as degraded when its indexed text was
                    // lossily decoded or it produced neither entities nor
                    // chunks: the run "succeeded" but the content is missing
                    // or corrupted, which must be visible in the result.
                    if content_lossy {
                        tracing::warn!(
                            file = %pf.path,
                            "Indexed content contains U+FFFD replacement characters (lossy encoding detection)"
                        );
                        batch_result.degraded_files += 1;
                    } else if pf.entities.is_empty() && !produced_chunks {
                        tracing::warn!(
                            file = %pf.path,
                            "File indexed successfully but produced zero entities and zero chunks"
                        );
                        if let Some(metrics) = &self.quality_metrics {
                            metrics.record_empty_file();
                        }
                        batch_result.degraded_files += 1;
                    } else if process_result.dropped_blank_segments > 0 {
                        tracing::warn!(
                            file = %pf.path,
                            dropped = process_result.dropped_blank_segments,
                            "Chunker dropped blank segments; indexed content is incomplete"
                        );
                        batch_result.degraded_files += 1;
                    } else if process_result.document_degraded {
                        tracing::warn!(
                            file = %pf.path,
                            "Structured document degraded to plain text; indexed content is incomplete"
                        );
                        batch_result.degraded_files += 1;
                    } else if process_result.has_syntax_errors {
                        tracing::warn!(
                            file = %pf.path,
                            syntax_tag = ?process_result.syntax_diagnostic_tag,
                            "Parsed with syntax errors; entities inside error regions may be missing"
                        );
                        batch_result.degraded_files += 1;
                    }
                }
                Err((path_str, error)) => {
                    let failure = error.as_module_failure();
                    // Mirror the file-level outcome into the shared state
                    // tracker so full-index failures feed the same retry and
                    // dead-letter reporting as hot updates: transient
                    // failures stay retryable, permanent ones dead-letter
                    // immediately with zero retries.
                    let file_path = std::path::Path::new(&path_str);
                    for module in ModuleType::all() {
                        self.state_tracker
                            .mark_failed(file_path, module, failure.clone())
                            .await
                            .unwrap_or_else(|e| {
                                tracing::warn!(error = %e, "State tracking operation failed");
                            });
                    }
                    if failure.retryable {
                        batch_result
                            .errors
                            .push(format!("Failed to process {}: {}", path_str, error));
                        batch_result.failed_files.push(path_str);
                        batch_result.failed_count += 1;
                    } else {
                        // Deterministic failure (unsupported language, drifted
                        // content, undecodable bytes): retrying it on every
                        // resume would loop forever, so it is recorded as a
                        // permanent skip instead of a retryable failure.
                        tracing::warn!(
                            file = %path_str,
                            error = %error,
                            "File failed permanently; skipped and not retried on resume"
                        );
                        batch_result.skipped_permanent.push(path_str);
                    }
                }
            }
        }

        batch_result
    }
}

/// Result of processing a batch of files
#[derive(Default)]
struct BatchResult {
    parsed_files: Vec<ParsedFile>,
    processing_results: Vec<cce_parser::grouper::ProcessingResult>,
    chunks: Vec<ChunkedResult>,
    /// Files whose failure is transient and will be retried on resume.
    failed_files: Vec<String>,
    errors: Vec<String>,
    success_count: usize,
    /// Count of transient file failures only; permanent failures are tracked
    /// in `skipped_permanent` and never retried within the operation.
    failed_count: usize,
    /// Files whose failure is deterministic (unsupported language, drifted or
    /// undecodable content): the checkpoint boundary advances over them so a
    /// resume does not retry them without a re-scan.
    skipped_permanent: Vec<String>,
    /// Successfully processed files whose indexed content is missing or
    /// corrupted (lossy decode, or zero entities and zero chunks).
    degraded_files: usize,
    entity_count: usize,
}

/// How the batch loop should react after finishing one batch.
///
/// Batch-level infrastructure errors (a nonzero `new_errors` pushed by entity/
/// vector/BM25 storage) outrank file-level failures: every remaining
/// batch would hit the same fault, so the loop stops and stays resumable.
/// Transient file-level failures only affect their own files, so later batches
/// keep going. Permanent file failures are short-circuited at collection time
/// and do not appear here: advancing the boundary past them is intentional.
#[derive(Debug, PartialEq, Eq)]
enum BatchAction {
    Stop,
    Continue,
    Proceed,
}

fn classify_batch_action(new_errors: usize, failed_count: usize) -> BatchAction {
    if new_errors > 0 {
        BatchAction::Stop
    } else if failed_count > 0 {
        BatchAction::Continue
    } else {
        BatchAction::Proceed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_level_error_stops_loop_even_with_file_failures() {
        // A storage fault (new error) must break the whole run, not just skip
        // the batch, regardless of how many individual files failed.
        assert_eq!(classify_batch_action(1, 0), BatchAction::Stop);
        assert_eq!(classify_batch_action(2, 5), BatchAction::Stop);
    }

    #[test]
    fn file_failures_continue_without_stopping() {
        // Bad files must not halt remaining batches: continue, don't stop.
        assert_eq!(classify_batch_action(0, 1), BatchAction::Continue);
        assert_eq!(classify_batch_action(0, 3), BatchAction::Continue);
    }

    #[test]
    fn clean_batch_proceeds() {
        assert_eq!(classify_batch_action(0, 0), BatchAction::Proceed);
    }
}
