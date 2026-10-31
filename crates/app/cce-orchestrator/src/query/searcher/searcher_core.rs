//! Unified searcher implementation
//!
//! Provides a unified search interface that combines vector retrieval,
//! BM25 enhancement, and result ranking into a cohesive search flow.
//!
//! # Pipeline invariants
//!
//! Every execution strategy runs the same stage contract:
//! retrieval → (hybrid: entity expansion + weighted fusion) → glob filter →
//! optional score normalization → SQLite enrichment → post-processing
//! (rerank, sort, filter, threshold).
//!
//! - SQLite enrichment is always the last data-shaping step before
//!   post-processing: fusion, filtering and normalization only need
//!   index-carried fields (scores, entity/segment ids, file_path), so the
//!   expensive content lookups run once, on the surviving candidate set.
//! - When `config.score.enable` is set, normalization applies uniformly to
//!   all strategies, so the final `result.min_score` threshold observes one
//!   score scale regardless of recall mode.

use std::sync::Arc;

use cce_config::project_registry::ProjectScope;

use crate::index::vector_store::VectorStore;
use crate::index::vector_store::{FulltextStore, RelationStore};
use crate::query::CachedEmbedder;
use crate::query::cache::CacheKey;
use crate::query::error::QueryError;
use crate::query::error::Result;
use crate::query::filter::QueryFilter;
use crate::query::ranking::{LlmReranker, PluginReranker, ScoreSorter, ThresholdFilter};
use crate::query::retrieval::post_processing::GlobFilter;
use crate::query::types::{ExecutionStrategy, QueryOptions, QueryResult, SearchResult};
use cce_llm_client::OpenAICompatibleProvider;
use cce_metrics::{SearchMetrics, SearchType};

use super::search_builder::SearcherBuilder;

/// Unified searcher
///
/// Combines vector retrieval and BM25 enhancement into a single search flow.
/// Vector retrieval is the core, BM25 is used for enhancement only.
#[derive(Clone)]

pub struct Searcher {
    /// Vector backend used by DenseRetrieval strategy
    pub(crate) vector: VectorStore,
    pub(crate) embedder: Arc<CachedEmbedder<OpenAICompatibleProvider>>,
    pub(crate) fulltext: FulltextStore,
    /// Relation backend for the epoch view and chunk content lookup (optional)
    pub(crate) relation: Option<RelationStore>,
    /// Immutable project scope binding project_id and project_group_id.
    pub(crate) scope: ProjectScope,
    /// Reranker for LLM-based reranking
    pub(crate) reranker: Arc<LlmReranker>,
    /// Reranker backed by `Rerank`-capability plugins
    pub(crate) plugin_reranker: Arc<PluginReranker>,
    /// Plugin registry for the query-side capabilities (`QueryRewrite` /
    /// `Fusion` / `ResultFilter`).
    pub(crate) plugin_registry: Option<Arc<cce_plugin::PluginRegistry>>,
    /// Score sorter for result ranking
    pub(crate) score_sorter: Arc<ScoreSorter>,
    /// Threshold filter for result filtering
    pub(crate) threshold_filter: Arc<ThresholdFilter>,
    /// Glob filter for include/exclude pattern filtering
    pub(crate) glob_filter: Arc<GlobFilter>,
    /// Optional search metrics collector
    pub(crate) search_metrics: Option<Arc<SearchMetrics>>,
    /// Shared query result cache. Clones share the underlying store,
    /// so every searcher handed the same instance populates one cache.
    pub(crate) cache: crate::query::cache::QueryCache,
}

/// Expand multi-entity results into single-entity results for entity-level fusion.
///
/// A single chunk may contain multiple entities. Before hybrid fusion, we expand
/// such results so each entity gets its own entry with the same score. This enables
/// entity-level alignment in fusion instead of chunk-level alignment.
///
/// Results with 0 or 1 entity_ids are passed through unchanged.
///
/// Defined in `post_processing::fusion` (fusion enforces the expansion contract
/// itself); re-exported here to keep the historical import path stable.
pub use crate::query::retrieval::post_processing::fusion::expand_multi_entity_results;

impl Searcher {
    /// Create a new searcher builder with a required project scope
    ///
    /// # Example
    ///
    /// ```ignore
    /// let searcher = Searcher::builder(vector, embedder, fulltext, scope)
    ///     .with_sqlite(sqlite)
    ///     .with_rerank(rerank_handler)
    ///     .build();
    /// ```
    pub fn builder(
        vector: VectorStore,
        embedder: Arc<OpenAICompatibleProvider>,
        fulltext: FulltextStore,
        scope: ProjectScope,
    ) -> SearcherBuilder {
        SearcherBuilder::new(vector, embedder, fulltext, scope)
    }

