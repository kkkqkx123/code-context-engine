//! Query workflow test helper
//!
//! This module provides a test helper for query workflow tests.

use anyhow::Result;
use std::sync::Arc;
use tempfile::TempDir;

use cce_config::project_registry::ProjectScope;
use cce_llm_client::OpenAICompatibleProvider;
use cce_orchestrator::{
    AggregatedQueryOptions, IndexOptions, IndexOrchestrator, IndexResult, QueryCoordinator,
    QueryOptions, QueryResult, SearchConfig, SearchSources, SubQuery,
};
use cce_storage_bm25::Bm25Config;
use cce_storage_common::{VectorStorage, generate_project_group_id};
use cce_storage_vector_local::LocalVectorStore;

use super::EmbeddingConfig;
use crate::fixture::FixtureAccess;
use crate::index_test::TestRelationPublisher;

/// Query workflow test helper
///
/// Provides utilities for testing query workflows including:
/// - Vector search
/// - BM25 search
/// - Relation queries
/// - Hybrid search
pub struct QueryWorkflowTest<F: FixtureAccess> {
    /// Test fixture
    fixture: F,
    /// Embedding configuration
    embedding_config: EmbeddingConfig,
    /// Project ID for isolation
    project_id: i64,
    /// Index orchestrator
    index_orchestrator: Option<IndexOrchestrator>,
    /// Query coordinator
    query_coordinator: Option<QueryCoordinator>,
    /// Last index result
    last_index_result: Option<IndexResult>,
    /// Last query result
    last_query_result: Option<QueryResult>,
    /// Search sources configuration
    search_sources: SearchSources,
    /// Search configuration
    search_config: SearchConfig,
    /// Shared BM25 client (index + query use same instance)
    bm25_client: Option<Arc<cce_storage_bm25::Bm25Client>>,
    /// Shared local vector store (index + query use same instance).
    ///
    /// This is the default vector backend: zero external services, data rooted
    /// at a per-run temp directory dropped with the test.
    local_vector: Option<Arc<LocalVectorStore>>,
    /// Shared Qdrant client, only present when the remote backend is opted in
    /// via [`Self::with_qdrant_url`] (remote-branch verification). The default
    /// local path never sets this.
    qdrant_client: Option<Arc<cce_storage_vector_qdrant::QdrantClient>>,
    /// Remote Qdrant endpoint overriding the default local backend.
    /// `None` (default) means embedded simvec; `Some(url)` selects Qdrant.
    qdrant_url: Option<String>,
    /// Shared SQLite database (index + query use same instance)
    relation_db: Option<cce_orchestrator::index::MetadataStore>,
    /// Shared embedder for vector index and query (optional; without it,
    /// vector storage is skipped during indexing)
    embedder: Option<Arc<OpenAICompatibleProvider>>,
    /// Metrics registry wired into the query searcher (optional)
    metrics_registry: Option<Arc<cce_metrics::MetricsRegistry>>,
    /// Generative rerank handler wired into the query searcher (optional)
    rerank_handler: Option<Arc<cce_llm_client::ProductionRerankHandler>>,
    /// Per-query rerank override used when building query options
    enable_rerank: Option<bool>,
    /// File extensions to index (defaults to the orchestrator defaults)
    extensions: Option<Vec<String>>,
    /// BM25 index temp directory, dropped after test completes
    _bm25_temp_dir: Option<TempDir>,
    /// Local vector data directory, dropped after test completes. Per-run so
    /// no stale vectors leak across runs.
    _vector_temp_dir: Option<TempDir>,
    /// Project group id shared by the index fingerprint and the query scope
    project_group_id: Option<String>,
    /// Optional stable scenario key used to derive the project group id.
    /// Without it the group id is hashed from the per-run fixture path; with
    /// a stable key repeated runs share one group partition, keeping storage
    /// bounded on backends with a shared collection.
    scenario_name: Option<String>,
}

