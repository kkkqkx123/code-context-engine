//! Unified searcher implementation
//!
//! Provides a unified search interface that combines vector retrieval,
//! BM25 enhancement, and result ranking into a cohesive search flow.
//!
//! # Pipeline invariants
//!
//! Every execution strategy runs the same stage contract:
//! retrieval → (hybrid: entity expansion + weighted fusion) → glob filter →
//! optional score normalization → summary boost (dense + hybrid; skipped in
//! SummaryRecall where the summary path is already the scorer) → SQLite chunk
//! enrichment → post-processing (rerank, sort, filter, threshold).
//!
//! - SQLite enrichment is always the last data-shaping step before
//!   post-processing: fusion, filtering, normalization and boost only need
//!   index-carried fields (scores, entity/segment ids, file_path), so the
//!   expensive content lookups run once, on the surviving candidate set.
//! - When `config.score.enable` is set, normalization applies uniformly to
//!   all strategies, so the final `result.min_score` threshold observes one
//!   score scale regardless of recall mode.

use std::sync::Arc;

use cce_config::project_registry::ProjectScope;

use crate::query::boost::{SummaryBoost, apply_boosts};
use crate::query::error::QueryError;
use crate::query::error::Result;
use crate::query::filter::QueryFilter;
use crate::query::ranking::{LlmReranker, PluginReranker, ScoreSorter, ThresholdFilter};
use crate::query::retrieval::post_processing::GlobFilter;
use crate::query::types::{ExecutionStrategy, QueryOptions, QueryResult, SearchResult};
use cce_llm::Embedder;
use cce_metrics::{SearchMetrics, SearchType};

use cce_storage_qdrant::QdrantRetrieval;

use cce_storage_bm25::Bm25Client;
use cce_storage_qdrant::QdrantClient;
use cce_storage_sqlite::SqliteClient;

use super::search_builder::SearcherBuilder;

/// Unified searcher
///
/// Combines vector retrieval and BM25 enhancement into a single search flow.
/// Vector retrieval is the core, BM25 is used for enhancement only.
#[derive(Clone)]

pub struct Searcher {
    /// Qdrant retrieval implementation used by DenseRetrieval strategy
    pub(crate) qdrant_retrieval: Arc<QdrantRetrieval>,
    pub(crate) embedder: Arc<dyn Embedder>,
    pub(crate) bm25: Arc<tokio::sync::Mutex<Bm25Client>>,
    /// SQLite database for chunk content lookup (optional)
    pub(crate) sqlite: Option<Arc<SqliteClient>>,
    /// Optional summary relevance boost contributor
    pub(crate) summary_boost: Option<Arc<SummaryBoost>>,
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
    /// let searcher = Searcher::builder(qdrant, embedder, bm25, scope)
    ///     .with_sqlite(sqlite)
    ///     .with_rerank(rerank_handler)
    ///     .build();
    /// ```
    pub fn builder(
        qdrant: Arc<QdrantClient>,
        embedder: Arc<dyn Embedder>,
        bm25: Arc<tokio::sync::Mutex<Bm25Client>>,
        scope: ProjectScope,
    ) -> SearcherBuilder {
        SearcherBuilder::new(qdrant, embedder, bm25, scope)
    }

    /// Extract the BM25 client from a searcher reference (used by strategy factory).
    ///
    /// This static method provides access to the BM25 client for the BM25 recall strategy,
    /// avoiding circular dependency between Searcher and strategies.
    pub fn extract_bm25_client(searcher: &Self) -> Arc<tokio::sync::Mutex<Bm25Client>> {
        searcher.bm25.clone()
    }

    /// Get the SQLite database reference for project isolation filtering (used by BM25 strategy)
    pub fn get_sqlite(&self) -> Option<Arc<SqliteClient>> {
        self.sqlite.clone()
    }

