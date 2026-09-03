//! Index orchestrator for coordinating indexing operations
//!
//! This module provides high-level indexing coordination that orchestrates
//! the complete indexing workflow: scan → parse → convert → embed → store.
//!
//! # Batch Processing Architecture
//!
//! The orchestrator uses a batch processing model to control memory usage:
//!
//! ```text
//! Scan (all files) → Split into batches → Process batch → Store immediately → Release memory
//!                                                  ↓
//!                                          Next batch...
//! ```
//!
//! This ensures that memory usage is bounded by batch size rather than total file count.
//!
//! # Module Structure
//!
//! The orchestrator is split into phase modules so each index stage keeps its
//! own responsibility:
//!
//! - `mod.rs`: struct definition, builder assembly, `execute()` phase dispatch
//! - `checkpoint`: checkpoint recovery, hash validation, relation/export rehydration
//! - `batch`: per-batch processing and the full-index batch loop
//! - `finalize`: relation publication, NL document export, manifest activation
//! - `incremental`: single-file indexing, removal, and relation maintenance

mod batch;
mod checkpoint;
mod finalize;
mod incremental;

use std::collections::HashMap;
use std::sync::Arc;

use crate::export::NlDocumentExporter;
use cce_config::modules::summary::SummaryGenerationStrategy as SummaryStrategy;
use cce_config::{AstToNlConfig, BatchConfig, NestProcessorConfig, RelationConfig, SummaryConfig};
use cce_llm::ChatConfig;
use cce_llm_client::HttpLlmClient;
use cce_metrics::{
    FileProcessingMetrics, MetricsRegistry, ParserMetrics, PipelineStage, PipelineStageMetrics,
    RelationMetrics, SummaryMetrics,
};
use cce_metrics::{ScannerMetrics, SearchMetrics};
use cce_metrics_infra::ProgressTracker;
use cce_parser::summary::{
    FileSummary, ModelEnhancedGenerator, RuleBasedGenerator, SummaryGenerator,
};
use cce_plugin::PluginRegistry;
use cce_relation::IndexBuilder;
use cce_scanner::ScanOptions;
use cce_types::OutputMode;

use super::super::CheckpointManager;
use super::super::error::OrchestratorError;
use super::super::index_state_tracker::UpdateStateTracker;
use super::export_spool::ExportSpool;
use super::file_indexer::FileIndexer;
use super::file_processor::FileProcessor;
use super::options::IndexOptions;
use super::relation_build_spool::RelationBuildSpool;
use super::relation_publisher::RelationSnapshotPublisher;
use super::result::{IndexExecutionOutcome, IndexResult};
use super::storage_coordinator::StorageCoordinator;

/// Index orchestrator
///
/// Coordinates the complete indexing workflow across multiple modules
/// using batch processing for memory efficiency.
pub struct IndexOrchestrator {
    file_processor: FileProcessor,
    storage: StorageCoordinator,
    project_id: i64,
    relation_builder: Option<IndexBuilder>,
    project_fingerprint: Option<String>,
    summary_generator: Box<dyn SummaryGenerator>,
    /// Stored summary config (for deferred ModelEnhancedGenerator creation)
    summary_config: Option<SummaryConfig>,
    /// LLM client for model-enhanced generation
    llm_client: Option<Arc<HttpLlmClient>>,
    /// Chat completion configuration
    chat_config: Option<ChatConfig>,
    state_tracker: UpdateStateTracker,
    batch_config: BatchConfig,
    /// NL document exporter (optional, for export during full index)
    nl_exporter: Option<Arc<NlDocumentExporter>>,
    /// Progress tracker for indexing operations
    progress: Arc<ProgressTracker>,
    /// Progress callback (optional)
    /// Plugin registry for NL template generation
    plugin_registry: Option<Arc<PluginRegistry>>,
    /// Plugin registry shared with the scanner for the `FileFilter` capability.
    scanner_plugin_registry: Option<Arc<PluginRegistry>>,
    /// Project-specific policy for relation graph construction and querying.
    relation_config: RelationConfig,
    /// Global metrics registry for pipeline-level metrics
    metrics_registry: Option<Arc<MetricsRegistry>>,
    /// Scanner metrics for file scanning operations
    scanner_metrics: Option<Arc<ScannerMetrics>>,
    /// Search/index metrics collector
    search_metrics: Option<Arc<SearchMetrics>>,
    /// Checkpoint manager for operation progress persistence
    checkpoint_manager: Option<Arc<CheckpointManager>>,
    /// Publisher for final, complete relationship snapshots.
    relation_publisher: Option<Arc<dyn RelationSnapshotPublisher>>,
    /// Cached build-config parser from `init_relation_builder`; reused in
    /// `build_and_publish_relations` to avoid a second filesystem scan.
    cached_build_config: Option<cce_relation::BuildConfigParser>,
}

