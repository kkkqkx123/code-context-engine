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

use cce_config::project_registry::ProjectScope;
use cce_llm::Embedder;
use cce_llm_client::ProductionRerankHandler;
use cce_metrics::{MetricsRegistry, QueryMetrics, SearchMetrics};
use cce_relation::CallChainQuery;
use cce_storage_bm25::Bm25Client;
use cce_storage_qdrant::QdrantClient;
use cce_storage_sqlite::SqliteClient;

use super::SearcherBuilder;
use super::assembly::SPSRGraphAssembler;
use super::cache::{CacheConfig, QueryCache};
use super::capabilities::IndexCapabilities;
use super::error::{QueryError, Result};
use super::relation_bridge::{RelationBridge, RelationEnrichmentConfig};
use super::relation_searcher::{PathQueryOptions, RelationQueryOptions, RelationSearcher};
use super::retry_queue::RetryQueue;
use super::searcher::Searcher;
use super::types::{AggregatedQueryOptions, QueryOptions, QueryResult};

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
    /// Relation enrichment bridge (optional)
    relation_bridge: Option<Arc<RelationBridge>>,
    /// SQLite database for FTS5 entity search (optional)
    sqlite: Option<Arc<SqliteClient>>,
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
    sqlite: Option<Arc<SqliteClient>>,
    metrics_registry: Option<Arc<MetricsRegistry>>,
    project_id: i64,
}

impl QueryCoordinatorBuilder {
    /// Create a new builder with required components
    fn new(
        qdrant: Arc<QdrantClient>,
        embedder: Arc<dyn Embedder>,
        bm25: Arc<tokio::sync::Mutex<Bm25Client>>,
        call_chain_query: Arc<CallChainQuery>,
        scope: ProjectScope,
    ) -> Self {
        let project_id = scope.project_id();
        let searcher_builder = Searcher::builder(qdrant, embedder, bm25, scope);
        let relation_searcher = Arc::new(RelationSearcher::new(call_chain_query));

        Self {
            searcher_builder: Some(searcher_builder),
            relation_searcher: Some(relation_searcher),
            cache_config: None,
            capabilities: None,
            sqlite: None,
            metrics_registry: None,
            project_id,
        }
    }

    /// Enable SQLite support
    pub fn with_sqlite(mut self, sqlite: Arc<SqliteClient>) -> Self {
        self.sqlite = Some(sqlite.clone());
        if let Some(builder) = self.searcher_builder.take() {
            self.searcher_builder = Some(builder.with_sqlite(sqlite));
        }
        self
    }

    /// Enable SPSR-Graph assembly support
    pub fn with_assembler(mut self, assembler: Arc<SPSRGraphAssembler>) -> Self {
        if let Some(builder) = self.searcher_builder.take() {
            self.searcher_builder = Some(builder.with_assembler(assembler));
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
            relation_bridge: None,
            sqlite: self.sqlite,
            metrics: None,
            retry_queue: Arc::new(RetryQueue::new()),
            project_id: self.project_id,
        }
    }
}

