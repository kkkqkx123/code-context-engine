//! Embedding update processor
//!
//! This module handles updates to embeddings during hot updates.
//!
//! # Batch Processing
//!
//! Chunks are streamed through a fixed-size buffer so embedding requests stay
//! packed at batch size across file boundaries:
//! 1. Batch delete old vectors for removed/modified files
//! 2. Stream added/modified files through the buffer, storing full batches as
//!    they fill up
//! 3. Persist a file's completion marker as soon as its last chunk leaves the
//!    buffer (or the final flush drains it)
//!
//! # Crash Recovery Granularity
//!
//! File-module progress (`checkpoint_file.module_progress`) records per-file
//! completion as soon as that file's vectors are durably stored, so a crash
//! re-processes at most the in-flight file instead of the whole batch set.
//! Micro-batch retry inside one file is owned by the storage layer
//! (`StorageCoordinator::store_vectors_batched` with `work_unit_checkpoint`
//! on `embedding_generation`).

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use async_trait::async_trait;

use cce_llm::Embedder;
use cce_storage_sqlite::ChunkRepository;

use crate::hot_update::ParseResultWithChanges;
use crate::hot_update::error::{HotUpdateError, Result};
use crate::hot_update::processors::context::ProcessorContext;
use crate::hot_update::processors::deletion::process_deletions;
use crate::hot_update::processors::trait_def::UpdateProcessor;

/// Default batch size for embedding operations
const DEFAULT_EMBEDDING_BATCH_SIZE: usize = 32;

/// Page size for regeneration sweeps over stored chunk records.
const REEMBED_PAGE_SIZE: usize = 256;

/// Page size for chunking-drift sweeps over the `files` table.
const RECHUNK_PAGE_SIZE: i64 = 128;

/// `project_meta` key persisting the AST-to-NL pipeline fingerprint that
/// produced the currently stored embedding-path data.
const EMBEDDING_RECHUNK_KEY: &str = "chunking_fingerprint_embedding";

/// `project_meta` key persisting the embedder fingerprint that produced the
/// currently stored vectors.
const EMBEDDER_FINGERPRINT_KEY: &str = "embedding_model_fingerprint";

/// Fingerprint of an embedder configuration.
///
/// Covers model identity and vector dimension — the inputs that make stored
/// vectors incompatible when they change. A fingerprint drift between the
/// persisted value and the current embedder triggers a regeneration sweep
/// that refreshes vectors from stored chunk texts without re-parsing.
pub fn embedder_fingerprint(embedder: &dyn Embedder) -> String {
    let raw = format!("{}::{}", embedder.model_name(), embedder.dimension());
    cce_utils::hash::calculate_hash(raw.as_bytes())
}

/// Embedding update processor
pub struct EmbeddingUpdateProcessor {
    /// Shared processor context
    context: Arc<ProcessorContext>,
    /// Whether this processor is enabled
    enabled: Arc<AtomicBool>,
    /// Batch size for embedding operations
    batch_size: usize,
    /// Test-only counter incremented whenever a file is actually embedded
    /// (skipped files whose module progress marker matches are not counted).
    embedding_counter: Option<Arc<AtomicUsize>>,
}

impl EmbeddingUpdateProcessor {
    /// Create a new embedding update processor
    pub fn new(context: Arc<ProcessorContext>) -> Self {
        Self {
            context,
            enabled: Arc::new(AtomicBool::new(true)),
            batch_size: DEFAULT_EMBEDDING_BATCH_SIZE,
            embedding_counter: None,
        }
    }

    /// Create with custom batch size
    pub fn with_batch_size(context: Arc<ProcessorContext>, batch_size: usize) -> Self {
        Self {
            context,
            enabled: Arc::new(AtomicBool::new(true)),
            batch_size: batch_size.max(1),
            embedding_counter: None,
        }
    }

    /// Attach a test-only counter that is incremented per embedded file.
    pub fn with_embedding_counter(mut self, counter: Arc<AtomicUsize>) -> Self {
        self.embedding_counter = Some(counter);
        self
    }

