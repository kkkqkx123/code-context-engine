//! Update processor factory
//!
//! Provides a unified way to create all update processors with correct dependencies
//! and guaranteed execution order.

use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::Mutex;

use crate::NlDocumentUpdateProcessor;
use crate::export::ExportConfig;
use crate::export::NlDocumentExporter;
use crate::index::RelationSnapshotPublisher;
use crate::index::StorageCoordinator;
use cce_config::{NestProcessorConfig, RelationConfig};
use cce_llm::Embedder;
use cce_metrics::RelationMetrics;
use cce_parser::summary::{RuleBasedGenerator, SummaryGenerator};
use cce_plugin::PluginRegistry;
use cce_storage_bm25::Bm25Client;
use cce_storage_qdrant::QdrantClient;
use cce_storage_sqlite::SqliteClient;
use cce_types::error::ConfigError;

use super::{
    BoxedUpdateProcessor, EmbeddingUpdateProcessor, ProcessorContext, RelationUpdateProcessor,
    SummaryUpdateProcessor,
};

/// Processor creation configuration
///
/// Controls which processors are enabled when creating the processor collection.
#[derive(Debug, Clone)]
pub struct ProcessorConfig {
    /// Enable embedding update processor
    pub enable_embedding: bool,
    /// Enable BM25 update processor
    pub enable_bm25: bool,
    /// Enable relation update processor
    pub enable_relation: bool,
    /// Enable summary update processor
    pub enable_summary: bool,
    /// Enable NL document export processor
    pub enable_export: bool,
    /// Export configuration (optional)
    pub export_config: Option<ExportConfig>,
}

impl Default for ProcessorConfig {
    fn default() -> Self {
        Self {
            enable_embedding: true,
            enable_bm25: true,
            enable_relation: true,
            enable_summary: true,
            enable_export: true,
            export_config: None,
        }
    }
}

impl ProcessorConfig {
    /// Create a new processor config with all processors enabled
    pub fn new() -> Self {
        Self::default()
    }

    /// Disable embedding processor
    pub fn without_embedding(mut self) -> Self {
        self.enable_embedding = false;
        self
    }

    /// Disable BM25 processor
    pub fn without_bm25(mut self) -> Self {
        self.enable_bm25 = false;
        self
    }

    /// Disable relation processor
    pub fn without_relation(mut self) -> Self {
        self.enable_relation = false;
        self
    }

    /// Disable summary processor
    pub fn without_summary(mut self) -> Self {
        self.enable_summary = false;
        self
    }

    /// Disable export processor
    pub fn without_export(mut self) -> Self {
        self.enable_export = false;
        self
    }

    /// Set export configuration
    pub fn with_export_config(mut self, config: ExportConfig) -> Self {
        self.export_config = Some(config);
        self
    }
}

/// Create index-phase processors (embedding, bm25, relation)
///
/// These processors update index data and can run in sequence within the same phase.
/// They must complete before derived-phase processors run.
pub fn create_index_processors(
    context: Arc<ProcessorContext>,
    config: &ProcessorConfig,
) -> Vec<BoxedUpdateProcessor> {
    let mut processors: Vec<BoxedUpdateProcessor> = Vec::new();

    // Order matters: embedding → bm25 → relation
    if config.enable_embedding {
        processors.push(Box::new(EmbeddingUpdateProcessor::new(context.clone())));
    }

    if config.enable_bm25 {
        processors.push(Box::new(super::Bm25UpdateProcessor::new(context.clone())));
    }

    // RelationUpdateProcessor is added by create_all_processors when enabled.

    processors
}

/// Create the NL document export processor (derived phase)
///
/// This processor depends on relation and summary data being available,
/// so it must run after the index phase processors. The optional
/// `ast_to_nl_config` keeps the hot-update rendering pipeline consistent
/// with the full-index path; when absent, a default pipeline is used.
/// `grouper_config` drives both the render-input fingerprint and the grouper
/// that derives entity groups from parsed files at export time.
pub fn create_export_processor(
    exporter: Arc<NlDocumentExporter>,
    ast_to_nl_config: Option<&cce_config::AstToNlConfig>,
    grouper_config: &NestProcessorConfig,
    summary_config: Option<&cce_config::SummaryConfig>,
) -> NlDocumentUpdateProcessor {
    let grouper_fingerprint = crate::export::fingerprint::config_fingerprint(grouper_config);
    let summary_fingerprint = summary_config
        .map(crate::export::fingerprint::config_fingerprint)
        .unwrap_or_default();
    let processor = match ast_to_nl_config {
        Some(config) => NlDocumentUpdateProcessor::from_config(exporter, config),
        None => NlDocumentUpdateProcessor::new(exporter),
    };
    processor
        .with_grouper_config(grouper_config.clone())
        .with_render_inputs(grouper_fingerprint, summary_fingerprint)
}

/// Processor phase classification for ordered execution
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessorPhase {
    /// Index update phase: embedding, bm25, relation
    Index,
    /// Derived data phase: summary, nl_document
    Derived,
}

