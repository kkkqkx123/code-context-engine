//! Update processor for hot update integration
//!
//! This module provides the update processor that integrates
//! the export functionality into the hot update workflow.
//!
//! # Consistency behavior
//!
//! Export writes Markdown files directly to `.cce/nl_docs/` during the hot
//! update `process_operation` phase, without the epoch-based candidate
//! lifecycle used by the storage-backed processors (embedding, summary).
//! Individual files are written atomically (temp file + rename), so external
//! readers never observe a partially-written document. However, the
//! directory as a whole is NOT a consistent snapshot: during a hot update,
//! some files may already reflect the new generation while others still show
//! the previous one. This is acceptable for reference documentation but
//! should be documented for any consumer that relies on cross-file
//! consistency.
//!
//! # Transactional lifecycle
//!
//! The processor participates in the candidate lifecycle:
//! - `prepare_operation` clears per-operation staging state;
//! - `process_operation` renders documents into memory staging (no disk I/O);
//! - `commit_operation` flushes staged documents into `.cce/nl_docs/`
//!   atomically (backing up pre-existing files) and records `export_path`
//!   checkpoints so recovery can skip already-exported files;
//! - `abort_operation` rolls back committed files (restoring backed-up old
//!   documents or removing newly-written ones), so a failed hot update never
//!   leaves new content behind.

use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::hot_update::{
    BatchChangeResult, HotUpdateError, ParseResultWithChanges, Result, UpdateProcessor,
};
use cce_config::{AstToNlConfig, NestProcessorConfig, Settings};
use cce_parser::ast_to_nl::AstToNlConverter;
use cce_parser::ast_to_nl::chunker::GroupChunker;
use cce_parser::grouper::PreprocessingPipeline;
use cce_parser::grouper::types::ProcessingResult;
use cce_relation::index::snapshot_loader::RelationSnapshotLoader;
use cce_storage_sqlite::SqliteClient;
use cce_storage_sqlite::snapshot_store::SqliteSnapshotStore;

use crate::export::export_config_rebuild;
use crate::export::export_fingerprint::{current_relation_epoch, should_skip_export};
use crate::export::export_staging::{self, ExportContext, ExportStaging, StagedWrite};
use crate::export::export_transaction;
use crate::export::nl_exporter::NlDocumentExporter;
use crate::export::presentation::PresentationConverter;
use crate::export::summary_view::ExportSummaryView;
use crate::operation::checkpoint::CheckpointManager;
use crate::operation::{
    ModuleFailure, OperationContext, OperationMetrics, OperationProcessResult, OperationType,
};

/// Natural language document update processor
///
/// Implements UpdateProcessor trait to integrate with hot update workflow.
/// All exports go through the complete pipeline (including IndexTextEnricher).
pub struct NlDocumentUpdateProcessor {
    /// Exporter for file I/O and fallback path (chunk-based export).
    /// Wrapped in an async RwLock so configuration reload can swap it.
    exporter: Arc<tokio::sync::RwLock<Arc<NlDocumentExporter>>>,
    /// Presentation text converter
    converter: PresentationConverter,
    /// AST-to-NL converter for enriching groups before export (swappable on config reload)
    ast_converter: Arc<tokio::sync::RwLock<Arc<AstToNlConverter>>>,
    /// Chunker for splitting large texts (uses async mutex for thread safety)
    chunker: Arc<Mutex<GroupChunker>>,
    /// Grouper producing the entity groups rendered by this processor.
    ///
    /// Hot-update parse results carry no pre-computed groups — the parse stage
    /// is responsible for reading, routing and change records only. The groups
    /// are therefore derived here from the parsed file, using the same grouper
    /// configuration as the full-index path so rendered documents stay
    /// consistent with it.
    grouper: PreprocessingPipeline,
    /// Whether this processor is enabled
    enabled: bool,
    /// Checkpoint manager used to persist `export_path` for recovery skipping.
    checkpoint_manager: Option<Arc<CheckpointManager>>,
    /// SQLite client used to load the published relation snapshot for
    /// relation enhancement during hot updates.
    sqlite: Option<Arc<SqliteClient>>,
    /// Project ID used for relation snapshot queries.
    project_id: i64,
    /// Per-operation staging state.
    staging: Mutex<ExportStaging>,
    /// Last relation epoch loaded for enhancement caching.
    relation_epoch_cache: Mutex<Option<i64>>,
    /// AST-to-NL pipeline configuration used for rendering (fingerprinted so
    /// recovery can detect configuration drift).
    ast_to_nl_config: Arc<tokio::sync::RwLock<AstToNlConfig>>,
    /// Fingerprint of the grouper configuration that produced the entity
    /// groups rendered by this processor.
    grouper_fingerprint: String,
    /// Fingerprint of the summary configuration that produced the summaries
    /// rendered by this processor.
    summary_fingerprint: String,
}