    /// Create a searcher builder from backend enums.
    ///
    /// Calls go through the fulltext contract, so both branches share one
    /// read path and branch selection stays inside the enum.
    pub fn builder_from_stores(
        vector: VectorStore,
        embedder: Arc<OpenAICompatibleProvider>,
        fulltext: FulltextStore,
        scope: ProjectScope,
    ) -> Result<SearcherBuilder> {
        Ok(SearcherBuilder::new(vector, embedder, fulltext, scope))
    }

    /// Fulltext backend enum backing this searcher.
    pub fn fulltext_store(&self) -> FulltextStore {
        self.fulltext.clone()
    }

    /// Relation backend enum backing this searcher.
    pub fn relation_store(&self) -> Option<RelationStore> {
        self.relation.clone()
    }

    /// Extract the fulltext backend from a searcher reference (used by strategy factory).
    ///
    /// This static method provides access to the fulltext backend for the BM25 recall strategy,
    /// avoiding circular dependency between Searcher and strategies.
    pub fn extract_fulltext_store(searcher: &Self) -> FulltextStore {
        searcher.fulltext.clone()
    }

    /// Execute search with given options
    ///
    /// Loads the active epoch view itself. Callers that already resolved a
    /// [`QueryFilter`] (query cache, retry replay) should use
    /// [`Searcher::search_with_view`] to avoid a second manifest read.
    pub async fn search(&self, options: &QueryOptions) -> Result<QueryResult> {
        let query_filter = self.load_query_filter(options.project_id).await?;
        self.search_with_view(options, &query_filter).await
    }

    /// Execute search against a caller-resolved epoch view.
    pub async fn search_with_view(
        &self,
        options: &QueryOptions,
        query_filter: &QueryFilter,
    ) -> Result<QueryResult> {
        let project_id = self.scope.project_id();
        if project_id != options.project_id {
            return Err(QueryError::config(&format!(
                "Searcher is bound to project {project_id}, but query requested project {}",
                options.project_id
            )));
        }
        if options.sources.is_empty() {
            return Err(QueryError::invalid(
                "at least one search source (vector, bm25) must be enabled",
            ));
        }
        let start = std::time::Instant::now();

        // Query rewriting and strategy selection run inside the compute
        // closure: the cache key is the pre-rewrite identity (what the next
        // identical request looks up). Concurrent identical queries
        // single-flight onto one computation, so they do not each queue
        // behind the shared LLM rate limiter.
        let key = CacheKey::from_options_with_view(options, query_filter);
        let result = self
            .cache
            .get_or_compute(key, || async move {
                let mut effective = options.clone();
                if effective.config.plugin.rewrite_enabled {
                    effective = self.apply_query_rewrite(effective).await?;
                }

                let strategy = effective.execution_strategy();
                let results = self
                    .execute_search_flow(&effective, &strategy, query_filter)
                    .await?;

                let elapsed_ms = start.elapsed().as_millis() as u64;
                tracing::trace!(
                    total_elapsed_ms = elapsed_ms,
                    strategy = %strategy,
                    result_count = results.len(),
                    "Search completed"
                );

                Ok(QueryResult {
                    total: results.len(),
                    items: results,
                    elapsed_ms,
                    sources: vec![strategy.to_string()],
                    sub_queries_count: 1, // Single query by default
                    failed_sub_queries: Vec::new(),
                    from_cache: false,
                })
            })
            .await?;

        // Record search metrics with query type distribution. `sources` holds
        // the display form (hybrid includes fusion weights); the metric label
        // is the bare strategy name.
        if let Some(metrics) = &self.search_metrics {
            let label = result.sources.first().map(String::as_str).unwrap_or("");
            let search_type = SearchType::from_label(label)
                .or_else(|| label.split('(').next().and_then(SearchType::from_label));
            metrics.record_search(start.elapsed().as_secs_f64() * 1000.0, search_type);
        }

        Ok(result)
    }

    /// Execute the complete search flow, dispatching per strategy.
    async fn execute_search_flow(
        &self,
        options: &QueryOptions,
        strategy: &ExecutionStrategy,
        query_filter: &QueryFilter,
    ) -> Result<Vec<SearchResult>> {
        tracing::trace!(
            epoch = query_filter.epoch_value(),
            "Using query filter with epoch"
        );
        match strategy {
            ExecutionStrategy::Bm25Recall => self.run_bm25_recall(options, query_filter).await,
            ExecutionStrategy::HybridRecall {
                vector_weight,
                bm25_weight,
            } => {
                self.run_hybrid_recall(options, query_filter, *vector_weight, *bm25_weight)
                    .await
            }
            ExecutionStrategy::DenseRecall => self.run_vector_recall(options, query_filter).await,
        }
    }

