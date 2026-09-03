//! Code Context Engine - High-level facade
//!
//! Provides a unified entry point for all core operations: indexing, searching,
//! and file watching. All interaction modes (CLI, HTTP, embedded library) should
//! use this facade rather than assembling individual components manually.

use std::path::Path;
use std::sync::Arc;

use tokio::sync::Mutex;

use crate::runtime::RelationRuntime;
use cce_config::{AppConfig, Settings};
use cce_llm::Embedder;
use cce_llm_client::OpenAICompatibleProvider;
use cce_metrics_infra::{
    AggregationConfig, LlmRetryMetrics, MetricsAggregator, MetricsRegistry, ProgressTracker,
    RenderCache,
};
use cce_orchestrator::OperationCoordinator;
use cce_orchestrator::hot_update::HotUpdateCoordinator;
use cce_orchestrator::index::IndexOrchestrator;
use cce_orchestrator::query::retry_queue::RetryQueue;
use cce_orchestrator::query::searcher::Searcher;
use cce_plugin::PluginRegistry;
use cce_storage_bm25::Bm25Client;
use cce_storage_qdrant::QdrantClient;
use cce_storage_qdrant::QdrantProcessHandle;
use cce_storage_sqlite::SqliteClient;
use cce_storage_sqlite::project_registry::ProjectRegistry;