impl<F: FixtureAccess> QueryWorkflowTest<F> {
    /// Create a new query workflow test
    pub fn new(fixture: F, embedding_config: EmbeddingConfig) -> Self {
        Self {
            fixture,
            embedding_config,
            project_id: 1,
            index_orchestrator: None,
            query_coordinator: None,
            last_index_result: None,
            last_query_result: None,
            search_sources: SearchSources::default(),
            search_config: SearchConfig::default(),
            bm25_client: None,
            local_vector: None,
            qdrant_client: None,
            qdrant_url: None,
            relation_db: None,
            embedder: None,
            metrics_registry: None,
            rerank_handler: None,
            enable_rerank: None,
            extensions: None,
            _bm25_temp_dir: None,
            _vector_temp_dir: None,
            project_group_id: None,
            scenario_name: None,
        }
    }

    /// Set the project ID for this test
    #[allow(dead_code)]
    pub fn with_project_id(mut self, project_id: i64) -> Self {
        self.project_id = project_id;
        self
    }

    /// Set search sources
    #[allow(dead_code)]
    pub fn with_sources(mut self, sources: SearchSources) -> Self {
        self.search_sources = sources;
        self
    }

    /// Set search configuration
    #[allow(dead_code)]
    pub fn with_config(mut self, config: SearchConfig) -> Self {
        self.search_config = config;
        self
    }

    /// Set a shared embedder used for both vector indexing and querying
    #[allow(dead_code)]
    pub fn with_embedder(mut self, embedder: Arc<OpenAICompatibleProvider>) -> Self {
        self.embedder = Some(embedder);
        self
    }

    /// Set a metrics registry wired into the query searcher (hybrid alignment
    /// coverage observation). A separate `SearchMetrics` handle created from
    /// the same registry can read the recorded values.
    #[allow(dead_code)]
    pub fn with_metrics_registry(mut self, registry: Arc<cce_metrics::MetricsRegistry>) -> Self {
        self.metrics_registry = Some(registry);
        self
    }

    /// Wire a generative rerank handler into the query searcher (simulating
    /// `[rerank] enabled = true` at the config layer).
    #[allow(dead_code)]
    pub fn with_rerank_handler(
        mut self,
        handler: Arc<cce_llm_client::ProductionRerankHandler>,
    ) -> Self {
        self.rerank_handler = Some(handler);
        self
    }

    /// Force reranking on/off for the next queries (request-level override).
    #[allow(dead_code)]
    pub fn with_enable_rerank(mut self, enable: bool) -> Self {
        self.enable_rerank = Some(enable);
        self
    }

    /// Override the file extensions to index (e.g. to include markdown)
    #[allow(dead_code)]
    pub fn with_extensions(mut self, extensions: Vec<String>) -> Self {
        self.extensions = Some(extensions);
        self
    }

    /// Bind a stable scenario key used to derive the project group id.
    ///
    /// Without it the group id is hashed from the per-run fixture path. With a
    /// stable key repeated runs of the same scenario share one group partition,
    /// keeping storage bounded on backends with a shared collection. The local
    /// default backend rebuilds its data dir per run, so the key only changes
    /// the group string there, not storage reuse.
    #[allow(dead_code)]
    pub fn with_scenario_name(mut self, scenario: impl Into<String>) -> Self {
        self.scenario_name = Some(scenario.into());
        self
    }

    /// Opt into the remote Qdrant backend for this test (remote-branch
    /// verification only). The default is the embedded local engine.
    #[allow(dead_code)]
    pub fn with_qdrant_url(mut self, url: impl Into<String>) -> Self {
        self.qdrant_url = Some(url.into());
        self
    }

    /// The project group id this test indexes into (if an index has run).
    #[allow(dead_code)]
    pub fn project_group_id(&self) -> Option<&str> {
        self.project_group_id.as_deref()
    }

    /// Get the test fixture
    pub fn fixture(&self) -> &F {
        &self.fixture
    }

    /// Get the last index result
    pub fn last_index_result(&self) -> Option<&IndexResult> {
        self.last_index_result.as_ref()
    }

