//! Per-batch processing for full index runs.
//!
//! The full-index batch loop drives deterministic batch boundaries from the
//! `FileIndexer`, processes each batch with path-aware output modes, persists
//! entities/vectors/BM25/summaries, and records progress. Processing a single
//! batch concurrently with bounded concurrency lives in `process_batch_with_mode`.

use std::collections::HashMap;
use std::sync::Arc;

use crate::CheckpointManager;
use crate::operation::checkpoint::{ParsedCheckpointPayload, decode_parsed_checkpoint};
use cce_parser::ast_to_nl::chunker::ChunkedResult;
use cce_parser::document::DocSummaryExt;
use cce_scanner::FileEntry;
use cce_types::{OutputMode, ParsedFile};

use super::checkpoint::persist_parsed_checkpoint;
use super::{FullIndexContext, IndexOrchestrator};
use crate::error::OrchestratorError;
use crate::index::file_processor::read_verified_utf8;

use crate::index::options::IndexOptions;
use crate::index_state::{IndexPhase, ModuleType, ModuleUpdateState};

impl IndexOrchestrator {
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

        for batch_idx in ctx.start_batch..ctx.total_batches {
            let errors_before_batch = ctx.errors.len();
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
            self.storage.begin_batch(batch_num as i64)?;

            // Create state tracking for this batch
            self.state_tracker
                .create_full_index_batch(
                    &batch_paths,
                    batch_idx,
                    ctx.total_batches,
                    batch_size,
                    root_dir.clone(),
                )
                .await;

            // Create batch checkpoint for foreign key constraint
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
                    let checkpoint_files = cm
                        .get_batch_files(&operation_id, batch_idx as u32)
                        .await
                        .unwrap_or_default();
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
            ctx.errors.extend(batch_result.errors.iter().cloned());
            ctx.total_indexed += batch_result.success_count;
            ctx.total_failed += batch_result.failed_count;
            ctx.total_entities += batch_result.entity_count;
            if let Some(spool) = ctx.relation_spool.as_mut() {
                let builder = self.relation_builder.as_ref().ok_or_else(|| {
                    OrchestratorError::index("relation_build", "relation builder is unavailable")
                })?;
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
                    spool.append(parsed).map_err(|error| {
                        OrchestratorError::index(
                            "relation_build_spool",
                            format!("Failed to store relation input: {error}"),
                        )
                    })?;
                    builder.add_file_symbols(parsed, spool.project_symbols());
                }
            }

            // Mark batch as completing Parsing phase (after actual processing)
            self.state_tracker
                .mark_phase_complete(&batch_paths, IndexPhase::Parsing)
                .await
                .unwrap_or_else(|e| {
                    tracing::warn!(error = %e, "State tracking operation failed");
                });

            self.progress.set_total(ctx.total_files);

            // Store results immediately
            if !batch_result.parsed_files.is_empty() {
                if let Err(error) = self.storage.store_parsed_files(&batch_result.parsed_files) {
                    tracing::error!(
                        error = %error,
                        batch = batch_num,
                        "Failed to persist ordinary entity generation"
                    );
                    ctx.errors.push(format!(
                        "Entity storage failed for batch {}: {}",
                        batch_num, error
                    ));
                }
            }

