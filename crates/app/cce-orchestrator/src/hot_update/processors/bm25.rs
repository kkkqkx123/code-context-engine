//! BM25 update processor
//!
//! This module handles updates to BM25 full-text search index during hot updates.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering as AtomicOrdering};

use async_trait::async_trait;

use crate::hot_update::ParseResultWithChanges;
use crate::hot_update::error::{HotUpdateError, Result};
use crate::hot_update::processors::context::ProcessorContext;
use crate::hot_update::processors::deletion::process_deletions;
use crate::hot_update::processors::trait_def::UpdateProcessor;

/// BM25 update processor
pub struct Bm25UpdateProcessor {
    /// Shared processor context
    context: Arc<ProcessorContext>,
    /// Whether this processor is enabled
    enabled: Arc<AtomicBool>,
    /// Test-only counter incremented whenever a file is actually indexed
    /// (skipped files whose module progress marker matches are not counted).
    index_counter: Option<Arc<AtomicUsize>>,
}

/// Page size for chunking-drift sweeps over the `files` table.
const RECHUNK_PAGE_SIZE: i64 = 128;

/// `project_meta` key persisting the AST-to-NL pipeline fingerprint that
/// produced the currently stored BM25 documents.
const BM25_RECHUNK_KEY: &str = "chunking_fingerprint_bm25";

impl Bm25UpdateProcessor {
    /// Create a new BM25 update processor
    pub fn new(context: Arc<ProcessorContext>) -> Self {
        Self {
            context,
            enabled: Arc::new(AtomicBool::new(true)),
            index_counter: None,
        }
    }

    /// Attach a test-only counter that is incremented per indexed file.
    pub fn with_index_counter(mut self, counter: Arc<AtomicUsize>) -> Self {
        self.index_counter = Some(counter);
        self
    }

    fn count_indexed_file(&self) {
        if let Some(counter) = &self.index_counter {
            counter.fetch_add(1, AtomicOrdering::Relaxed);
        }
    }

    /// Set whether this processor is enabled
    pub async fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, AtomicOrdering::Relaxed);
    }

    /// Process a single file for BM25 update
    async fn process_file(
        &self,
        ctx: &crate::operation::OperationContext,
        parse_result: ParseResultWithChanges,
    ) -> Result<()> {
        let content_hash =
            cce_utils::hash::calculate_hash(parse_result.parsed_file.source.as_bytes());

        // Generate chunks and compute the module input fingerprint (chunking
        // configuration + content) in one lock scope. Files whose BM25
        // documents already exist for the current inputs are skipped.
        let (chunks, module_fp) = {
            let mut file_processor = self.context.file_processor.lock().await;
            let module_fp = crate::hot_update::progress::module_input_fingerprint(
                &file_processor.chunking_fingerprint(),
                &content_hash,
            );
            if parse_result
                .module_progress
                .get(crate::hot_update::progress::MODULE_BM25)
                == Some(&module_fp)
            {
                return Ok(());
            }

            // Branch on the explicit route marker carried by the parse result:
            // document placeholders go through the document pipeline, code
            // files through the AST pipeline.
            let chunks = if parse_result.content_route.is_document() {
                file_processor
                    .process_document_chunks(
                        &parse_result.parsed_file.path,
                        &parse_result.parsed_file.source,
                        cce_types::OutputMode::Both,
                    )
                    .await
            } else {
                file_processor
                    .process_parsed_file(&parse_result.parsed_file)
                    .await
            }
            .map_err(|e| {
                HotUpdateError::bm25(format!(
                    "Failed to process file {:?}: {}",
                    parse_result.file_path, e
                ))
            })?;
            (chunks, module_fp)
        };

        // Storage operations happen after the file_processor lock is released
        // - allows concurrent processing
        self.context
            .storage
            .prepare_hot_update_bm25(&parse_result.file_path)
            .await
            .map_err(|e| HotUpdateError::bm25(e.to_string()))?;
        self.context
            .storage
            .store_parsed_files(std::slice::from_ref(&parse_result.parsed_file))
            .map_err(|e| HotUpdateError::bm25(e.to_string()))?;

        self.context
            .storage
            .hot_update_bm25_file(&parse_result.file_path, &chunks)
            .await
            .map_err(|e| {
                HotUpdateError::bm25(format!(
                    "Failed to update BM25 index for {:?}: {}",
                    parse_result.file_path, e
                ))
            })?;
        self.count_indexed_file();

        crate::hot_update::progress::persist_module_progress(
            &self.context.checkpoint_manager,
            ctx,
            &parse_result.file_path,
            crate::hot_update::progress::MODULE_BM25,
            &module_fp,
        )
        .await
        .map_err(|e| HotUpdateError::bm25(e.to_string()))?;

        Ok(())
    }

    /// Re-index every unchanged file from disk when the AST-to-NL pipeline
    /// configuration drifted.
    ///
    /// Mirrors [`Self::regenerate_on_chunking_drift`] on the embedding side:
    /// each unchanged file is re-parsed and re-chunked locally (no LLM calls)
    /// and rewritten into the candidate generation. Files whose on-disk
    /// content no longer matches the recorded hash are skipped and left to
    /// the regular change flow.
    async fn regenerate_on_chunking_drift(
        &self,
        ctx: &crate::operation::OperationContext,
        skip_paths: &std::collections::HashSet<String>,
    ) -> Result<()> {
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
        match detect_fingerprint_drift(client, storage.project_id(), BM25_RECHUNK_KEY, &pipeline_fp)
            .map_err(|e| HotUpdateError::bm25(e.to_string()))?
        {
            FingerprintDrift::Current | FingerprintDrift::BaselineWritten => return Ok(()),
            FingerprintDrift::Drifted => {}
        }

        let Some(active_epoch) = storage
            .active_data_epoch()
            .map_err(|e| HotUpdateError::bm25(e.to_string()))?
        else {
            persist_fingerprint(client, storage.project_id(), BM25_RECHUNK_KEY, &pipeline_fp)
                .map_err(|e| HotUpdateError::bm25(e.to_string()))?;
            return Ok(());
        };

        tracing::info!(
            project_id = storage.project_id(),
            active_epoch,
            "Chunking configuration changed; re-indexing BM25 unchanged files from disk"
        );

        let root_path = resolve_project_root(client, storage.project_id())
            .map_err(|e| HotUpdateError::bm25(e.to_string()))?;

        let mut offset: i64 = 0;
        let mut rebuilt = 0usize;
        let mut skipped = 0usize;
        loop {
            let page = {
                let conn = client
                    .read_connection()
                    .map_err(|e| HotUpdateError::bm25(e.to_string()))?;
                FileRepository::get_by_project_and_epoch_paged(
                    &conn,
                    storage.project_id(),
                    active_epoch,
                    RECHUNK_PAGE_SIZE,
                    offset,
                )
                .map_err(|e| HotUpdateError::bm25(e.to_string()))?
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
                    .map_err(|e| HotUpdateError::bm25(e.to_string()))?;
                let Some(chunks) = chunks else {
                    skipped += 1;
                    continue;
                };
                if chunks.is_empty() {
                    continue;
                }
                let path = std::path::PathBuf::from(&file.path);
                storage
                    .prepare_hot_update_bm25(&path)
                    .await
                    .map_err(|e| HotUpdateError::bm25(e.to_string()))?;
                storage
                    .hot_update_bm25_file(&path, &chunks)
                    .await
                    .map_err(|e| HotUpdateError::bm25(e.to_string()))?;
                rebuilt += 1;
            }
        }

        persist_fingerprint(client, storage.project_id(), BM25_RECHUNK_KEY, &pipeline_fp)
            .map_err(|e| HotUpdateError::bm25(e.to_string()))?;

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
            "Chunking-drift BM25 regeneration finished"
        );
        Ok(())
    }
}

