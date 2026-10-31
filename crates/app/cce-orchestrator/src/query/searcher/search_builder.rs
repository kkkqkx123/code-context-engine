//! Unified searcher implementation
//!
//! Provides a unified search interface that combines vector retrieval,
//! BM25 enhancement, and result ranking into a cohesive search flow.
//!
//! # Architecture
//!
//! The searcher delegates to specialized components:
//! - ResultProcessor: Ranking, filtering, and threshold application

use std::sync::Arc;

use cce_config::project_registry::ProjectScope;

use crate::index::vector_store::{FulltextStore, RelationStore, VectorStore};
use crate::query::ranking::{LlmReranker, PluginReranker, ScoreSorter, ThresholdFilter};
use crate::query::retrieval::post_processing::GlobFilter;
use cce_llm_client::OpenAICompatibleProvider;
use cce_llm_client::ProductionRerankHandler;
use cce_metrics::SearchMetrics;

use super::searcher_core::Searcher;

pub struct SearcherBuilder {
    vector: VectorStore,
    embedder: Arc<OpenAICompatibleProvider>,
    fulltext: FulltextStore,
    relation: Option<RelationStore>,
    rerank_handler: Option<Arc<ProductionRerankHandler>>,
    plugin_rerank_plugins: Vec<std::sync::Arc<dyn cce_plugin::CodePlugin>>,
    plugin_registry: Option<Arc<cce_plugin::PluginRegistry>>,
    scope: ProjectScope,
    search_metrics: Option<Arc<SearchMetrics>>,
    cache: Option<crate::query::cache::QueryCache>,
}

impl SearcherBuilder {
    /// Create a new builder with required components and project scope
    pub(crate) fn new(
        vector: VectorStore,
        embedder: Arc<OpenAICompatibleProvider>,
        fulltext: FulltextStore,
        scope: ProjectScope,
    ) -> Self {
        Self {
            vector,
            embedder,
            fulltext,
            relation: None,
            rerank_handler: None,
            plugin_rerank_plugins: Vec::new(),
            plugin_registry: None,
            scope,
            search_metrics: None,
            cache: None,
        }
    }

    /// Attach the local relation database for chunk content lookup.
    ///
    /// Local-only port: the embedded branch owns per-project database files,
    /// so only a local handle can be attached here. Callers holding the
    /// backend enum use [`Self::with_relation_store`] instead.
    pub fn with_sqlite(mut self, sqlite: Arc<cce_storage_metadb_sqlite::SqliteClient>) -> Self {
        self.relation = Some(RelationStore::local(sqlite));
        self
    }

    /// Enable the relation backend via enum dispatch.
    ///
    /// Reads go through the relation contract, so both branches share one
    /// read path and branch selection stays inside the enum.
    pub fn with_relation_store(mut self, store: RelationStore) -> Self {
        self.relation = Some(store);
        self
    }

    /// Enable reranking support
    ///
    /// Note: Reranking is still controlled by query-level config.enable_reranking.
    /// This method only provides the rerank handler capability.
    pub fn with_rerank(mut self, rerank_handler: Arc<ProductionRerankHandler>) -> Self {
        self.rerank_handler = Some(rerank_handler);
        self
    }

    /// Enable plugin-based reranking.
    ///
    /// `plugins` are `Rerank`-capability plugins (already matched by the
    /// registry). The execution order vs. the LLM reranker is controlled by
    /// `config.rerank.order`.
    pub fn with_plugin_rerank(
        mut self,
        plugins: Vec<std::sync::Arc<dyn cce_plugin::CodePlugin>>,
    ) -> Self {
        self.plugin_rerank_plugins = plugins;
        self
    }

    /// Attach the plugin registry for the query-side capabilities
    /// (`QueryRewrite` / `Fusion` / `ResultFilter`).
    pub fn with_plugin_registry(mut self, registry: Arc<cce_plugin::PluginRegistry>) -> Self {
        self.plugin_registry = Some(registry);
        self
    }

    /// Enable search metrics collection
    pub fn with_search_metrics(mut self, metrics: Arc<SearchMetrics>) -> Self {
        self.search_metrics = Some(metrics);
        self
    }

    /// Attach a shared query result cache.
    ///
    /// Clones share the underlying store, so one project-wide instance makes
    /// every request's cache lookups observe each other's inserts. Without
    /// this the searcher falls back to a private cache, which is equivalent
    /// to no cache at all for a per-request searcher.
    pub fn with_shared_cache(mut self, cache: crate::query::cache::QueryCache) -> Self {
        self.cache = Some(cache);
        self
    }

    /// Build the Searcher instance
    pub fn build(self) -> Searcher {
        // Vector backend (local or Qdrant) backs dense retrieval;
        // VectorStore is cheap to clone (holds Arc inside).
        let vector = self.vector.clone();

        // Memoize query embeddings so repeated embedding calls within one
        // search flow collapse into a single remote call.
        let embedder = Arc::new(crate::query::cached_embedder::CachedEmbedder::new(
            self.embedder.clone(),
        ));

        Searcher {
            vector,
            embedder,
            fulltext: self.fulltext,
            relation: self.relation,
            reranker: Arc::new(LlmReranker::new(self.rerank_handler)),
            plugin_reranker: Arc::new(PluginReranker::new(self.plugin_rerank_plugins)),
            plugin_registry: self.plugin_registry,
            score_sorter: Arc::new(ScoreSorter::new()),
            threshold_filter: Arc::new(ThresholdFilter::new()),
            glob_filter: Arc::new(GlobFilter::new()),
            scope: self.scope,
            search_metrics: self.search_metrics,
            cache: self
                .cache
                .unwrap_or_else(|| crate::query::cache::QueryCache::new(Default::default())),
        }
    }
}