    fn count_embedded_file(&self) {
        if let Some(counter) = &self.embedding_counter {
            counter.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Set whether this processor is enabled
    pub async fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }

    /// Regenerate stored vectors when the embedder configuration drifted.
    ///
    /// The persisted embedder fingerprint (`project_meta`) is compared against
    /// the current one. On drift, vectors and summary vectors are refreshed
    /// **in place at the active generation** directly from the stored chunk
    /// texts — no tree-sitter parsing, re-chunking, or LLM summarization is
    /// involved. A missing fingerprint (first run / fresh database) is only
    /// recorded as baseline; sweeping requires a previous value so an upgrade
    /// never triggers a surprise full regeneration.
    async fn regenerate_on_embedder_drift(
        &self,
        ctx: &crate::operation::OperationContext,
    ) -> std::result::Result<(), HotUpdateError> {
        let storage = &self.context.storage;
        let Some(embedder) = storage.embedder() else {
            return Ok(());
        };
        let current_fp = embedder_fingerprint(embedder.as_ref());

        let Some(client) = storage.metadata_client() else {
            return Ok(());
        };
        let stored_fp = client
            .project_meta_get_string_optional(storage.project_id(), EMBEDDER_FINGERPRINT_KEY)
            .map_err(|e| HotUpdateError::embedding(e.to_string()))?;

        match stored_fp.as_deref() {
            Some(fp) if fp == current_fp => return Ok(()),
            None => {
                client
                    .project_meta_set_string(
                        storage.project_id(),
                        EMBEDDER_FINGERPRINT_KEY,
                        &current_fp,
                    )
                    .map_err(|e| HotUpdateError::embedding(e.to_string()))?;
                return Ok(());
            }
            _ => {}
        }

        // Drift detected: sweep chunks page by page at the active epoch.
        let Some(active_epoch) = storage
            .active_data_epoch()
            .map_err(|e| HotUpdateError::embedding(e.to_string()))?
        else {
            // Never published: nothing to regenerate; record the baseline.
            client
                .project_meta_set_string(
                    storage.project_id(),
                    EMBEDDER_FINGERPRINT_KEY,
                    &current_fp,
                )
                .map_err(|e| HotUpdateError::embedding(e.to_string()))?;
            return Ok(());
        };

        tracing::info!(
            project_id = storage.project_id(),
            active_epoch,
            "Embedder configuration changed; regenerating vectors from stored chunk texts"
        );

        let mut offset: i64 = 0;
        loop {
            let page = {
                let conn = client
                    .read_connection()
                    .map_err(|e| HotUpdateError::embedding(e.to_string()))?;
                ChunkRepository::get_by_project_and_epoch_with_category_paged(
                    &conn,
                    storage.project_id(),
                    active_epoch,
                    REEMBED_PAGE_SIZE as i64,
                    offset,
                )
                .map_err(|e| HotUpdateError::embedding(e.to_string()))?
            };
            if page.is_empty() {
                break;
            }
            offset += page.len() as i64;
            storage
                .reembed_vectors_from_records(&page, self.batch_size)
                .await
                .map_err(|e| HotUpdateError::embedding(e.to_string()))?;
        }

        storage
            .reembed_stored_summaries(self.batch_size)
            .await
            .map_err(|e| HotUpdateError::embedding(e.to_string()))?;

        client
            .project_meta_set_string(storage.project_id(), EMBEDDER_FINGERPRINT_KEY, &current_fp)
            .map_err(|e| HotUpdateError::embedding(e.to_string()))?;

        tracing::info!(
            files_page_offset = offset,
            operation_id = %ctx.operation_id,
            "Embedder drift regeneration finished"
        );
        Ok(())
    }

    /// Re-chunk every unchanged file from disk when the AST-to-NL pipeline
    /// configuration drifted.
    ///
    /// Each unchanged file of the active generation is re-parsed and
    /// re-chunked locally under the current configuration — no LLM calls.
    /// Files whose on-disk content no longer matches the recorded hash are
    /// skipped and left to the regular change flow. On success the new
    /// pipeline fingerprint is persisted.
    async fn regenerate_on_chunking_drift(
        &self,
        ctx: &crate::operation::OperationContext,
        skip_paths: &std::collections::HashSet<String>,
    ) -> std::result::Result<(), HotUpdateError> {
        use crate::hot_update::processors::rechunk::{
            FingerprintDrift, detect_fingerprint_drift, persist_fingerprint, resolve_project_root,
        };
        use cce_storage_sqlite::FileRepository;

        let storage = &self.context.storage;
        let Some(client) = storage.metadata_client() else {
            return Ok(());
        };
        let pipeline_fp = self
            .context
            .file_processor
            .lock()
            .await
            .pipeline_fingerprint();
        match detect_fingerprint_drift(
            client,
            storage.project_id(),
            EMBEDDING_RECHUNK_KEY,
            &pipeline_fp,
        )
        .map_err(|e| HotUpdateError::embedding(e.to_string()))?
        {
            FingerprintDrift::Current | FingerprintDrift::BaselineWritten => return Ok(()),
            FingerprintDrift::Drifted => {}
        }

        let Some(active_epoch) = storage
            .active_data_epoch()
            .map_err(|e| HotUpdateError::embedding(e.to_string()))?
        else {
            persist_fingerprint(
                client,
                storage.project_id(),
                EMBEDDING_RECHUNK_KEY,
                &pipeline_fp,
            )
            .map_err(|e| HotUpdateError::embedding(e.to_string()))?;
            return Ok(());
        };

        tracing::info!(
            project_id = storage.project_id(),
            active_epoch,
            "Chunking configuration changed; re-chunking unchanged files from disk"
        );

        let root_path = resolve_project_root(client, storage.project_id())
            .map_err(|e| HotUpdateError::embedding(e.to_string()))?;

        let mut offset: i64 = 0;
        let mut rebuilt = 0usize;
        let mut skipped = 0usize;
        loop {
            let page = {
                let conn = client
                    .read_connection()
                    .map_err(|e| HotUpdateError::embedding(e.to_string()))?;
                FileRepository::get_by_project_and_epoch_paged(
                    &conn,
                    storage.project_id(),
                    active_epoch,
                    RECHUNK_PAGE_SIZE,
                    offset,
                )
                .map_err(|e| HotUpdateError::embedding(e.to_string()))?
            };
            if page.is_empty() {
                break;
            }
            offset += page.len() as i64;

            for file in page {
                // Files changed by this operation are handled by the normal
                // flow below; sweeping them here would rewrite stale content
                // that is immediately replaced again.
                if skip_paths.contains(&file.path) {
                    continue;
                }
                let Some(hash) = file.content_hash.as_deref().filter(|h| !h.is_empty()) else {
                    continue;
                };
                let read_path = root_path.join(&file.path);
                let chunks = self
                    .context
                    .file_processor
                    .lock()
                    .await
                    .rechunk_file_from_disk(
                        &read_path,
                        &file.path,
                        hash,
                        cce_types::OutputMode::Both,
                    )
                    .await
                    .map_err(|e| HotUpdateError::embedding(e.to_string()))?;
                let Some(chunks) = chunks else {
                    skipped += 1;
                    continue;
                };
                if chunks.is_empty() {
                    continue;
                }
                // Give the re-chunked file own candidate-epoch entity rows so
                // the vector-store flow writes detail mappings against the new
                // point ids instead of leaving them in the ancestor generation,
                // where compaction would eventually retire them.
                if let Err(e) =
                    storage.copy_file_rows_between_epochs(active_epoch, storage.epoch(), &file.path)
                {
                    tracing::warn!(
                        path = %file.path,
                        error = %e,
                        "Failed to forward entity rows for a swept file; \
                         its entity mappings stay in the ancestor generation"
                    );
                }
                storage
                    .prepare_hot_update_embedding(std::path::Path::new(&file.path))
                    .await
                    .map_err(|e| HotUpdateError::embedding(e.to_string()))?;
                storage
                    .store_vectors_batched(&chunks, self.batch_size, 0)
                    .await
                    .map_err(|e| HotUpdateError::embedding(e.to_string()))?;
                rebuilt += 1;
            }
        }

        persist_fingerprint(
            client,
            storage.project_id(),
            EMBEDDING_RECHUNK_KEY,
            &pipeline_fp,
        )
        .map_err(|e| HotUpdateError::embedding(e.to_string()))?;

        if let Some(metrics) = &self.context.storage_metrics {
            metrics.record_chunking_drift_sweep(rebuilt, skipped);
        }
        if skipped > 0 {
            tracing::warn!(
                skipped,
                rebuilt,
                "Chunking-drift sweep skipped files whose on-disk content no longer \
                 matches the recorded hash; the regular change flow will refresh them"
            );
        }

        tracing::info!(
            rebuilt,
            skipped,
            operation_id = %ctx.operation_id,
            "Chunking-drift regeneration finished"
        );
        Ok(())
    }

    /// Process files by streaming their chunks through a batch buffer.
    ///
    /// A file's completion marker is persisted immediately after its last
    /// chunk is stored, so an interrupted run resumes with only the in-flight
    /// file re-chunked and re-embedded. Files whose embeddings already
    /// completed for the current content (recovered from a durable candidate)
    /// are skipped, and files completed during this run get their in-memory
    /// marker updated so a same-operation retry skips them too.
    async fn process_files_batched(
        &self,
        ctx: &crate::operation::OperationContext,
        parse_results: &mut [ParseResultWithChanges],
    ) -> Result<usize> {
        if parse_results.is_empty() {
            return Ok(0);
        }

        // The per-file progress marker folds the chunking configuration in, so
        // a chunking-config change between a crash and its resume invalidates
        // previously-completed embeddings.
        let chunking_fp = self
            .context
            .file_processor
            .lock()
            .await
            .chunking_fingerprint();

        let module_fingerprint = |source: &str| -> String {
            let content_hash = cce_utils::hash::calculate_hash(source.as_bytes());
            crate::hot_update::progress::module_input_fingerprint(&chunking_fp, &content_hash)
        };
        let is_pending = |result: &ParseResultWithChanges| -> bool {
            let module_fp = module_fingerprint(&result.parsed_file.source);
            result
                .module_progress
                .get(crate::hot_update::progress::MODULE_EMBEDDING)
                != Some(&module_fp)
        };
        if !parse_results.iter().any(&is_pending) {
            return Ok(parse_results.len());
        }

        {
            let parsed_files: Vec<_> = parse_results
                .iter()
                .filter(|result| is_pending(result))
                .map(|result| result.parsed_file.clone())
                .collect();
            self.context
                .storage
                .store_parsed_files(&parsed_files)
                .map_err(|e| HotUpdateError::embedding(e.to_string()))?;
        }

        let mut file_processor = self.context.file_processor.lock().await;
        // Files whose chunks may still sit in the buffer, in fill order:
        // (index into `parse_results`, chunks not yet drained).
        let mut buffered: VecDeque<(usize, usize)> = VecDeque::new();
        let mut buffer: Vec<_> = Vec::with_capacity(self.batch_size);
        let mut completed_count = 0usize;

        let pending_indices: Vec<usize> = (0..parse_results.len())
            .filter(|&index| is_pending(&parse_results[index]))
            .collect();
        for index in pending_indices {
            self.context
                .storage
                .prepare_hot_update_embedding(&parse_results[index].file_path)
                .await
                .map_err(|e| HotUpdateError::embedding(e.to_string()))?;
            let parsed_file = &parse_results[index].parsed_file;
            // Branch on the explicit route marker carried by the parse result:
            // document placeholders go through the document pipeline, code
            // files through the AST pipeline.
            let chunks = if parse_results[index].content_route.is_document() {
                file_processor
                    .process_document_chunks(
                        &parsed_file.path,
                        &parsed_file.source,
                        cce_types::OutputMode::Both,
                    )
                    .await
            } else {
                file_processor.process_parsed_file(parsed_file).await
            }
            .map_err(|e| {
                HotUpdateError::embedding(format!(
                    "Failed to process file {:?}: {}",
                    parsed_file.path, e
                ))
            })?;
            let file_chunks = chunks.len();
            buffer.extend(chunks);
            // Track only this file's own chunk count: leftover chunks from
            // earlier files may still sit in the buffer when batches did not
            // fill up at their boundary.
            buffered.push_back((index, file_chunks));
            while buffer.len() >= self.batch_size {
                let batch: Vec<_> = buffer.drain(..self.batch_size).collect();
                self.context
                    .storage
                    .store_vectors_batched(&batch, self.batch_size, 0)
                    .await
                    .map_err(|e| HotUpdateError::embedding(e.to_string()))?;
                completed_count += self
                    .settle_stored_files(
                        ctx,
                        parse_results,
                        &mut buffered,
                        batch.len(),
                        &chunking_fp,
                    )
                    .await?;
            }
        }
        drop(file_processor);

        // Final flush: drain the trailing partial batch so every file's
        // completion marker reflects durably stored vectors.
        if !buffer.is_empty() {
            let batch = std::mem::take(&mut buffer);
            self.context
                .storage
                .store_vectors_batched(&batch, self.batch_size, 0)
                .await
                .map_err(|e| HotUpdateError::embedding(e.to_string()))?;
            completed_count += self
                .settle_stored_files(ctx, parse_results, &mut buffered, batch.len(), &chunking_fp)
                .await?;
        }

        Ok(completed_count)
    }

    /// Mark files fully drained from the buffer as complete.
    ///
    /// Persists each file's embedding progress marker right away (crash
    /// safety) and mirrors it into the in-memory map so a same-operation
    /// retry skips already-stored files.
    async fn settle_stored_files(
        &self,
        ctx: &crate::operation::OperationContext,
        parse_results: &mut [ParseResultWithChanges],
        buffered: &mut VecDeque<(usize, usize)>,
        stored: usize,
        chunking_fp: &str,
    ) -> Result<usize> {
        let mut remaining = stored;
        let mut completed = 0usize;
        while remaining > 0 {
            let Some((_, pending_chunks)) = buffered.front_mut() else {
                // The buffer only ever holds chunks of queued files, so the
                // queue cannot run dry before `remaining` reaches zero.
                break;
            };
            let take = (*pending_chunks).min(remaining);
            *pending_chunks -= take;
            remaining -= take;
            if *pending_chunks == 0
                && let Some((index, _)) = buffered.pop_front()
            {
                let module_fp = {
                    let source = &parse_results[index].parsed_file.source;
                    let content_hash = cce_utils::hash::calculate_hash(source.as_bytes());
                    crate::hot_update::progress::module_input_fingerprint(
                        chunking_fp,
                        &content_hash,
                    )
                };
                let file_path = parse_results[index].file_path.clone();
                crate::hot_update::progress::persist_module_progress(
                    &self.context.checkpoint_manager,
                    ctx,
                    &file_path,
                    crate::hot_update::progress::MODULE_EMBEDDING,
                    &module_fp,
                )
                .await
                .map_err(|e| HotUpdateError::embedding(e.to_string()))?;
                parse_results[index].module_progress.insert(
                    crate::hot_update::progress::MODULE_EMBEDDING.to_string(),
                    module_fp,
                );
                self.count_embedded_file();
                completed += 1;
            }
        }
        Ok(completed)
    }
}

#[async_trait]
impl UpdateProcessor for EmbeddingUpdateProcessor {
    fn name(&self) -> &'static str {
        "embedding"
    }

    fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    fn supports_config_reload(&self) -> bool {
        // Embedding processor doesn't need config reload as it uses runtime configuration
        false
    }

    async fn prepare_operation(
        &self,
        ctx: &crate::operation::OperationContext,
    ) -> crate::hot_update::Result<()> {
        self.context
            .storage
            .begin_hot_update_candidate(&ctx.operation_id, ctx.resume)
            .await
            .map(|_| ())
            .map_err(|e| HotUpdateError::embedding(e.to_string()))
    }

    async fn commit_operation(
        &self,
        ctx: &crate::operation::OperationContext,
    ) -> crate::hot_update::Result<()> {
        self.context
            .storage
            .activate_hot_update_candidate(&ctx.operation_id)
            .map_err(|e| HotUpdateError::embedding(e.to_string()))?;
        if let Err(error) = self.context.storage.gc_stale_generations().await {
            tracing::warn!(error = %error, "Generation GC after embedding publication failed");
        }
        Ok(())
    }

    async fn abort_operation(
        &self,
        ctx: &crate::operation::OperationContext,
        reason: &str,
    ) -> crate::hot_update::Result<()> {
        self.context
            .storage
            .fail_hot_update_candidate(&ctx.operation_id, reason)
            .map_err(|e| HotUpdateError::embedding(e.to_string()))
    }