#[async_trait]
impl UpdateProcessor for Bm25UpdateProcessor {
    fn name(&self) -> &'static str {
        "bm25"
    }

    fn is_enabled(&self) -> bool {
        self.enabled.load(AtomicOrdering::Relaxed)
    }

    fn supports_config_reload(&self) -> bool {
        // BM25 processor doesn't need config reload as it uses runtime configuration
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
            .map_err(|e| HotUpdateError::bm25(e.to_string()))
    }

    async fn commit_operation(
        &self,
        ctx: &crate::operation::OperationContext,
    ) -> crate::hot_update::Result<()> {
        self.context
            .storage
            .activate_hot_update_candidate(&ctx.operation_id)
            .map_err(|e| HotUpdateError::bm25(e.to_string()))?;
        if let Err(error) = self.context.storage.gc_stale_generations().await {
            tracing::warn!(error = %error, "Generation GC after BM25 publication failed");
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
            .map_err(|e| HotUpdateError::bm25(e.to_string()))
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

        // Process removed files using shared deletion handler
        processed_count += process_deletions(
            &self.context.storage,
            batch_result,
            "bm25",
            &mut failed_modules,
        )
        .await;

        // Refresh BM25 documents when the pipeline configuration drifted;
        // local re-parse only, no LLM round-trips.
        if let Err(e) = self
            .regenerate_on_chunking_drift(
                ctx,
                &crate::hot_update::processors::rechunk::operation_changed_paths(batch_result),
            )
            .await
        {
            failed_modules.push(ModuleFailure {
                file_path: String::new(),
                module_name: "bm25".to_string(),
                error: e.to_string(),
                retry_count: 0,
                next_retry_time: None,
            });
            tracing::error!(error = %e, "Chunking-drift BM25 regeneration failed");
        }

        // Process modified/added files
        for parse_result in &batch_result.parse_results {
            match self.process_file(ctx, parse_result.clone()).await {
                Ok(_) => {
                    processed_count += 1;
                    success_files.push(parse_result.file_path.to_string_lossy().to_string());
                }
                Err(e) => {
                    failed_modules.push(ModuleFailure {
                        file_path: parse_result.file_path.to_string_lossy().to_string(),
                        module_name: "bm25".to_string(),
                        error: e.to_string(),
                        retry_count: 0,
                        next_retry_time: None,
                    });
                    tracing::warn!(
                        file = %parse_result.file_path.display(),
                        error = %e,
                        "Failed to update BM25 index"
                    );
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