    /// Build the mock or configured embedder from the test configuration.
    ///
    /// Indexing and querying share this provider so the benchmark's vector
    /// counters describe the same write path as the service assembly.
    fn build_embedder(&self) -> Result<Arc<OpenAICompatibleProvider>> {
        let mut providers = std::collections::HashMap::new();
        let base_url = if self.embedding_config.is_mock() {
            "http://mock.local".to_string()
        } else {
            self.embedding_config.endpoint.clone()
        };
        let api_key = if self.embedding_config.api_key.is_empty() {
            "test-key".to_string()
        } else {
            self.embedding_config.api_key.clone()
        };
        providers.insert(
            "test-provider".to_string(),
            cce_config::modules::ProviderConfig {
                id: "test-provider".to_string(),
                name: "Test Provider".to_string(),
                base_url,
                api_keys: vec![api_key],
                ..Default::default()
            },
        );
        let mut models = std::collections::HashMap::new();
        models.insert(
            self.embedding_config.model.clone(),
            cce_config::modules::EmbeddingModelConfig {
                provider_id: "test-provider".to_string(),
                model: self.embedding_config.model.clone(),
                vector_dimension: self.embedding_config.dimension,
                ..Default::default()
            },
        );
        let mut app_config = cce_config::AppConfig::default();
        app_config.llm.providers = providers;
        app_config.llm.embedding_models = models;
        app_config.embedder.default_model = self.embedding_config.model.clone();
        Ok(Arc::new(OpenAICompatibleProvider::from_model(
            &app_config,
            &self.embedding_config.model,
        )?))
    }