    /// Pure BM25 keyword recall (no vector dependency).
    async fn run_bm25_recall(
        &self,
        options: &QueryOptions,
        query_filter: &QueryFilter,
    ) -> Result<Vec<SearchResult>> {
        use crate::query::retrieval::strategies::RecallAlgorithm;

        tracing::trace!("Starting pure BM25 recall");
        let retrieval_start = std::time::Instant::now();

        let strategy = RecallAlgorithm::Bm25.create_strategy(self);
        let mut results = strategy.retrieve(options, query_filter).await?;

        tracing::trace!(
            count = results.len(),
            elapsed_ms = retrieval_start.elapsed().as_millis(),
            "BM25 recall completed"
        );

        results = self.apply_glob_filter(results, options)?;
        self.apply_score_normalization(&mut results, options);
        self.enrich_results(
            &mut results,
            options.project_id,
            query_filter,
            options.config.result.max_content_tokens,
        )
        .await;
        self.post_process_results(results, options).await
    }

    /// Hybrid recall: vector + BM25 parallel paths, fused by weighted normalization.
    async fn run_hybrid_recall(
        &self,
        options: &QueryOptions,
        query_filter: &QueryFilter,
        vector_weight: f32,
        bm25_weight: f32,
    ) -> Result<Vec<SearchResult>> {
        use crate::query::retrieval::post_processing::{
            HybridFusionConfig, fuse_hybrid_results_with_stats,
        };
        use crate::query::retrieval::strategies::RecallAlgorithm;

        tracing::trace!(
            vector_weight = vector_weight,
            bm25_weight = bm25_weight,
            "Starting hybrid recall (vector + BM25 parallel)"
        );

        let retrieve = |algo: RecallAlgorithm| async move {
            let strategy = algo.create_strategy(self);
            let start = std::time::Instant::now();
            let results = strategy.retrieve(options, query_filter).await;
            match &results {
                Ok(r) => tracing::trace!(
                    path = %algo,
                    count = r.len(),
                    elapsed_ms = start.elapsed().as_millis(),
                    "Recall path completed"
                ),
                Err(e) => tracing::warn!(
                    path = %algo,
                    error = %e,
                    elapsed_ms = start.elapsed().as_millis(),
                    "Recall path failed"
                ),
            }
            results
        };

        let (vector_attempt, bm25_attempt) = tokio::join!(
            retrieve(RecallAlgorithm::Dense),
            retrieve(RecallAlgorithm::Bm25)
        );

        // Both paths must succeed — never degrade one to the other.
        // Genuine configuration errors pass through unchanged so they are
        // not mislabeled as transient; runtime failures become retryable.
        let vector_results = vector_attempt.map_err(|e| {
            if e.is_config_error() {
                e
            } else {
                QueryError::retryable("vector", format!("Vector recall path failed: {e}"))
            }
        })?;
        let bm25_results = bm25_attempt.map_err(|e| {
            if e.is_config_error() {
                e
            } else {
                QueryError::retryable("bm25", format!("BM25 recall path failed: {e}"))
            }
        })?;

        // Expand multi-entity results before fusion for entity-level alignment.
        // Fusion consumes only index-carried fields (scores, entity/segment
        // ids), so SQLite enrichment runs once after fusion, on the surviving
        // candidate set.
        let vector_results = expand_multi_entity_results(vector_results);
        let bm25_results = expand_multi_entity_results(bm25_results);

        // The per-request weights arrive via the execution strategy (static
        // config refined by query-intent resolution); the algorithm and the
        // runtime switches come straight from `[search.fusion]` so new
        // file-side switches propagate without touching this call site.
        let fusion_config = HybridFusionConfig {
            vector_weight,
            bm25_weight,
            ..options.config.fusion.clone()
        };
        // Plugin fusion-weight override (Fusion capability).
        let fusion_config = if options.config.plugin.fusion_enabled {
            self.apply_fusion_override(
                options,
                fusion_config,
                vector_results.len(),
                bm25_results.len(),
            )
            .await
        } else {
            fusion_config
        };
        let fusion_start = std::time::Instant::now();
        let (mut fused_results, alignment_stats) =
            fuse_hybrid_results_with_stats(vector_results, bm25_results, &fusion_config);
        if let Some(metrics) = &self.search_metrics {
            metrics.record_hybrid_alignment(
                alignment_stats.vector_keys,
                alignment_stats.bm25_keys,
                alignment_stats.matched_keys,
            );
        }
        tracing::trace!(
            count = fused_results.len(),
            elapsed_ms = fusion_start.elapsed().as_millis(),
            "Hybrid fusion completed"
        );

        fused_results = self.apply_glob_filter(fused_results, options)?;
        self.enrich_results(
            &mut fused_results,
            options.project_id,
            query_filter,
            options.config.result.max_content_tokens,
        )
        .await;
        self.post_process_results(fused_results, options).await
    }

