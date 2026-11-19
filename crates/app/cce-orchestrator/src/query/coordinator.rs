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

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use cce_codegraph::CallChainQuery;
use cce_config::project_registry::ProjectScope;
use cce_llm_client::ProductionRerankHandler;
use cce_metrics::{MetricsRegistry, QueryMetrics, SearchMetrics};
use cce_types::error::common::ErrorClassify;

use super::SearcherBuilder;
use super::aggregation::{
    aggregate_dedup_key, align_sub_query_scores, post_processing_reserve_ms, validate_plan,
};
use super::cache::{CacheConfig, CacheKey, QueryCache};
use super::capabilities::IndexCapabilities;
use super::error::{QueryError, Result};
use super::relation_searcher::{PathQueryOptions, RelationQueryOptions, RelationSearcher};
use super::retry_queue::RetryQueue;
use super::searcher::Searcher;
use super::types::{
    AggregatedQueryOptions, QueryOptions, QueryResult, QueryStageTimings, SearchSources, SubQuery,
    SubQueryStat, SubQueryStatus,
};
use crate::index::vector_store::FulltextStore;
use crate::index::vector_store::MetadataStore;
use crate::index::vector_store::VectorStore;

/// Combine two sub-query contributions that share one dedup key.
///
/// `Max` keeps the strongest contribution (historical behaviour). `NoisyOr`
/// treats each contribution as independent evidence and accumulates it:
/// `1 - (1 - a)(1 - b)`, so a second sub-query hitting the same entity always
/// raises its score while the result stays in `[0, 1]` — which is what keeps
/// the single global `result.min_score` comparable with single-query scores.
/// Both strategies sanitize contributions into the same bounded domain; API
/// weights are restricted to `(0, 1]` so evidence stays comparable.
///
/// Public so offline evaluations can measure the shipped formula instead of
/// keeping a copy that drifts: the benchmark baseline has to move when this
/// changes.
pub fn combine_aggregate_scores(
    current: f32,
    contribution: f32,
    strategy: cce_config::modules::search::AggregationScoreStrategy,
) -> f32 {
    use cce_config::modules::search::AggregationScoreStrategy;
    let current = sanitize_aggregate_score(current);
    let contribution = sanitize_aggregate_score(contribution);
    match strategy {
        AggregationScoreStrategy::Max => current.max(contribution),
        AggregationScoreStrategy::NoisyOr => {
            let acc = 1.0 - (1.0 - current) * (1.0 - contribution);
            acc.clamp(0.0, 1.0)
        }
    }
}

fn sanitize_aggregate_score(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 0.999)
    } else {
        0.0
    }
}

/// Collapse identical sub-queries while preserving caller order.
///
/// Repeating one query must not be counted as independent evidence, and it
/// must not pay the retrieval cost twice.
fn normalize_sub_queries(sub_queries: &[SubQuery]) -> Vec<SubQuery> {
    let mut seen = HashSet::new();
    sub_queries
        .iter()
        .filter(|sub| {
            let key = (sub.text.clone(), sub.sources, sub.weight.to_bits());
            seen.insert(key)
        })
        .cloned()
        .collect()
}

