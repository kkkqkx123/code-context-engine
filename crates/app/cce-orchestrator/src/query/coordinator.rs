//! Query coordinator for unified query interface
//!
//! Provides a single entry point for all query operations,
//! coordinating between different search strategies.
//!
//! # Design Note
//!
//! This coordinator serves as a unified facade that:
//! - Provides a single entry point for all query operations
//! - Hides internal implementation details from callers
//! - Enables future cross-searcher coordination without API changes
//!
//! While current methods delegate to internal searchers, this design
//! allows for future enhancements like:
//! - Cross-searcher result fusion
//! - Query planning and optimization
//! - Caching at the coordinator level

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use cce_codegraph::CallChainQuery;
use cce_config::project_registry::ProjectScope;
use cce_llm_client::ProductionRerankHandler;
use cce_metrics::{MetricsRegistry, QueryMetrics, SearchMetrics};
use cce_types::error::common::ErrorClassify;

use super::SearcherBuilder;
use super::cache::{CacheConfig, CacheKey, QueryCache};
use super::capabilities::IndexCapabilities;
use super::error::{QueryError, Result};
use super::relation_searcher::{PathQueryOptions, RelationQueryOptions, RelationSearcher};
use super::retry_queue::RetryQueue;
use super::searcher::Searcher;
use super::types::{AggregatedQueryOptions, QueryOptions, QueryResult};
use crate::index::vector_store::FulltextStore;
use crate::index::vector_store::RelationStore;
use crate::index::vector_store::VectorStore;

/// Query coordinator
///
/// Unified entry point for all query operations:
/// - Vector search (semantic similarity)
/// - BM25 search (keyword-based)
/// - Relation search (call chains, inheritance)
/// - Entity search (FTS5 full-text search for entity names/signatures)
/// - File search (fuzzy path matching over project files)
///
/// # Example
///
/// ```ignore
/// use code_context_engine::orchestrator::query::coordinator::QueryCoordinator;
///
/// // let coordinator = QueryCoordinator::new(searcher, relation_searcher);
/// //
/// // // Vector search
/// // let result = coordinator.search(&query_options).await?;
/// //
/// // // Relation search
/// // let callees = coordinator.get_callees(entity_id, &relation_options)?;
/// //
/// // // Entity search (FTS5)
/// // let entities = coordinator.search_entities("auth*", project_id, 20)?;
/// ```
pub struct QueryCoordinator {
    /// Unified searcher for vector/BM25 searches
    searcher: Arc<Searcher>,
    /// Relation searcher for call chain queries
    relation_searcher: Arc<RelationSearcher>,
    /// Query cache
    cache: QueryCache,
    /// Index capabilities
    capabilities: IndexCapabilities,
    /// Relation backend for FTS5 entity search (optional, local branch only)
    relation: Option<RelationStore>,
    /// Monitoring metrics (optional)
    metrics: Option<Arc<QueryMetrics>>,
    /// Retry queue for preserving query progress during service outages
    retry_queue: Arc<RetryQueue>,
    /// Bound project ID — all queries are scoped to this project
    project_id: i64,
}

/// Builder for QueryCoordinator that eliminates the need for multiple factory methods
/// and resolves clippy's too_many_arguments warning.
#[derive(Default)]
pub struct QueryCoordinatorBuilder {
    searcher_builder: Option<SearcherBuilder>,
    relation_searcher: Option<Arc<RelationSearcher>>,
    cache_config: Option<CacheConfig>,
    capabilities: Option<IndexCapabilities>,
    relation: Option<RelationStore>,
    metrics_registry: Option<Arc<MetricsRegistry>>,
    project_id: i64,
}

impl QueryCoordinatorBuilder {
    /// Create a new builder with required components
    fn new(
        vector: VectorStore,
        embedder: Arc<cce_llm_client::OpenAICompatibleProvider>,
        fulltext: FulltextStore,
        call_chain_query: Arc<CallChainQuery>,
        scope: ProjectScope,
    ) -> Self {
        let project_id = scope.project_id();
        let searcher_builder = Searcher::builder(vector, embedder, fulltext, scope);
        let relation_searcher = Arc::new(RelationSearcher::new(call_chain_query));

        Self {
            searcher_builder: Some(searcher_builder),
            relation_searcher: Some(relation_searcher),
            cache_config: None,
            capabilities: None,
            relation: None,
            metrics_registry: None,
            project_id,
        }
    }

    /// Attach the local relation database.
    ///
    /// Local-only port: FTS5 entity search runs inside the embedded branch.
    /// The handle is also forwarded to the searcher through the backend enum.
    pub fn with_sqlite(mut self, sqlite: Arc<cce_storage_metadb_sqlite::SqliteClient>) -> Self {
        let store = RelationStore::local(sqlite);
        self.relation = Some(store.clone());
        if let Some(builder) = self.searcher_builder.take() {
            self.searcher_builder = Some(builder.with_relation_store(store));
        }
        self
    }