impl NlDocumentUpdateProcessor {
    /// Create a new update processor
    pub fn new(exporter: Arc<NlDocumentExporter>) -> Self {
        Self::new_with_config(exporter, &AstToNlConfig::default())
    }

    /// Create a new update processor with an explicit AST-to-NL pipeline config.
    ///
    /// The hot-update export path renders documents from grouped entities, so
    /// it must use the same pipeline configuration as the full-index path to
    /// produce consistent output.
    pub fn from_config(exporter: Arc<NlDocumentExporter>, config: &AstToNlConfig) -> Self {
        Self::new_with_config(exporter, config)
    }

    /// Shared constructor capturing the AST-to-NL pipeline configuration.
    fn new_with_config(exporter: Arc<NlDocumentExporter>, config: &AstToNlConfig) -> Self {
        let ast_to_nl_config = config.clone();
        Self {
            exporter: Arc::new(tokio::sync::RwLock::new(exporter)),
            converter: PresentationConverter::new(),
            ast_converter: Arc::new(tokio::sync::RwLock::new(Arc::new(
                AstToNlConverter::with_config(config),
            ))),
            chunker: Arc::new(Mutex::new(GroupChunker::new(config.chunking.clone()))),
            grouper: PreprocessingPipeline::with_config(NestProcessorConfig::default()),
            enabled: true,
            checkpoint_manager: None,
            sqlite: None,
            project_id: 0,
            staging: Mutex::new(ExportStaging::default()),
            relation_epoch_cache: Mutex::new(None),
            ast_to_nl_config: Arc::new(tokio::sync::RwLock::new(ast_to_nl_config)),
            grouper_fingerprint: String::new(),
            summary_fingerprint: String::new(),
        }
    }

    /// Create a new update processor from initialized Settings
    ///
    /// Returns an error if Settings has not been initialized.
    pub fn from_settings(
        exporter: Arc<NlDocumentExporter>,
    ) -> std::result::Result<Self, cce_types::error::ConfigError> {
        let config = Settings::ast_to_nl()?;
        Ok(Self::new_with_config(exporter, &config))
    }

    /// Set enabled state
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Wire the checkpoint manager used to persist `export_path` markers.
    pub fn with_checkpoint_manager(mut self, manager: Arc<CheckpointManager>) -> Self {
        self.checkpoint_manager = Some(manager);
        self
    }

    /// Wire the SQLite client (for published relation snapshot queries) and
    /// the owning project ID.
    pub fn with_relation_context(
        mut self,
        sqlite: Option<Arc<SqliteClient>>,
        project_id: i64,
    ) -> Self {
        self.sqlite = sqlite;
        self.project_id = project_id;
        self
    }

    /// Wire the fingerprints of the grouper and summary configurations that
    /// produced the inputs rendered by this processor. Recovery uses them to
    /// detect configuration drift and re-export affected documents.
    pub fn with_render_inputs(
        mut self,
        grouper_fingerprint: String,
        summary_fingerprint: String,
    ) -> Self {
        self.grouper_fingerprint = grouper_fingerprint;
        self.summary_fingerprint = summary_fingerprint;
        self
    }

    /// Set the grouper configuration used to derive the rendered entity
    /// groups from parsed files.
    ///
    /// Production wiring passes the project grouper configuration so hot-update
    /// rendering groups entities exactly like the full-index path.
    pub fn with_grouper_config(mut self, config: NestProcessorConfig) -> Self {
        self.grouper = PreprocessingPipeline::with_config(config);
        self
    }

    /// Get a snapshot of the current exporter.
    async fn exporter(&self) -> Arc<NlDocumentExporter> {
        self.exporter.read().await.clone()
    }

    /// Group the entities of a parsed file for rendering.
    ///
    /// Files without entities (document placeholders, empty parses) yield
    /// `None` and are exported through the chunk-based fallback, which in
    /// turn skips them when no chunks exist.
    fn compute_render_groups(
        &self,
        parsed_file: &cce_types::entity::ParsedFile,
    ) -> Option<ProcessingResult> {
        if parsed_file.entities.is_empty() {
            return None;
        }
        Some(self.grouper.process(parsed_file))
    }