/// Union the `sources` of two hits merged onto one dedup key.
fn merge_sources(
    target: &mut crate::query::types::SearchResult,
    other: &crate::query::types::SearchResult,
) {
    for source in &other.sources {
        if !target.sources.contains(source) {
            target.sources.push(source.clone());
        }
    }
}

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
/// // let coordinator = QueryCoordinator::builder(
/// //     vector, embedder, fulltext, call_chain_query, scope,
/// // )
/// // .with_capabilities(capabilities)
/// // .build();
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
#[derive(Clone)]
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
    metadata_store: Option<MetadataStore>,
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
    shared_cache: Option<QueryCache>,
    capabilities: Option<IndexCapabilities>,
    metadata_store: Option<MetadataStore>,
    metrics_registry: Option<Arc<MetricsRegistry>>,
    retry_queue: Option<Arc<RetryQueue>>,
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
            shared_cache: None,
            capabilities: None,
            metadata_store: None,
            metrics_registry: None,
            retry_queue: None,
            project_id,
        }
    }

    /// Attach the relation backend via enum dispatch.
    ///
    /// Reads go through the relation contract; the handle is also forwarded
    /// to the searcher through the backend enum.
    pub fn with_metadata_store(mut self, store: MetadataStore) -> Self {
        self.metadata_store = Some(store.clone());
        if let Some(builder) = self.searcher_builder.take() {
            self.searcher_builder = Some(builder.with_metadata_store(store));
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

    /// Enable plugin-based reranking (forwarded to the searcher).
    pub fn with_plugin_rerank(
        mut self,
        plugins: Vec<std::sync::Arc<dyn cce_plugin::CodePlugin>>,
    ) -> Self {
        if let Some(builder) = self.searcher_builder.take() {
            self.searcher_builder = Some(builder.with_plugin_rerank(plugins));
        }
        self
    }

    /// Attach the plugin registry for query-side capabilities (forwarded).
    pub fn with_plugin_registry(mut self, registry: Arc<cce_plugin::PluginRegistry>) -> Self {
        if let Some(builder) = self.searcher_builder.take() {
            self.searcher_builder = Some(builder.with_plugin_registry(registry));
        }
        self
    }

    /// Attach one shared query result cache to the searcher and the
    /// coordinator. Single-query and aggregated keys never collide, so both
    /// layers can use the same single-flight store.
    pub fn with_shared_cache(mut self, cache: QueryCache) -> Self {
        self.shared_cache = Some(cache.clone());
        if let Some(builder) = self.searcher_builder.take() {
            self.searcher_builder = Some(builder.with_shared_cache(cache));
        }
        self
    }

    /// Override the relation searcher (e.g. snapshot-backed).
    pub fn with_relation_searcher(mut self, searcher: Arc<RelationSearcher>) -> Self {
        self.relation_searcher = Some(searcher);
        self
    }

    /// Attach the engine-owned retry queue instead of a throwaway one.
    pub fn with_retry_queue(mut self, queue: Arc<RetryQueue>) -> Self {
        self.retry_queue = Some(queue);
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

        let cache = self
            .shared_cache
            .unwrap_or_else(|| QueryCache::new(self.cache_config.unwrap_or_default()));

        QueryCoordinator {
            searcher: Arc::new(searcher),
            relation_searcher,
            cache,
            capabilities: self.capabilities.unwrap_or_default(),
            metadata_store: self.metadata_store,
            metrics: None,
            retry_queue: self
                .retry_queue
                .unwrap_or_else(|| Arc::new(RetryQueue::new())),
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

    /// Attach the relation backend for FTS5 entity search via enum dispatch.
    pub fn with_metadata_store(mut self, store: MetadataStore) -> Self {
        self.metadata_store = Some(store);
        self
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
    ) -> Result<Vec<cce_storage_common::metadb::EntityRecord>> {
        let store = self
            .metadata_store
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
        store: &MetadataStore,
        query: &str,
        project_id: i64,
        limit: i64,
        view: &crate::query::filter::QueryFilter,
    ) -> std::result::Result<Vec<cce_storage_common::metadb::EntityRecord>, cce_types::StorageError>
    {
        use cce_storage_common::MetadataStorage;

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
        self.metadata_store.is_some()
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
        let filter_started = Instant::now();
        let view = self.searcher.load_query_filter(options.project_id).await?;
        let filter_ms = filter_started.elapsed().as_millis() as u64;
        let mut result = self.search_with_view(options, &view).await?;
        result.stages.filter_ms = filter_ms;
        Ok(result)
    }

    /// Execute a search against a pre-resolved epoch view, sharing it with
    /// the searcher so one request reads the active manifest only once.
    async fn search_with_view(
        &self,
        options: &QueryOptions,
        view: &crate::query::filter::QueryFilter,
    ) -> Result<QueryResult> {
        self.search_with_view_inner(options, view, true).await
    }

    /// Execute one sub-query of an aggregated request.
    ///
    /// Identical to [`Self::search_with_view`] except that a retryable failure
    /// is not queued for reprocessing: a sub-query is only meaningful as part
    /// of its merge, so replaying it alone would warm a result the caller never
    /// asked for and multiply one aggregated failure into N queued retries.
    /// The aggregated request as a whole is not retried either — its partial
    /// output is a valid answer.
    async fn search_sub_query_with_view(
        &self,
        options: &QueryOptions,
        view: &crate::query::filter::QueryFilter,
    ) -> Result<QueryResult> {
        self.search_with_view_inner(options, view, false).await
    }

    async fn search_with_view_inner(
        &self,
        options: &QueryOptions,
        view: &crate::query::filter::QueryFilter,
        enqueue_retry: bool,
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
            Err(e) if e.is_retryable() && enqueue_retry => {
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

    /// Upper bound for one sub-query limit after inflation. Keeps the merged
    /// recall cost predictable when many sub-queries run at once.
    const MAX_SUBQUERY_LIMIT: usize = 100;

    /// Runs sub-queries concurrently against one shared epoch view and
    /// merges the results, deduplicating by entity ID and sorting by score.
    /// Each sub-query's weight must be in `(0, 1]` so all contributions stay
    /// in the same score domain as the global threshold. A failed sub-query
    /// does not abort a partially successful search; its text is reported in
    /// [`QueryResult::failed_sub_queries`]. When every sub-query fails the
    /// request itself fails.
    pub async fn search_aggregated(
        &self,
        agg_options: &AggregatedQueryOptions,
    ) -> Result<QueryResult> {
        let start = std::time::Instant::now();
        let view = self
            .searcher
            .load_query_filter(agg_options.project_id)
            .await?;
        let planned_sub_queries = agg_options.sub_queries.len();
        // Shape, weight and feasibility checks live in the aggregation module so
        // the HTTP surface, the engine and the CLI cannot drift apart. The rule
        // that rejects weights below the result threshold needs the effective
        // project config, which only reaches the engine here.
        validate_plan(
            &agg_options.sub_queries,
            agg_options.global_config.result.min_score,
            agg_options.global_config.aggregation.score_strategy,
        )
        .map_err(|message| QueryError::invalid(&message))?;

        let sub_queries = normalize_sub_queries(&agg_options.sub_queries);
        let executed_sub_queries = sub_queries.len();
        let mut planned_options = agg_options.clone();
        planned_options.sub_queries = sub_queries.clone();
        if cce_utils::text::is_blank(&planned_options.original_query) {
            planned_options.original_query = sub_queries
                .iter()
                .map(|sub| sub.text.as_str())
                .collect::<Vec<_>>()
                .join(" + ");
        }

        // Build every sub-query's options up front so the shared filters and
        // config are decomposed in one place (see `build_sub_query_options`).
        // Sub-queries run with the business threshold disabled and an inflated
        // limit; weighting happens before the single global threshold so the
        // weight decides ranking instead of threshold luck.
        let sub_options: Vec<QueryOptions> = sub_queries
            .iter()
            .map(|sub_query| self.build_sub_query_options(&planned_options, sub_query))
            .collect();
        let merge_options = self.build_merge_query_options(&planned_options);

        // The merged union is cached under the aggregated key, which covers
        // normalized sub-query identities, weights, the rerank context and
        // the relation snapshot. Sub-queries keep their own single-query
        // entries underneath; the aggregated key differs from every
        // sub-query key, so the nested single-flight guards never re-enter.
        let key = CacheKey::for_aggregated(planned_options.project_id, &view, &planned_options);
        let timeout_ms = planned_options.global_config.timeout_ms;
        // Absolute deadline of the whole request. Sub-queries stop earlier by
        // the post-merge reserve so the merge, rerank and expansion still have
        // a budget: a slow recall then yields a partial answer instead of the
        // request-level deadline firing mid-merge and discarding everything the
        // surviving sub-queries already produced.
        let deadline =
            tokio::time::Instant::now() + std::time::Duration::from_millis(timeout_ms.max(1));
        let sub_query_deadline = deadline
            .checked_sub(std::time::Duration::from_millis(
                post_processing_reserve_ms(timeout_ms),
            ))
            .unwrap_or(deadline);
        let aggregated = self.cache.get_or_compute(key, || async move {
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
                        let started = std::time::Instant::now();
                        let outcome = tokio::time::timeout_at(
                            sub_query_deadline,
                            self.search_sub_query_with_view(&options, &view),
                        )
                        .await;
                        (outcome, started.elapsed().as_millis() as u64)
                    }
                })
                .collect();
            let attempts = futures::future::join_all(attempts).await;

            let mut all_results: Vec<(usize, crate::query::types::SearchResult)> = Vec::new();
            let mut sources_used: Vec<String> = Vec::new();
            let mut failed_sub_queries: Vec<String> = Vec::new();
            let mut aggregate_stages = QueryStageTimings::default();
            let mut sub_query_cache_hits = 0;
            let mut succeeded_sub_queries = 0;
            let mut stats: Vec<SubQueryStat> = Vec::with_capacity(executed_sub_queries);

            for (sub_index, (sub_query, (attempt, elapsed_ms))) in
                sub_queries.iter().zip(attempts).enumerate()
            {
                let mut stat = SubQueryStat {
                    index: sub_index,
                    text: sub_query.text.clone(),
                    status: SubQueryStatus::Succeeded,
                    candidates: 0,
                    contributed: 0,
                    elapsed_ms,
                    error: None,
                };
                match attempt {
                    Ok(Ok(mut result)) => {
                        aggregate_stages.embedding_ms =
                            aggregate_stages.embedding_ms.saturating_add(result.stages.embedding_ms);
                        aggregate_stages.vector_search_ms = aggregate_stages
                            .vector_search_ms
                            .saturating_add(result.stages.vector_search_ms);
                        aggregate_stages.bm25_retrieval_ms = aggregate_stages
                            .bm25_retrieval_ms
                            .saturating_add(result.stages.bm25_retrieval_ms);
                        aggregate_stages.fusion_ms =
                            aggregate_stages.fusion_ms.saturating_add(result.stages.fusion_ms);
                        aggregate_stages.post_process_ms = aggregate_stages
                            .post_process_ms
                            .saturating_add(result.stages.post_process_ms);
                        if result.from_cache {
                            sub_query_cache_hits += 1;
                        }
                        for source in &result.sources {
                            if !sources_used.contains(source) {
                                sources_used.push(source.clone());
                            }
                        }
                        stat.candidates = result.items.len();
                        if result.items.is_empty() {
                            // A sub-query that recalled nothing contributes
                            // nothing; reporting it as succeeded would hide a
                            // dead intent behind a healthy counter.
                            stat.status = SubQueryStatus::Empty;
                            stats.push(stat);
                            continue;
                        }
                        succeeded_sub_queries += 1;
                        // Recall normalizes a single-path list (head 1.0) but
                        // leaves a hybrid list carrying the weighted fusion sum
                        // (head at the path weight), so align every list to a
                        // shared ceiling before weights and merging apply.
                        align_sub_query_scores(&mut result.items);
                        let weight = sub_query.weight;
                        for item in &mut result.items {
                            item.score = sanitize_aggregate_score(item.score * weight);
                            item.original_score = item.score;
                        }
                        all_results.extend(result.items.into_iter().map(|item| (sub_index, item)));
                        stats.push(stat);
                    }
                    Ok(Err(e)) => {
                        tracing::warn!(
                            query = %sub_query.text,
                            error = %e,
                            "Sub-query failed in aggregated search"
                        );
                        stat.status = SubQueryStatus::Failed;
                        stat.error = Some(e.to_string());
                        failed_sub_queries.push(sub_query.text.clone());
                        stats.push(stat);
                    }
                    Err(_elapsed) => {
                        // The request keeps its partial answer: a deadline hit
                        // by one sub-query is the same degradation as a failed
                        // sub-query, not a reason to discard the merge.
                        tracing::warn!(
                            query = %sub_query.text,
                            elapsed_ms,
                            timeout_ms,
                            "Sub-query hit the aggregated deadline, returning partial results"
                        );
                        stat.status = SubQueryStatus::TimedOut;
                        stat.error = Some(format!(
                            "sub-query deadline reached after {elapsed_ms} ms (request budget {timeout_ms} ms)"
                        ));
                        failed_sub_queries.push(sub_query.text.clone());
                        stats.push(stat);
                    }
                }
            }

            if failed_sub_queries.len() == executed_sub_queries {
                return Err(QueryError::InvalidQuery(format!(
                    "all {executed_sub_queries} sub-queries failed or timed out: {}",
                    failed_sub_queries.join("; ")
                )));
            }

            // Merge by unified key (alignment key, else content weak key).
            // Chunk grouping is on by default; it keeps one entry per physical
            // chunk inside a sub-query but picks the highest-scoring entity as
            // the representative, so the same chunk can arrive under different
            // entity ids from different sub-queries. Keying on the chunk in that
            // case keeps one entry per body instead of repeating it.
            let strategy = planned_options.global_config.aggregation.score_strategy;
            let group_by_chunk = planned_options.global_config.fusion.dedup_by_chunk;
            let mut dedup: HashMap<String, crate::query::types::SearchResult> = HashMap::new();
            // Strongest single contribution per key; the entry carrying it
            // stays the representative so content and metadata come from
            // the sub-query that scored it highest.
            let mut best_contribution: HashMap<String, f32> = HashMap::new();
            let mut contributors: HashMap<String, HashSet<usize>> = HashMap::new();
            let mut unkeyed: Vec<crate::query::types::SearchResult> = Vec::new();
            for (sub_index, mut item) in all_results {
                let Some(key) = aggregate_dedup_key(&item, group_by_chunk) else {
                    item.contributing_sub_queries = 1;
                    unkeyed.push(item);
                    continue;
                };
                contributors
                    .entry(key.clone())
                    .or_default()
                    .insert(sub_index);
                let contribution_count = contributors.get(&key).map_or(1, HashSet::len);
                match dedup.get_mut(&key) {
                    Some(existing) => {
                        let combined =
                            combine_aggregate_scores(existing.score, item.score, strategy);
                        let is_stronger =
                            item.score > best_contribution.get(&key).copied().unwrap_or(f32::MIN);
                        if is_stronger {
                            let mut replacement = item.clone();
                            replacement.score = combined;
                            replacement.original_score = combined;
                            replacement.contributing_sub_queries = contribution_count;
                            merge_sources(&mut replacement, existing);
                            *existing = replacement;
                            best_contribution.insert(key, item.score);
                        } else {
                            existing.score = combined;
                            existing.original_score = combined;
                            existing.contributing_sub_queries = contribution_count;
                            merge_sources(existing, &item);
                        }
                    }
                    None => {
                        best_contribution.insert(key.clone(), item.score);
                        item.contributing_sub_queries = contribution_count;
                        dedup.insert(key, item);
                    }
                }
            }

            let merged: Vec<crate::query::types::SearchResult> = dedup.into_values().collect();
            let mut merged = {
                let mut combined = merged;
                combined.extend(unkeyed);
                combined
            };

            // Rerank and result filters run once on the cross-query union.
            // Sub-queries only contribute candidates, so their scores stay
            // comparable when the global ranker observes one merged list.
            merged = self.searcher.rerank_results(merged, &merge_options).await?;

            let min_score = planned_options.global_config.result.min_score;
            if min_score > 0.0 {
                merged.retain(|item| item.score >= min_score);
            }
            merged.sort_by(|a, b| a.cmp_score_desc_then_stable(b));
            // Same near-tie rule as the single-query path: a full body outranks
            // a file-and-range reference when their scores are within tolerance,
            // so both endpoints order an equal-score pair identically.
            merged = crate::query::searcher::post_processing::demote_references_for_sort(merged);

            let limit = planned_options.global_config.result.limit;
            if merged.len() > limit {
                merged.truncate(limit);
            }

            // Attribution runs on the final cut: a candidate only counts as
            // contributed when it survives the merge, the threshold and the
            // limit, which is what separates a productive sub-query from one
            // that recalls a lot and loses every key to a stronger neighbour.
            for item in &merged {
                let Some(key) = aggregate_dedup_key(item, group_by_chunk) else {
                    continue;
                };
                let Some(indices) = contributors.get(&key) else {
                    continue;
                };
                for index in indices {
                    if let Some(stat) = stats.iter_mut().find(|stat| stat.index == *index) {
                        stat.contributed += 1;
                    }
                }
            }

            // A sub-query that recalled candidates and still placed none is the
            // silent cost this accounting exists to surface: it paid for
            // embedding and recall, then lost every merge key to a stronger
            // neighbour. Failures and empty recalls already log on their own.
            for stat in stats
                .iter()
                .filter(|stat| stat.status == SubQueryStatus::Succeeded && stat.is_wasted())
            {
                tracing::warn!(
                    index = stat.index,
                    query = %stat.text,
                    candidates = stat.candidates,
                    elapsed_ms = stat.elapsed_ms,
                    "Sub-query recalled candidates but contributed no final hit"
                );
            }

            // Relation expansion runs once on the merged hits so budget,
            // shared dedup and single wrapping observe the final Top-N.
            if let Some(resolver) = planned_options.neighbour_resolver.as_ref() {
                if planned_options.global_config.annotation.expansion_enabled {
                    crate::query::expansion::annotate_hits_with_resolver(
                        resolver,
                        &mut merged,
                        &planned_options.global_config.annotation,
                    )
                    .await;
                }
            }

            let total = merged.len();
            let cacheable = failed_sub_queries.is_empty();

            Ok(QueryResult {
                items: merged,
                total,
                elapsed_ms: 0,
                sources: sources_used,
                planned_sub_queries,
                executed_sub_queries,
                succeeded_sub_queries,
                sub_query_cache_hits,
                failed_sub_queries,
                sub_query_stats: stats,
                cacheable,
                from_cache: false,
                stages: aggregate_stages,
            })
        });

        let timeout = std::time::Duration::from_millis(timeout_ms);
        let mut aggregated = match tokio::time::timeout(timeout, aggregated).await {
            Ok(result) => result?,
            Err(_) => return Err(QueryError::Timeout { timeout_ms }),
        };
        aggregated.elapsed_ms = start.elapsed().as_millis() as u64;
        if aggregated.from_cache {
            // Nothing ran for this request: no sub-query executed, contributed
            // or hit its own cache entry. The per-sub-query accounting belongs
            // to the run that produced the entry, so it is dropped together
            // with the counters instead of describing work this call did not do.
            aggregated.executed_sub_queries = 0;
            aggregated.succeeded_sub_queries = 0;
            aggregated.sub_query_cache_hits = 0;
            aggregated.sub_query_stats.clear();
        }
        Ok(aggregated)
    }

    /// Decompose one sub-query's options out of the aggregated options.
    ///
    /// Centralizes the field mapping so the aggregated and per-sub-query
    /// option surfaces stay in sync: global filters/config/patterns are shared,
    /// the query text and sources come from the sub-query itself, and source
    /// content is always requested for aggregated results.
    /// Sub-queries run without annotation, reranking or result filters;
    /// those stages apply once to the merged list in search_aggregated.
    fn build_sub_query_options(
        &self,
        agg_options: &AggregatedQueryOptions,
        sub_query: &SubQuery,
    ) -> QueryOptions {
        let filters = agg_options.filters.as_ref();
        let mut config = agg_options.global_config.clone();
        config.annotation.enable_annotation = false;
        config.annotation.expansion_enabled = false;
        // Threshold-free sub-queries with inflated limit: the merged list
        // applies the single global threshold after weighting. The per-query
        // cap bounds the worst-case recall cost when many sub-queries run.
        config.result.min_score = 0.0;
        let inflated = agg_options
            .global_config
            .result
            .limit
            .saturating_mul(agg_options.sub_queries.len().max(1))
            .max(agg_options.global_config.result.limit);
        config.result.limit = inflated.clamp(10, Self::MAX_SUBQUERY_LIMIT);
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
            enable_rerank: Some(false),
            enable_result_filter: Some(false),
            neighbour_resolver: None,
        }
    }

    /// Build the single query context used for merged-list ranking.
    fn build_merge_query_options(&self, agg_options: &AggregatedQueryOptions) -> QueryOptions {
        let filters = agg_options.filters.as_ref();
        let query = if cce_utils::text::is_blank(&agg_options.original_query) {
            agg_options
                .sub_queries
                .first()
                .map_or_else(String::new, |sub| sub.text.clone())
        } else {
            agg_options.original_query.clone()
        };
        QueryOptions {
            query,
            project_id: agg_options.project_id,
            sources: agg_options
                .sub_queries
                .first()
                .map_or_else(SearchSources::default, |sub| sub.sources),
            config: agg_options.global_config.clone(),
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
    // latest filesystem state; caller-supplied path filters apply through
    // the shared path filter. Branches without live project files rank the
    // indexed file rows of the active epoch view with the same matcher and
    // ordering instead, returning path-only hits without live metadata.

    /// Search file paths matching `query`, returning at most `limit` paths.
    ///
    /// Paths are project-relative and ordered by match quality with a lexical
    /// tiebreak. No index capability is required: files exist regardless of
    /// which indexes are enabled.
    pub async fn search_file_paths(
        &self,
        query: &str,
        limit: usize,
        options: &super::files::FileSearchOptions,
    ) -> Result<Vec<String>> {
        let hits = self.search_files(query, limit, options).await?;
        Ok(hits.into_iter().map(|hit| hit.path).collect())
    }

    /// Search files matching `query`, attaching size and line count.
    ///
    /// Shares the matcher and ordering with [`Self::search_file_paths`];
    /// files unreadable at stat time are skipped. The embedded store always
    /// keeps live project files, so search walks the project directory.
    pub async fn search_files(
        &self,
        query: &str,
        limit: usize,
        options: &super::files::FileSearchOptions,
    ) -> Result<Vec<super::files::FileHit>> {
        self.metadata_store.as_ref().ok_or_else(|| {
            QueryError::config("file search requires a relation store for project lookup")
        })?;
        let root = self.resolve_file_search_root().await?;
        let query = query.to_string();
        let options = options.clone();
        tokio::task::spawn_blocking(move || {
            super::files::search_files(&root, &query, limit, &options)
        })
        .await
        .map_err(|e| QueryError::storage(&format!("File search task failed: {e}")))?
    }

    /// Resolve the project directory file search walks.
    async fn resolve_file_search_root(&self) -> Result<std::path::PathBuf> {
        use cce_storage_common::MetadataStorage;
        let store = self.metadata_store.as_ref().ok_or_else(|| {
            QueryError::config("file search requires a relation store for project lookup")
        })?;
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
    use super::{combine_aggregate_scores, merge_sources, normalize_sub_queries};
    use crate::query::types::{SearchResult, SearchSources, SubQuery};
    use cce_config::modules::search::AggregationScoreStrategy;

    #[test]
    fn test_coordinator_creation() {
        // This test would require mock components
        // For now, we just verify the structure compiles
    }

    #[test]
    fn noisy_or_rewards_repeated_hits_and_stays_bounded() {
        let strategy = AggregationScoreStrategy::NoisyOr;
        let first = combine_aggregate_scores(0.0, 0.5, strategy);
        assert!((first - 0.5).abs() < 1e-6);
        let second = combine_aggregate_scores(first, 0.5, strategy);
        assert!(second > first, "a second hit must raise the score");
        assert!((second - 0.75).abs() < 1e-6);
        // Monotone and bounded no matter how many sub-queries agree.
        let mut acc = 0.0;
        for _ in 0..64 {
            acc = combine_aggregate_scores(acc, 0.9, strategy);
            assert!((0.0..=1.0).contains(&acc));
        }
        assert!(acc <= 1.0);
    }

    #[test]
    fn out_of_range_scores_are_clamped_before_combining() {
        let strategy = AggregationScoreStrategy::NoisyOr;
        let score = combine_aggregate_scores(0.4, 2.0, strategy);
        assert!((0.0..=1.0).contains(&score));
        assert!(score > 0.4);
    }

    #[test]
    fn max_strategy_keeps_the_strongest_contribution() {
        let strategy = AggregationScoreStrategy::Max;
        assert!((combine_aggregate_scores(0.3, 0.6, strategy) - 0.6).abs() < 1e-6);
        assert!((combine_aggregate_scores(0.8, 0.6, strategy) - 0.8).abs() < 1e-6);
        assert!((combine_aggregate_scores(0.2, 2.0, strategy) - 0.999).abs() < 1e-6);
    }

    #[test]
    fn duplicate_sub_queries_are_collapsed_in_order() {
        let sub = SubQuery {
            text: "auth flow".to_string(),
            sources: SearchSources::default(),
            weight: 0.8,
        };
        let different = SubQuery {
            text: "login".to_string(),
            sources: SearchSources::default(),
            weight: 0.8,
        };
        let normalized = normalize_sub_queries(&[sub.clone(), different, sub]);
        assert_eq!(normalized.len(), 2);
        assert_eq!(normalized[0].text, "auth flow");
        assert_eq!(normalized[1].text, "login");
    }

    #[test]
    fn merged_entry_keeps_both_provenance_labels() {
        let mut target = SearchResult {
            sources: vec!["vector".to_string()],
            ..Default::default()
        };
        let other = SearchResult {
            sources: vec!["bm25".to_string(), "vector".to_string()],
            ..Default::default()
        };
        merge_sources(&mut target, &other);
        assert_eq!(
            target.sources,
            vec!["vector".to_string(), "bm25".to_string()]
        );
    }
}