    /// Enable the generative LLM rerank handler
    pub fn with_rerank(mut self, rerank_handler: Arc<ProductionRerankHandler>) -> Self {
        if let Some(builder) = self.searcher_builder.take() {
            self.searcher_builder = Some(builder.with_rerank(rerank_handler));
        }
        self
    }

    /// Enable search metrics collection
    pub fn with_metrics_registry(mut self, registry: Arc<MetricsRegistry>) -> Self {
        self.metrics_registry = Some(registry.clone());
        if let Some(builder) = self.searcher_builder.take() {
            self.searcher_builder =
                Some(builder.with_search_metrics(SearchMetrics::new(&registry, self.project_id)));
        }
        self
    }

    /// Configure query cache
    pub fn with_cache_config(mut self, cache_config: CacheConfig) -> Self {
        self.cache_config = Some(cache_config);
        self
    }

    /// Configure index capabilities
    pub fn with_capabilities(mut self, capabilities: IndexCapabilities) -> Self {
        self.capabilities = Some(capabilities);
        self
    }

    /// Build the final QueryCoordinator
    pub fn build(self) -> QueryCoordinator {
        let searcher = self
            .searcher_builder
            .expect("searcher_builder must be set")
            .build();
        let relation_searcher = self
            .relation_searcher
            .expect("relation_searcher must be set");

        QueryCoordinator {
            searcher: Arc::new(searcher),
            relation_searcher,
            cache: QueryCache::new(self.cache_config.unwrap_or_default()),
            capabilities: self.capabilities.unwrap_or_default(),
            relation: self.relation,
            metrics: None,
            retry_queue: Arc::new(RetryQueue::new()),
            project_id: self.project_id,
        }
    }
}

impl QueryCoordinator {
    /// Create a new builder for QueryCoordinator with required components
    pub fn builder(
        vector: VectorStore,
        embedder: Arc<cce_llm_client::OpenAICompatibleProvider>,
        fulltext: FulltextStore,
        call_chain_query: Arc<CallChainQuery>,
        scope: ProjectScope,
    ) -> QueryCoordinatorBuilder {
        QueryCoordinatorBuilder::new(vector, embedder, fulltext, call_chain_query, scope)
    }

    /// Create a new query coordinator bound to a specific project
    pub fn new(
        searcher: Arc<Searcher>,
        relation_searcher: Arc<RelationSearcher>,
        project_id: i64,
    ) -> Self {
        Self {
            searcher,
            relation_searcher,
            cache: QueryCache::new(CacheConfig::default()),
            capabilities: IndexCapabilities::default(),
            relation: None,
            metrics: None,
            retry_queue: Arc::new(RetryQueue::new()),
            project_id,
        }
    }

    /// Create a new query coordinator with cache configuration
    pub fn with_cache(
        searcher: Arc<Searcher>,
        relation_searcher: Arc<RelationSearcher>,
        cache_config: CacheConfig,
        project_id: i64,
    ) -> Self {
        Self {
            searcher,
            relation_searcher,
            cache: QueryCache::new(cache_config),
            capabilities: IndexCapabilities::default(),
            relation: None,
            metrics: None,
            retry_queue: Arc::new(RetryQueue::new()),
            project_id,
        }
    }

    /// Create a new query coordinator with capabilities
    pub fn with_capabilities(
        searcher: Arc<Searcher>,
        relation_searcher: Arc<RelationSearcher>,
        capabilities: IndexCapabilities,
        project_id: i64,
    ) -> Self {
        Self {
            searcher,
            relation_searcher,
            cache: QueryCache::new(CacheConfig::default()),
            capabilities,
            relation: None,
            metrics: None,
            retry_queue: Arc::new(RetryQueue::new()),
            project_id,
        }
    }

    /// Create a new query coordinator with all options
    pub fn with_options(
        searcher: Arc<Searcher>,
        relation_searcher: Arc<RelationSearcher>,
        cache_config: CacheConfig,
        capabilities: IndexCapabilities,
        project_id: i64,
    ) -> Self {
        Self {
            searcher,
            relation_searcher,
            cache: QueryCache::new(cache_config),
            capabilities,
            relation: None,
            metrics: None,
            retry_queue: Arc::new(RetryQueue::new()),
            project_id,
        }
    }