    /// Refresh relation enhancement from the published SQLite relation
    /// snapshot when the active relation epoch changes.
    async fn refresh_relation_enhancement(&self) -> Result<()> {
        let (Some(sqlite), enable) = (
            self.sqlite.clone(),
            self.exporter().await.config().enable_relation_enhancement,
        ) else {
            return Ok(());
        };
        if !enable {
            return Ok(());
        }
        let epoch =
            match sqlite.project_meta_get_int_optional(self.project_id, "active_relation_epoch") {
                Ok(Some(epoch)) => epoch,
                _ => 0,
            };
        if epoch <= 0 {
            return Ok(());
        }
        let mut cache = self.relation_epoch_cache.lock().await;
        if *cache == Some(epoch) {
            return Ok(());
        }
        match RelationSnapshotLoader::load(
            &SqliteSnapshotStore::new(sqlite.as_ref().clone()),
            self.project_id,
            epoch,
        ) {
            Ok(index) => {
                let exporter = self.exporter().await;
                let enhancer_config = crate::export::RelationEnhancerConfig::default();
                exporter.set_relation_enhancement(Arc::new(index), enhancer_config);
                *cache = Some(epoch);
                tracing::info!(
                    project_id = self.project_id,
                    relation_epoch = epoch,
                    "Loaded published relation snapshot for export enhancement"
                );
            }
            Err(error) => {
                tracing::warn!(
                    project_id = self.project_id,
                    relation_epoch = epoch,
                    error = %error,
                    "Failed to load relation snapshot for export enhancement"
                );
            }
        }
        Ok(())
    }

    /// Decide whether an already-exported document may be skipped on recovery.
    async fn should_skip_export(&self, parse_result: &ParseResultWithChanges) -> bool {
        let exporter = self.exporter().await;
        let export_config = exporter.config().clone();
        let ast_to_nl_config = self.ast_to_nl_config.read().await.clone();
        should_skip_export(
            &exporter,
            parse_result,
            &export_config,
            &ast_to_nl_config,
            &self.grouper_fingerprint,
            &self.sqlite,
            self.project_id,
        )
        .await
    }

    fn export_summary_view(
        summary: Option<&cce_parser::summary::FileSummary>,
    ) -> Option<ExportSummaryView> {
        summary.map(ExportSummaryView::from)
    }

    /// Handle file update (add/modify) - using enriched GroupConversions
    async fn stage_file_update_direct(
        &self,
        file_path: &Path,
        processing_result: &ProcessingResult,
        source: &str,
        summary: Option<&ExportSummaryView>,
        staging: &mut ExportStaging,
    ) -> Result<()> {
        let exporter = self.exporter().await;
        let ast_converter = self.ast_converter.read().await.clone();
        let export_config = exporter.config().clone();
        let ast_to_nl_config = self.ast_to_nl_config.read().await.clone();
        let epoch = current_relation_epoch(&self.sqlite, self.project_id);

        let ctx = ExportContext {
            exporter: &exporter,
            export_config: &export_config,
            ast_to_nl_config: &ast_to_nl_config,
            grouper_fingerprint: &self.grouper_fingerprint,
            relation_epoch: epoch,
        };

        export_staging::stage_file_update_direct(
            &ctx,
            &ast_converter,
            file_path,
            processing_result,
            source,
            summary,
            staging,
        )
        .await
    }

    /// Handle file update (add/modify) - chunk-based fallback
    async fn stage_file_update(
        &self,
        _file_path: &Path,
        chunks: &[cce_parser::ast_to_nl::chunker::ChunkedResult],
        source: &str,
        summary: Option<&ExportSummaryView>,
        staging: &mut ExportStaging,
    ) -> Result<()> {
        let exporter = self.exporter().await;
        let export_config = exporter.config().clone();
        let ast_to_nl_config = self.ast_to_nl_config.read().await.clone();
        let epoch = current_relation_epoch(&self.sqlite, self.project_id);

        let ctx = ExportContext {
            exporter: &exporter,
            export_config: &export_config,
            ast_to_nl_config: &ast_to_nl_config,
            grouper_fingerprint: &self.grouper_fingerprint,
            relation_epoch: epoch,
        };

        export_staging::stage_file_update(&ctx, _file_path, chunks, source, summary, staging).await
    }