/// Classify a processor by its phase based on name
pub fn classify_processor(name: &str) -> ProcessorPhase {
    match name {
        "embedding" | "bm25" | "relation" => ProcessorPhase::Index,
        "summary" | "nl_document" => ProcessorPhase::Derived,
        _ => ProcessorPhase::Index, // Unknown processors default to index phase
    }
}

/// Processor factory for creating processors from application state
pub struct ProcessorFactory;

impl Default for ProcessorFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessorFactory {
    /// Create a new processor factory
    pub fn new() -> Self {
        Self
    }

    /// Create all enabled processors from individual components
    ///
    /// This method constructs the necessary context and creates all processors
    /// based on the configuration.
    ///
    /// Returns the processors together with the shared storage coordinator
    /// they write through, so the caller can reuse the same coordinator for
    /// read-only queries (e.g. candidate adoptability checks on resume).
    ///
    /// # Arguments
    ///
    /// * `config` - Processor configuration controlling which processors to create.
    ///   Use `ProcessorConfig::new().without_export()` to skip the export processor.
    ///     - the in-memory fallback metadata store could not be created, or
    ///       the project ID is invalid (both surfaced as `ConfigError`).
    #[allow(clippy::too_many_arguments)]
    pub fn create_all_processors(
        &self,
        qdrant: Option<Arc<QdrantClient>>,
        bm25: Option<Arc<Mutex<Bm25Client>>>,
        metadata_store: Option<Arc<SqliteClient>>,
        embedder: Option<Arc<dyn Embedder>>,
        project_group_id: Option<String>,
        project_id: i64,
        relation_publisher: Option<Arc<dyn RelationSnapshotPublisher>>,
        relation_config: &RelationConfig,
        checkpoint_manager: Option<Arc<crate::operation::CheckpointManager>>,
        summary_generator: Option<Arc<dyn SummaryGenerator>>,
        ast_to_nl_config: Option<&cce_config::AstToNlConfig>,
        grouper_config: &NestProcessorConfig,
        summary_config: Option<&cce_config::SummaryConfig>,
        config: &ProcessorConfig,
        plugin_registry: Option<Arc<PluginRegistry>>,
        relation_metrics: Option<Arc<RelationMetrics>>,
        storage_metrics: Option<Arc<cce_metrics::HotUpdateStorageMetrics>>,
    ) -> Result<(Vec<BoxedUpdateProcessor>, Arc<StorageCoordinator>), ConfigError> {
        let config = config.clone();

        // Processors are independent: missing vector or BM25 infrastructure
        // must not disable SQLite-backed relation publication.
        let enable_embedding = config.enable_embedding && qdrant.is_some() && embedder.is_some();
        if config.enable_embedding && !enable_embedding {
            tracing::warn!(
                "Embedding processor disabled because Qdrant or embedder is unavailable"
            );
        }
        let enable_bm25 = config.enable_bm25 && bm25.is_some();
        if config.enable_bm25 && !enable_bm25 {
            tracing::warn!("BM25 processor disabled because BM25 is unavailable");
        }

        let metadata_store = match metadata_store {
            Some(m) => m,
            None => {
                tracing::warn!(
                    "Metadata store not configured; relation data will use a temporary in-memory \
                     database and will NOT persist across restarts"
                );
                // Create a temporary in-memory database for processors that need it
                Arc::new(SqliteClient::with_path(":memory:").map_err(|error| {
                    ConfigError::Other(format!(
                        "Failed to create in-memory SQLite database: {error}"
                    ))
                })?)
            }
        };
        let relation_sqlite = Some(metadata_store.as_ref().clone());

        // Create storage coordinator using builder pattern. These seed the
        // coordinator's write epoch, but the durable project manifest is
        // re-read by begin_hot_update_candidate before any writes, so they are
        // best-effort initial values only. A missing meta row is the legitimate
        // default 0; real DB failures are surfaced as warnings instead of being
        // silently downgraded.
        let client = metadata_store.as_ref();
        let read_seed = |key: &str| match client.project_meta_get_int_optional(project_id, key) {
            Ok(value) => value.unwrap_or(0),
            Err(error) => {
                tracing::warn!(
                    project_id,
                    key,
                    error = %error,
                    "Failed to read project_meta seed for hot-update storage coordinator"
                );
                0
            }
        };
        let (active_epoch, active_batch) = (read_seed("active_epoch"), read_seed("batch_id"));
        let mut storage_coordinator = StorageCoordinator::new(project_id)?
            .with_metadata_store(metadata_store.clone())
            .with_epoch(active_epoch)
            .with_batch_id(active_batch);

        if let Some(group_id) = project_group_id {
            storage_coordinator = storage_coordinator.with_project_group_id(group_id);
        }

        if let Some(qdrant) = qdrant {
            storage_coordinator = storage_coordinator.with_qdrant(qdrant);
        }

        if let Some(bm25) = bm25 {
            storage_coordinator = storage_coordinator.with_bm25(bm25);
        }

        if let Some(emb) = embedder {
            storage_coordinator = storage_coordinator.with_embedder(emb);
        }

        let storage_coordinator = Arc::new(storage_coordinator);
        let shared_storage = storage_coordinator.clone();

        // Create processor context
        let pre_processor_config = NestProcessorConfig::default();
        let mut context = ProcessorContext::new_with_project(
            storage_coordinator.clone(),
            pre_processor_config,
            project_id,
        );
        if let Some(cm) = &checkpoint_manager {
            context = context.with_checkpoint_manager(cm.clone());
        }
        if let Some(metrics) = storage_metrics {
            context = context.with_storage_metrics(metrics);
        }
        let context = Arc::new(context);

        // Create index-phase processors
        let mut index_config = config.clone();
        index_config.enable_embedding = enable_embedding;
        index_config.enable_bm25 = enable_bm25;
        let mut processors = create_index_processors(context, &index_config);

        if config.enable_summary {
            // Use the caller-provided generator (built from the project summary
            // config, matching the full-index path) when available; otherwise
            // fall back to a default rule-based generator.
            let generator: Arc<dyn SummaryGenerator> = match summary_generator {
                Some(generator) => generator,
                None => Arc::new(RuleBasedGenerator::default()),
            };
            let summary_fingerprint = summary_config
                .map(crate::export::fingerprint::config_fingerprint)
                .unwrap_or_default();
            let mut summary_processor =
                SummaryUpdateProcessor::new(shared_storage.clone(), generator)
                    .with_summary_fingerprint(summary_fingerprint);
            if let Some(cm) = &checkpoint_manager {
                summary_processor = summary_processor.with_checkpoint_manager(cm.clone());
            }
            processors.push(Box::new(summary_processor));
        }

        // Add relation processor if enabled
        if config.enable_relation {
            let mut relation_processor = if let Some(ref sqlite) = relation_sqlite {
                RelationUpdateProcessor::with_persistence(Arc::new(sqlite.clone()))
            } else {
                RelationUpdateProcessor::new()
            };
            relation_processor.set_project_id(project_id);
            relation_processor.set_relation_config(relation_config);
            if let Some(publisher) = relation_publisher {
                relation_processor = relation_processor.with_publisher(publisher);
            } else {
                relation_processor.set_safe_mode(true);
                tracing::warn!(
                    "Relation processor disabled until a unified snapshot publisher is configured"
                );
            }
            if let Some(registry) = plugin_registry {
                relation_processor = relation_processor.with_plugin_registry(registry);
            }
            if let Some(metrics) = relation_metrics {
                relation_processor = relation_processor.with_relation_metrics(metrics);
            }
            relation_processor = relation_processor.with_storage(shared_storage);
            processors.push(Box::new(relation_processor));
            tracing::info!("Relation update processor created and added to pipeline");
        }

        // Add export processor if enabled
        // The configured `enable_relation_enhancement` flag is respected; the
        // processor loads the published relation snapshot from SQLite so the
        // flag takes effect on hot updates too.
        if config.enable_export {
            let export_cfg = config
                .export_config
                .clone()
                .unwrap_or_else(|| ExportConfig::new(PathBuf::from("."), project_id));
            let exporter = Arc::new(NlDocumentExporter::new(export_cfg));
            let mut export_processor =
                create_export_processor(exporter, ast_to_nl_config, grouper_config, summary_config);
            if let Some(cm) = &checkpoint_manager {
                export_processor = export_processor.with_checkpoint_manager(cm.clone());
            }
            export_processor =
                export_processor.with_relation_context(relation_sqlite.map(Arc::new), project_id);
            processors.push(Box::new(export_processor));
        }

        tracing::info!("Created {} update processors", processors.len());
        Ok((processors, storage_coordinator))
    }