/// Error type for engine operations
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Initialization error: {0}")]
    Init(String),

    #[error("Index error: {0}")]
    Index(#[from] cce_orchestrator::OrchestratorError),

    #[error("Query error: {0}")]
    Query(#[from] cce_orchestrator::QueryError),

    #[error("LLM error: {0}")]
    Llm(#[from] cce_llm_client::LlmError),

    #[error("Storage error: {0}")]
    Storage(#[from] cce_storage_qdrant::QdrantError),

    #[error("Recovery error: {0}")]
    Recovery(String),
}

/// Code Context Engine - High-level facade
///
/// Provides indexing, searching, and file watching capabilities through
/// a single unified interface. All interaction modes should use this
/// facade rather than assembling individual components manually.
#[derive(Clone)]
pub struct CodeContextEngine {
    qdrant: Arc<QdrantClient>,
    bm25: Arc<Mutex<Bm25Client>>,
    embedder: Arc<dyn Embedder>,

    /// SQLite metadata store for persistent storage
    metadata_store: Option<Arc<SqliteClient>>,

    /// Project-specific IndexOrchestrator cache (lazy-loaded per project)
    orchestrator_cache: ProjectCache<Mutex<IndexOrchestrator>>,

    /// Project-specific plugin registry cache (lazy-loaded per project)
    plugin_registry_cache: ProjectCache<PluginRegistry>,

    /// Project-specific Searcher cache (lazy-loaded per project)
    searcher_cache: ProjectCache<Mutex<Searcher>>,

    /// Project registry for multi-project support
    project_registry: Arc<ProjectRegistry>,

    /// Project-specific hot update coordinators cache
    hot_update_cache: ProjectCache<Mutex<HotUpdateCoordinator>>,

    /// Project-specific operation coordinators cache
    operation_coordinator_cache: ProjectCache<OperationCoordinator>,

    /// Project-specific relation runtimes cache
    relation_runtime_cache: ProjectCache<RelationRuntime>,

    /// Per-project progress trackers for lock-free metrics access
    progress_tracker: ProjectCache<ProgressTracker>,

    /// Shared handle for controlling Qdrant process lifecycle
    qdrant_control: Option<QdrantProcessHandle>,

    /// Global metrics registry for all business metrics
    metrics_registry: Arc<MetricsRegistry>,

    /// Metrics aggregation engine for historical data persistence
    metrics_aggregator: Option<Arc<MetricsAggregator<SqliteClient>>>,

    /// Tokio runtime metrics collector
    runtime_metrics: Option<Arc<cce_metrics_infra::RuntimeMetrics>>,

    /// System resource metrics collector (CPU, memory, disk)
    system_metrics: Option<Arc<cce_metrics_infra::SystemMetrics>>,

    /// Per-project retry queues for failed queries
    retry_queue: ProjectCache<RetryQueue>,

    /// Single-core metric render cache (Prometheus/JSON served from cache)
    render_cache: Arc<tokio::sync::RwLock<Option<Arc<RenderCache>>>>,
}

impl CodeContextEngine {
    /// Create a new engine builder
    pub fn builder() -> EngineBuilder {
        EngineBuilder::new()
    }

    /// Create engine from an already-initialized AppConfig
    pub async fn from_config(config: AppConfig) -> Result<Self, EngineError> {
        // Initialize global settings if not already done
        if !Settings::is_initialized() {
            Settings::init(config.clone()).map_err(|e| EngineError::Config(e.to_string()))?;
        }

        let engine_config = Settings::global().map_err(|e| EngineError::Config(e.to_string()))?;
        Self::init_components(engine_config.clone()).await
    }

    /// Create engine from a config file path
    pub async fn from_config_file(path: &Path) -> Result<Self, EngineError> {
        Settings::init_from_file(Some(path)).map_err(|e| EngineError::Config(e.to_string()))?;

        let config = Settings::global()
            .map_err(|e| EngineError::Config(e.to_string()))?
            .clone();
        Self::init_components(config).await
    }

    /// Initialize all components from config
    async fn init_components(mut config: AppConfig) -> Result<Self, EngineError> {
        // Resolve configuration dependencies (auto-enable required features)
        config.resolve_dependencies();

        // Create global metrics registry
        let metrics_registry = Arc::new(MetricsRegistry::new());

        // Create SQLite client
        let sqlite_config = config.database.sqlite.clone();
        let sqlite_client = {
            let client =
                SqliteClient::new(sqlite_config).map_err(|e| EngineError::Config(e.to_string()))?;
            Arc::new(client)
        };

        // The SQLite client is shared as the metadata store
        let metadata_store = Some(sqlite_client.clone());

        // Create project registry
        let project_registry = Arc::new(ProjectRegistry::new(
            metrics_registry.clone(),
            (*sqlite_client).clone(),
        ));

        let qdrant_config = config.database.qdrant.clone();
        let qdrant = {
            let client = QdrantClient::new(qdrant_config, ".").map_err(EngineError::Storage)?;
            Arc::new(client)
        };

        let bm25_config = config.database.bm25.clone();
        let bm25 = {
            let client = Bm25Client::new(bm25_config);
            Arc::new(Mutex::new(client))
        };

        // Initialize BM25 index (Tantivy) if enabled
        {
            let mut bm25_client = bm25.lock().await;
            if let Err(e) = bm25_client.connect().await {
                tracing::warn!(error = %e, "Failed to connect BM25 index, BM25 will be unavailable");
            }
            drop(bm25_client);
        }

        let embedder_config = config.embedder.clone();
        // Use from_model to create embedder with the default model
        let default_model = &embedder_config.default_model;
        // Attach LLM retry/circuit-breaker metrics labeled by the provider
        let embedder_retry_metrics = config
            .resolve_llm_connection(default_model, cce_config::modules::ServiceType::Embedding)
            .ok()
            .map(|connection| LlmRetryMetrics::new(&metrics_registry, &connection.provider_id));
        let mut embedder = OpenAICompatibleProvider::from_model_with_retry_metrics(
            &config,
            default_model,
            embedder_retry_metrics,
        )
        .map_err(EngineError::Llm)?;

        // Attach embedding metrics (always enabled when metrics registry exists)
        let embedding_metrics =
            cce_metrics_infra::EmbeddingMetrics::new(&metrics_registry, embedder.model_name());
        embedder = embedder.with_metrics(embedding_metrics);

        let embedder = Arc::new(embedder);

        // Create metrics aggregator (optional, can be disabled via config)
        let metrics_aggregator = if config.metrics.aggregation.enabled {
            let agg_config = AggregationConfig {
                interval_secs: config.metrics.aggregation.interval_secs,
                enabled: config.metrics.aggregation.enabled,
                retention_seconds: config.metrics.aggregation.retention_seconds,
                cleanup_interval_secs: config.metrics.aggregation.cleanup_interval_secs,
                aggregate_counters: true,
                aggregate_gauges: true,
            };
            let aggregator =
                MetricsAggregator::new(sqlite_client.clone(), metrics_registry.clone(), agg_config)
                    .with_background_metrics(cce_metrics_infra::BackgroundTaskMetrics::new(
                        &metrics_registry,
                    ));
            Some(Arc::new(aggregator))
        } else {
            None
        };

        // Create runtime metrics collector
        let runtime_metrics = Arc::new(cce_metrics_infra::RuntimeMetrics::new(&metrics_registry));

        // Create system metrics collector
        let system_metrics = Arc::new(cce_metrics_infra::SystemMetrics::new(&metrics_registry));

        // Note: IndexOrchestrator and Searcher are now created per-project on demand
        // They will be cached in orchestrator_cache and searcher_cache respectively
        // Plugin loading is also deferred to get_orchestrator() for project-specific config
        // OperationCoordinator is also created per-project on demand
        // RelationRuntime is created per-project on demand

        Ok(Self {
            qdrant,
            bm25,
            embedder,
            metadata_store,
            orchestrator_cache: ProjectCache::new(),
            plugin_registry_cache: ProjectCache::new(),
            searcher_cache: ProjectCache::new(),
            project_registry,
            hot_update_cache: ProjectCache::new(),
            operation_coordinator_cache: ProjectCache::new(),
            relation_runtime_cache: ProjectCache::new(),
            progress_tracker: ProjectCache::new(),
            qdrant_control: None,
            metrics_registry,
            metrics_aggregator,
            runtime_metrics: Some(runtime_metrics),
            system_metrics: Some(system_metrics),
            retry_queue: ProjectCache::new(),
            render_cache: Arc::new(tokio::sync::RwLock::new(None)),
        })
    }
}

/// Engine builder for step-by-step configuration
pub struct EngineBuilder {
    config: Option<AppConfig>,
    config_path: Option<std::path::PathBuf>,
    qdrant_url: Option<String>,
    bm25_url: Option<String>,
    workspace: Option<String>,
}

impl EngineBuilder {
    /// Create a new engine builder
    pub fn new() -> Self {
        Self {
            config: None,
            config_path: None,
            qdrant_url: None,
            bm25_url: None,
            workspace: None,
        }
    }

    /// Set application config directly
    pub fn config(mut self, config: AppConfig) -> Self {
        self.config = Some(config);
        self
    }

    /// Set config from file path
    pub fn config_file(mut self, path: impl Into<std::path::PathBuf>) -> Self {
        self.config_path = Some(path.into());
        self
    }

    /// Override Qdrant URL
    pub fn qdrant_url(mut self, url: impl Into<String>) -> Self {
        self.qdrant_url = Some(url.into());
        self
    }

    /// Override BM25 URL
    pub fn bm25_url(mut self, url: impl Into<String>) -> Self {
        self.bm25_url = Some(url.into());
        self
    }

    /// Set workspace path (affects Qdrant collection naming)
    pub fn workspace(mut self, path: impl Into<String>) -> Self {
        self.workspace = Some(path.into());
        self
    }

    /// Build the engine
    pub async fn build(self) -> Result<CodeContextEngine, EngineError> {
        // Initialize settings from config
        let config = if let Some(config) = self.config {
            if !Settings::is_initialized() {
                Settings::init(config.clone()).map_err(|e| EngineError::Config(e.to_string()))?;
            }
            config
        } else if let Some(ref path) = self.config_path {
            Settings::init_from_file(Some(path.as_path()))
                .map_err(|e| EngineError::Config(e.to_string()))?;
            Settings::global()
                .map_err(|e| EngineError::Config(e.to_string()))?
                .clone()
        } else {
            // Use default config
            let config = AppConfig::default();
            if !Settings::is_initialized() {
                Settings::init(config.clone()).map_err(|e| EngineError::Config(e.to_string()))?;
            }
            config
        };

        // Apply URL overrides
        let mut effective_config = config;
        if let Some(qdrant_url) = self.qdrant_url {
            effective_config.database.qdrant.url = qdrant_url;
        }
        if let Some(bm25_url) = self.bm25_url {
            effective_config.database.bm25.index_path = Some(bm25_url);
        }

        CodeContextEngine::init_components(effective_config).await
    }
}

impl Default for EngineBuilder {
    fn default() -> Self {
        Self::new()
    }
}

mod builders;
mod components;
mod ops;
mod project_cache;
mod providers;
mod qdrant;
mod relation;

pub use project_cache::ProjectCache;