    /// Stage the removal of a deleted file's document.
    async fn stage_deleted_file(&self, path: &Path, staging: &mut ExportStaging) -> Result<()> {
        let exporter = self.exporter().await;
        export_staging::stage_deleted_file(&exporter, path, staging)
    }

    /// Extract chunks from parse result
    async fn extract_chunks_from_parse_result(
        &self,
        parse_result: &ParseResultWithChanges,
        processing_result: Option<&ProcessingResult>,
    ) -> Vec<cce_parser::ast_to_nl::chunker::ChunkedResult> {
        export_staging::extract_chunks_from_parse_result(
            &self.converter,
            &self.chunker,
            &parse_result.file_path,
            &parse_result.parsed_file,
            processing_result,
        )
        .await
    }

    /// Flush one staged deletion to disk, backing up any existing document.
    async fn flush_deletion(
        &self,
        ctx: &OperationContext,
        output: &Path,
        staging: &mut ExportStaging,
    ) -> Result<()> {
        let exporter = self.exporter().await;
        export_transaction::flush_deletion(&exporter, ctx, output, staging).await
    }

    /// Flush one staged write to disk, backing up any existing document and
    /// persisting the `export_path` checkpoint marker.
    async fn flush_write(
        &self,
        ctx: &OperationContext,
        write: StagedWrite,
        staging: &mut ExportStaging,
    ) -> Result<()> {
        let exporter = self.exporter().await;
        export_transaction::flush_write(&exporter, ctx, write, staging, &self.checkpoint_manager)
            .await
    }

    /// Abort operation: restore backed-up documents and remove newly-written ones.
    async fn restore_from_backup(
        &self,
        ctx: &OperationContext,
        staging: &mut ExportStaging,
    ) -> Result<()> {
        let exporter = self.exporter().await;
        export_transaction::restore_from_backup(&exporter, ctx, staging).await
    }
}

#[async_trait]
impl UpdateProcessor for NlDocumentUpdateProcessor {
    fn name(&self) -> &'static str {
        "nl_document"
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn supports_config_reload(&self) -> bool {
        true
    }

    async fn prepare_operation(&self, _ctx: &OperationContext) -> Result<()> {
        let mut staging = self.staging.lock().await;
        for (_, backup) in staging.drain_stale_backups() {
            let _ = tokio::fs::remove_file(&backup).await;
        }
        staging.clear();
        drop(staging);
        self.refresh_relation_enhancement().await?;
        Ok(())
    }

