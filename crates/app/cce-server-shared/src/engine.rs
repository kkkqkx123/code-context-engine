//! Code Context Engine - High-level facade
//!
//! Provides a unified entry point for all core operations: indexing, searching,
//! and file watching. All interaction modes (CLI, HTTP, embedded library) should
//! use this facade rather than assembling individual components manually.

use std::sync::Arc;

use tokio::sync::Mutex;

use crate::runtime::RelationRuntime;
use cce_config::{AppConfig, Settings};

use cce_llm_client::OpenAICompatibleProvider;
use cce_metrics::{
    AggregationConfig, MetricsAggregator, MetricsRegistry, ProgressTracker, RenderCache,
};
use cce_orchestrator::OperationCoordinator;
use cce_orchestrator::hot_update::HotUpdateCoordinator;
use cce_orchestrator::index::IndexOrchestrator;
use cce_orchestrator::index::{FulltextStore, MetadataStore, VectorStore};
use cce_orchestrator::query::cache::{CacheConfig, QueryCache};
use cce_orchestrator::query::retry_queue::RetryQueue;
use cce_plugin::PluginRegistry;
use cce_storage_metadb_sqlite::SqliteClient;
use cce_storage_metadb_sqlite::project_registry::ProjectRegistry;
use cce_storage_vector_qdrant::QdrantProcessHandle;

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
    Storage(#[from] cce_types::StorageError),

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
    vector: VectorStore,
    fulltext: FulltextStore,
    embedder: Arc<OpenAICompatibleProvider>,

    /// Relation storage backend selected at assembly time.
    ///
    /// Kept as the backend-neutral handle so callers cannot assume the
    /// SQLite branch when a remote backend is configured.
    metadata_store: MetadataStore,

    /// Project-specific IndexOrchestrator cache (lazy-loaded per project)
    orchestrator_cache: ProjectCache<Mutex<IndexOrchestrator>>,

    /// Project-specific plugin registry cache (lazy-loaded per project)
    plugin_registry_cache: ProjectCache<PluginRegistry>,

    /// Per-project shared query result cache. Injected into every
    /// searcher and query coordinator so identical queries across requests
    /// populate one cache instead of one throwaway cache per request.
    query_result_cache: ProjectCache<QueryCache>,

    /// Configuration for the shared query result cache, resolved from the
    /// global `[cache]` section at startup.
    query_cache_config: CacheConfig,

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
    runtime_metrics: Option<Arc<cce_metrics::RuntimeMetrics>>,

    /// System resource metrics collector (CPU, memory, disk)
    system_metrics: Option<Arc<cce_metrics::SystemMetrics>>,

    /// LLM provider health-probe metrics (fed by the periodic probe task)
    llm_health_metrics: Option<Arc<cce_metrics::LlmHealthMetrics>>,

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

    /// Get (or lazily create) the shared query result cache for a project.
    ///
    /// The returned clone shares the underlying store with every other
    /// caller, so per-request searchers and query coordinators populate one
    /// cache instead of one throwaway cache each.
    pub async fn get_query_cache(&self, project_id: i64) -> QueryCache {
        self.query_result_cache
            .get_or_insert_with(project_id, || {
                Arc::new(QueryCache::new(self.query_cache_config.clone()))
            })
            .await
            .as_ref()
            .clone()
    }

    /// Drop the shared query cache for a project (deletion / full reindex).
    /// Cache keys already embed the epoch view, so this is memory hygiene,
    /// not a correctness requirement.
    pub async fn invalidate_query_cache(&self, project_id: i64) {
        self.query_result_cache.remove(project_id).await;
    }

    /// Create engine from an already-initialized AppConfig
    pub async fn from_config(config: AppConfig) -> Result<Self, EngineError> {
        seed_settings(&config)?;
        Self::init_components(config).await
    }

    /// Initialize all components from config
    async fn init_components(mut config: AppConfig) -> Result<Self, EngineError> {
        // Resolve configuration dependencies (auto-enable required features)
        config.resolve_dependencies();

        // Resolve the shared query result cache configuration once; each
        // project's cache is created lazily from this value.
        let query_cache_config = CacheConfig::from(&config.cache);

        // Create global metrics registry with the configured label allowlist.
        let metrics_registry = Arc::new(MetricsRegistry::with_label_config(
            config.metrics.labels.clone(),
        ));
        metrics_registry.update_memory_config(config.metrics.memory.clone());
        if config.metrics.memory.eviction_enabled {
            metrics_registry.start_cleanup_task();
        }
        let metrics_system_metrics = cce_metrics::MetricsSystemMetrics::new(&metrics_registry);

        // Wire the llm-suite gateway token sink before any chat request so
        // token usage, request outcome and retry counts land in this
        // registry. Late calls are ignored by the installer.
        cce_llm_client::suite::init_gateway(&metrics_registry);

        // Create the embedded metadata store directly. Remote selection is
        // rejected by configuration validation, so assembly only opens the
        // local database path here.
        let sqlite_client = Arc::new(
            SqliteClient::new(config.database.sqlite.clone())
                .map_err(|e| EngineError::Config(e.to_string()))?,
        );
        let metadata_store: MetadataStore = sqlite_client.clone();
        let metadata_store_for_engine = metadata_store.clone();

        // Create project registry
        let project_registry = Arc::new(ProjectRegistry::new(
            metrics_registry.clone(),
            (*sqlite_client).clone(),
        ));

        let vector = VectorStore::from_database_config(&config.database)
            .map_err(|e| EngineError::Config(e.to_string()))?;

        let mut fulltext_store = FulltextStore::from_database_config(&config.database)
            .map_err(|e| EngineError::Config(e.to_string()))?;

        // Initialize the fulltext index for whichever branch is active.
        // The local branch opens its Tantivy index; the remote branch
        // ensures the Elasticsearch index exists. Both calls run on the
        // single owner before sharing.
        match &mut fulltext_store {
            FulltextStore::Local(shared) => {
                if let Some(client) = Arc::get_mut(shared)
                    && let Err(e) = client.connect().await
                {
                    tracing::warn!(error = %e, "Failed to connect BM25 index, BM25 will be unavailable");
                }
            }
            FulltextStore::Remote(shared) => {
                if let Err(e) = shared.ensure_index().await {
                    tracing::warn!(error = %e, "Failed to ensure Elasticsearch index, BM25 will be unavailable");
                }
            }
        }

        let embedder_config = config.embedder.clone();
        // Use from_model to create embedder with the default model.
        // Retries run inside llm-suite; per-attempt accounting is
        // gateway-owned, so no CCE retry metrics are attached here.
        let default_model = &embedder_config.default_model;
        let mut embedder = OpenAICompatibleProvider::from_model(&config, default_model)
            .map_err(EngineError::Llm)?;

        // Attach embedding metrics (always enabled when metrics registry exists)
        let embedding_metrics =
            cce_metrics::EmbeddingMetrics::new(&metrics_registry, embedder.model_name());
        embedder = embedder.with_metrics(embedding_metrics);

        let embedder = Arc::new(embedder);

        // Create LLM health-probe metrics labeled by the provider
        let llm_provider_label = config
            .resolve_llm_connection(default_model, cce_config::modules::ServiceType::Embedding)
            .map(|connection| connection.provider_id)
            .unwrap_or_else(|_| "unknown".to_string());
        let llm_health_metrics =
            cce_metrics::LlmHealthMetrics::new(&metrics_registry, &llm_provider_label);

        // Create the metrics aggregator when aggregation is enabled.
        let metrics_aggregator = if config.metrics.aggregation.enabled {
            let global = &config.metrics.aggregation;
            let agg_config = AggregationConfig {
                interval_secs: global.interval_secs,
                enabled: global.enabled,
                retention_seconds: global.retention_seconds,
                cleanup_interval_secs: global.cleanup_interval_secs,
                aggregate_counters: global.aggregate_counters,
                aggregate_gauges: global.aggregate_gauges,
                batch_size: global.batch_size.max(1),
                default_interval_secs: global.default_interval_secs,
                metric_overrides: global.metric_overrides.clone(),
            };
            let aggregator =
                MetricsAggregator::new(sqlite_client.clone(), metrics_registry.clone(), agg_config)
                    .with_background_metrics(cce_metrics::BackgroundTaskMetrics::new(
                        &metrics_registry,
                    ))
                    .with_system_metrics(metrics_system_metrics.clone());
            Some(Arc::new(aggregator))
        } else {
            None
        };

        // Create runtime metrics collector
        let runtime_metrics = Arc::new(cce_metrics::RuntimeMetrics::new(&metrics_registry));

        // Create system metrics collector
        let system_metrics = Arc::new(cce_metrics::SystemMetrics::new(&metrics_registry));

        // Note: IndexOrchestrator and OperationCoordinator are created per-project on demand.
        // Plugin loading stays deferred to get_orchestrator() for project-specific config.

        Ok(Self {
            vector,
            fulltext: fulltext_store,
            embedder,
            metadata_store: metadata_store_for_engine,
            orchestrator_cache: ProjectCache::new(),
            plugin_registry_cache: ProjectCache::new(),
            query_result_cache: ProjectCache::new(),
            query_cache_config,
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
            llm_health_metrics: Some(llm_health_metrics),
            retry_queue: ProjectCache::new(),
            render_cache: Arc::new(tokio::sync::RwLock::new(None)),
        })
    }
}

/// Engine builder for step-by-step configuration.
pub struct EngineBuilder {
    config: Option<AppConfig>,
}

impl EngineBuilder {
    /// Create a new engine builder.
    pub fn new() -> Self {
        Self { config: None }
    }

    /// Set the application config this engine assembles from.
    pub fn config(mut self, config: AppConfig) -> Self {
        self.config = Some(config);
        self
    }

    /// Build the engine from the supplied config.
    pub async fn build(self) -> Result<CodeContextEngine, EngineError> {
        let config = self.config.ok_or_else(|| {
            EngineError::Config("engine builder requires an explicit config".to_string())
        })?;
        seed_settings(&config)?;
        CodeContextEngine::init_components(config).await
    }
}

/// Seed the process-wide settings when empty.
///
/// The supplied config always drives the engine being built; an already
/// initialized global is left untouched so unrelated readers keep a stable
/// view.
fn seed_settings(config: &AppConfig) -> Result<(), EngineError> {
    if Settings::is_initialized() {
        tracing::warn!(
            "Settings already initialized; the supplied config applies to this engine only"
        );
        return Ok(());
    }
    Settings::init(config.clone()).map_err(|e| EngineError::Config(e.to_string()))
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