    /// Execute indexing
    ///
    /// Configures shared BM25 and vector clients for both indexing and subsequent queries.
    /// Uses a unique per-instance BM25 index path to prevent cross-test data pollution.
    /// The vector backend defaults to the embedded local engine rooted at a
    /// per-run temp directory; opt into Qdrant with [`Self::with_qdrant_url`].
    pub async fn index(&mut self) -> Result<&IndexResult> {
        use cce_orchestrator::CheckpointManager;
        use cce_orchestrator::index::in_memory_metadata_store;
        use cce_storage_common::MetadataStorage;

        let mut orchestrator =
            IndexOrchestrator::new(self.project_id).expect("failed to create IndexOrchestrator");

        // The project group id must be shared by the index and query sides so
        // stored payloads and the query scope reference the same partition,
        // mirroring the production wiring where `generate_project_group_id`
        // feeds both `with_project_fingerprint` (index) and `ProjectScope`
        // (query). A stable scenario name (when set) replaces the per-run
        // fixture path as the workspace key so repeated runs share one group.
        let workspace_key = self
            .scenario_name
            .clone()
            .unwrap_or_else(|| self.fixture.root_path().to_string_lossy().to_string());
        let project_group_id = generate_project_group_id(self.project_id, &workspace_key);
        self.project_group_id = Some(project_group_id.clone());
        orchestrator = orchestrator.with_project_fingerprint(project_group_id);

        // Set up checkpoint manager for operation progress tracking
        let metadata_store = in_memory_metadata_store()?;
        // Register the project row before any metadata write so FK-scoped
        // file/entity/chunk inserts resolve, and so query-time source reads
        // locate the real fixture root. Inserted first so the publisher's
        // placeholder ensure cannot win the INSERT OR IGNORE race.
        {
            let root_path = self.fixture.root_path().to_string_lossy().to_string();
            let project_id = self.project_id;
            metadata_store
                .ensure_project(project_id, &root_path)
                .await?;
        }
        let checkpoint_manager = Arc::new(CheckpointManager::new_for_project(
            self.project_id,
            metadata_store.clone(),
        ));
        self.relation_db = Some(metadata_store.clone());
        orchestrator = orchestrator.with_checkpoint_manager(checkpoint_manager);
        orchestrator = orchestrator.with_metadata_store(metadata_store.clone());
        orchestrator = orchestrator.with_relation_publisher(Arc::new(
            TestRelationPublisher::with_store(metadata_store.clone()),
        ));

        // Each test instance gets a unique BM25 index temp directory to prevent cross-test
        // data pollution (Tantivy lock contention, stale data from other tests).
        // TempDir is automatically cleaned up when QueryWorkflowTest is dropped.
        let bm25_temp_dir = TempDir::new()?;
        let bm25_path = bm25_temp_dir.path().to_string_lossy().to_string();
        let index_name = "default";
        let bm25_config = Bm25Config::default()
            .enabled()
            .with_index_name(index_name)
            .with_index_path(&bm25_path);
        let mut bm25 = cce_storage_bm25::Bm25Client::new(bm25_config);
        bm25.connect().await?;
        let bm25 = Arc::new(bm25);
        self.bm25_client = Some(bm25.clone());
        self._bm25_temp_dir = Some(bm25_temp_dir);
        orchestrator = orchestrator.with_bm25_client(bm25);

        if self.search_sources.vector {
            if let Some(ref url) = self.qdrant_url.clone() {
                // Remote-branch verification: match the collection dimension
                // to the configured embedder.
                let mut qdrant_config =
                    cce_storage_vector_qdrant::QdrantConfig::with_url(url.as_str());
                qdrant_config.vector_size = self.embedding_config.dimension;
                let qdrant = Arc::new(cce_storage_vector_qdrant::QdrantClient::new(
                    qdrant_config,
                    "test",
                )?);

                // Verify the service is reachable before starting the index.
                qdrant.initialize().await.map_err(|e| {
                    anyhow::anyhow!(
                        "Failed to connect to Qdrant at {}. \
                         The remote vector backend is enabled but unreachable. \
                         Please start the Qdrant service or drop with_qdrant_url \
                         to use the default local backend. \
                         Underlying error: {}",
                        url,
                        e
                    )
                })?;

                self.qdrant_client = Some(qdrant.clone());
                orchestrator = orchestrator.with_qdrant_client(qdrant);
            } else {
                // Default: embedded local engine. Collection dimension comes
                // explicitly from the configured (mock) embedder; HNSW tuning
                // stays at engine defaults since fixtures are far below the
                // exact-scan promotion threshold.
                let vector_temp_dir = TempDir::new()?;
                let local_config = cce_config::modules::LocalVectorConfig {
                    vector_size: self.embedding_config.dimension,
                    ..Default::default()
                };
                let store = LocalVectorStore::open_at(vector_temp_dir.path(), &local_config)?;
                let store = Arc::new(store);
                self.local_vector = Some(store.clone());
                self._vector_temp_dir = Some(vector_temp_dir);
                orchestrator = orchestrator.with_local(store);
            }
        }
        if let Some(ref embedder) = self.embedder {
            orchestrator = orchestrator.with_embedder(embedder.clone());
        }
        self.index_orchestrator = Some(orchestrator);

        let options = IndexOptions {
            root_dir: self.fixture.root_path().to_path_buf(),
            extensions: self
                .extensions
                .clone()
                .unwrap_or_else(|| IndexOptions::default().extensions),
            build_relations: true,
            // Always enable BM25 so search_bm25() queries return results
            store_bm25: true,
            store_vectors: self.search_sources.vector,
            ..Default::default()
        };

        let result = self
            .index_orchestrator
            .as_mut()
            .expect("Orchestrator not initialized")
            .execute(options)
            .await?;

        self.last_index_result = Some(result);
        Ok(self.last_index_result.as_ref().expect("Result not set"))
    }