            if !batch_result.chunks.is_empty() {
                // Mark all files as updating Embedding module
                for path in &batch_paths {
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

                if options.store_vectors {
                    // Issue 4 fix: Only send Embedding-path chunks to vector store
                    let embedding_chunks: Vec<_> = batch_result
                        .chunks
                        .iter()
                        .filter(|c| c.path == cce_parser::ast_to_nl::chunker::ChunkPath::Embedding)
                        .cloned()
                        .collect();
                    match self
                        .storage
                        .store_vectors_batched(
                            &embedding_chunks,
                            self.batch_config.embedding_batch_size,
                            self.batch_config.embedding_batch_delay_ms,
                        )
                        .await
                    {
                        Ok(stored) => {
                            ctx.total_vectors += stored;
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "Failed to store vectors for batch {}", batch_num);
                            ctx.errors.push(format!(
                                "Vector storage failed for batch {}: {}",
                                batch_num, e
                            ));
                        }
                    }
                }

                if options.store_bm25 {
                    // Issue 4 fix: Only send BM25-path chunks to BM25 index
                    let bm25_chunks: Vec<_> = batch_result
                        .chunks
                        .iter()
                        .filter(|c| c.path == cce_parser::ast_to_nl::chunker::ChunkPath::Bm25)
                        .cloned()
                        .collect();
                    if let Err(e) = self.storage.store_bm25(&bm25_chunks).await {
                        tracing::error!(error = %e, "Failed to store BM25 for batch {}", batch_num);
                        ctx.errors.push(format!(
                            "BM25 storage failed for batch {}: {}",
                            batch_num, e
                        ));
                    }
                }

                // Issue 3 fix: Mark Embedding phase complete AFTER storing (fixed timing)
                self.state_tracker
                    .mark_phase_complete(&batch_paths, IndexPhase::Embedding)
                    .await
                    .unwrap_or_else(|e| {
                        tracing::warn!(error = %e, "State tracking operation failed");
                    });

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

            // Store summaries for this batch
            if options.store_summaries && !batch_result.parsed_files.is_empty() {
                // Mark phase as SummaryGenerating
                self.state_tracker
                    .mark_phase_complete(&batch_paths, IndexPhase::SummaryGenerating)
                    .await
                    .unwrap_or_else(|e| {
                        tracing::warn!(error = %e, "State tracking operation failed");
                    });

                // Mark all files as updating Summary module
                for path in &batch_paths {
                    self.state_tracker
                        .update_module_state(path, ModuleType::Summary, ModuleUpdateState::Updating)
                        .await
                        .unwrap_or_else(|e| {
                            tracing::warn!(error = %e, "State tracking operation failed");
                        });
                }

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
                    .map(crate::export::fingerprint::config_fingerprint);
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

                match self.storage.store_summaries(&summaries).await {
                    Ok(_) => {
                        // Mark Summary success
                        for path in &batch_paths {
                            self.state_tracker
                                .mark_success(path, ModuleType::Summary)
                                .await
                                .unwrap_or_else(|e| {
                                    tracing::warn!(error = %e, "State tracking operation failed");
                                });
                        }
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "Failed to store summaries for batch {}", batch_num);
                        ctx.errors.push(format!(
                            "Summary storage failed for batch {}: {}",
                            batch_num, e
                        ));
                        // Mark Summary failed
                        for path in &batch_paths {
                            self.state_tracker
                                .mark_failed(path, ModuleType::Summary, e.to_string())
                                .await
                                .unwrap_or_else(|e| {
                                    tracing::warn!(error = %e, "State tracking operation failed");
                                });
                        }
                    }
                }
            }

            // Store document summaries for document files
            if options.store_summaries && !batch_result.doc_summaries.is_empty() {
                // Convert DocSummary to FileSummary for storage
                let doc_file_summaries: Vec<cce_parser::summary::FileSummary> = batch_result
                    .doc_summaries
                    .iter()
                    .map(|ds| ds.to_file_summary())
                    .collect();

                match self.storage.store_summaries(&doc_file_summaries).await {
                    Ok(_) => {}
                    Err(e) => {
                        tracing::error!(
                            error = %e,
                            "Failed to store document summaries for batch {}",
                            batch_num
                        );
                        ctx.errors.push(format!(
                            "Document summary storage failed for batch {}: {}",
                            batch_num, e
                        ));
                    }
                }
            }

            // Accumulate chunks and summaries for the final (post-finalize)
            // NL document export. Documents are exported once all batches
            // complete and the relation index is finalized, so relation
            // enhancement is active.
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

            // Mark phase as Completed
            self.state_tracker
                .mark_phase_complete(&batch_paths, IndexPhase::Completed)
                .await
                .unwrap_or_else(|e| {
                    tracing::warn!(error = %e, "State tracking operation failed");
                });

            if batch_result.failed_count > 0 || ctx.errors.len() > errors_before_batch {
                ctx.all_batches_completed = false;
                tracing::warn!(
                    batch = batch_num,
                    "Batch failed; checkpoint remains at the previous completed boundary"
                );
                break;
            }