impl IndexOrchestrator {
    /// Create a new index orchestrator with required project ID
    pub fn new(project_id: i64) -> Result<Self, cce_types::error::ConfigError> {
        if project_id <= 0 {
            return Err(cce_types::error::ConfigError::invalid_project_id(
                project_id,
            ));
        }
        Ok(Self {
            file_processor: FileProcessor::default().with_project_id(project_id),
            storage: StorageCoordinator::new(project_id)?,
            project_id,
            relation_builder: None,
            project_fingerprint: None,
            summary_generator: Box::new(RuleBasedGenerator::default()),
            summary_config: None,
            llm_client: None,
            chat_config: None,
            state_tracker: UpdateStateTracker::new(project_id),
            batch_config: BatchConfig::default(),
            nl_exporter: None,
            progress: Arc::new(ProgressTracker::new(0)),
            plugin_registry: None,
            scanner_plugin_registry: None,
            relation_config: RelationConfig::default(),
            metrics_registry: None,
            scanner_metrics: None,
            search_metrics: None,
            checkpoint_manager: None,
            relation_publisher: None,
            cached_build_config: None,
        })
    }

    /// Create with custom batch configuration
    pub fn with_batch_config(
        project_id: i64,
        config: BatchConfig,
    ) -> Result<Self, cce_types::error::ConfigError> {
        if project_id <= 0 {
            return Err(cce_types::error::ConfigError::invalid_project_id(
                project_id,
            ));
        }
        let mut orchestrator = Self::new(project_id)?;
        orchestrator.batch_config = config;
        Ok(orchestrator)
    }

    /// Set shared progress tracker for lock-free metrics access
    pub fn with_progress_tracker(mut self, tracker: Arc<ProgressTracker>) -> Self {
        self.progress = tracker;
        self
    }

    /// Set checkpoint manager for operation progress persistence
    pub fn with_checkpoint_manager(mut self, checkpoint_manager: Arc<CheckpointManager>) -> Self {
        self.state_tracker
            .set_database(checkpoint_manager.database());
        self.checkpoint_manager = Some(checkpoint_manager);
        self
    }

    /// Set the sole publisher for complete relationship snapshots.
    pub fn with_relation_publisher(
        mut self,
        publisher: Arc<dyn RelationSnapshotPublisher>,
    ) -> Self {
        self.relation_publisher = Some(publisher);
        self
    }
}

impl IndexOrchestrator {
    /// Set embedder
    pub fn with_embedder(mut self, embedder: Arc<dyn cce_llm::Embedder>) -> Self {
        self.storage = self.storage.with_embedder(embedder);
        self
    }

    /// Set BM25 client
    pub fn with_bm25_client(
        mut self,
        client: Arc<tokio::sync::Mutex<cce_storage_bm25::Bm25Client>>,
    ) -> Self {
        self.storage = self.storage.with_bm25(client);
        self
    }

    /// Set Qdrant client
    pub fn with_qdrant_client(mut self, client: Arc<cce_storage_qdrant::QdrantClient>) -> Self {
        self.storage = self.storage.with_qdrant(client);
        self
    }

    /// Set metadata store (SQLite)
    pub fn with_metadata_store(mut self, store: Arc<cce_storage_sqlite::SqliteClient>) -> Self {
        self.storage = self.storage.with_metadata_store(store);
        self
    }

    /// Set pre-processor configuration
    pub fn with_pre_processor_config(mut self, config: NestProcessorConfig) -> Self {
        self.file_processor =
            FileProcessor::with_pre_processor_config(config).with_project_id(self.project_id);
        self
    }

