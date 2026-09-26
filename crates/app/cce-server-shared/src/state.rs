//! HTTP server state management
//!
//! Provides AppState construction from the Engine facade,
//! eliminating duplicated initialization logic.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::engine::CodeContextEngine;
use crate::engine::ProjectCache;
use cce_metrics::ProgressTracker;
use cce_orchestrator::hot_update::watcher::WatchStatusTracker;
use cce_orchestrator::query::RelationSearcher;
use cce_relation::CallChainQuery;

type RelationSearcherEntry = (i64, Arc<RelationSearcher>);
type RelationSearcherCache = Arc<RwLock<HashMap<i64, RelationSearcherEntry>>>;

/// Shared application state for HTTP handlers
#[derive(Clone)]
pub struct AppState {
    /// Code Context Engine - provides unified access to all components
    pub engine: Arc<CodeContextEngine>,
    /// Watch status tracker (per-project)
    pub watch_status: Arc<RwLock<HashMap<i64, WatchStatusTracker>>>,
    /// Per-project progress trackers for lock-free metrics access
    pub progress_tracker: ProjectCache<ProgressTracker>,
    /// Qdrant subprocess lifecycle control handle
    pub qdrant_control: Option<cce_storage_qdrant::QdrantProcessHandle>,
    /// Per-project relation searcher cache (LRU caching for hot queries)
    pub relation_searcher_cache: RelationSearcherCache,
}

impl AppState {
    /// Create a new application state from Engine (preferred method)
    ///
    /// This ensures all components are initialized consistently through
    /// the Engine facade, supporting multi-project architecture.
    ///
    /// # Arguments
    ///
    /// * `engine` - The CodeContextEngine instance
    /// * `qdrant_handle` - Optional Qdrant process handle
    pub async fn from_engine(
        engine: &CodeContextEngine,
        qdrant_handle: Option<cce_storage_qdrant::QdrantProcessHandle>,
    ) -> Self {
        Self {
            engine: Arc::new(engine.clone()),
            watch_status: Arc::new(RwLock::new(HashMap::new())),
            progress_tracker: engine.progress_tracker().clone(),
            qdrant_control: qdrant_handle,
            relation_searcher_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get or create a cached `RelationSearcher` for a project.
    ///
    /// The cache is keyed by `(project_id, relation_epoch)`: when the
    /// snapshot epoch advances the cached searcher is replaced so queries
    /// never see stale graph data while still benefiting from LRU caching
    /// within an epoch.
    pub async fn get_relation_searcher(
        &self,
        project_id: i64,
    ) -> Result<Arc<RelationSearcher>, crate::engine::EngineError> {
        let runtime = self.engine.get_relation_runtime(project_id).await?;
        if !runtime.can_serve_queries().await {
            return Ok(Arc::new(RelationSearcher::new(Arc::new(
                CallChainQuery::new(),
            ))));
        }
        let snapshot = match runtime.get_snapshot().await {
            Some(s) => s,
            None => {
                return Ok(Arc::new(RelationSearcher::new(Arc::new(
                    CallChainQuery::new(),
                ))));
            }
        };
        let epoch = snapshot.relation_epoch;
        {
            let cache = self.relation_searcher_cache.read().await;
            if let Some((cached_epoch, searcher)) = cache.get(&project_id) {
                if *cached_epoch == epoch {
                    return Ok(Arc::clone(searcher));
                }
            }
        }
        let query = CallChainQuery::from_snapshot(Arc::clone(&snapshot.index));
        let searcher = Arc::new(RelationSearcher::from_query(query));
        {
            let mut cache = self.relation_searcher_cache.write().await;
            cache.insert(project_id, (epoch, Arc::clone(&searcher)));
        }
        Ok(searcher)
    }

    /// Invalidate the cached searcher for a project (e.g. on deletion).
    pub async fn invalidate_relation_searcher(&self, project_id: i64) {
        let mut cache = self.relation_searcher_cache.write().await;
        cache.remove(&project_id);
    }

    // --- Component access helpers ---
    // These provide convenient access to components stored in the engine.

    /// Get a clone of the Qdrant client
    pub fn qdrant_clone(&self) -> Arc<cce_storage_qdrant::QdrantClient> {
        self.engine.qdrant_clone()
    }

    /// Get a clone of the BM25 client
    pub fn bm25_clone(&self) -> Arc<tokio::sync::Mutex<cce_storage_bm25::Bm25Client>> {
        self.engine.bm25_clone()
    }

    /// Get a clone of the embedder
    pub fn embedder_clone(&self) -> Arc<dyn cce_llm::Embedder> {
        self.engine.embedder_clone()
    }

    /// Get a clone of the metadata store (SQLite client)
    pub fn metadata_store_clone(&self) -> Option<Arc<cce_storage_sqlite::SqliteClient>> {
        self.engine.metadata_store_clone()
    }

    /// Get a clone of the project registry
    pub fn project_registry_clone(&self) -> Arc<cce_storage_sqlite::project_registry::ProjectRegistry> {
        self.engine.project_registry_clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::CodeContextEngine;
    use cce_config::AppConfig;
    use cce_config::modules::{EmbeddingModelConfig, ProviderConfig};
    use std::collections::HashMap;

    /// Create a minimal test config with mock provider and model for engine construction
    fn create_test_config() -> AppConfig {
        let mut config = AppConfig::default();

        // Use temp directory for sqlite to avoid polluting workspace
        let temp_dir = std::env::temp_dir();
        let db_path = temp_dir.join(format!("cce_test_state_{}.db", std::process::id()));
        config.database.sqlite.path = db_path.to_string_lossy().to_string();

        // Add a mock provider
        let mut providers = HashMap::new();
        providers.insert(
            "test-provider".to_string(),
            ProviderConfig {
                id: "test-provider".to_string(),
                name: "Test Provider".to_string(),
                base_url: "http://localhost:0".to_string(),
                api_keys: vec!["test-key".to_string()],
                ..ProviderConfig::default()
            },
        );
        config.llm.providers = providers;

        // Add a mock embedding model
        let mut models = HashMap::new();
        models.insert(
            "test-model".to_string(),
            EmbeddingModelConfig {
                provider_id: "test-provider".to_string(),
                model: "test-model".to_string(),
                vector_dimension: 384,
                ..EmbeddingModelConfig::default()
            },
        );
        config.llm.embedding_models = models;
        config.embedder.default_model = "test-model".to_string();

        config
    }

    /// Test metadata store accessibility via engine
    ///
    /// Verifies that the engine properly initializes its SQLite metadata store
    /// so all project CRUD API endpoints can operate correctly by accessing it
    /// through the engine facade.
    #[tokio::test]
    async fn test_metadata_store_accessible_via_engine() {
        let config = create_test_config();
        let db_path = config.database.sqlite.path.clone();

        // Build engine with test config
        let engine = CodeContextEngine::builder()
            .config(config)
            .build()
            .await
            .expect("CodeContextEngine should build with test config");

        // Verify engine has a metadata store
        assert!(
            engine.metadata_store().is_some(),
            "CodeContextEngine should have a metadata_store after construction"
        );

        // Create AppState - it delegates to engine for component access
        let _state = AppState::from_engine(&engine, None).await;

        // Metadata store is accessible through the engine
        let store = engine
            .metadata_store()
            .expect("metadata_store should be initialized");
        assert!(
            store.as_ref().read_connection().is_ok(),
            "metadata_store should have a usable SQLite connection"
        );

        // Clean up temp file
        let _ = std::fs::remove_file(&db_path);
    }
}