    /// Set monitoring metrics
    pub fn with_metrics(mut self, metrics: Arc<QueryMetrics>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Get a reference to the metrics (if enabled)
    pub fn metrics(&self) -> Option<&Arc<QueryMetrics>> {
        self.metrics.as_ref()
    }

    /// Replace the query cache with a shared instance.
    ///
    /// `QueryCache` clones share the underlying store, so handing every
    /// per-request coordinator the same clone makes them populate one cache
    /// instead of one throwaway cache each. A per-request cache always
    /// starts empty, so identical concurrent queries each re-run embedding
    /// and all queue behind the shared LLM rate limiter.
    pub fn with_shared_cache(mut self, cache: QueryCache) -> Self {
        self.cache = cache;
        self
    }

    /// Replace the retry queue with a shared instance.
    ///
    /// Coordinators are built per request, so the default queue is
    /// throwaway and retryable failures would be lost with it. Sharing the
    /// engine-owned per-project queue keeps single and aggregated queries
    /// on one retry path with the background retry processor.
    pub fn with_retry_queue(mut self, queue: Arc<RetryQueue>) -> Self {
        self.retry_queue = queue;
        self
    }

    /// Attach the local relation database for FTS5 entity search.
    ///
    /// Local-only port: FTS5 entity search runs inside the embedded branch.
    pub fn with_sqlite(mut self, sqlite: Arc<cce_storage_metadb_sqlite::SqliteClient>) -> Self {
        self.relation = Some(RelationStore::local(sqlite));
        self
    }

    /// Attach the relation backend for FTS5 entity search via enum dispatch.
    pub fn with_relation_store(mut self, store: RelationStore) -> Self {
        self.relation = Some(store);
        self
    }

    /// Get a reference to the searcher
    pub fn searcher(&self) -> &Searcher {
        &self.searcher
    }

    /// Get a reference to the relation searcher
    pub fn relation_searcher(&self) -> &RelationSearcher {
        &self.relation_searcher
    }

    /// Clone the relation searcher handle for graph queries.
    pub fn relation_searcher_arc(&self) -> Arc<RelationSearcher> {
        Arc::clone(&self.relation_searcher)
    }

    /// Get index capabilities
    pub fn capabilities(&self) -> IndexCapabilities {
        self.capabilities
    }

    /// Check if a specific index is available
    pub fn has_capability(&self, index: &str) -> bool {
        match index {
            "vector" | "vectors" => self.capabilities.has_vectors(),
            "bm25" => self.capabilities.has_bm25(),
            "relation" | "relations" => self.capabilities.has_relations(),
            _ => false,
        }
    }

    // ========== Entity Search (FTS5) ==========

    /// Search entities using FTS5 full-text search
    ///
    /// This method provides fast entity name and signature searching using SQLite FTS5.
    /// It's ideal for symbol lookup, autocomplete, and finding entities by partial names.
    ///
    /// # Arguments
    ///
    /// * `query` - FTS5 query string (supports prefix matching: `auth*`, phrases: `"test function"`, etc.)
    /// * `project_id` - Project ID to search within
    /// * `limit` - Maximum number of results to return
    ///
    /// # Returns
    ///
    /// Vector of entity records matching the search query, ordered by relevance.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Search for entities starting with "auth"
    /// let entities = coordinator.search_entities("auth*", 1, 20)?;
    ///
    /// // Search for exact phrase in signature
    /// let entities = coordinator.search_entities("\"fn test()\"", 1, 10)?;
    /// ```
    pub async fn search_entities(
        &self,
        query: &str,
        limit: i64,
    ) -> Result<Vec<cce_storage_metadb_sqlite::EntityRecord>> {
        let store = self
            .relation
            .as_ref()
            .ok_or_else(|| QueryError::Config("FTS5 entity search not configured".to_string()))?;

        let view = self.searcher.load_query_filter(self.project_id).await?;

        let start = Instant::now();
        let result =
            Self::search_entities_at_view(store, query, self.project_id, limit, &view).await;
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;

        // Record metrics for FTS5 search (using QueryMetrics)
        if let Some(metrics) = &self.metrics {
            let count = result.as_ref().map_or(0, |r| r.len());
            // For FTS5 search, always treat as cache miss since it's a direct DB query
            metrics.record_query(elapsed, false, count);
        }

        result.map_err(|e| QueryError::InvalidQuery(format!("FTS5 search failed: {e}")))
    }

    /// Entity search over the full epoch view through the relation contract.
    ///
    /// Two-stage resolution ("own first, miss → parent"): an empty
    /// own-generation result falls back to the inherited parent epoch.
    /// Excluded-file filtering runs inside the branch so only the visible
    /// view is returned.
    async fn search_entities_at_view(
        store: &RelationStore,
        query: &str,
        project_id: i64,
        limit: i64,
        view: &crate::query::filter::QueryFilter,
    ) -> std::result::Result<Vec<cce_storage_metadb_sqlite::EntityRecord>, cce_types::StorageError>
    {
        use cce_storage_common::RelationStorage;

        let entities = store
            .entities_search_fts(project_id, query, limit, view.epoch_value(), &[])
            .await?;
        if !entities.is_empty() || view.parent_epoch().is_none() {
            return Ok(entities);
        }
        store
            .entities_search_fts(
                project_id,
                query,
                limit,
                view.parent_epoch().expect("parent checked above"),
                view.excluded_files(),
            )
            .await
    }

    /// Check if entity search is available (relation store attached)
    pub fn has_fts5_search(&self) -> bool {
        self.relation.is_some()
    }

    // ========== Unified Search ==========

    /// Execute search with given options
    ///
    /// Supports all search strategies based on SearchSources:
    /// - VectorOnly: Pure vector semantic search (BM25 for consensus boost only)
    /// - HybridFusion: Dense + BM25 hybrid search with application-level fusion
    ///
    /// # Fault Tolerance
    ///
    /// If a retryable error occurs (service unavailable), the query is
    /// automatically preserved in the retry queue for later reprocessing
    /// when the service recovers. The error is still propagated to the
    /// caller so the degradation is visible.
    pub async fn search(&self, options: &QueryOptions) -> Result<QueryResult> {
        let view = self.searcher.load_query_filter(options.project_id).await?;
        self.search_with_view(options, &view).await
    }

    /// Execute a search against a pre-resolved epoch view, sharing it with
    /// the searcher so one request reads the active manifest only once.
    async fn search_with_view(
        &self,
        options: &QueryOptions,
        view: &crate::query::filter::QueryFilter,
    ) -> Result<QueryResult> {
        let start = std::time::Instant::now();

        // Check capabilities before executing
        self.check_capabilities(options)?;

        // Caching and the single-flight guard already live on the searcher:
        // `Searcher::search_with_view` routes through `QueryCache::get_or_compute`
        // keyed by the same `CacheKey` this coordinator would otherwise build.
        // Delegating to it (instead of a separate outer get/put on the same
        // shared cache) removes the redundant double-write and, importantly,
        // avoids re-entering `get_or_compute` for the same key: that method
        // takes a non-reentrant per-key mutex, so a coordinator that called it
        // and then invoked the searcher would deadlock on itself.
        let timeout_ms = options.config.timeout_ms;
        let search_result = match tokio::time::timeout(
            std::time::Duration::from_millis(timeout_ms),
            self.searcher.search_with_view(options, view),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(QueryError::Timeout { timeout_ms }),
        };

        match search_result {
            Ok(result) => {
                // Record metrics if enabled. The cache-hit flag is carried on
                // the result by the searcher's `get_or_compute`.
                if let Some(metrics) = &self.metrics {
                    let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
                    metrics.record_query(latency_ms, result.from_cache, result.items.len());
                }
                Ok(result)
            }
            Err(e) if e.is_retryable() => {
                // Preserve the query progress in the retry queue
                self.retry_queue.push(options.clone()).await;
                let queue_len = self.retry_queue.len().await;
                tracing::warn!(
                    error = %e,
                    queue_len,
                    "Retryable error, query queued for later reprocessing"
                );
                Err(e)
            }
            Err(e) => Err(e),
        }
    }

    /// Get a reference to the retry queue
    pub fn retry_queue(&self) -> &Arc<RetryQueue> {
        &self.retry_queue
    }

    /// Process queued queries that are ready for retry
    ///
    /// Called when the circuit breaker transitions to half-open or when
    /// an external signal indicates services may have recovered.
    ///
    /// Returns the number of queries that were re-attempted.
    pub async fn process_retry_queue(&self) -> usize {
        let pending = self.retry_queue.drain_ready().await;
        if pending.is_empty() {
            return 0;
        }

        let count = pending.len();
        tracing::trace!(count, "Processing retry queue");

        for options in pending {
            let view = match self.searcher.load_query_filter(options.project_id).await {
                Ok(view) => view,
                Err(error) => {
                    tracing::warn!(%error, "Failed to resolve retry query epoch");
                    self.retry_queue.push(options).await;
                    continue;
                }
            };
            match self.searcher.search_with_view(&options, &view).await {
                Ok(result) => {
                    // Warm the searcher-owned cache: it owns the read path,
                    // so the entry is visible however the coordinator was
                    // assembled, even when the coordinator handle is private.
                    self.searcher
                        .cache_handle()
                        .put_result_for_view(&options, &view, result)
                        .await;
                    tracing::trace!(
                        query = %options.query,
                        "Retry queue query succeeded"
                    );
                }
                Err(e) if e.is_retryable() => {
                    // Re-queue for next retry cycle
                    self.retry_queue.push(options).await;
                    tracing::warn!(
                        error = %e,
                        "Retry queue query failed again, re-queued"
                    );
                }
                Err(e) => {
                    tracing::error!(
                        error = %e,
                        "Retry queue query failed with non-retryable error, discarding"
                    );
                }
            }
        }

        count
    }

    /// Execute aggregated search with multiple sub-queries
    ///
    /// Max concurrently executing sub-queries in an aggregated search. Each
    /// sub-query runs the full search pipeline, so unbounded concurrency would
    /// multiply the load on Qdrant / the embedder by the sub-query count.
    const AGGREGATED_SUB_QUERY_CONCURRENCY: usize = 4;

    /// Runs sub-queries concurrently against one shared epoch view and
    /// merges the results, deduplicating by entity ID and sorting by score.
    /// Each sub-query's `weight` scales its candidates' scores before the
    /// merge, so a weight of 2.0 counts that sub-query's hits double against
    /// the rest. A failed sub-query does not abort the search; its text is
    /// reported in [`QueryResult::failed_sub_queries`] so partial degradation
    /// is visible.
    pub async fn search_aggregated(
        &self,
        agg_options: &AggregatedQueryOptions,
    ) -> Result<QueryResult> {
        let start = std::time::Instant::now();
        let view = self
            .searcher
            .load_query_filter(agg_options.project_id)
            .await?;
        let sub_queries_count = agg_options.sub_queries.len();
        for sub_query in &agg_options.sub_queries {
            if !sub_query.weight.is_finite() || sub_query.weight < 0.0 {
                return Err(QueryError::invalid(&format!(
                    "invalid sub-query weight {} for '{}'",
                    sub_query.weight, sub_query.text
                )));
            }
        }

        // Build every sub-query's options up front so the shared filters and
        // config are decomposed in one place (see `build_sub_query_options`).
        // Sub-queries run with the business threshold disabled and an inflated
        // limit; weighting happens before the single global threshold so the
        // weight decides ranking instead of threshold luck.
        let sub_options: Vec<QueryOptions> = agg_options
            .sub_queries
            .iter()
            .map(|sub_query| self.build_sub_query_options(agg_options, sub_query))
            .collect();

        // The merged union is cached under the aggregated key, which covers
        // sub-query texts, sources, weights and the global configuration.
        // Sub-queries keep their own single-query entries underneath; the
        // aggregated key differs from every sub-query key, so the nested
        // single-flight guards never re-enter on one key.
        let key = CacheKey::for_aggregated(agg_options.project_id, &view, agg_options);
        self.cache
            .get_or_compute(key, || async move {
                // Execute sub-queries concurrently, bounded by a semaphore: the results
                // stay aligned with the sub-query order (`join_all` preserves input
                // order), so the merged output is deterministic regardless of which
                // sub-query finishes first.
                let semaphore = Arc::new(tokio::sync::Semaphore::new(
                    Self::AGGREGATED_SUB_QUERY_CONCURRENCY,
                ));
                let attempts: Vec<_> = sub_options
                    .into_iter()
                    .map(|options| {
                        let semaphore = semaphore.clone();
                        let view = view.clone();
                        async move {
                            // The semaphore is never closed, so acquire cannot fail.
                            let _permit = semaphore
                                .acquire_owned()
                                .await
                                .expect("aggregated search semaphore closed");
                            self.search_with_view(&options, &view).await
                        }
                    })
                    .collect();
                let attempts = futures::future::join_all(attempts).await;

                let mut all_results: Vec<crate::query::types::SearchResult> = Vec::new();
                let mut sources_used: Vec<String> = Vec::new();
                let mut failed_sub_queries: Vec<String> = Vec::new();

                for (sub_query, attempt) in agg_options.sub_queries.iter().zip(attempts) {
                    match attempt {
                        Ok(mut result) => {
                            for source in &result.sources {
                                if !sources_used.contains(source) {
                                    sources_used.push(source.clone());
                                }
                            }
                            // Weights are validated upfront; scale here unconditionally.
                            let weight = sub_query.weight;
                            if weight != 1.0 {
                                for item in &mut result.items {
                                    item.score *= weight;
                                    item.original_score *= weight;
                                }
                            }
                            all_results.extend(result.items);
                        }
                        Err(e) => {
                            tracing::warn!(
                                query = %sub_query.text,
                                error = %e,
                                "Sub-query failed in aggregated search"
                            );
                            failed_sub_queries.push(sub_query.text.clone());
                        }
                    }
                }

                // Deduplicate by unified key (alignment key, else content weak key),
                // keeping the highest weighted score. Shares the key space with
                // hybrid fusion and single-search post-processing.
                let mut dedup: HashMap<String, crate::query::types::SearchResult> = HashMap::new();
                let mut unkeyed: Vec<crate::query::types::SearchResult> = Vec::new();
                for item in all_results {
                    let Some(key) =
                        crate::query::retrieval::post_processing::unified_dedup_key(&item)
                    else {
                        unkeyed.push(item);
                        continue;
                    };
                    dedup
                        .entry(key)
                        .and_modify(|existing| {
                            if item.score > existing.score {
                                *existing = item.clone();
                            }
                        })
                        .or_insert(item);
                }

                let mut merged: Vec<crate::query::types::SearchResult> =
                    dedup.into_values().collect();
                merged.extend(unkeyed);
                // Single global threshold on weighted scores; sub-queries run
                // threshold-free (see build_sub_query_options).
                let min_score = agg_options.global_config.result.min_score;
                if min_score > 0.0 {
                    merged.retain(|item| item.score >= min_score);
                }
                merged.sort_by(|a, b| a.cmp_score_desc_then_stable(b));

                let limit = agg_options.global_config.result.limit;
                if merged.len() > limit {
                    merged.truncate(limit);
                }

                // Relation expansion runs once on the merged hits so budget,
                // shared dedup and single wrapping observe the final Top-N instead of
                // each sub-query's intermediate list. Sub-queries run without
                // annotation when a resolver is present (see build_sub_query_options).
                if let Some(resolver) = agg_options.neighbour_resolver.as_ref() {
                    if agg_options.global_config.annotation.expansion_enabled {
                        crate::query::expansion::annotate_hits_with_resolver(
                            resolver,
                            &mut merged,
                            &agg_options.global_config.annotation,
                        )
                        .await;
                    }
                }

                let total = merged.len();

                Ok(QueryResult {
                    items: merged,
                    total,
                    elapsed_ms: start.elapsed().as_millis() as u64,
                    sources: sources_used,
                    sub_queries_count,
                    failed_sub_queries,
                    from_cache: false,
                })
            })
            .await
    }

    /// Decompose one sub-query's options out of the aggregated options.
    ///
    /// Centralizes the field mapping so the aggregated and per-sub-query
    /// option surfaces stay in sync: global filters/config/patterns are shared,
    /// the query text and sources come from the sub-query itself, and source
    /// content is always requested for aggregated results.
    /// When a neighbour resolver is present, sub-queries run without
    /// annotation and without the resolver; expansion is applied once to the
    /// merged hits in search_aggregated so budget and shared dedup observe
    /// the final Top-N.
    fn build_sub_query_options(
        &self,
        agg_options: &AggregatedQueryOptions,
        sub_query: &crate::query::types::SubQuery,
    ) -> QueryOptions {
        let filters = agg_options.filters.as_ref();
        let has_expansion = agg_options.neighbour_resolver.is_some();
        let mut config = agg_options.global_config.clone();
        if has_expansion {
            config.annotation.enable_annotation = false;
            config.annotation.expansion_enabled = false;
        }
        // Threshold-free sub-queries with inflated limit: the merged list
        // applies the single global threshold after weighting.
        config.result.min_score = 0.0;
        let inflated = agg_options
            .global_config
            .result
            .limit
            .saturating_mul(agg_options.sub_queries.len().max(1))
            .max(agg_options.global_config.result.limit);
        config.result.limit = inflated.max(10);
        QueryOptions {
            query: sub_query.text.clone(),
            project_id: agg_options.project_id,
            sources: sub_query.sources,
            config,
            directory_prefix: filters.and_then(|f| f.directory_prefix.clone()),
            exclude_content_types: filters.map_or(Vec::new(), |f| f.exclude_content_types.clone()),
            include_categories: filters.map_or(Vec::new(), |f| f.include_categories.clone()),
            exclude_categories: filters.map_or(Vec::new(), |f| f.exclude_categories.clone()),
            exclude_patterns: agg_options.exclude_patterns.clone(),
            include_patterns: agg_options.include_patterns.clone(),
            with_source: true,
            query_intent: agg_options.query_intent,
            enable_rerank: agg_options.enable_rerank,
            enable_result_filter: agg_options.enable_result_filter,
            neighbour_resolver: None,
        }
    }

    /// Check if required capabilities are available for the query
    fn check_capabilities(&self, options: &QueryOptions) -> Result<()> {
        // Check vector capability
        if options.sources.vector && !self.capabilities.has_vectors() {
            return Err(QueryError::index_not_available("vector"));
        }

        // Check BM25 capability
        if options.sources.bm25 && !self.capabilities.has_bm25() {
            return Err(QueryError::index_not_available("bm25"));
        }

        Ok(())
    }

    /// Invalidate all caches
    pub async fn invalidate_cache(&self) {
        self.cache.invalidate_all().await;
    }

    // ========== File Search ==========
    // Fuzzy path matching over project files, independent of every index.
    // The local branch walks the project directory directly and observes the
    // latest filesystem state with no generation filtering. The remote branch
    // holds no project files, so it ranks the indexed file rows of the active
    // epoch view with the same matcher and ordering instead.

    /// Search file paths matching `query`, returning at most `limit` paths.
    ///
    /// Paths are project-relative and ordered by match quality with a lexical
    /// tiebreak. No index capability is required: files exist regardless of
    /// which indexes are enabled.
    pub async fn search_file_paths(&self, query: &str, limit: usize) -> Result<Vec<String>> {
        let hits = self.search_files(query, limit).await?;
        Ok(hits.into_iter().map(|hit| hit.path).collect())
    }

    /// Search files matching `query`, attaching size and line count.
    ///
    /// Shares the matcher and ordering with [`Self::search_file_paths`];
    /// files unreadable at stat time are skipped. Size and line metadata is
    /// present only on the local branch; remote hits carry paths alone.
    pub async fn search_files(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<super::files::FileHit>> {
        let store = self.relation.as_ref().ok_or_else(|| {
            QueryError::config("file search requires a relation store for project lookup")
        })?;
        if store.is_local() {
            let root = self.resolve_file_search_root().await?;
            let query = query.to_string();
            return tokio::task::spawn_blocking(move || {
                super::files::search_files(&root, &query, limit)
            })
            .await
            .map_err(|e| QueryError::storage(&format!("File search task failed: {e}")))?;
        }
        self.search_files_from_index(store, query, limit).await
    }

    /// Index-based file search for branches without live project files.
    ///
    /// Ranks the indexed file rows of the active epoch view with the same
    /// matcher and ordering as the live walk; excluded files of an inherited
    /// generation never surface.
    async fn search_files_from_index(
        &self,
        store: &RelationStore,
        query: &str,
        limit: usize,
    ) -> Result<Vec<super::files::FileHit>> {
        use cce_storage_common::RelationStorage;

        let view = self.searcher.load_query_filter(self.project_id).await?;
        let rows = store
            .files_list(self.project_id, &view.epochs())
            .await
            .map_err(|e| QueryError::storage(&format!("File search failed: {e}")))?;
        let excluded: std::collections::HashSet<&str> =
            view.excluded_files().iter().map(String::as_str).collect();
        let paths: Vec<String> = rows
            .into_iter()
            .map(|row| row.path)
            .filter(|path| !excluded.contains(path.as_str()))
            .collect();
        let query = query.to_string();
        tokio::task::spawn_blocking(move || super::files::rank_file_paths(&paths, &query, limit))
            .await
            .map_err(|e| QueryError::storage(&format!("File search task failed: {e}")))?
    }

    /// Resolve the project directory file search walks.
    async fn resolve_file_search_root(&self) -> Result<std::path::PathBuf> {
        use cce_storage_common::RelationStorage;
        let store = self.relation.as_ref().ok_or_else(|| {
            QueryError::config("file search requires a relation store for project lookup")
        })?;
        if !store.is_local() {
            return Err(QueryError::config(
                "file search is unavailable on the remote relation branch",
            ));
        }
        let record = store
            .project_record(self.project_id)
            .await
            .map_err(|e| QueryError::storage(&format!("Failed to resolve project root: {e}")))?
            .ok_or_else(|| {
                QueryError::not_found(format!("Project {} not found", self.project_id))
            })?;
        Ok(std::path::PathBuf::from(record.root_path))
    }

    // ========== Relation Queries ==========
    // These methods delegate to RelationSearcher for consistent behavior

    /// Get callees across every relation domain (raw diagnostic accessor).
    ///
    /// Not scoped to the call domain: prefer [`Self::get_callees_paginated`]
    /// with [`RelationQueryOptions`] when the result is presented as call
    /// semantics.
    ///
    /// Missing entities report not found; existing entities without outgoing
    /// edges return an empty list.
    pub fn get_callees(
        &self,
        entity_id: cce_types::EntityId,
    ) -> Result<Vec<cce_types::ResolvedRelation>> {
        if !self.capabilities.has_relations() {
            return Err(QueryError::index_not_available("relation"));
        }
        self.relation_searcher
            .get_callees_checked(entity_id)
            .map_err(QueryError::from)
    }

    /// Get callers across every relation domain (raw diagnostic accessor).
    ///
    /// Not scoped to the call domain: prefer
    /// [`Self::get_callers_paginated`] with [`RelationQueryOptions`] when the
    /// result is presented as call semantics.
    pub fn get_callers(&self, entity_id: cce_types::EntityId) -> Result<Vec<cce_types::EntityId>> {
        if !self.capabilities.has_relations() {
            return Err(QueryError::index_not_available("relation"));
        }
        Ok(self.relation_searcher.get_callers(entity_id))
    }

    /// Get callees with pagination
    pub fn get_callees_paginated(
        &self,
        entity_id: cce_types::EntityId,
        options: &RelationQueryOptions,
    ) -> Result<Vec<cce_types::ResolvedRelation>> {
        if !self.capabilities.has_relations() {
            return Err(QueryError::index_not_available("relation"));
        }
        Ok(self
            .relation_searcher
            .get_callees_paginated(entity_id, options))
    }

    /// Get callers with pagination.
    ///
    /// Filtering is driven by [`RelationQueryOptions`] (relation domains,
    /// external edges, file scope); each returned relation is the edge by
    /// which that caller calls `entity_id`, mirroring
    /// [`Self::get_callees_paginated`].
    pub fn get_callers_paginated(
        &self,
        entity_id: cce_types::EntityId,
        options: &RelationQueryOptions,
    ) -> Result<Vec<cce_types::ResolvedRelation>> {
        if !self.capabilities.has_relations() {
            return Err(QueryError::index_not_available("relation"));
        }
        Ok(self
            .relation_searcher
            .get_callers_paginated(entity_id, options))
    }

    /// Query forward call chain (caller -> callees)
    pub fn query_forward(
        &self,
        entity_id: cce_types::EntityId,
        options: &RelationQueryOptions,
    ) -> Result<Vec<cce_codegraph::CallChainNode>> {
        if !self.capabilities.has_relations() {
            return Err(QueryError::index_not_available("relation"));
        }
        self.relation_searcher.query_forward(entity_id, options)
    }

    /// Query backward call chain (callee -> callers)
    pub fn query_backward(
        &self,
        entity_id: cce_types::EntityId,
        options: &RelationQueryOptions,
    ) -> Result<Vec<cce_codegraph::CallChainNode>> {
        if !self.capabilities.has_relations() {
            return Err(QueryError::index_not_available("relation"));
        }
        self.relation_searcher.query_backward(entity_id, options)
    }

    /// Query forward call chain with pagination
    pub fn query_forward_paginated(
        &self,
        entity_id: cce_types::EntityId,
        options: &RelationQueryOptions,
    ) -> Result<Vec<cce_codegraph::CallChainNode>> {
        if !self.capabilities.has_relations() {
            return Err(QueryError::index_not_available("relation"));
        }
        self.relation_searcher
            .query_forward_paginated(entity_id, options)
    }

    /// Query backward call chain with pagination
    pub fn query_backward_paginated(
        &self,
        entity_id: cce_types::EntityId,
        options: &RelationQueryOptions,
    ) -> Result<Vec<cce_codegraph::CallChainNode>> {
        if !self.capabilities.has_relations() {
            return Err(QueryError::index_not_available("relation"));
        }
        self.relation_searcher
            .query_backward_paginated(entity_id, options)
    }

    /// Find call chain path between two functions
    pub fn find_path(
        &self,
        start_id: cce_types::EntityId,
        end_id: cce_types::EntityId,
        options: &PathQueryOptions,
    ) -> Result<Option<Vec<cce_codegraph::CallChainNode>>> {
        if !self.capabilities.has_relations() {
            return Err(QueryError::index_not_available("relation"));
        }
        self.relation_searcher.find_path(start_id, end_id, options)
    }

    // ========== Inheritance Queries ==========

    /// Get base classes (classes this class extends)
    pub fn get_base_classes(
        &self,
        class_id: cce_types::EntityId,
    ) -> Result<Vec<cce_types::EntityId>> {
        if !self.capabilities.has_relations() {
            return Err(QueryError::index_not_available("relation"));
        }
        Ok(self.relation_searcher.get_base_classes(class_id))
    }

    /// Get derived classes (classes that extend this class)
    pub fn get_derived_classes(
        &self,
        class_id: cce_types::EntityId,
    ) -> Result<Vec<cce_types::EntityId>> {
        if !self.capabilities.has_relations() {
            return Err(QueryError::index_not_available("relation"));
        }
        Ok(self.relation_searcher.get_derived_classes(class_id))
    }

    /// Get implemented interfaces
    pub fn get_implemented_interfaces(
        &self,
        class_id: cce_types::EntityId,
    ) -> Result<Vec<cce_types::EntityId>> {
        if !self.capabilities.has_relations() {
            return Err(QueryError::index_not_available("relation"));
        }
        Ok(self.relation_searcher.get_implemented_interfaces(class_id))
    }

    /// Get implementing classes (classes that implement this interface)
    pub fn get_implementing_classes(
        &self,
        interface_id: cce_types::EntityId,
    ) -> Result<Vec<cce_types::EntityId>> {
        if !self.capabilities.has_relations() {
            return Err(QueryError::index_not_available("relation"));
        }
        Ok(self
            .relation_searcher
            .get_implementing_classes(interface_id))
    }

    /// Get inheritance hierarchy (all ancestors), each with its hop distance.
    pub fn get_inheritance_hierarchy(
        &self,
        class_id: cce_types::EntityId,
        max_depth: usize,
    ) -> Result<Vec<(cce_types::EntityId, usize)>> {
        if !self.capabilities.has_relations() {
            return Err(QueryError::index_not_available("relation"));
        }
        Ok(self
            .relation_searcher
            .get_inheritance_hierarchy(class_id, max_depth))
    }

    /// Get all derived classes (transitive closure), each with its hop distance.
    pub fn get_all_derived_classes(
        &self,
        class_id: cce_types::EntityId,
        max_depth: usize,
    ) -> Result<Vec<(cce_types::EntityId, usize)>> {
        if !self.capabilities.has_relations() {
            return Err(QueryError::index_not_available("relation"));
        }
        Ok(self
            .relation_searcher
            .get_all_derived_classes(class_id, max_depth))
    }

    // Graph-shaped answers stay on the dedicated graph retrieval path over the
    // relation snapshot. Handlers construct `GraphService` directly from the
    // relation searcher, so no graph wrapper lives here.
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_coordinator_creation() {
        // This test would require mock components
        // For now, we just verify the structure compiles
    }
}