    /// Set pre-processor and AST to NL configurations
    pub fn with_file_processor_configs(
        mut self,
        pre_config: NestProcessorConfig,
        ast_to_nl_config: &AstToNlConfig,
    ) -> Self {
        self.file_processor = FileProcessor::with_configs(pre_config, ast_to_nl_config)
            .with_project_id(self.project_id);
        self
    }

    /// Set the chunk cache capacity for the file processor.
    ///
    /// Overrides the default 100-entry LRU limit.  Must be called after
    /// `with_file_processor_configs` to take effect on the constructed
    /// `FileProcessor`.
    pub fn with_chunk_cache_size(mut self, cache_size: usize) -> Self {
        self.file_processor = self
            .file_processor
            .clone()
            .with_chunk_cache_size(cache_size);
        self
    }

    /// Set summary generator configuration
    pub fn with_summary_config(mut self, config: SummaryConfig) -> Self {
        self.summary_config = Some(config.clone());
        self.summary_generator = self.build_generator(config);
        self
    }

    /// Set LLM client for model-enhanced summary generation
    ///
    /// When combined with `with_summary_config()` using `Auto` or `ModelEnhanced`,
    /// the orchestrator uses `ModelEnhancedGenerator` for per-file decisions.
    pub fn with_llm_client(
        mut self,
        llm_client: Arc<HttpLlmClient>,
        chat_config: ChatConfig,
    ) -> Self {
        self.llm_client = Some(llm_client);
        self.chat_config = Some(chat_config);
        // Rebuild strategies that can use the newly available model client.
        if let Some(config) = self.summary_config.clone() {
            if matches!(
                config.strategy,
                SummaryStrategy::Auto | SummaryStrategy::ModelEnhanced
            ) {
                self.summary_generator = self.build_generator(config);
            }
        }
        self
    }

    /// Build the appropriate summary generator based on config and available resources
    fn build_generator(&self, config: SummaryConfig) -> Box<dyn SummaryGenerator> {
        let summary_metrics = self
            .metrics_registry
            .as_ref()
            .map(|registry| SummaryMetrics::new(registry, self.project_id));

        if matches!(
            config.strategy,
            SummaryStrategy::Auto | SummaryStrategy::ModelEnhanced
        ) {
            if let (Some(client), Some(chat_cfg)) = (&self.llm_client, &self.chat_config) {
                let mut generator =
                    ModelEnhancedGenerator::with_config(client.clone(), chat_cfg.clone(), config);

                if let Some(metrics) = summary_metrics.as_ref().cloned() {
                    generator = generator.with_metrics(metrics);
                }

                return Box::new(generator);
            }
        }
        let mut generator = RuleBasedGenerator::with_config(config);
        if let Some(metrics) = summary_metrics {
            generator = generator.with_metrics(metrics);
        }
        Box::new(generator)
    }

    /// Apply project relation graph construction policies.
    pub fn with_relation_config(mut self, config: RelationConfig) -> Self {
        self.relation_config = config;
        self
    }

    /// Set global metrics registry and inject metrics into all components
    pub fn with_metrics_registry(mut self, registry: Arc<MetricsRegistry>) -> Self {
        // Inject parser metrics into the file processor
        let parser_metrics = ParserMetrics::new(&registry, self.project_id);
        self.file_processor = self.file_processor.with_parser_metrics(parser_metrics);

        // Inject pipeline stage metrics for grouper/converter/chunker
        self.file_processor = self
            .file_processor
            .with_grouper_metrics(PipelineStageMetrics::new(
                &registry,
                PipelineStage::Grouper,
                self.project_id,
            ))
            .with_converter_metrics(PipelineStageMetrics::new(
                &registry,
                PipelineStage::Converter,
                self.project_id,
            ))
            .with_chunker_metrics(PipelineStageMetrics::new(
                &registry,
                PipelineStage::Chunker,
                self.project_id,
            ));

        // Inject scanner metrics for file scanning operations
        self.scanner_metrics = Some(ScannerMetrics::new(&registry, self.project_id));

        // Inject search metrics for index size and document counters
        self.search_metrics = Some(SearchMetrics::new(&registry, self.project_id));

        // Inject file-level end-to-end processing metrics
        let file_processing_metrics = FileProcessingMetrics::new(&registry, self.project_id);
        self.file_processor = self
            .file_processor
            .with_file_processing_metrics(file_processing_metrics);

        self.metrics_registry = Some(registry);
        if let Some(config) = self.summary_config.clone() {
            self.summary_generator = self.build_generator(config);
        }
        self
    }