    /// Create summary update processor
    ///
    /// This processor handles summary updates during hot updates.
    /// It requires both a storage coordinator and a summary generator.
    ///
    /// # Arguments
    ///
    /// * `storage` - Storage coordinator for summary operations
    /// * `summary_generator` - Summary generator for generating file summaries
    pub fn create_summary_processor(
        storage: Arc<StorageCoordinator>,
        summary_generator: Arc<dyn cce_parser::summary::SummaryGenerator>,
    ) -> BoxedUpdateProcessor {
        Box::new(SummaryUpdateProcessor::new(storage, summary_generator))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_processor_config_default() {
        let config = ProcessorConfig::default();
        assert!(config.enable_embedding);
        assert!(config.enable_bm25);
        assert!(config.enable_relation);
        assert!(config.enable_summary);
        assert!(config.enable_export);
    }

    #[test]
    fn test_processor_config_builder() {
        let config = ProcessorConfig::new().without_embedding().without_export();

        assert!(!config.enable_embedding);
        assert!(config.enable_bm25);
        assert!(!config.enable_export);
    }

    #[test]
    fn test_classify_processor() {
        assert_eq!(classify_processor("embedding"), ProcessorPhase::Index);
        assert_eq!(classify_processor("bm25"), ProcessorPhase::Index);
        assert_eq!(classify_processor("relation"), ProcessorPhase::Index);
        assert_eq!(classify_processor("summary"), ProcessorPhase::Derived);
        assert_eq!(classify_processor("nl_document"), ProcessorPhase::Derived);
    }
}