    /// Execute a query
    pub async fn query(&mut self, query_text: &str) -> Result<&QueryResult> {
        // Ensure index is available
        if self.last_index_result.is_none() {
            self.index().await?;
        }

        // Create query options
        let options = QueryOptions {
            query: query_text.to_string(),
            project_id: self.project_id,
            sources: self.search_sources,
            config: self.search_config.clone(),
            query_intent: None,
            directory_prefix: None,
            exclude_content_types: Vec::new(),
            include_categories: Vec::new(),
            exclude_categories: Vec::new(),
            exclude_patterns: Vec::new(),
            include_patterns: Vec::new(),
            with_source: true,
            enable_rerank: self.enable_rerank,
            enable_result_filter: None,
            neighbour_resolver: None,
        };

        // Initialize query coordinator if not already done
        if self.query_coordinator.is_none() {
            self.init_query_coordinator().await?;
        }

        // Execute the query
        if let Some(ref coordinator) = self.query_coordinator {
            let result = coordinator.search(&options).await?;
            self.last_query_result = Some(result);
        }

        Ok(self.last_query_result.as_ref().expect("Result not set"))
    }

    /// Execute a vector search query
    pub async fn search_vector(&mut self, query_text: &str, top_k: usize) -> Result<&QueryResult> {
        self.search_sources = SearchSources::none().with_vector();
        self.search_config.vector.top_k = top_k;
        self.query(query_text).await
    }

    /// Execute a BM25 search query
    pub async fn search_bm25(&mut self, query_text: &str, limit: usize) -> Result<&QueryResult> {
        self.search_sources = SearchSources::none().with_bm25();
        self.search_config.result.limit = limit;
        self.query(query_text).await
    }

    /// Execute a hybrid search query
    pub async fn search_hybrid(&mut self, query_text: &str, limit: usize) -> Result<&QueryResult> {
        self.search_sources = SearchSources::default();
        self.search_config.result.limit = limit;
        self.query(query_text).await
    }

    /// Get the query coordinator
    #[allow(dead_code)]
    pub fn coordinator(&self) -> &QueryCoordinator {
        self.query_coordinator
            .as_ref()
            .expect("Query coordinator not initialized")
    }

    /// Initialize the query coordinator without running a query.
    ///
    /// Relation-only consumers (relation review, annotation expansion)
    /// need the coordinator right after indexing; queries lazily init it,
    /// but direct `coordinator()` access panics before the first query.
    pub async fn ensure_coordinator(&mut self) -> Result<()> {
        if self.query_coordinator.is_none() {
            self.init_query_coordinator().await?;
        }
        Ok(())
    }

    /// Best-effort removal of this test's vector points.
    ///
    /// Logical isolation comes from the `group_id` payload field on both
    /// backends. Deleting the group's points at teardown keeps shared
    /// collections from growing without bound across test runs. BM25 (Tantivy)
    /// and local vector state live in per-instance `TempDir`s removed on drop,
    /// so no explicit cleanup is needed there.
    ///
    /// Callers should invoke this in their teardown path; failures are logged
    /// and swallowed so a missing backend cannot fail an otherwise-passing test.
    pub async fn cleanup(&mut self) {
        let Some(group_id) = self.project_group_id.clone() else {
            return;
        };
        if let Some(store) = self.local_vector.clone() {
            if let Err(e) = store.delete_by_group(&group_id).await {
                tracing::warn!("Local vector cleanup for group '{}' failed: {e}", group_id);
            }
            return;
        }
        let Some(client) = self.qdrant_client.clone() else {
            return;
        };
        if let Err(e) = client.delete_by_group(&group_id).await {
            tracing::warn!("Qdrant cleanup for group '{}' failed: {e}", group_id);
        }
    }

    /// Execute an aggregated search query.
    pub async fn search_aggregated(
        &mut self,
        query_text: &str,
        sub_queries: Vec<String>,
    ) -> Result<&QueryResult> {
        let texts = if sub_queries.is_empty() {
            vec![query_text.to_string()]
        } else {
            sub_queries
        };
        let options = AggregatedQueryOptions {
            original_query: query_text.to_string(),
            project_id: self.project_id,
            sub_queries: texts
                .into_iter()
                .map(|text| SubQuery {
                    text,
                    sources: self.search_sources,
                    weight: 1.0,
                })
                .collect(),
            global_config: self.search_config.clone(),
            ..Default::default()
        };

        if self.query_coordinator.is_none() {
            self.init_query_coordinator().await?;
        }

        if let Some(ref coordinator) = self.query_coordinator {
            let result = coordinator.search_aggregated(&options).await?;
            self.last_query_result = Some(result);
        }

        Ok(self.last_query_result.as_ref().expect("Result not set"))
    }