    /// Single-path dense vector recall over chunk embeddings.
    async fn run_vector_recall(
        &self,
        options: &QueryOptions,
        query_filter: &QueryFilter,
    ) -> Result<Vec<SearchResult>> {
        use crate::query::retrieval::strategies::RecallAlgorithm;

        let recall_algo = RecallAlgorithm::Dense;
        let retrieval = recall_algo.create_strategy(self);
        tracing::trace!(algorithm = %recall_algo, "Starting retrieval");
        let retrieval_start = std::time::Instant::now();
        let mut results = retrieval
            .retrieve(options, query_filter)
            .await
            .map_err(|e| {
                QueryError::retryable(
                    &recall_algo.to_string(),
                    format!("{} retrieval failed: {}", recall_algo, e),
                )
            })?;
        tracing::trace!(
            count = results.len(),
            elapsed_ms = retrieval_start.elapsed().as_millis(),
            "Retrieval completed"
        );

        // Glob filter uses the index-carried file path, so it runs before
        // enrichment to keep the SQLite lookups on the surviving set.
        results = self.apply_glob_filter(results, options)?;
        self.apply_score_normalization(&mut results, options);
        self.enrich_results(
            &mut results,
            options.project_id,
            query_filter,
            options.config.result.max_content_tokens,
        )
        .await;
        self.post_process_results(results, options).await
    }

    /// Apply include/exclude path patterns to a result set.
    fn apply_glob_filter(
        &self,
        results: Vec<SearchResult>,
        options: &QueryOptions,
    ) -> Result<Vec<SearchResult>> {
        self.glob_filter.apply(
            results,
            &options.include_patterns,
            &options.exclude_patterns,
        )
    }

    /// Optional score normalization, applied uniformly across all strategies
    /// when `config.score.enable` is set so the final threshold sees one
    /// score scale. Fields carrying the same raw score (`vector_score`,
    /// `bm25_score`) are mirrored; fields that already diverged from the
    /// effective score stay untouched.
    fn apply_score_normalization(&self, results: &mut [SearchResult], options: &QueryOptions) {
        if !options.config.score.enable || results.is_empty() {
            return;
        }
        tracing::trace!(
            strategy = ?options.config.score.strategy,
            "Applying score normalization"
        );
        let mut scores: Vec<f32> = results.iter().map(|r| r.score).collect();
        if let Err(e) =
            crate::query::boost::normalize_scores(&mut scores, &options.config.score.strategy)
        {
            tracing::warn!(error = %e, "Score normalization failed, using unnormalized scores");
            return;
        }
        for (result, new_score) in results.iter_mut().zip(scores) {
            let old = result.score;
            result.score = new_score;
            if (result.vector_score - old).abs() <= f32::EPSILON {
                result.vector_score = new_score;
            }
            if result
                .bm25_score
                .is_some_and(|b| (b - old).abs() <= f32::EPSILON)
            {
                result.bm25_score = Some(new_score);
            }
        }
    }

    /// Batch-enrich results from relation-store chunk records (content/snippet,
    /// line ranges, kind, entity fallback). Lookup failures degrade to
    /// reference results; enrichment is data-only and never fails the request.
    /// Without a relation store empty bodies still downgrade honestly instead
    /// of keeping a full state with no content.
    async fn enrich_results(
        &self,
        results: &mut Vec<SearchResult>,
        project_id: i64,
        query_filter: &QueryFilter,
        max_content_tokens: usize,
    ) {
        use crate::query::retrieval::post_processing::{
            enrich_results as enrich_results_batch, get_chunk_records_from_store,
            resolve_project_root_from_store,
        };

        if results.is_empty() {
            return;
        }
        let Some(store) = &self.relation else {
            crate::query::retrieval::post_processing::downgrade_empty_bodies(results);
            return;
        };
        let point_ids: Vec<String> = results.iter().map(|r| r.id.clone()).collect();
        let records =
            match get_chunk_records_from_store(store, &point_ids, project_id, query_filter).await {
                Ok(Some(records)) => records,
                Ok(None) => {
                    crate::query::retrieval::post_processing::downgrade_empty_bodies(results);
                    return;
                }
                Err(e) => {
                    tracing::warn!("Chunk enrichment failed: {e}");
                    crate::query::retrieval::post_processing::downgrade_empty_bodies(results);
                    return;
                }
            };
        let project_root = resolve_project_root_from_store(store, project_id).await;
        enrich_results_batch(
            results,
            &records,
            project_root.as_deref(),
            max_content_tokens,
        );
    }
}
