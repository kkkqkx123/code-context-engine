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
use super::cache::{CacheConfig, QueryCache};
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

    /// Attach the local relation database for FTS5 entity search.
    ///
    /// Local-only port: FTS5 entity search runs inside the embedded branch.
    pub fn with_sqlite(mut self, sqlite: Arc<cce_storage_metadb_sqlite::SqliteClient>) -> Self {
        self.relation = Some(RelationStore::local(sqlite));
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

    /// Get index capabilities
    pub fn capabilities(&self) -> IndexCapabilities {
        self.capabilities
    }

    /// Check if a specific index is available
    pub fn has_capability(&self, index: &str) -> bool {
        match index {
            "vector" | "vectors" => self.capabilities.has_vectors(),
            "bm25" => self.capabilities.has_bm25(),
            "summary" | "summaries" => self.capabilities.has_summaries(),
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
        // FTS5 is an embedded-branch capability: the only fenced downcast
        // on this path. Other branches report unavailability instead of
        // silently returning no entities.
        let sqlite = self
            .relation
            .as_ref()
            .and_then(RelationStore::as_local)
            .ok_or_else(|| QueryError::Config("FTS5 entity search not configured".to_string()))?;

        let view = self.searcher.load_query_filter(self.project_id).await?;

        let conn = sqlite.read_connection().map_err(|e| {
            QueryError::InvalidQuery(format!("Failed to get database connection: {}", e))
        })?;

        let start = Instant::now();
        let result = Self::search_entities_at_view(&conn, query, self.project_id, limit, &view);
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;

        // Record metrics for FTS5 search (using QueryMetrics)
        if let Some(metrics) = &self.metrics {
            let count = result.as_ref().map_or(0, |r| r.len());
            // For FTS5 search, always treat as cache miss since it's a direct DB query
            metrics.record_query(elapsed, false, count);
        }

        result.map_err(|e| QueryError::InvalidQuery(format!("FTS5 search failed: {}", e)))
    }

    /// FTS5 entity search over the full epoch view.
    ///
    /// Two-stage resolution ("own first, miss → parent"): an empty
    /// own-generation result falls back to the inherited parent epoch, and
    /// parent hits belonging to overridden files (replaced/deleted) are
    /// dropped so only the visible view is returned.
    fn search_entities_at_view(
        conn: &rusqlite::Connection,
        query: &str,
        project_id: i64,
        limit: i64,
        view: &crate::query::filter::QueryFilter,
    ) -> Result<Vec<cce_storage_metadb_sqlite::EntityRecord>> {
        use cce_storage_metadb_sqlite::EntityRepository;
        use cce_storage_metadb_sqlite::repo::FileRepository;

        let mut entities = EntityRepository::search_fts_at_epoch(
            conn,
            query,
            project_id,
            limit,
            view.epoch_value(),
        )
        .map_err(|e| QueryError::InvalidQuery(format!("FTS5 search failed: {}", e)))?;
        if !entities.is_empty() || view.parent_epoch().is_none() {
            return Ok(entities);
        }
        entities = EntityRepository::search_fts_at_epoch(
            conn,
            query,
            project_id,
            limit,
            view.parent_epoch().expect("parent checked above"),
        )
        .map_err(|e| QueryError::InvalidQuery(format!("FTS5 search failed: {}", e)))?;
        if !view.excluded_files().is_empty() && !entities.is_empty() {
            let excluded: std::collections::HashSet<&str> =
                view.excluded_files().iter().map(String::as_str).collect();
            entities.retain(|entity| {
                FileRepository::get_by_id(conn, entity.file_id)
                    .ok()
                    .flatten()
                    .is_some_and(|file| !excluded.contains(file.path.as_str()))
            });
        }
        Ok(entities)
    }

    /// Check if FTS5 entity search is available (local branch attached)
    pub fn has_fts5_search(&self) -> bool {
        self.relation.as_ref().is_some_and(RelationStore::is_local)
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

        // Check cache first
        if let Some(mut cached) = self.cache.get_result_for_view(options, view).await {
            tracing::trace!("Cache hit for query: {}", options.query);

            // Record metrics if enabled (cache hit)
            if let Some(metrics) = &self.metrics {
                let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
                metrics.record_query(latency_ms, true, cached.items.len());
            }

            cached.from_cache = true;
            return Ok(cached);
        }

        tracing::trace!("Cache miss for query: {}", options.query);

        // Execute search — no silent degradation. If services are unavailable,
        // the error propagates to the caller, and the query is queued for retry.
        // The overall timeout bounds the entire pipeline so a slow component
        // cannot stall the request indefinitely.
        let timeout_ms = options.config.timeout_ms;
        let search_future = self.searcher.search_with_view(options, view);
        let search_result =
            match tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), search_future)
                .await
            {
                Ok(result) => result,
                Err(_) => Err(QueryError::Timeout { timeout_ms }),
            };
        match search_result {
            Ok(result) => {
                // Store in cache
                self.cache
                    .put_result_for_view(options, view, result.clone())
                    .await;

                // Record metrics if enabled (cache miss)
                if let Some(metrics) = &self.metrics {
                    let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
                    metrics.record_query(latency_ms, false, result.items.len());
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
                    self.cache
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

        // Build every sub-query's options up front so the shared filters and
        // config are decomposed in one place (see `build_sub_query_options`).
        let sub_options: Vec<QueryOptions> = agg_options
            .sub_queries
            .iter()
            .map(|sub_query| self.build_sub_query_options(agg_options, sub_query))
            .collect();

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
                    // Weight the sub-query's candidates before the merge so
                    // the weight decides cross-query ranking, not just
                    // bookkeeping. Invalid weights degrade to neutral (1.0)
                    // with a warning instead of dropping results; the API
                    // boundary rejects them outright.
                    let weight = if sub_query.weight.is_finite() && sub_query.weight >= 0.0 {
                        sub_query.weight
                    } else {
                        tracing::warn!(
                            query = %sub_query.text,
                            weight = sub_query.weight,
                            "Sub-query has an invalid weight; treating it as 1.0"
                        );
                        1.0
                    };
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

        // Deduplicate by alignment key (entity_ids or segment_id), keeping the highest score.
        // Using chunk id would fail to deduplicate the same entity across BM25/embedding paths,
        // since chunk id format is {groupId}_{path}_{index} (path-specific).
        // Reuses the fusion key derivation so the aggregated dedup and hybrid
        // fusion collapse results onto the same key space.
        let mut dedup: HashMap<String, crate::query::types::SearchResult> = HashMap::new();
        for item in all_results {
            let key = crate::query::retrieval::post_processing::alignment_key(
                &item.entity_ids,
                item.segment_id.as_deref(),
                &item.id,
            )
            .unwrap_or_else(|| item.id.clone());
            dedup
                .entry(key)
                .and_modify(|existing| {
                    if item.score > existing.score {
                        *existing = item.clone();
                    }
                })
                .or_insert(item);
        }

        let mut merged: Vec<crate::query::types::SearchResult> = dedup.into_values().collect();
        merged.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let limit = agg_options.global_config.result.limit;
        if merged.len() > limit {
            merged.truncate(limit);
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
    }

    /// Decompose one sub-query's options out of the aggregated options.
    ///
    /// Centralizes the field mapping so the aggregated and per-sub-query
    /// option surfaces stay in sync: global filters/config/patterns are shared,
    /// the query text and sources come from the sub-query itself, and source
    /// content is always requested for aggregated results.
    fn build_sub_query_options(
        &self,
        agg_options: &AggregatedQueryOptions,
        sub_query: &crate::query::types::SubQuery,
    ) -> QueryOptions {
        let filters = agg_options.filters.as_ref();
        QueryOptions {
            query: sub_query.text.clone(),
            project_id: agg_options.project_id,
            sources: sub_query.sources,
            config: agg_options.global_config.clone(),
            directory_prefix: filters.and_then(|f| f.directory_prefix.clone()),
            exclude_content_types: filters.map_or(Vec::new(), |f| f.exclude_content_types.clone()),
            include_categories: filters.map_or(Vec::new(), |f| f.include_categories.clone()),
            exclude_categories: filters.map_or(Vec::new(), |f| f.exclude_categories.clone()),
            exclude_patterns: agg_options.exclude_patterns.clone(),
            include_patterns: agg_options.include_patterns.clone(),
            with_source: true,
            query_intent: None, // Auto-detect for sub-queries
            enable_rerank: agg_options.enable_rerank,
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

        // Check summary capability
        if options.sources.summary && !self.capabilities.has_summaries() {
            return Err(QueryError::index_not_available("summary"));
        }

        Ok(())
    }

    /// Invalidate all caches
    pub async fn invalidate_cache(&self) {
        self.cache.invalidate_all().await;
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

    // ========== Graph Queries ==========
    // Graph-shaped answers over the relation snapshot. These never
    // participate in semantic scoring; clients call them explicitly.

    /// Ego neighborhood of one entity up to `depth` hops.
    pub fn graph_ego(
        &self,
        entity_id: cce_types::EntityId,
        depth: usize,
        direction: super::graph::GraphDirection,
    ) -> Result<super::graph::SubGraph> {
        if !self.capabilities.has_relations() {
            return Err(QueryError::index_not_available("relation"));
        }
        super::graph::GraphService::new(Arc::clone(&self.relation_searcher))
            .ego_graph(entity_id, depth, direction)
    }

    /// Shortest path between two entities as a linear subgraph.
    pub fn graph_path(
        &self,
        start_id: cce_types::EntityId,
        end_id: cce_types::EntityId,
        max_depth: usize,
    ) -> Result<Option<super::graph::SubGraph>> {
        if !self.capabilities.has_relations() {
            return Err(QueryError::index_not_available("relation"));
        }
        super::graph::GraphService::new(Arc::clone(&self.relation_searcher))
            .shortest_path(start_id, end_id, max_depth)
    }

    /// Induced subgraph over an explicit entity set.
    pub fn graph_subgraph(
        &self,
        entity_ids: &[cce_types::EntityId],
    ) -> Result<super::graph::SubGraph> {
        if !self.capabilities.has_relations() {
            return Err(QueryError::index_not_available("relation"));
        }
        super::graph::GraphService::new(Arc::clone(&self.relation_searcher)).subgraph(entity_ids)
    }

    /// Connected components over internal edges.
    pub fn graph_components(&self) -> Result<Vec<Vec<cce_types::EntityId>>> {
        if !self.capabilities.has_relations() {
            return Err(QueryError::index_not_available("relation"));
        }
        super::graph::GraphService::new(Arc::clone(&self.relation_searcher)).connected_components()
    }

    /// Full project export capped at `limit` nodes in entity order.
    pub fn graph_export(&self, limit: usize) -> Result<super::graph::SubGraph> {
        if !self.capabilities.has_relations() {
            return Err(QueryError::index_not_available("relation"));
        }
        super::graph::GraphService::new(Arc::clone(&self.relation_searcher)).export_full(limit)
    }

    /// Get file summary by file path (direct lookup, no vector search)
    ///
    /// This is a direct lookup that retrieves the file summary from SQLite
    /// without any vector embedding or semantic search. Useful when:
    /// - User knows the exact file path
    /// - User wants a quick file overview without retrieval overhead
    /// - Integration with file browsing/navigation UI
    ///
    /// # Arguments
    /// * `file_path` - The file path to look up (e.g., "src/main.rs")
    /// * `project_id` - The project ID for isolation
    ///
    /// # Returns
    /// Returns the file summary JSON if found, or QueryError if not available
    pub async fn get_file_summary(
        &self,
        file_path: &str,
        project_id: i64,
    ) -> Result<serde_json::Value> {
        // File-row lookup stays on the embedded branch; the epoch view
        // itself resolves through the relation contract.
        let sqlite = self
            .relation
            .as_ref()
            .and_then(RelationStore::as_local)
            .ok_or_else(|| QueryError::index_not_available("sqlite"))?;

        let view = self.searcher.load_query_filter(project_id).await?;

        let conn = sqlite
            .read_connection()
            .map_err(|e| QueryError::invalid(&format!("Failed to connect to SQLite: {}", e)))?;

        // Two-stage resolution ("own first, miss → parent"): an inherited
        // file's rows live in the parent generation; overridden files never
        // resolve against it.
        use cce_storage_metadb_sqlite::repo::FileRepository;
        let resolve_file = |epoch: i64| {
            FileRepository::get_by_path_and_project_at_epoch(&conn, file_path, project_id, epoch)
                .map_err(|e| QueryError::invalid(&format!("Failed to get file: {}", e)))
        };
        let mut resolved_epoch = view.epoch_value();
        let file_record = match resolve_file(resolved_epoch)? {
            Some(record) => Some(record),
            None => match view.parent_epoch() {
                Some(parent) if !view.excluded_files().iter().any(|f| f == file_path) => {
                    resolved_epoch = parent;
                    resolve_file(parent)?
                }
                _ => None,
            },
        }
        .ok_or_else(|| {
            QueryError::not_found(format!(
                "File not found: {} in project {}",
                file_path, project_id
            ))
        })?;

        // Get summary from file_summaries table (returns JSON string)
        use cce_storage_metadb_sqlite::repo::FileSummaryRepository;
        let summary_json_str =
            FileSummaryRepository::get_by_file_id_at_epoch(&conn, file_record.id, resolved_epoch)
                .map_err(|e| QueryError::invalid(&format!("Failed to get summary: {}", e)))?
                .ok_or_else(|| {
                    QueryError::not_found(format!("Summary not found for file: {}", file_path))
                })?;

        // Parse and update file_path to actual path
        let mut summary: serde_json::Value = serde_json::from_str(&summary_json_str)
            .map_err(|e| QueryError::invalid(&format!("Invalid summary JSON: {}", e)))?;

        if let Some(obj) = summary.as_object_mut() {
            obj["file_path"] = serde_json::json!(file_path);
        }

        Ok(summary)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_coordinator_creation() {
        // This test would require mock components
        // For now, we just verify the structure compiles
    }
}
