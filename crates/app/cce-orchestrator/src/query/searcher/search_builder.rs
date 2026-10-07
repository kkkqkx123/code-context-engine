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

use crate::index::vector_store::VectorStore;
use crate::query::boost::SummaryBoost;
use crate::query::ranking::{LlmReranker, PluginReranker, ScoreSorter, ThresholdFilter};
use crate::query::retrieval::post_processing::GlobFilter;
use cce_llm_client::OpenAICompatibleProvider;
use cce_llm_client::ProductionRerankHandler;
use cce_metrics::SearchMetrics;

use cce_storage_bm25::Bm25Client;
use cce_storage_sqlite::SqliteClient;

use super::searcher_core::Searcher;

pub struct SearcherBuilder {
    vector: VectorStore,
    embedder: Arc<OpenAICompatibleProvider>,
    bm25: Arc<tokio::sync::Mutex<Bm25Client>>,
    sqlite: Option<Arc<SqliteClient>>,
    rerank_handler: Option<Arc<ProductionRerankHandler>>,
    plugin_rerank_plugins: Vec<std::sync::Arc<dyn cce_plugin::CodePlugin>>,
    plugin_registry: Option<Arc<cce_plugin::PluginRegistry>>,
    enable_summary_boost: bool,
    scope: ProjectScope,
    search_metrics: Option<Arc<SearchMetrics>>,
}

impl SearcherBuilder {
    /// Create a new builder with required components and project scope
    pub(crate) fn new(
        vector: VectorStore,
        embedder: Arc<OpenAICompatibleProvider>,
        bm25: Arc<tokio::sync::Mutex<Bm25Client>>,
        scope: ProjectScope,
    ) -> Self {
        Self {
            vector,
            embedder,
            bm25,
            sqlite: None,
            rerank_handler: None,
            plugin_rerank_plugins: Vec::new(),
            plugin_registry: None,
            enable_summary_boost: false,
            scope,
            search_metrics: None,
        }
    }

    /// Enable SQLite support for chunk content lookup
    pub fn with_sqlite(mut self, sqlite: Arc<SqliteClient>) -> Self {
        self.sqlite = Some(sqlite);
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

    /// Enable summary boost enhancement support
    pub fn with_summary_boost(mut self) -> Self {
        self.enable_summary_boost = true;
        self
    }

    /// Enable search metrics collection
    pub fn with_search_metrics(mut self, metrics: Arc<SearchMetrics>) -> Self {
        self.search_metrics = Some(metrics);
        self
    }

    /// Build the Searcher instance
    pub fn build(self) -> Searcher {
        // Vector backend (local or Qdrant) is shared by dense retrieval and
        // summary boost; VectorStore is cheap to clone (holds Arc inside).
        let vector = self.vector.clone();

        // Memoize query embeddings: dense retrieval and summary boost embed
        // the same query text within one search flow; the shared wrapper
        // collapses those into a single remote call.
        let _embedder = Arc::new(crate::query::cached_embedder::CachedEmbedder::new(
            self.embedder.clone(),
        ));

        // Create summary boost if enabled
        let summary_boost = if self.enable_summary_boost {
            Some(Arc::new(SummaryBoost::new(
                vector.clone(),
                self.embedder.clone(),
                self.scope.project_group_id().to_string(),
            )))
        } else {
            None
        };

        Searcher {
            vector,
            embedder: self.embedder.clone(),
            bm25: self.bm25,
            sqlite: self.sqlite,
            reranker: Arc::new(LlmReranker::new(self.rerank_handler)),
            plugin_reranker: Arc::new(PluginReranker::new(self.plugin_rerank_plugins)),
            plugin_registry: self.plugin_registry,
            score_sorter: Arc::new(ScoreSorter::new()),
            threshold_filter: Arc::new(ThresholdFilter::new()),
            glob_filter: Arc::new(GlobFilter::new()),
            summary_boost,
            scope: self.scope,
            search_metrics: self.search_metrics,
        }
    }
}