    /// Get callees of a function
    #[allow(dead_code)]
    pub async fn get_callees(
        &mut self,
        entity_id: cce_types::EntityId,
    ) -> Result<Vec<cce_types::ResolvedRelation>> {
        if self.query_coordinator.is_none() {
            self.init_query_coordinator().await?;
        }

        if let Some(ref coordinator) = self.query_coordinator {
            coordinator
                .get_callees(entity_id)
                .map_err(|e| anyhow::anyhow!("Failed to get callees: {}", e))
        } else {
            Ok(vec![])
        }
    }

    /// Get callers of a function
    #[allow(dead_code)]
    pub async fn get_callers(
        &mut self,
        entity_id: cce_types::EntityId,
    ) -> Result<Vec<cce_types::EntityId>> {
        if self.query_coordinator.is_none() {
            self.init_query_coordinator().await?;
        }

        if let Some(ref coordinator) = self.query_coordinator {
            coordinator
                .get_callers(entity_id)
                .map_err(|e| anyhow::anyhow!("Failed to get callers: {}", e))
        } else {
            Ok(vec![])
        }
    }

    /// Initialize query coordinator from index orchestrator
    async fn init_query_coordinator(&mut self) -> Result<()> {
        use cce_codegraph::{CallChainQuery, RelationIndex};
        use cce_orchestrator::query::{IndexCapabilities, QueryCoordinator};
        use cce_storage_vector_qdrant::QdrantClient;
        use std::sync::Arc;

        // Vector backend mirrors the index side: reuse the indexed store when
        // present, otherwise fall back to a fresh instance of the selected
        // backend so the builder always has a store. The local fallback keeps
        // its temp dir alive on `self`; the Qdrant fallback is only reachable
        // on the remote opt-in path. Capability bits below are snapshotted
        // before the fallback so an empty fallback store never poses as
        // indexed vectors (a BM25-only index reports no vectors, matching the
        // previous Qdrant-fallback semantics).
        let has_vectors = self.local_vector.is_some() || self.qdrant_client.is_some();
        let vector_store = if let Some(store) = self.local_vector.clone() {
            cce_orchestrator::index::VectorStore::local(store)
        } else if let Some(client) = self.qdrant_client.clone() {
            cce_orchestrator::index::VectorStore::Qdrant(client)
        } else if let Some(url) = self.qdrant_url.clone() {
            let mut qdrant_config = cce_storage_vector_qdrant::QdrantConfig::with_url(url);
            qdrant_config.vector_size = self.embedding_config.dimension;
            let client = Arc::new(QdrantClient::new(qdrant_config, "test")?);
            self.qdrant_client = Some(client.clone());
            cce_orchestrator::index::VectorStore::Qdrant(client)
        } else {
            let vector_temp_dir = TempDir::new()?;
            let local_config = cce_config::modules::LocalVectorConfig {
                vector_size: self.embedding_config.dimension,
                ..Default::default()
            };
            let store = Arc::new(LocalVectorStore::open_at(
                vector_temp_dir.path(),
                &local_config,
            )?);
            self.local_vector = Some(store.clone());
            self._vector_temp_dir = Some(vector_temp_dir);
            cce_orchestrator::index::VectorStore::local(store)
        };

        let bm25 = if let Some(client) = self.bm25_client.clone() {
            client
        } else {
            // Fallback: create standalone BM25 (no index data)
            let bm25_temp_dir = TempDir::new().expect("bm25 temp dir");
            let bm25_path = bm25_temp_dir.path().to_string_lossy().to_string();
            let bm25_config = Bm25Config::default()
                .enabled()
                .with_index_name("default")
                .with_index_path(&bm25_path);
            let mut bm25 = cce_storage_bm25::Bm25Client::new(bm25_config);
            bm25.connect().await.expect("Failed to connect BM25 client");
            self._bm25_temp_dir = Some(bm25_temp_dir);
            Arc::new(bm25)
        };

        // Index and query must use the same embedder, otherwise a fixture
        // without an explicit one would index metadata-only chunks and still
        // report a vector count.
        let embedder = match self.embedder.clone() {
            Some(embedder) => embedder,
            None => self.build_embedder()?,
        };

        // Create scope sharing the same project group id used during indexing.
        // Falls back to the canonical fingerprint when no index has run yet.
        let project_group_id = self.project_group_id.clone().unwrap_or_else(|| {
            let workspace_key = self
                .scenario_name
                .clone()
                .unwrap_or_else(|| self.fixture.root_path().to_string_lossy().to_string());
            generate_project_group_id(self.project_id, &workspace_key)
        });
        let scope =
            ProjectScope::new(self.project_id, project_group_id).expect("valid project scope");

        // Get relation index from the index orchestrator after indexing
        let relation_index = if let Some(ref orchestrator) = self.index_orchestrator {
            if let Some(index) = orchestrator.get_relation_index() {
                tracing::info!("Using relation index from orchestrator with relations enabled");
                Some(index)
            } else {
                tracing::warn!(
                    "No relation index available from orchestrator (relations may not be built)"
                );
                None
            }
        } else {
            tracing::warn!("Index orchestrator not initialized, creating empty relation index");
            None
        };

        // Create relation searcher with real or empty index
        let (call_chain_query, has_relations) = if let Some(index) = &relation_index {
            (Arc::new(CallChainQuery::from_index(index.clone())), true)
        } else {
            // Fallback to empty index
            let empty_index = RelationIndex::new();
            (Arc::new(CallChainQuery::from_index(empty_index)), false)
        };

        // Build capabilities based on what was actually indexed. Using the
        // storage presence (not the current query's source selection) means a
        // test may run vector-only and bm25-only queries against one shared
        // coordinator, matching production where all indexed sources stay
        // available regardless of the per-query source mix.
        let capabilities = IndexCapabilities::new()
            .with_vectors(has_vectors)
            .with_bm25(self.bm25_client.is_some())
            .with_relations(has_relations);

        // Create query coordinator using builder pattern
        let mut coordinator_builder = QueryCoordinator::builder(
            vector_store,
            embedder,
            cce_orchestrator::index::FulltextStore::local(bm25.clone()),
            call_chain_query,
            scope,
        )
        .with_capabilities(capabilities);

        if let Some(ref metadata_store) = self.relation_db {
            coordinator_builder = coordinator_builder.with_metadata_store(metadata_store.clone());
        }
        if let Some(ref registry) = self.metrics_registry {
            coordinator_builder = coordinator_builder.with_metrics_registry(registry.clone());
        }
        if let Some(ref handler) = self.rerank_handler {
            coordinator_builder = coordinator_builder.with_rerank(handler.clone());
        }

        let coordinator = coordinator_builder.build();

        self.query_coordinator = Some(coordinator);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EmptyFixture;

    #[tokio::test]
    async fn test_query_workflow_basic() {
        let fixture = EmptyFixture::new().expect("Failed to create fixture");
        fixture
            .add_file(
                "src/lib.rs",
                r#"
/// Calculate the sum of two numbers
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}
"#,
            )
            .expect("Failed to add file");

        let mut query_test =
            QueryWorkflowTest::new(fixture.into_test_fixture(), EmbeddingConfig::mock())
                .with_sources(SearchSources::none().with_bm25());

        // Index (BM25 only, no vector dependency)
        let index_result = query_test.index().await.expect("Index failed");
        assert!(index_result.total_entities >= 1);

        // Use BM25-only search (no vector dependency)
        let _query_result = query_test
            .search_bm25("add numbers", 10)
            .await
            .expect("Query failed");
    }
}