    async fn process_operation(
        &self,
        ctx: &crate::operation::OperationContext,
        batch_result: &mut crate::hot_update::BatchChangeResult,
    ) -> crate::hot_update::Result<crate::operation::OperationProcessResult> {
        use crate::operation::{ModuleFailure, OperationMetrics, OperationProcessResult};
        use std::time::Instant;

        if !self.is_enabled() {
            return Ok(OperationProcessResult {
                operation_id: ctx.operation_id.clone(),
                processed_files: 0,
                success_files: Vec::new(),
                failed_modules: Vec::new(),
                metrics: OperationMetrics::default(),
            });
        }

        let start = Instant::now();
        let mut failed_modules = Vec::new();
        let mut success_files = Vec::new();
        let mut processed_count = 0usize;

        // Step 1: Batch remove deleted files using shared deletion handler
        processed_count += process_deletions(
            &self.context.storage,
            batch_result,
            "embedding",
            &mut failed_modules,
        )
        .await;

        // Step 1.5: refresh stored data when configuration drifted. The
        // embedder sweep re-embeds from stored chunk texts; the chunking sweep
        // re-chunks unchanged files from disk (local parse, no LLM). Neither
        // conflicts with Step 2 (which targets the candidate epoch for
        // actually-changed files).
        if let Err(e) = self.regenerate_on_embedder_drift(ctx).await {
            failed_modules.push(ModuleFailure {
                file_path: String::new(),
                module_name: "embedding".to_string(),
                error: e.to_string(),
                retry_count: 0,
                next_retry_time: None,
            });
            tracing::error!(error = %e, "Embedder drift regeneration failed");
        }
        if let Err(e) = self
            .regenerate_on_chunking_drift(
                ctx,
                &crate::hot_update::processors::rechunk::operation_changed_paths(batch_result),
            )
            .await
        {
            failed_modules.push(ModuleFailure {
                file_path: String::new(),
                module_name: "embedding".to_string(),
                error: e.to_string(),
                retry_count: 0,
                next_retry_time: None,
            });
            tracing::error!(error = %e, "Chunking-drift regeneration failed");
        }

        // Step 2: Batch process added/modified files
        if !batch_result.parse_results.is_empty() {
            match self
                .process_files_batched(ctx, &mut batch_result.parse_results)
                .await
            {
                Ok(_) => {
                    processed_count += batch_result.parse_results.len();
                    for parse_result in &batch_result.parse_results {
                        success_files.push(parse_result.file_path.to_string_lossy().to_string());
                    }
                }
                Err(e) => {
                    for parse_result in &batch_result.parse_results {
                        failed_modules.push(ModuleFailure {
                            file_path: parse_result.file_path.to_string_lossy().to_string(),
                            module_name: "embedding".to_string(),
                            error: e.to_string(),
                            retry_count: 0,
                            next_retry_time: None,
                        });
                    }
                    tracing::error!(error = %e, "Failed to process files for embeddings");
                }
            }
        }

        let error_count = failed_modules.len();
        Ok(OperationProcessResult {
            operation_id: ctx.operation_id.clone(),
            processed_files: processed_count,
            success_files,
            failed_modules,
            metrics: OperationMetrics {
                duration_ms: start.elapsed().as_millis() as i64,
                llm_tokens_used: None,
                llm_cost_usd: None,
                error_count,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_batch_size() {
        assert_eq!(DEFAULT_EMBEDDING_BATCH_SIZE, 32);
    }
}