    /// Set Qdrant client
    pub fn with_qdrant(mut self, client: Arc<cce_storage_qdrant::QdrantClient>) -> Self {
        self.storage = self.storage.with_qdrant(client);
        self
    }

    /// Set BM25 client
    pub fn with_bm25(
        mut self,
        client: Arc<tokio::sync::Mutex<cce_storage_bm25::Bm25Client>>,
    ) -> Self {
        self.storage = self.storage.with_bm25(client);
        self
    }

    /// Set NL document exporter for export during full index
    pub fn with_nl_exporter(mut self, exporter: Arc<NlDocumentExporter>) -> Self {
        self.nl_exporter = Some(exporter);
        self
    }

    /// Set project fingerprint for cache isolation
    pub fn with_project_fingerprint(mut self, fingerprint: String) -> Self {
        self.project_fingerprint = Some(fingerprint.clone());
        self.storage = self.storage.with_project_group_id(fingerprint);
        self
    }

    /// Set plugin registry for NL template generation
    pub fn with_plugin_registry(mut self, plugin_registry: Arc<PluginRegistry>) -> Self {
        self.plugin_registry = Some(plugin_registry.clone());
        self.scanner_plugin_registry = Some(plugin_registry.clone());
        // Pass to file processor
        self.file_processor = self.file_processor.with_plugin_registry(plugin_registry);
        self
    }
}

/// Mutable state shared across the phases of one full-index run.
///
/// Passed through the checkpoint, batch, and finalize phases so each stage
/// accumulates results without the orchestrator holding a single giant method.
struct FullIndexContext {
    operation_id: String,
    file_indexer: FileIndexer,
    start_batch: usize,
    total_files: usize,
    total_batches: usize,
    relation_spool: Option<RelationBuildSpool>,
    export_spool: Option<ExportSpool>,
    export_summaries_by_file: HashMap<String, FileSummary>,
    errors: Vec<String>,
    total_indexed: usize,
    total_failed: usize,
    total_entities: usize,
    total_vectors: usize,
    all_batches_completed: bool,
    published_relation_epoch: Option<i64>,
}

