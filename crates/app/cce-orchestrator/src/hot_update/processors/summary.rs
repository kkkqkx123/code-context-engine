//! Summary update processor
//!
//! This module handles updates to file summaries during hot updates.

use async_trait::async_trait;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use crate::hot_update::BatchChangeResult;
use crate::hot_update::error::HotUpdateError;
use crate::hot_update::error::Result;
use crate::hot_update::processors::deletion::process_deletions;
use crate::hot_update::processors::trait_def::UpdateProcessor;
use crate::index::StorageCoordinator;
use crate::operation::{
    CheckpointManager, ModuleFailure, OperationContext, OperationMetrics, OperationProcessResult,
};
use cce_parser::document::{DocSummaryExt, PipelineRouter};
use cce_parser::summary::SummaryGenerator;

/// Summary update processor
pub struct SummaryUpdateProcessor {
    /// Storage coordinator for summary operations
    storage: Arc<StorageCoordinator>,
    /// Summary generator
    summary_generator: Arc<dyn SummaryGenerator>,
    /// Whether this processor is enabled
    enabled: Arc<AtomicBool>,
    /// Fingerprint of the summary configuration driving the generator.
    summary_fingerprint: String,
    /// Checkpoint manager used to persist per-file summary progress markers.
    checkpoint_manager: Option<Arc<CheckpointManager>>,
}

impl SummaryUpdateProcessor {
    /// Create a new summary update processor
    pub fn new(
        storage: Arc<StorageCoordinator>,
        summary_generator: Arc<dyn SummaryGenerator>,
    ) -> Self {
        Self {
            storage,
            summary_generator,
            enabled: Arc::new(AtomicBool::new(true)),
            summary_fingerprint: String::new(),
            checkpoint_manager: None,
        }
    }

    /// Record the fingerprint of the summary configuration driving the
    /// generator. Recovery regenerates a persisted summary when this no longer
    /// matches the fingerprint captured at generation time.
    pub fn with_summary_fingerprint(mut self, fingerprint: String) -> Self {
        self.summary_fingerprint = fingerprint;
        self
    }

    /// Wire the checkpoint manager used to persist per-file progress markers.
    pub fn with_checkpoint_manager(mut self, manager: Arc<CheckpointManager>) -> Self {
        self.checkpoint_manager = Some(manager);
        self
    }

    /// Set whether this processor is enabled
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }
}