impl QueryCoordinator {
    /// Create a new builder for QueryCoordinator with required components
    pub fn builder(
        qdrant: Arc<QdrantClient>,
        embedder: Arc<dyn Embedder>,
        bm25: Arc<tokio::sync::Mutex<Bm25Client>>,
        call_chain_query: Arc<CallChainQuery>,
        scope: ProjectScope,
    ) -> QueryCoordinatorBuilder {
        QueryCoordinatorBuilder::new(qdrant, embedder, bm25, call_chain_query, scope)
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
            relation_bridge: None,
            sqlite: None,
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
            relation_bridge: None,
            sqlite: None,
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
            relation_bridge: None,
            sqlite: None,
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
            relation_bridge: None,
            sqlite: None,
            metrics: None,
            retry_queue: Arc::new(RetryQueue::new()),
            project_id,
        }
    }

    /// Create a query coordinator with relation enrichment bridge enabled
    ///
    /// This enables automatic enrichment of search results with relation context.
    pub fn with_relation_bridge(
        searcher: Arc<Searcher>,
        relation_searcher: Arc<RelationSearcher>,
        relation_index: Arc<cce_relation::RelationIndex>,
        scope: ProjectScope,
    ) -> Self {
        let project_id = scope.project_id();
        let bridge = Arc::new(RelationBridge::new(scope, relation_index));
        Self {
            searcher,
            relation_searcher,
            cache: QueryCache::new(CacheConfig::default()),
            capabilities: IndexCapabilities::default(),
            relation_bridge: Some(bridge),
            sqlite: None,
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

    /// Create with relation bridge and custom configuration
    pub fn with_relation_bridge_config(
        searcher: Arc<Searcher>,
        relation_searcher: Arc<RelationSearcher>,
        relation_index: Arc<cce_relation::RelationIndex>,
        config: RelationEnrichmentConfig,
        scope: ProjectScope,
    ) -> Self {
        let project_id = scope.project_id();
        let bridge = Arc::new(RelationBridge::with_config(scope, relation_index, config));
        Self {
            searcher,
            relation_searcher,
            cache: QueryCache::new(CacheConfig::default()),
            capabilities: IndexCapabilities::default(),
            relation_bridge: Some(bridge),
            sqlite: None,
            metrics: None,
            retry_queue: Arc::new(RetryQueue::new()),
            project_id,
        }
    }

    /// Set SQLite database for FTS5 entity search
    pub fn with_sqlite(mut self, sqlite: Arc<SqliteClient>) -> Self {
        self.sqlite = Some(sqlite);
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

    // ========== Relation Enrichment Bridge ==========

    /// Enrich chunks with relation context
    ///
    /// This method uses the relation bridge to map chunks to entities and then
    /// expand their relations. Returns enriched chunks with caller/callee information.
    ///
    /// # Arguments
    ///
    /// * `chunks` - The chunks to enrich (typically from search results)
    ///
    /// # Returns
    ///
    /// Enriched chunks with relation context, or the original chunks if the bridge is not enabled.
    pub async fn enrich_chunks_with_relations(
        &self,
        chunks: &[cce_parser::ast_to_nl::chunker::ChunkedResult],
        project_id: i64,
    ) -> Result<Vec<super::relation_bridge::EnrichedChunk>> {
        if let Some(ref bridge) = self.relation_bridge {
            bridge
                .enrich_chunks(chunks, project_id)
                .await
                .map_err(|e| QueryError::InvalidQuery(format!("Failed to enrich chunks: {}", e)))
        } else {
            // If bridge is not enabled, return chunks without enrichment
            Err(QueryError::InvalidQuery(
                "Relation bridge is not enabled. Use with_relation_bridge() to enable it."
                    .to_string(),
            ))
        }
    }

    /// Check if relation bridge is enabled
    pub fn is_relation_bridge_enabled(&self) -> bool {
        self.relation_bridge.is_some()
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
    pub fn search_entities(
        &self,
        query: &str,
        limit: i64,
    ) -> Result<Vec<cce_storage_sqlite::EntityRecord>> {
        let sqlite = self
            .sqlite
            .as_ref()
            .ok_or_else(|| QueryError::Config("SQLite database not configured".to_string()))?;

        let conn = sqlite.read_connection().map_err(|e| {
            QueryError::InvalidQuery(format!("Failed to get database connection: {}", e))
        })?;

        let view = self.searcher.load_query_filter(self.project_id)?;

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
    ) -> Result<Vec<cce_storage_sqlite::EntityRecord>> {
        use cce_storage_sqlite::EntityRepository;
        use cce_storage_sqlite::repo::FileRepository;

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

    /// Check if FTS5 entity search is available
    pub fn has_fts5_search(&self) -> bool {
        self.sqlite.is_some()
    }

    // ========== Unified Search ==========

    /// Execute search with given options
    ///
    /// Supports all search strategies based on SearchSources:
    /// - VectorOnly: Pure vector semantic search (BM25 for consensus boost only)
    /// - HybridFusion: Dense + BM25 hybrid search with application-level fusion
    /// - WithRelationExpansion: Search with relation expansion
    /// - WithAssembly: Search with SPSR-Graph assembly
    ///
    /// Assembly is now handled as a strategy within the Searcher,
    /// so no special handling is needed here.
    ///
    /// # Fault Tolerance
    ///
    /// If a retryable error occurs (service unavailable), the query is
    /// automatically preserved in the retry queue for later reprocessing
    /// when the service recovers. The error is still propagated to the
    /// caller so the degradation is visible.
    pub async fn search(&self, options: &QueryOptions) -> Result<QueryResult> {
        let start = std::time::Instant::now();

        // Check capabilities before executing
        self.check_capabilities(options)?;
        let view = self.searcher.load_query_filter(options.project_id)?;

        // Check cache first
        if let Some(cached) = self.cache.get_result_for_view(options, &view).await {
            tracing::trace!("Cache hit for query: {}", options.query);

            // Record metrics if enabled (cache hit)
            if let Some(metrics) = &self.metrics {
                let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
                metrics.record_query(latency_ms, true, cached.items.len());
            }

            return Ok(cached);
        }

        tracing::trace!("Cache miss for query: {}", options.query);

        // Execute search — no silent degradation. If services are unavailable,
        // the error propagates to the caller, and the query is queued for retry.
        match self.searcher.search(options).await {
            Ok(result) => {
                // Store in cache
                self.cache
                    .put_result_for_view(options, &view, result.clone())
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
            let view = match self.searcher.load_query_filter(options.project_id) {
                Ok(view) => view,
                Err(error) => {
                    tracing::warn!(%error, "Failed to resolve retry query epoch");
                    self.retry_queue.push(options).await;
                    continue;
                }
            };
            match self.searcher.search(&options).await {
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
    /// Runs each sub-query in sequence and merges the results,
    /// deduplicating by entity ID and sorting by score.
    pub async fn search_aggregated(
        &self,
        agg_options: &AggregatedQueryOptions,
    ) -> Result<QueryResult> {
        let start = std::time::Instant::now();
        let mut all_results: Vec<crate::query::types::SearchResult> = Vec::new();
        let mut sources_used: Vec<String> = Vec::new();
        let sub_queries_count = agg_options.sub_queries.len();

        for sub_query in &agg_options.sub_queries {
            let options = QueryOptions {
                query: sub_query.text.clone(),
                project_id: agg_options.project_id,
                sources: sub_query.sources,
                config: agg_options.global_config.clone(),
                directory_prefix: agg_options
                    .filters
                    .as_ref()
                    .and_then(|f| f.directory_prefix.clone()),
                exclude_content_types: agg_options
                    .filters
                    .as_ref()
                    .map_or(Vec::new(), |f| f.exclude_content_types.clone()),
                include_categories: agg_options
                    .filters
                    .as_ref()
                    .map_or(Vec::new(), |f| f.include_categories.clone()),
                exclude_categories: agg_options
                    .filters
                    .as_ref()
                    .map_or(Vec::new(), |f| f.exclude_categories.clone()),
                exclude_patterns: agg_options.exclude_patterns.clone(),
                include_patterns: agg_options.include_patterns.clone(),
                with_source: true,
                query_intent: None, // Auto-detect for sub-queries
                enable_rerank: agg_options.enable_rerank,
            };

            match self.search(&options).await {
                Ok(result) => {
                    for source in &result.sources {
                        if !sources_used.contains(source) {
                            sources_used.push(source.clone());
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
        })
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

        // Check relation capability
        if options.sources.relation && !self.capabilities.has_relations() {
            return Err(QueryError::index_not_available("relation"));
        }

        Ok(())
    }

    /// Invalidate all caches
    pub async fn invalidate_cache(&self) {
        self.cache.invalidate_all().await;
    }

    // ========== Relation Queries ==========
    // These methods delegate to RelationSearcher for consistent behavior

    /// Get callees (functions called by this function)
    pub fn get_callees(
        &self,
        entity_id: cce_types::EntityId,
    ) -> Result<Vec<cce_types::ResolvedRelation>> {
        if !self.capabilities.has_relations() {
            return Err(QueryError::index_not_available("relation"));
        }
        Ok(self.relation_searcher.get_callees(entity_id))
    }

    /// Get callers (functions that call this function)
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

    /// Get callers with pagination
    pub fn get_callers_paginated(
        &self,
        entity_id: cce_types::EntityId,
        options: &RelationQueryOptions,
    ) -> Result<Vec<cce_types::EntityId>> {
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
    ) -> Result<Vec<cce_relation::CallChainNode>> {
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
    ) -> Result<Vec<cce_relation::CallChainNode>> {
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
    ) -> Result<Vec<cce_relation::CallChainNode>> {
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
    ) -> Result<Vec<cce_relation::CallChainNode>> {
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
    ) -> Result<Option<Vec<cce_relation::CallChainNode>>> {
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

    /// Get inheritance hierarchy (all ancestors)
    pub fn get_inheritance_hierarchy(
        &self,
        class_id: cce_types::EntityId,
        max_depth: usize,
    ) -> Result<Vec<cce_types::EntityId>> {
        if !self.capabilities.has_relations() {
            return Err(QueryError::index_not_available("relation"));
        }
        Ok(self
            .relation_searcher
            .get_inheritance_hierarchy(class_id, max_depth))
    }

    /// Get all derived classes (transitive closure)
    pub fn get_all_derived_classes(
        &self,
        class_id: cce_types::EntityId,
        max_depth: usize,
    ) -> Result<Vec<cce_types::EntityId>> {
        if !self.capabilities.has_relations() {
            return Err(QueryError::index_not_available("relation"));
        }
        Ok(self
            .relation_searcher
            .get_all_derived_classes(class_id, max_depth))
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
    pub fn get_file_summary(&self, file_path: &str, project_id: i64) -> Result<serde_json::Value> {
        let sqlite = self
            .sqlite
            .as_ref()
            .ok_or_else(|| QueryError::index_not_available("sqlite"))?;

        let conn = sqlite
            .write_connection()
            .map_err(|e| QueryError::invalid(&format!("Failed to connect to SQLite: {}", e)))?;

        let view = self.searcher.load_query_filter(project_id)?;

        // Two-stage resolution ("own first, miss → parent"): an inherited
        // file's rows live in the parent generation; overridden files never
        // resolve against it.
        use cce_storage_sqlite::repo::FileRepository;
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
        use cce_storage_sqlite::repo::FileSummaryRepository;
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