impl IndexOrchestrator {
    /// Execute indexing with batch processing
    ///
    /// This method uses a batch processing model:
    /// 1. Scan all files first
    /// 2. Split files into batches (controlled by scan_batch_size)
    /// 3. Process each batch and store results immediately
    /// 4. Release memory before next batch
    pub async fn execute(
        &mut self,
        options: IndexOptions,
    ) -> Result<IndexResult, OrchestratorError> {
        // Initialize relation builder if needed (offloaded to blocking pool)
        if options.build_relations {
            self.init_relation_builder_async(&options).await?;
        }

        let mut errors = Vec::new();

        // Ensure storage backends are ready before processing
        if let Err(e) = self.storage.initialize_qdrant().await {
            tracing::warn!(error = %e, "Failed to initialize Qdrant collections, vector storage may be unavailable");
            errors.push(format!("Qdrant initialization failed: {}", e));
        }
        let target_epoch = self.storage.begin_full_index()?;
        tracing::info!(
            project_id = self.project_id,
            target_epoch,
            "Started project index epoch"
        );

        // ===== CHECKPOINT RECOVERY (before creating new operation) =====
        // Check if we can resume from a previous checkpoint FIRST,
        // before creating a new operation/checkpoint.
        let scan_options = self.build_scan_options(&options);
        let batch_size = self.batch_config.scan_batch_size;

        let (file_indexer, recovered_start_batch, operation_id) = self
            .recover_file_indexer(&options.root_dir, batch_size, &scan_options)
            .await?;
        let mut start_batch = recovered_start_batch;

        // Pass checkpoint context to storage coordinator for work-unit-level tracking
        self.storage
            .set_checkpoint_context(self.checkpoint_manager.clone(), Some(operation_id.clone()));
        self.storage.begin_project_manifest(&operation_id)?;

        // Create file rows for the candidate epoch before summaries, entities
        // and chunks reference them. The change-detector hashes remain
        // unpublished until the project manifest is activated.
        if let Err(error) = self.storage.ensure_file_records(file_indexer.files()) {
            let _ = self.storage.fail_project_manifest(
                &operation_id,
                &format!("failed to publish file hashes: {error}"),
            );
            return Err(error);
        }

        // Validate content hash for all completed batches.
        // When content hash mismatches, move the recovery boundary backwards
        // so those batches get re-processed.
        start_batch = self
            .validate_recovered_hashes(&file_indexer, &operation_id, &options, start_batch)
            .await?;

        let total_files = file_indexer.files().len();
        let total_batches = file_indexer.total_batches();

        tracing::info!(
            "Found {} files to index, processing in batches of {} (file_list_hash: {})",
            total_files,
            batch_size,
            file_indexer.file_list_hash()
        );

        if start_batch > 0 {
            tracing::info!(
                "Recovering from checkpoint: resuming from batch {}/{}",
                start_batch,
                total_batches
            );
        }

        let mut relation_spool = if options.build_relations {
            let builder = self.relation_builder.as_ref().ok_or_else(|| {
                OrchestratorError::index(
                    "relation_build",
                    "relation builder was not initialized for a relation-enabled index",
                )
            })?;
            let symbols = builder.create_project_symbol_table(&options.root_dir);
            Some(
                RelationBuildSpool::new(self.project_id, &options.root_dir, symbols).map_err(
                    |error| {
                        OrchestratorError::index(
                            "relation_build_spool",
                            format!("Failed to create relation build spool: {error}"),
                        )
                    },
                )?,
            )
        } else {
            None
        };

        let export_spool = if self.nl_exporter.is_some() {
            Some(
                ExportSpool::new(self.project_id, &options.root_dir).map_err(|error| {
                    OrchestratorError::index(
                        "export_spool",
                        format!("Failed to create export spool: {error}"),
                    )
                })?,
            )
        } else {
            None
        };

        // Rehydrate only one recovered parsed file at a time. The completed
        // portion of a resumed operation must take part in the final graph,
        // but must not recreate the old all-files in-memory cache.
        if start_batch > 0 && options.build_relations {
            self.replay_recovered_relations(
                &file_indexer,
                &operation_id,
                start_batch,
                relation_spool.as_mut(),
            )
            .await?;
        }

        let _operation_id = self
            .state_tracker
            .start_full_index_for_operation(
                operation_id.clone(),
                total_files,
                batch_size,
                options.root_dir.to_string_lossy().to_string(),
            )
            .await;
        if recovered_start_batch > 0 {
            self.state_tracker.restore_operation(&operation_id).await;
        }

        // Determine output mode based on configured storage backends
        let output_mode = self.determine_output_mode(&options)?;
        tracing::info!("Using output mode: {:?}", output_mode);

        let mut ctx = FullIndexContext {
            operation_id: operation_id.clone(),
            file_indexer,
            start_batch,
            total_files,
            total_batches,
            relation_spool,
            export_spool,
            export_summaries_by_file: HashMap::new(),
            errors,
            total_indexed: 0,
            total_failed: 0,
            total_entities: 0,
            total_vectors: 0,
            all_batches_completed: true,
            published_relation_epoch: None,
        };

        // On resume, accumulate the chunks of batches completed in the previous
        // run so their documents are exported once at the end (they were never
        // exported because export now runs after relation finalize).
        if start_batch > 0 && self.nl_exporter.is_some() {
            self.accumulate_recovered_export(&mut ctx).await?;
        }

        // Process files in batches using FileIndexer for deterministic batch boundaries
        self.run_batch_loop(&mut ctx, &options, output_mode).await?;

        // Rebuild against one complete symbol snapshot. Parsed relation inputs
        // are replayed from the operation-local spool so peak memory remains
        // bounded by one parsed file plus the final relation index.
        if ctx.all_batches_completed && options.build_relations {
            self.build_and_publish_relations(&mut ctx).await?;
        }

        // Finalize relation index and wire up export enhancements in the correct order.
        // This guarantees relation data is fully resolved before derived consumers use it.
        let total_relations = self.finalize_index();

        // Export NL documents after the relation index is finalized so
        // relation enhancement is active. Only runs when every batch succeeded
        // (the accumulated chunks reflect complete data).
        if ctx.all_batches_completed
            && ctx
                .export_spool
                .as_ref()
                .is_some_and(|spool| !spool.is_empty())
        {
            self.export_nl_documents(&mut ctx).await?;
        }

        self.finalize_manifest(&ctx).await?;

        if let Some(ref search_metrics) = self.search_metrics {
            search_metrics.record_index(ctx.total_vectors);

            if let Some(qdrant) = self.storage.qdrant() {
                match qdrant.get_collection_info().await {
                    Ok(info) => {
                        search_metrics.update_index_size(info.points_count as usize);
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to refresh search index size");
                    }
                }
            }
        }

        // Create result with accumulated values
        let outcome = if ctx.all_batches_completed {
            IndexExecutionOutcome::Success
        } else {
            IndexExecutionOutcome::Incomplete {
                errors: ctx.errors.clone(),
            }
        };
        let result = IndexResult {
            total_files,
            indexed_files: ctx.total_indexed,
            failed_files: ctx.total_failed,
            total_entities: ctx.total_entities,
            total_relations,
            total_vectors: ctx.total_vectors,
            total_tokens: 0,
            outcome,
            elapsed_ms: 0,
        };

        tracing::info!(
            "Indexing complete: {} files, {} entities, {} relations, {} vectors",
            result.indexed_files,
            result.total_entities,
            result.total_relations,
            result.total_vectors
        );

        // Mark operation checkpoint as completed after successful full index
        if ctx.all_batches_completed {
            if let Some(cm) = self.checkpoint_manager.as_ref() {
                if let Err(e) = cm.mark_operation_completed(&ctx.operation_id).await {
                    tracing::warn!(
                        error = %e,
                        operation_id = %ctx.operation_id,
                        "Failed to mark operation checkpoint as completed"
                    );
                } else {
                    tracing::info!(
                        operation_id = %ctx.operation_id,
                        "Operation checkpoint marked as completed"
                    );
                }
            }
        } else {
            tracing::warn!(
                operation_id = %ctx.operation_id,
                "Index operation remains resumable because one or more batches failed"
            );
        }

        Ok(result)
    }