#[async_trait]
impl UpdateProcessor for SummaryUpdateProcessor {
    fn name(&self) -> &'static str {
        "summary"
    }

    fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    fn supports_config_reload(&self) -> bool {
        // Summary processor doesn't need config reload as it uses runtime configuration
        false
    }

    async fn process_operation(
        &self,
        ctx: &OperationContext,
        batch_result: &mut BatchChangeResult,
    ) -> Result<OperationProcessResult> {
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
            self.storage.as_ref(),
            batch_result,
            "summary",
            &mut failed_modules,
        )
        .await;

        // Process modified/added files
        let mut summaries = Vec::new();
        let mut summary_writes = Vec::new();

        for parse_result in &mut batch_result.parse_results {
            let content_hash =
                cce_utils::hash::calculate_hash(parse_result.parsed_file.source.as_bytes());
            // The module progress marker folds the summary configuration and
            // the content hash in. A matching marker means the candidate
            // generation already holds a summary rendered from these exact
            // inputs, so storing it again (and re-running the generator) can be
            // skipped.
            let module_fp = crate::hot_update::progress::module_input_fingerprint(
                &self.summary_fingerprint,
                &content_hash,
            );
            let already_stored = parse_result
                .module_progress
                .get(crate::hot_update::progress::MODULE_SUMMARY)
                == Some(&module_fp);

            // Reuse the persisted summary in memory only when the configuration
            // that produced it still matches the current one and it was
            // generated from the current content. A summary-config or content
            // change between a crash and its resume forces a regeneration so a
            // stale summary never flows into downstream exports.
            let reuse = parse_result.file_summary.is_some()
                && parse_result.stored_summary_fingerprint.as_deref()
                    == Some(self.summary_fingerprint.as_str())
                && parse_result.stored_content_hash.as_deref() == Some(&content_hash);

            let generated = if reuse {
                parse_result.file_summary.clone().unwrap_or_default()
            } else if parse_result.content_route.is_document() {
                // Document-route files take their summary from the document
                // pipeline's own summarize stage — the exact same source the
                // full index uses — so both paths produce identical
                // category/language/heading encodings. The generic generator
                // only backs the failure case.
                match PipelineRouter::global().summarize_only(
                    &parse_result.parsed_file.source,
                    &parse_result.parsed_file.path,
                ) {
                    Some(doc_summary) => doc_summary.to_file_summary(),
                    None => {
                        self.summary_generator
                            .generate(&parse_result.parsed_file)
                            .await
                    }
                }
            } else {
                self.summary_generator
                    .generate(&parse_result.parsed_file)
                    .await
            };
            // Backfill so later derived processors (e.g. NL document export)
            // can reuse the pre-generated summary instead of re-generating it.
            parse_result.file_summary = Some(generated.clone());
            let summary = generated;
            parse_result.summary_fingerprint = Some(self.summary_fingerprint.clone());

            // A marker that still matches the current inputs and a valid
            // in-memory summary mean the candidate already has everything this
            // module needs to write for the file.
            if already_stored && reuse {
                continue;
            }

            // Remove any stale candidate summary before writing the fresh one.
            if let Err(error) = self
                .storage
                .prepare_hot_update_summary(&parse_result.file_path)
                .await
            {
                tracing::warn!(
                    path = %parse_result.file_path.display(),
                    error = %error,
                    "Failed to prepare candidate summary"
                );
            }

            summaries.push(summary);
            summary_writes.push((parse_result.file_path.clone(), module_fp));
        }

        // Store all summaries in batch
        if !summaries.is_empty() {
            match self.storage.store_summaries(&summaries).await {
                Ok(_) => {
                    processed_count += summaries.len();
                    for (path, module_fp) in &summary_writes {
                        success_files.push(path.to_string_lossy().to_string());
                        // Record completion so recovery can skip re-summarizing.
                        if let Err(error) = crate::hot_update::progress::persist_module_progress(
                            &self.checkpoint_manager,
                            ctx,
                            path,
                            crate::hot_update::progress::MODULE_SUMMARY,
                            module_fp,
                        )
                        .await
                        {
                            tracing::warn!(
                                path = %path.display(),
                                error = %error,
                                "Failed to persist summary progress marker"
                            );
                        }
                    }
                }
                Err(e) => {
                    for (path, _) in &summary_writes {
                        failed_modules.push(ModuleFailure {
                            file_path: path.to_string_lossy().to_string(),
                            module_name: "summary".to_string(),
                            error: e.to_string(),
                            retry_count: 0,
                            next_retry_time: None,
                        });
                    }
                    tracing::error!(error = %e, "Failed to store summaries");
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

    async fn prepare_operation(&self, ctx: &OperationContext) -> Result<()> {
        self.storage
            .begin_hot_update_candidate(&ctx.operation_id, ctx.resume)
            .await
            .map(|_| ())
            .map_err(|error| HotUpdateError::summary(error.to_string()))
    }

    async fn commit_operation(&self, ctx: &OperationContext) -> Result<()> {
        self.storage
            .activate_hot_update_candidate(&ctx.operation_id)
            .map_err(|error| HotUpdateError::summary(error.to_string()))?;
        if let Err(error) = self.storage.gc_stale_generations().await {
            tracing::warn!(error = %error, "Generation GC after summary publication failed");
        }
        Ok(())
    }

    async fn abort_operation(&self, ctx: &OperationContext, reason: &str) -> Result<()> {
        self.storage
            .fail_hot_update_candidate(&ctx.operation_id, reason)
            .map_err(|error| HotUpdateError::summary(error.to_string()))
    }
}
