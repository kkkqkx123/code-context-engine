use super::*;
use cce_config::{AstToNlConfig, LicenseHeaderConfig, NestProcessorConfig, Settings};
use cce_metrics::{FileProcessingMetrics, ParserMetrics, PipelineStageMetrics};
use cce_parser::ast_to_nl::AstToNlConverter;
use cce_parser::ast_to_nl::chunker::GroupChunker;
use cce_parser::document::PipelineRouter;
use cce_parser::grouper::PreprocessingPipeline;
use cce_parser::parser::ParseCoordinator;
use cce_plugin::PluginRegistry;
use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;

impl FileProcessor {
    /// Create a new file processor with default configuration
    pub fn new() -> Self {
        Self::with_config(&AstToNlConfig::default())
    }

    /// Create a file processor from initialized Settings
    ///
    /// Returns an error if Settings has not been initialized.
    pub fn from_settings() -> Result<Self, cce_types::error::ConfigError> {
        let config = Settings::ast_to_nl()?;
        Ok(Self::with_config(&config).with_license_config(Settings::license_header()?))
    }

    /// Apply the license header filtering configuration to the parser
    /// components and remember it for coordinator rebuilds.
    pub fn with_license_config(mut self, config: LicenseHeaderConfig) -> Self {
        self.license_header = config.clone();
        let mut coordinator = self.coordinator.lock().unwrap_or_else(|poison| {
            tracing::warn!("Parser coordinator mutex was poisoned; recovering guard");
            poison.into_inner()
        });
        coordinator.set_license_config(config);
        drop(coordinator);
        self
    }

    /// Create with custom configuration
    pub fn with_config(config: &AstToNlConfig) -> Self {
        let cache_size = NonZeroUsize::new(Self::DEFAULT_CACHE_SIZE)
            .expect("DEFAULT_CACHE_SIZE must be non-zero");
        Self {
            coordinator: Arc::new(Mutex::new(ParseCoordinator::new())),
            pre_processor: Arc::new(PreprocessingPipeline::new()),
            converter: Arc::new(AstToNlConverter::with_config(config)),
            chunker: Arc::new(GroupChunker::new(config.chunking.clone())),
            doc_pipeline: Arc::new(PipelineRouter::new()),
            chunk_cache: Arc::new(RwLock::new(LruCache::new(cache_size))),
            chunking_config: config.chunking.clone(),
            document_chunking_config: config.document_chunking.clone(),
            pipeline_fingerprint: cce_utils::hash::hash_serializable(&config),
            plugin_registry: None,
            parser_metrics: None,
            grouper_metrics: None,
            converter_metrics: None,
            chunker_metrics: None,
            file_processing_metrics: None,
            project_id: 0,
            license_header: LicenseHeaderConfig::default(),
        }
    }

    /// Create with custom pre-processor configuration
    pub fn with_pre_processor_config(pre_config: NestProcessorConfig) -> Self {
        let config = AstToNlConfig::default();
        let cache_size = NonZeroUsize::new(Self::DEFAULT_CACHE_SIZE)
            .expect("DEFAULT_CACHE_SIZE must be non-zero");
        Self {
            coordinator: Arc::new(Mutex::new(ParseCoordinator::new())),
            pre_processor: Arc::new(PreprocessingPipeline::with_config(pre_config)),
            converter: Arc::new(AstToNlConverter::with_config(&config)),
            chunker: Arc::new(GroupChunker::new(config.chunking.clone())),
            doc_pipeline: Arc::new(PipelineRouter::new()),
            chunk_cache: Arc::new(RwLock::new(LruCache::new(cache_size))),
            pipeline_fingerprint: cce_utils::hash::hash_serializable(&config),
            chunking_config: config.chunking,
            document_chunking_config: config.document_chunking.clone(),
            plugin_registry: None,
            parser_metrics: None,
            grouper_metrics: None,
            converter_metrics: None,
            chunker_metrics: None,
            file_processing_metrics: None,
            project_id: 0,
            license_header: LicenseHeaderConfig::default(),
        }
    }

    /// Create with custom pre-processor and AST to NL configuration
    pub fn with_configs(pre_config: NestProcessorConfig, ast_to_nl_config: &AstToNlConfig) -> Self {
        let cache_size = NonZeroUsize::new(Self::DEFAULT_CACHE_SIZE)
            .expect("DEFAULT_CACHE_SIZE must be non-zero");
        Self {
            coordinator: Arc::new(Mutex::new(ParseCoordinator::new())),
            pre_processor: Arc::new(PreprocessingPipeline::with_config(pre_config)),
            converter: Arc::new(AstToNlConverter::with_config(ast_to_nl_config)),
            chunker: Arc::new(GroupChunker::new(ast_to_nl_config.chunking.clone())),
            doc_pipeline: Arc::new(PipelineRouter::new()),
            chunk_cache: Arc::new(RwLock::new(LruCache::new(cache_size))),
            chunking_config: ast_to_nl_config.chunking.clone(),
            document_chunking_config: ast_to_nl_config.document_chunking.clone(),
            pipeline_fingerprint: cce_utils::hash::hash_serializable(ast_to_nl_config),
            plugin_registry: None,
            parser_metrics: None,
            grouper_metrics: None,
            converter_metrics: None,
            chunker_metrics: None,
            file_processing_metrics: None,
            project_id: 0,
            license_header: LicenseHeaderConfig::default(),
        }
    }