    /// Initialize the relation builder based on project relation policies.
    async fn init_relation_builder_async(
        &mut self,
        options: &IndexOptions,
    ) -> Result<(), OrchestratorError> {
        let mut parser = cce_relation::BuildConfigParser::new();
        let root = options.root_dir.clone();
        let depth = self.relation_config.manifest_scan_depth;
        if let Err(error) = parser.scan_project_async(root, depth).await {
            if let Some(ref registry) = self.metrics_registry {
                let relation_metrics = RelationMetrics::new(registry, self.project_id);
                relation_metrics.config_scan_failures_total.increment();
            }
            tracing::error!(
                error = %error,
                root = %options.root_dir.display(),
                "Failed to scan build configurations during relation initialization"
            );
            return Err(OrchestratorError::index(
                "relation_build_config",
                format!("Failed to scan build configs: {error}"),
            ));
        }
        self.finish_init_relation_builder(parser)
    }

    fn finish_init_relation_builder(
        &mut self,
        parser: cce_relation::BuildConfigParser,
    ) -> Result<(), OrchestratorError> {
        let params = self.relation_config.to_builder_params();
        let mut builder = IndexBuilder::new();
        builder.auto_load_dependencies(&parser);
        builder.set_filter_stdlib_calls(params.filter_stdlib_calls);
        let mut builder = if let Some(ref registry) = self.metrics_registry {
            let relation_metrics = RelationMetrics::new(registry, self.project_id);
            builder.with_metrics(relation_metrics)
        } else {
            builder
        };
        builder.set_graph_options(
            params.max_relations_per_file,
            params.analyze_imports,
            params.track_cross_file_deps,
        );
        builder.set_symbol_extract_enabled(params.symbol_extract_enabled);
        // The registry is attached when either plugin-facing capability
        // is enabled; individual capabilities filter by their own guard.
        let use_plugin_registry = params.plugin_symbols_enabled || params.symbol_extract_enabled;
        let builder = if use_plugin_registry {
            if let Some(registry) = &self.plugin_registry {
                builder.with_plugin_registry(Arc::clone(registry))
            } else {
                builder
            }
        } else {
            builder
        };
        self.relation_builder = Some(builder);
        self.cached_build_config = Some(parser);

        // Query-only config fields have no effect on the constructed graph.
        if self.relation_config.max_call_depth != 10
            || self.relation_config.index.resolve_call_chains
        {
            tracing::debug!(
                max_call_depth = self.relation_config.max_call_depth,
                resolve_call_chains = self.relation_config.index.resolve_call_chains,
                "Relation query-only config fields have no effect during full index construction"
            );
        }
        Ok(())
    }

