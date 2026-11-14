//! Shared application context management
//!
//! Provides `SharedContext` — the engine plus watch status, progress
//! trackers, and caches — shared by the HTTP, MCP, and CLI surfaces.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::engine::CodeContextEngine;
use crate::engine::ProjectCache;
use cce_codegraph::CallChainQuery;
use cce_metrics::ProgressTracker;
use cce_orchestrator::hot_update::watcher::WatchStatusTracker;
use cce_orchestrator::query::RelationSearcher;

type RelationSearcherEntry = (i64, Arc<RelationSearcher>);
type RelationSearcherCache = Arc<RwLock<HashMap<i64, RelationSearcherEntry>>>;

/// Convert the backend-owned Qdrant lifecycle state into its wire copy.
///
/// Lives in the assembly layer because it is the only crate depending on
/// both the storage backend and the wire contract.
#[allow(clippy::needless_pass_by_value)]
pub fn to_wire_qdrant_status(
    domain: cce_storage_vector_qdrant::QdrantProcessStatus,
) -> cce_api::models::QdrantProcessStatus {
    match domain {
        cce_storage_vector_qdrant::QdrantProcessStatus::Idle => {
            cce_api::models::QdrantProcessStatus::Idle
        }
        cce_storage_vector_qdrant::QdrantProcessStatus::Starting => {
            cce_api::models::QdrantProcessStatus::Starting
        }
        cce_storage_vector_qdrant::QdrantProcessStatus::Running => {
            cce_api::models::QdrantProcessStatus::Running
        }
        cce_storage_vector_qdrant::QdrantProcessStatus::Stopping => {
            cce_api::models::QdrantProcessStatus::Stopping
        }
        cce_storage_vector_qdrant::QdrantProcessStatus::Crashed => {
            cce_api::models::QdrantProcessStatus::Crashed
        }
        cce_storage_vector_qdrant::QdrantProcessStatus::Stopped => {
            cce_api::models::QdrantProcessStatus::Stopped
        }
        cce_storage_vector_qdrant::QdrantProcessStatus::Failed(message) => {
            cce_api::models::QdrantProcessStatus::Failed(message)
        }
    }
}

/// Shared application state for HTTP handlers
#[derive(Clone)]
pub struct SharedContext {
    /// Code Context Engine - provides unified access to all components
    pub engine: Arc<CodeContextEngine>,
    /// Watch status tracker (per-project)
    pub watch_status: Arc<RwLock<HashMap<i64, WatchStatusTracker>>>,
    /// Per-project progress trackers for lock-free metrics access
    pub progress_tracker: ProjectCache<ProgressTracker>,
    /// Qdrant subprocess lifecycle control handle
    pub qdrant_control: Option<cce_storage_vector_qdrant::QdrantProcessHandle>,
    /// Per-project relation searcher cache (LRU caching for hot queries)
    pub relation_searcher_cache: RelationSearcherCache,
}

impl SharedContext {
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
        qdrant_handle: Option<cce_storage_vector_qdrant::QdrantProcessHandle>,
    ) -> Self {
        Self {
            engine: Arc::new(engine.clone()),
            watch_status: Arc::new(RwLock::new(HashMap::new())),
            progress_tracker: engine.progress_tracker().clone(),
            qdrant_control: qdrant_handle,
            relation_searcher_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Snapshot the managed Qdrant subprocess as wire types.
    ///
    /// Returns the management flag plus the converted status so HTTP handlers
    /// never name the backend domain type and the server crate needs no
    /// direct storage dependency.
    pub async fn qdrant_status_snapshot(
        &self,
    ) -> Option<(bool, cce_api::models::QdrantProcessStatus)> {
        let handle = self.qdrant_control.as_ref()?;
        let domain = handle.current_status().await;
        Some((handle.managed, to_wire_qdrant_status(domain)))
    }

    /// Forward a start command to the managed Qdrant subprocess.
    pub fn qdrant_start(&self) -> bool {
        match self.qdrant_control.as_ref() {
            Some(handle) => {
                handle.start();
                true
            }
            None => false,
        }
    }

    /// Forward a stop command to the managed Qdrant subprocess.
    pub fn qdrant_stop(&self) -> bool {
        match self.qdrant_control.as_ref() {
            Some(handle) => {
                handle.stop();
                true
            }
            None => false,
        }
    }

    /// Forward a restart command to the managed Qdrant subprocess.
    pub fn qdrant_restart(&self) -> bool {
        match self.qdrant_control.as_ref() {
            Some(handle) => {
                handle.restart();
                true
            }
            None => false,
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

    /// Invalidate all cached relation searchers (e.g. on global reload).
    pub async fn invalidate_relation_searcher_all(&self) {
        self.relation_searcher_cache.write().await.clear();
    }

    // --- Component access helpers ---
    // These provide convenient access to components stored in the engine.

    /// Get a reference to the vector store (backend-neutral)
    pub fn vector(&self) -> &cce_orchestrator::index::VectorStore {
        self.engine.vector()
    }

    /// Get a clone of the vector store
    pub fn vector_clone(&self) -> cce_orchestrator::index::VectorStore {
        self.engine.vector_clone()
    }

    /// Get a clone of the Qdrant client (Qdrant backend only, for diagnostics/process admin)
    pub fn qdrant_clone(&self) -> Option<Arc<cce_storage_vector_qdrant::QdrantClient>> {
        self.engine.qdrant_clone()
    }

    /// Get a clone of the fulltext backend
    pub fn fulltext_clone(&self) -> cce_orchestrator::index::FulltextStore {
        self.engine.fulltext_clone()
    }

    /// Get a clone of the embedder
    pub fn embedder_clone(&self) -> Arc<cce_llm_client::OpenAICompatibleProvider> {
        self.engine.embedder_clone()
    }

    /// Get the relation store selected at assembly time.
    pub fn metadata_store(&self) -> Option<cce_orchestrator::index::MetadataStore> {
        self.engine.metadata_store()
    }

    /// Get a clone of the project registry
    pub fn project_registry_clone(
        &self,
    ) -> Arc<cce_storage_metadb_sqlite::project_registry::ProjectRegistry> {
        self.engine.project_registry_clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::CodeContextEngine;
    use cce_config::AppConfig;
    use cce_config::modules::{EmbeddingModelConfig, ProviderConfig};
    use cce_storage_common::MetadataStorage;
    use std::collections::HashMap;

    /// Create a minimal test config with mock provider and model for engine construction
    fn create_test_config() -> AppConfig {
        let mut config = AppConfig::default();

        // Each engine opens its own vector store and metadata database under
        // the data dir, so parallel tests must not share one.
        let data_dir = crate::test_support::unique_test_dir();
        config.database.sqlite.path = data_dir.join("cce.db").to_string_lossy().to_string();

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

    /// Test relation store accessibility via engine
    ///
    /// Verifies that the engine properly initializes its relation store
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

        // Verify engine has a relation store
        assert!(
            engine.metadata_store().is_some(),
            "CodeContextEngine should have a metadata_store after construction"
        );

        // Create SharedContext - it delegates to engine for component access
        let _state = SharedContext::from_engine(&engine, None).await;

        // Relation store is accessible through the engine
        let store = engine
            .metadata_store()
            .expect("metadata_store should be initialized");
        assert!(
            store.projects_all().await.is_ok(),
            "metadata_store should serve contract reads"
        );

        // Clean up temp file
        let _ = std::fs::remove_file(&db_path);
    }
}