    /// Create with custom cache size
    pub fn with_cache_size(cache_size: usize) -> Self {
        let config = AstToNlConfig::default();
        let cache_size =
            NonZeroUsize::new(cache_size.max(1)).expect("cache_size must be non-zero after max(1)");
        Self {
            coordinator: Arc::new(Mutex::new(ParseCoordinator::new())),
            pre_processor: Arc::new(PreprocessingPipeline::new()),
            converter: Arc::new(AstToNlConverter::with_config(&config)),
            chunker: Arc::new(GroupChunker::new(config.chunking.clone())),
            doc_pipeline: Arc::new(PipelineRouter::new()),
            chunk_cache: Arc::new(RwLock::new(LruCache::new(cache_size))),
            pipeline_fingerprint: cce_utils::hash::hash_serializable(&config),
            chunking_config: config.chunking,
            document_chunking_config: config.document_chunking.clone(),
            plugin_registry: None,
            parser_metrics: None,
            grouper_metrics: None,
            converter_metrics: None,
            chunker_metrics: None,
            file_processing_metrics: None,
            project_id: 0,
            license_header: LicenseHeaderConfig::default(),
        }
    }

    /// Set the project ID for cache key isolation
    pub fn with_project_id(mut self, project_id: i64) -> Self {
        self.project_id = project_id;
        self
    }

    /// Inject a custom document pipeline router.
    ///
    /// This replaces the default global singleton and allows tests or
    /// per-project configurations to use a tailored pipeline.  The `Arc`
    /// wrapping is shared naturally by `FileProcessor: Clone`.
    pub fn with_doc_pipeline(mut self, doc_pipeline: Arc<PipelineRouter>) -> Self {
        self.doc_pipeline = doc_pipeline;
        self
    }

    /// Set the chunk cache capacity.
    ///
    /// Overrides the default 100-entry LRU limit.  A value of 0 is treated
    /// as 1 (minimum viable cache).
    pub fn with_chunk_cache_size(mut self, cache_size: usize) -> Self {
        let nz =
            NonZeroUsize::new(cache_size.max(1)).expect("cache_size must be non-zero after max(1)");
        self.chunk_cache = Arc::new(RwLock::new(LruCache::new(nz)));
        self
    }

    /// Set plugin registry for NL template generation
    pub fn with_plugin_registry(mut self, plugin_registry: Arc<PluginRegistry>) -> Self {
        // Create new components with plugin registry
        let new_coordinator = ParseCoordinator::with_plugin_registry(plugin_registry.clone())
            .with_license_config(self.license_header.clone());
        self.coordinator = Arc::new(Mutex::new(new_coordinator));

        self.pre_processor =
            Arc::new(PreprocessingPipeline::new().with_plugin_registry(plugin_registry.clone()));

        // Get current config from existing converter
        let config = AstToNlConfig::default();
        self.converter = Arc::new(
            AstToNlConverter::with_config(&config).with_plugin_registry(plugin_registry.clone()),
        );

        self.chunker = Arc::new(
            GroupChunker::new(config.chunking.clone())
                .with_plugin_registry(plugin_registry.clone()),
        );

        self.plugin_registry = Some(plugin_registry);

        // Re-inject parser metrics if previously set, since we replaced the coordinator
        if let Some(ref metrics) = self.parser_metrics {
            self.coordinator
                .lock()
                .unwrap_or_else(|poison| {
                    tracing::warn!("Parser coordinator mutex was poisoned; recovering guard");
                    poison.into_inner()
                })
                .set_metrics(metrics.clone());
        }

        self
    }

    /// Inject parser metrics into the coordinator
    pub fn with_parser_metrics(mut self, metrics: Arc<ParserMetrics>) -> Self {
        self.coordinator
            .lock()
            .unwrap_or_else(|poison| {
                tracing::warn!("Parser coordinator mutex was poisoned; recovering guard");
                poison.into_inner()
            })
            .set_metrics(metrics.clone());
        self.parser_metrics = Some(metrics);
        self
    }

    /// Inject pipeline stage metrics for grouper
    pub fn with_grouper_metrics(mut self, metrics: Arc<PipelineStageMetrics>) -> Self {
        self.grouper_metrics = Some(metrics);
        self
    }

    /// Inject pipeline stage metrics for converter
    pub fn with_converter_metrics(mut self, metrics: Arc<PipelineStageMetrics>) -> Self {
        self.converter_metrics = Some(metrics);
        self
    }

    /// Inject pipeline stage metrics for chunker
    pub fn with_chunker_metrics(mut self, metrics: Arc<PipelineStageMetrics>) -> Self {
        self.chunker_metrics = Some(metrics);
        self
    }

    /// Inject file-level end-to-end processing metrics
    pub fn with_file_processing_metrics(mut self, metrics: Arc<FileProcessingMetrics>) -> Self {
        self.file_processing_metrics = Some(metrics);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A custom pipeline injected via `with_doc_pipeline` must be used
    /// instead of the global singleton.
    #[tokio::test]
    async fn with_doc_pipeline_injects_custom_router() {
        let custom_router = Arc::new(PipelineRouter::new());
        let processor = FileProcessor::new().with_doc_pipeline(custom_router.clone());
        assert!(Arc::ptr_eq(&processor.doc_pipeline, &custom_router));
    }
}