    /// Determine the output mode based on configured storage backends.
    ///
    /// At least one storage backend (Qdrant or BM25) must be configured, or
    /// the NL exporter must be active (which implies Embedding text generation).
    /// Returns an error when no data path exists.
    fn determine_output_mode(
        &self,
        options: &IndexOptions,
    ) -> Result<OutputMode, OrchestratorError> {
        // Use requested mode (from options) rather than available mode (from storage clients).
        // If storage is requested but no client, warn and degrade gracefully.
        let has_qdrant = self.storage.has_qdrant();
        let has_bm25 = self.storage.has_bm25();

        let wants_vectors = options.store_vectors;
        let wants_bm25 = options.store_bm25;

        if wants_vectors && !has_qdrant {
            tracing::warn!(
                "store_vectors is true but no Qdrant client configured. \
                 Embeddings will be generated but not stored."
            );
        }
        if wants_bm25 && !has_bm25 {
            tracing::warn!(
                "store_bm25 is true but no BM25 client configured. \
                 BM25 indexes will not be built."
            );
        }

        // When NL exporter is configured, we must generate Embedding text
        // (BM25 text is keyword-optimized and unsuitable for export descriptions)
        if self.nl_exporter.is_some() {
            return Ok(if has_bm25 && wants_bm25 {
                OutputMode::Both
            } else {
                OutputMode::Embedding
            });
        }

        // Decide output mode based on what's requested and what's available.
        // Embedding mode is always available (just generate embeddings).
        match (has_qdrant && wants_vectors, has_bm25 && wants_bm25) {
            (true, true) => Ok(OutputMode::Both),
            (true, false) => Ok(OutputMode::Embedding),
            (false, true) => Ok(OutputMode::Bm25),
            (false, false) => {
                // No storage configured: generate embeddings only.
                // This is valid for benchmark data generation, testing without storage, etc.
                tracing::debug!(
                    "No storage backend configured. Generating embeddings only (no storage)."
                );
                Ok(OutputMode::Embedding)
            }
        }
    }

    /// Build scan options from index options
    fn build_scan_options(&self, options: &IndexOptions) -> ScanOptions {
        let include_patterns: Vec<String> = options
            .extensions
            .iter()
            .map(|ext| format!("*.{}", ext))
            .collect();

        ScanOptions {
            root_path: options.root_dir.to_string_lossy().to_string(),
            include_patterns,
            exclude_patterns: options.exclude_dirs.clone(),
            follow_symlinks: false,
            respect_gitignore: options.respect_gitignore,
            gitignore_patterns: options.additional_ignore_patterns.clone(),
            gitignore_path: options.custom_gitignore_path.clone(),
            max_content_size: None,
            max_file_size: None,
        }
    }

    /// Get the state tracker for querying index state
    pub fn state_tracker(&self) -> &UpdateStateTracker {
        &self.state_tracker
    }

    /// Get current index state report
    pub async fn get_state_report(&self) -> crate::index_state::IndexStateReport {
        self.state_tracker.get_report().await
    }

    /// Check if indexing operation is complete
    pub async fn is_complete(&self) -> bool {
        self.state_tracker.all_complete().await
    }

    /// Get files that can be resumed (for resumable indexing)
    pub async fn get_resumable_files(&self) -> Vec<std::path::PathBuf> {
        self.state_tracker
            .get_resumable_files()
            .await
            .into_iter()
            .map(|s| std::path::PathBuf::from(s.file_path))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_orchestrator_creation() {
        let _orchestrator = IndexOrchestrator::new(1).expect("failed to create IndexOrchestrator");
    }

    #[test]
    fn test_batch_config_presets() {
        let config = BatchConfig::small_project();
        assert!(config.scan_batch_size < 100);

        let config = BatchConfig::large_project();
        assert!(config.scan_batch_size > 100);
    }
}