    async fn process_operation(
        &self,
        ctx: &OperationContext,
        batch_result: &mut BatchChangeResult,
    ) -> Result<OperationProcessResult> {
        let start = std::time::Instant::now();
        let mut failed_modules = Vec::new();
        let mut success_files = Vec::new();
        let mut processed_count = 0usize;

        if ctx.operation_type == OperationType::ConfigChange {
            let config_path = ctx.config_path.as_deref().ok_or_else(|| {
                HotUpdateError::hot_update(
                    "ConfigChange operation requires a config path".to_string(),
                )
            })?;
            export_config_rebuild::rebuild_from_config(
                &self.exporter,
                &self.ast_converter,
                &self.chunker,
                &self.ast_to_nl_config,
                &self.relation_epoch_cache,
                config_path,
            )
            .await?;
            return Ok(OperationProcessResult {
                operation_id: ctx.operation_id.clone(),
                processed_files: 0,
                success_files,
                failed_modules,
                metrics: OperationMetrics {
                    duration_ms: start.elapsed().as_millis() as i64,
                    llm_tokens_used: None,
                    llm_cost_usd: None,
                    error_count: 0,
                },
            });
        }

        let mut staging = self.staging.lock().await;

        for file_change in &batch_result.file_changes {
            if file_change.change_type == crate::hot_update::FileChangeType::Deleted {
                match self
                    .stage_deleted_file(&file_change.path, &mut staging)
                    .await
                {
                    Ok(_) => {
                        processed_count += 1;
                    }
                    Err(e) => {
                        failed_modules.push(ModuleFailure {
                            file_path: file_change.path.to_string_lossy().to_string(),
                            module_name: "export".to_string(),
                            error: e.to_string(),
                            retry_count: 0,
                            next_retry_time: None,
                        });
                        tracing::error!(
                            path = %file_change.path.display(),
                            error = %e,
                            "Failed to stage deleted file"
                        );
                    }
                }
            }
        }

        for parse_result in &batch_result.parse_results {
            let path_str = parse_result.file_path.to_string_lossy().to_string();

            if self.should_skip_export(parse_result).await {
                processed_count += 1;
                success_files.push(path_str);
                continue;
            }

            let render_groups = self.compute_render_groups(&parse_result.parsed_file);
            let export_view = Self::export_summary_view(parse_result.file_summary.as_ref());
            let mut staged_result = None;
            if let Some(processing_result) = &render_groups {
                if !processing_result.groups.is_empty() {
                    let source = &*parse_result.parsed_file.source;
                    staged_result = Some(
                        self.stage_file_update_direct(
                            &parse_result.file_path,
                            processing_result,
                            source,
                            export_view.as_ref(),
                            &mut staging,
                        )
                        .await,
                    );
                }
            }

            let stage_outcome = match staged_result {
                Some(Ok(())) => Ok(()),
                _ => {
                    if let Some(Err(e)) = staged_result {
                        tracing::error!(
                            path = %parse_result.file_path.display(),
                            error = %e,
                            "Failed to stage via direct exporter, falling back to chunking"
                        );
                    }
                    let chunks = self
                        .extract_chunks_from_parse_result(parse_result, render_groups.as_ref())
                        .await;
                    let source = &*parse_result.parsed_file.source;
                    self.stage_file_update(
                        &parse_result.file_path,
                        &chunks,
                        source,
                        export_view.as_ref(),
                        &mut staging,
                    )
                    .await
                }
            };

            match stage_outcome {
                Ok(_) => {
                    processed_count += 1;
                    success_files.push(path_str);
                }
                Err(e) => {
                    failed_modules.push(ModuleFailure {
                        file_path: path_str,
                        module_name: "export".to_string(),
                        error: e.to_string(),
                        retry_count: 0,
                        next_retry_time: None,
                    });
                    tracing::error!(
                        path = %parse_result.file_path.display(),
                        error = %e,
                        "Failed to stage NL document"
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

    async fn commit_operation(&self, ctx: &OperationContext) -> Result<()> {
        let mut staging = self.staging.lock().await;

        let deletions: Vec<PathBuf> = std::mem::take(&mut staging.deletions);
        for output in &deletions {
            self.flush_deletion(ctx, output, &mut staging).await?;
        }

        let writes: Vec<StagedWrite> = std::mem::take(&mut staging.writes);
        for write in writes {
            self.flush_write(ctx, write, &mut staging).await?;
        }

        Ok(())
    }

    async fn abort_operation(&self, ctx: &OperationContext, reason: &str) -> Result<()> {
        let mut staging = self.staging.lock().await;
        self.restore_from_backup(ctx, &mut staging).await?;
        staging.writes.clear();
        staging.deletions.clear();

        *self.relation_epoch_cache.lock().await = None;

        tracing::warn!(
            operation_id = %ctx.operation_id,
            reason,
            "Aborted NL document exports"
        );
        Ok(())
    }

    async fn reload_config(&self, config_path: &Path, _project_root: &Path) -> Result<()> {
        export_config_rebuild::rebuild_from_config(
            &self.exporter,
            &self.ast_converter,
            &self.chunker,
            &self.ast_to_nl_config,
            &self.relation_epoch_cache,
            config_path,
        )
        .await
    }

    async fn on_config_change(&self, config_path: &Path, _project_root: &Path) -> Result<()> {
        export_config_rebuild::rebuild_from_config(
            &self.exporter,
            &self.ast_converter,
            &self.chunker,
            &self.ast_to_nl_config,
            &self.relation_epoch_cache,
            config_path,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::export_fingerprint::compute_render_fingerprint;
    use crate::operation::OperationType;
    use cce_parser::summary::FileSummary;

    fn test_processor(root: &std::path::Path) -> NlDocumentUpdateProcessor {
        let config = crate::export::config::ExportConfig::new(root.to_path_buf(), 1);
        let exporter = Arc::new(NlDocumentExporter::new(config));
        NlDocumentUpdateProcessor::new(exporter)
    }

    fn export_view_from_summary(summary: &FileSummary) -> ExportSummaryView {
        ExportSummaryView::from(summary)
    }

    fn test_context() -> OperationContext {
        OperationContext::new(1, "test-operation".to_string(), OperationType::HotUpdate, 0)
    }

    async fn compute_fingerprint_for_test(
        processor: &NlDocumentUpdateProcessor,
        source: &str,
        summary: Option<&ExportSummaryView>,
    ) -> String {
        let exporter = processor.exporter().await;
        let export_config = exporter.config().clone();
        let ast_to_nl_config = processor.ast_to_nl_config.read().await.clone();
        compute_render_fingerprint(
            &exporter,
            source,
            summary,
            &export_config,
            &ast_to_nl_config,
            &processor.grouper_fingerprint,
            current_relation_epoch(&processor.sqlite, processor.project_id),
        )
        .await
    }

    #[test]
    fn test_processor_name() {
        let config = crate::export::config::ExportConfig::default();
        let exporter = Arc::new(NlDocumentExporter::new(config));
        let processor = NlDocumentUpdateProcessor::new(exporter);

        assert_eq!(processor.name(), "nl_document");
    }

    #[test]
    fn test_processor_enabled() {
        let config = crate::export::config::ExportConfig::default();
        let exporter = Arc::new(NlDocumentExporter::new(config));
        let processor = NlDocumentUpdateProcessor::new(exporter);

        assert!(processor.is_enabled());

        let processor_disabled = processor.with_enabled(false);
        assert!(!processor_disabled.is_enabled());
    }

    #[test]
    fn test_processor_supports_config_reload() {
        let config = crate::export::config::ExportConfig::default();
        let exporter = Arc::new(NlDocumentExporter::new(config));
        let processor = NlDocumentUpdateProcessor::new(exporter);

        assert!(processor.supports_config_reload());
    }

    fn sample_parse_result(source: &str, summary: Option<FileSummary>) -> ParseResultWithChanges {
        let parsed = cce_types::ParsedFile::new(
            cce_types::Language::Rust,
            "src/main.rs".to_string(),
            source,
        );
        ParseResultWithChanges::new(
            PathBuf::from("src/main.rs"),
            parsed,
            crate::hot_update::FileChangeType::Modified,
            false,
        )
        .with_file_summary(summary)
    }

    fn distinct_summaries() -> (FileSummary, FileSummary) {
        let mut a = FileSummary {
            file_path: "src/main.rs".to_string(),
            language: "rust".to_string(),
            summary_text: "summarizes the module".to_string(),
            ..Default::default()
        };
        let mut b = a.clone();
        b.summary_text = "a different summary".to_string();
        a.main_entities.push("run".to_string());
        (a, b)
    }

    #[tokio::test]
    async fn test_resume_skip_requires_matching_render_fingerprint() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let processor = test_processor(tmp.path());
        let source = "fn run() {}";

        let expected = compute_fingerprint_for_test(&processor, source, None).await;

        let matching = sample_parse_result(source, None)
            .with_already_exported()
            .with_stored_render_fingerprint(Some(expected.clone()));
        assert!(
            processor.should_skip_export(&matching).await,
            "matching render fingerprint must allow skipping"
        );

        let drifted = sample_parse_result(source, None)
            .with_already_exported()
            .with_stored_render_fingerprint(Some("drifted".to_string()));
        assert!(
            !processor.should_skip_export(&drifted).await,
            "drifted render fingerprint must force a re-export"
        );

        let legacy = sample_parse_result(source, None).with_already_exported();
        assert!(
            !processor.should_skip_export(&legacy).await,
            "missing stored render fingerprint must force a re-export"
        );
    }

    #[tokio::test]
    async fn test_resume_summary_drift_forces_re_export() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let processor = test_processor(tmp.path());
        let source = "fn run() {}";
        let (summary_a, summary_b) = distinct_summaries();

        let view_a = export_view_from_summary(&summary_a);
        let view_b = export_view_from_summary(&summary_b);
        let fp_a = compute_fingerprint_for_test(&processor, source, Some(&view_a)).await;
        let fp_b = compute_fingerprint_for_test(&processor, source, Some(&view_b)).await;
        assert_ne!(
            fp_a, fp_b,
            "summary content drift must change the fingerprint"
        );

        let stored_for_a = sample_parse_result(source, Some(summary_a.clone()))
            .with_already_exported()
            .with_stored_render_fingerprint(Some(fp_a.clone()));
        assert!(processor.should_skip_export(&stored_for_a).await);

        let regenerated = sample_parse_result(source, Some(summary_b))
            .with_already_exported()
            .with_stored_render_fingerprint(Some(fp_a));
        assert!(
            !processor.should_skip_export(&regenerated).await,
            "regenerated summary must force a re-export"
        );
    }

    #[tokio::test]
    async fn test_resume_render_fingerprint_changes_with_configs() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let source = "fn run() {}";

        let base = test_processor(tmp.path());
        let fp_base = compute_fingerprint_for_test(&base, source, None).await;

        let mut grouper = cce_config::NestProcessorConfig::default();
        grouper.small_class_threshold += 1;
        let exporter = Arc::new(NlDocumentExporter::new(
            crate::export::config::ExportConfig::new(tmp.path().to_path_buf(), 1),
        ));
        let configured =
            NlDocumentUpdateProcessor::from_config(exporter, &cce_config::AstToNlConfig::default())
                .with_render_inputs(
                    crate::export::fingerprint::config_fingerprint(&grouper),
                    String::new(),
                );
        let fp_configured = compute_fingerprint_for_test(&configured, source, None).await;
        assert_ne!(
            fp_base, fp_configured,
            "grouper configuration drift must change the fingerprint"
        );
    }

    #[tokio::test]
    async fn test_renders_groups_from_parsed_file_without_parse_time_groups() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let root = tmp.path();
        let processor = test_processor(root);

        let mut coordinator = cce_parser::parser::ParseCoordinator::new();
        let parsed = coordinator
            .parse(
                "src/main.rs",
                "fn run_app() {\n    println!(\"started\");\n}\n",
            )
            .expect("rust source parses");
        assert!(!parsed.entities.is_empty(), "fixture must yield entities");
        let mut batch = BatchChangeResult::new();
        batch.add_parse_result(ParseResultWithChanges::new(
            PathBuf::from("src/main.rs"),
            parsed,
            crate::hot_update::FileChangeType::Modified,
            false,
        ));

        processor
            .prepare_operation(&test_context())
            .await
            .expect("prepare");
        let outcome = processor
            .process_operation(&test_context(), &mut batch)
            .await
            .expect("process");
        processor
            .commit_operation(&test_context())
            .await
            .expect("commit");

        assert_eq!(
            outcome.success_files.len(),
            1,
            "the changed file must be exported"
        );
        let doc = root.join(".cce/nl_docs/src/main.rs.md");
        assert!(doc.exists(), "rendered document must exist on disk");
        let content = std::fs::read_to_string(&doc).expect("read rendered document");
        assert!(
            content.contains("run_app"),
            "document must be rendered from the parsed entities"
        );

        let placeholder = cce_types::entity::ParsedFile::new(
            cce_types::Language::Unknown,
            "docs/README.md".to_string(),
            "# guide",
        );
        let mut batch = BatchChangeResult::new();
        batch.add_parse_result(ParseResultWithChanges::new(
            PathBuf::from("docs/README.md"),
            placeholder,
            crate::hot_update::FileChangeType::Modified,
            false,
        ));
        processor
            .prepare_operation(&test_context())
            .await
            .expect("prepare");
        let outcome = processor
            .process_operation(&test_context(), &mut batch)
            .await
            .expect("process document placeholder");
        assert_eq!(
            outcome.success_files.len(),
            1,
            "document files export nothing but must not fail"
        );
    }

    #[tokio::test]
    async fn test_commit_writes_staged_document() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let root = tmp.path();
        let processor = test_processor(root);

        processor
            .prepare_operation(&test_context())
            .await
            .expect("prepare");
        let write = export_staging::make_staged_write(
            &*processor.exporter().await,
            "src/main.rs",
            "# src/main.rs\n\nfn run()".to_string(),
            "fp".to_string(),
        )
        .await
        .expect("staged write");
        {
            let mut staging = processor.staging.lock().await;
            staging.writes.push(write);
        }

        processor
            .commit_operation(&test_context())
            .await
            .expect("commit");

        let output = root.join(".cce/nl_docs/src/main.rs.md");
        assert!(output.exists(), "committed document should exist");
        let content = std::fs::read_to_string(&output).expect("read document");
        assert!(content.contains("fn run()"));
    }

    #[tokio::test]
    async fn test_abort_removes_new_document() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let root = tmp.path();
        let processor = test_processor(root);

        processor
            .prepare_operation(&test_context())
            .await
            .expect("prepare");
        let write = export_staging::make_staged_write(
            &*processor.exporter().await,
            "src/new.rs",
            "# src/new.rs\n\nnew".to_string(),
            "fp".to_string(),
        )
        .await
        .expect("staged write");
        {
            let mut staging = processor.staging.lock().await;
            staging.writes.push(write);
        }

        processor
            .commit_operation(&test_context())
            .await
            .expect("commit");
        let output = root.join(".cce/nl_docs/src/new.rs.md");
        assert!(output.exists());

        processor
            .abort_operation(&test_context(), "test abort")
            .await
            .expect("abort");
        assert!(!output.exists(), "aborted new document must be removed");
    }

    #[tokio::test]
    async fn test_abort_restores_backed_up_document() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let root = tmp.path();
        let output = root.join(".cce/nl_docs/src/main.rs.md");
        std::fs::create_dir_all(output.parent().expect("parent")).expect("create dirs");
        std::fs::write(&output, "old content").expect("pre-write old document");

        let processor = test_processor(root);

        processor
            .prepare_operation(&test_context())
            .await
            .expect("prepare");
        let write = export_staging::make_staged_write(
            &*processor.exporter().await,
            "src/main.rs",
            "new content".to_string(),
            "fp".to_string(),
        )
        .await
        .expect("staged write");
        {
            let mut staging = processor.staging.lock().await;
            staging.writes.push(write);
        }

        processor
            .commit_operation(&test_context())
            .await
            .expect("commit");
        assert_eq!(
            std::fs::read_to_string(&output).expect("read new"),
            "new content"
        );

        processor
            .abort_operation(&test_context(), "test abort")
            .await
            .expect("abort");
        assert_eq!(
            std::fs::read_to_string(&output).expect("read restored"),
            "old content",
            "aborted modification must restore the previous document"
        );
    }