    /// Execute search with given options
    ///
    /// Loads the active epoch view itself. Callers that already resolved a
    /// [`QueryFilter`] (query cache, retry replay) should use
    /// [`Searcher::search_with_view`] to avoid a second manifest read.
    pub async fn search(&self, options: &QueryOptions) -> Result<QueryResult> {
        let query_filter = self.load_query_filter(options.project_id)?;
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
                "at least one search source (vector, bm25, summary) must be enabled",
            ));
        }
        let start = std::time::Instant::now();

        // Apply query rewriting (QueryRewrite capability) before strategy
        // determination. Plugins chain by priority; on failure the previous
        // query text is kept.
        let mut options = options.clone();
        if options.config.plugin.rewrite_enabled {
            options = self.apply_query_rewrite(options).await?;
        }

        // Determine execution strategy from sources
        let strategy = options.execution_strategy();

        // Execute search flow (retrieval + fusion + ranking)
        let results = self
            .execute_search_flow(&options, &strategy, query_filter)
            .await?;

        let elapsed_ms = start.elapsed().as_millis() as u64;
        tracing::trace!(
            total_elapsed_ms = elapsed_ms,
            strategy = %strategy,
            result_count = results.len(),
            "Search completed"
        );

        // Record search metrics with query type distribution
        if let Some(metrics) = &self.search_metrics {
            metrics.record_search(
                elapsed_ms as f64,
                SearchType::from_label(strategy.query_type_label()),
            );
        }

        Ok(QueryResult {
            total: results.len(),
            items: results,
            elapsed_ms,
            sources: vec![strategy.to_string()],
            sub_queries_count: 1, // Single query by default
            failed_sub_queries: Vec::new(),
        })
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
            ExecutionStrategy::DenseRecall | ExecutionStrategy::SummaryRecall => {
                self.run_vector_recall(options, strategy, query_filter)
                    .await
            }
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
        self.enrich_results(&mut results, options.project_id, query_filter);
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
                QueryError::retryable("qdrant", format!("Vector recall path failed: {e}"))
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

        // include_single_path/min_score/dedup_by_chunk are reserved tuning
        // switches pending the retrieval-method benchmark; the production
        // defaults keep single-path recall included, unbounded, and collapse
        // multi-entity duplicates per physical chunk (dedup_by_chunk).
        let fusion_config = HybridFusionConfig {
            vector_weight,
            bm25_weight,
            ..HybridFusionConfig::default()
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
        self.apply_score_normalization(&mut fused_results, options);
        // Summary boost applies on the fused score (not a third recall path):
        // `sources.summary` in hybrid mode means "boost fused hits by file
        // summary relevance", mirroring the dense-path boost.
        self.apply_summary_boost(&mut fused_results, options).await;
        self.enrich_results(&mut fused_results, options.project_id, query_filter);
        self.post_process_results(fused_results, options).await
    }

    /// Single-path vector recall (dense chunks or file-level summaries),
    /// with optional summary relevance boosting.
    async fn run_vector_recall(
        &self,
        options: &QueryOptions,
        strategy: &ExecutionStrategy,
        query_filter: &QueryFilter,
    ) -> Result<Vec<SearchResult>> {
        use crate::query::retrieval::strategies::RecallAlgorithm;

        let recall_algo = match strategy {
            ExecutionStrategy::DenseRecall => RecallAlgorithm::Dense,
            ExecutionStrategy::SummaryRecall => RecallAlgorithm::Summary,
            _ => unreachable!("only dense and summary strategies reach this path"),
        };

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

        // Summary boost is skipped in SummaryRecall where the summary path is
        // already the scorer.
        if !matches!(strategy, ExecutionStrategy::SummaryRecall) {
            self.apply_summary_boost(&mut results, options).await;
        }

        self.enrich_results(&mut results, options.project_id, query_filter);
        self.post_process_results(results, options).await
    }

    /// Collect summary relevance boost contributions and apply them to the
    /// incoming scores (fused score on hybrid, normalized vector score on
    /// dense). No-op unless the booster exists and both boost gates
    /// (`boost.enabled`, `summary.enable_boost`, `sources.summary`) are set.
    /// Boost collection failure degrades to unboosted scores, never an error.
    async fn apply_summary_boost(&self, results: &mut [SearchResult], options: &QueryOptions) {
        let boost_config = &options.config.boost;
        let Some(ref booster) = self.summary_boost else {
            return;
        };
        if !boost_config.enabled || !options.sources.summary || !options.config.summary.enable_boost
        {
            return;
        }
        tracing::trace!("Collecting summary relevance boost contributions");
        match booster
            .collect(results, &options.query, &options.config, boost_config)
            .await
        {
            Ok(contribs) => {
                tracing::trace!(
                    count = contribs.len(),
                    "Summary boost contributions collected"
                );
                apply_boosts(results, contribs, boost_config);
            }
            Err(e) => {
                tracing::warn!("Summary boost collection failed, skipping: {}", e);
            }
        }
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
    /// effective score (e.g. after boosting) stay untouched.
    fn apply_score_normalization(&self, results: &mut [SearchResult], options: &QueryOptions) {
        if !options.config.score.enable || results.is_empty() {
            return;
        }
        tracing::trace!(
            strategy = ?options.config.score.strategy,
            "Applying pre-boost score normalization"
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

    /// Batch-enrich results from SQLite chunk records (content/snippet, line
    /// ranges, kind, entity fallback). Lookup failures degrade to unenriched
    /// results; enrichment is data-only and never fails the request.
    fn enrich_results(
        &self,
        results: &mut [SearchResult],
        project_id: i64,
        query_filter: &QueryFilter,
    ) {
        use crate::query::retrieval::post_processing::{enrich_from_chunk, get_chunk_records};

        let Some(sqlite_db) = &self.sqlite else {
            return;
        };
        if results.is_empty() {
            return;
        }
        let point_ids: Vec<String> = results.iter().map(|r| r.id.clone()).collect();
        let Ok(conn) = sqlite_db.read_connection() else {
            tracing::warn!("Failed to get SQLite connection for enrichment");
            return;
        };
        match get_chunk_records(&conn, &point_ids, project_id, query_filter) {
            Ok(Some(records)) => {
                let project_root =
                    cce_storage_sqlite::source_reader::resolve_project_root(&conn, project_id);
                for result in results.iter_mut() {
                    enrich_from_chunk(result, &records, project_root.as_deref());
                }
            }
            Ok(None) => {}
            Err(e) => {
                tracing::warn!("Chunk enrichment failed: {}", e);
            }
        }
    }
}