            // Persist batch checkpoint progress
            if let Some(cm) = checkpoint_manager.as_ref() {
                if let Err(e) = cm
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
            }
        }

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

            let handle = tokio::spawn(async move {
                // The semaphore is never closed in this scope; acquisition can
                // only fail after the orchestrator is dropped, in which case
                // surfacing an error is correct.
                let _permit = match file_semaphore.acquire_owned().await {
                    Ok(permit) => permit,
                    Err(_) => {
                        return Err((
                            path_str.clone(),
                            "concurrency semaphore closed during batch processing".to_string(),
                        ));
                    }
                };
                file_progress.increment_scanned();
                file_progress.set_current_file(&file_entry_clone.path);

                let result = async {
                    let process_result = if let Some(parsed) = recovered {
                        let content_route = cce_types::ContentRoute::detect_from_path(&path_str);
                        processor
                            .process_parsed_file_complete(&parsed, content_route, output_mode)
                            .await
                            .map_err(|error| (path_str.clone(), error.to_string()))?
                    } else {
                        // Verified read: the raw bytes must still match the
                        // hash recorded during scanning, otherwise the file
                        // drifted inside the scan→process window.
                        let content = read_verified_utf8(
                            &file_entry_clone.path,
                            file_entry_clone.content_hash.as_deref(),
                        )
                        .await
                        .map_err(|error| {
                            (path_str.clone(), format!("Failed to read file: {error}"))
                        })?;
                        let language_info = file_entry_clone
                            .language_info
                            .as_ref()
                            .ok_or_else(|| (path_str.clone(), "No language info".to_string()))?;

                        if language_info.is_document_like() {
                            processor
                                .process_document_file_complete(
                                    &file_entry_clone,
                                    &content,
                                    output_mode,
                                )
                                .await
                                .map_err(|error| (path_str.clone(), error.to_string()))?
                        } else {
                            processor
                                .process_code_file_with_mode(
                                    &file_entry_clone,
                                    &content,
                                    output_mode,
                                )
                                .await
                                .map_err(|error| (path_str.clone(), error.to_string()))?
                        }
                    };

                    persist_parsed_checkpoint(
                        &file_checkpoint_manager,
                        &file_operation_id,
                        batch_idx as u32,
                        &file_entry_clone,
                        &process_result.parsed_file,
                    )
                    .await
                    .map_err(|error| (path_str.clone(), error.to_string()))?;

                    Ok(process_result)
                }
                .await;

                match result {
                    Ok(process_result) => {
                        file_progress.increment_processed();
                        file_progress.clear_current_file();
                        Ok(process_result)
                    }
                    Err(e) => {
                        file_progress.increment_error();
                        file_progress.clear_current_file();
                        Err(e)
                    }
                }
            });
            join_handles.push(handle);
        }

        // Await all handles; the semaphore already bounds concurrent work.
        let mut completed_handles = Vec::with_capacity(join_handles.len());
        for handle in join_handles {
            match handle.await {
                Ok(result) => completed_handles.push(result),
                Err(e) => completed_handles.push(Err(("unknown".to_string(), e.to_string()))),
            }
        }

        // Process completed results
        for result in completed_handles {
            match result {
                Ok(process_result) => {
                    let pf = &process_result.parsed_file;
                    batch_result.entity_count += pf.entities.len();
                    batch_result.parsed_files.push(pf.clone());
                    if let Some(pr) = &process_result.processing_result {
                        batch_result.processing_results.push(pr.clone());
                    }
                    // Collect document summaries if present
                    if let Some(doc_summary) = &process_result.doc_summary {
                        batch_result.doc_summaries.push(doc_summary.clone());
                    }
                    batch_result.chunks.extend(process_result.chunks);
                    batch_result.success_count += 1;
                }
                Err((path_str, error)) => {
                    batch_result
                        .errors
                        .push(format!("Failed to process {}: {}", path_str, error));
                    batch_result.failed_files.push(path_str);
                    batch_result.failed_count += 1;
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
    /// Document summaries from document files
    doc_summaries: Vec<cce_parser::document::DocSummary>,
    failed_files: Vec<String>,
    errors: Vec<String>,
    success_count: usize,
    failed_count: usize,
    entity_count: usize,
}