    #[tokio::test]
    async fn test_abort_restores_deleted_document() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let root = tmp.path();
        let output = root.join(".cce/nl_docs/src/old.rs.md");
        std::fs::create_dir_all(output.parent().expect("parent")).expect("create dirs");
        std::fs::write(&output, "old content").expect("pre-write old document");

        let processor = test_processor(root);

        processor
            .prepare_operation(&test_context())
            .await
            .expect("prepare");
        {
            let mut staging = processor.staging.lock().await;
            staging.deletions.push(output.clone());
        }

        processor
            .commit_operation(&test_context())
            .await
            .expect("commit");
        assert!(
            !output.exists(),
            "committed deletion should remove the document"
        );

        processor
            .abort_operation(&test_context(), "test abort")
            .await
            .expect("abort");
        assert_eq!(
            std::fs::read_to_string(&output).expect("read restored"),
            "old content",
            "aborted deletion must restore the previous document"
        );
    }

    #[test]
    fn test_include_summary_gates_metadata_section() {
        use cce_parser::ast_to_nl::chunker::ChunkPath;
        use cce_types::ast_to_nl::chunked::{ChunkMetadata, ChunkedResult};
        use cce_types::entity::EntityKind;
        use cce_types::language::Language;

        let tmp = tempfile::tempdir().expect("temp dir");
        let root = tmp.path();

        let mut chunk = ChunkedResult::new("c1".into(), "g1".into(), ChunkPath::Embedding, 0, 1);
        chunk.text = "run function. Starts the application.".to_string();
        chunk.metadata = ChunkMetadata::for_code(
            "src/lib.rs".to_string(),
            cce_types::Span::from_lines(1, 2),
            Language::Rust,
            cce_types::ast_to_nl::chunked::CodeSpecificMetadata {
                entity_kind: EntityKind::Function,
                modifiers: vec![],
                split_reason: Default::default(),
                content_entity_ids: vec![],
                content_entity_names: vec![],
                context_entity_ids: vec![],
                overlap_entities: vec![],
                has_overlap: false,
                is_fragment: false,
                fragment_index: None,
                total_fragments: None,
                original_entity_id: None,
                pattern_info: None,
            },
        );
        let chunks = vec![chunk];

        let summary = export_view_from_summary(
            &FileSummary::new("src/lib.rs")
                .with_summary("Entry point")
                .with_imports(vec!["std::io".to_string()]),
        );

        let config =
            crate::export::config::ExportConfig::new(root.to_path_buf(), 1).with_summary(true);
        let exporter = NlDocumentExporter::new(config);
        let (_, content_with_summary) = exporter
            .render_file(&chunks, Some(&summary))
            .expect("render with summary");
        assert!(content_with_summary.contains("- imports:"));
        assert!(content_with_summary.contains("summary:"));

        let config =
            crate::export::config::ExportConfig::new(root.to_path_buf(), 1).with_summary(false);
        let exporter = NlDocumentExporter::new(config);
        let (_, content_without_summary) = exporter
            .render_file(&chunks, Some(&summary))
            .expect("render without summary");
        assert!(!content_without_summary.contains("- imports:"));
        assert!(!content_without_summary.contains("summary:"));
    }
}
