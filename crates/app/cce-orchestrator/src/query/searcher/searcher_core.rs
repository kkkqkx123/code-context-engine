//! Unified searcher implementation
//!
//! Provides a unified search interface that combines vector retrieval,
//! BM25 enhancement, and result ranking into a cohesive search flow.
//!
//! # Architecture
//!
//! The searcher delegates to specialized components:
//! - ResultProcessor: Ranking, filtering, and threshold application
//! - AssemblyHandler: SPSR-Graph assembly operations

use std::sync::Arc;

use cce_config::project_registry::ProjectScope;

use crate::query::assembly::AssemblyHandler;
use crate::query::boost::{RelationBoost, SummaryBoost, apply_boosts};
use crate::query::error::QueryError;
use crate::query::error::Result;
use crate::query::ranking::{LlmReranker, PluginReranker, ScoreSorter, ThresholdFilter};
use crate::query::retrieval::post_processing::GlobFilter;
use crate::query::types::{ExecutionStrategy, QueryOptions, QueryResult, SearchResult};
use cce_llm::Embedder;
use cce_metrics::{SearchMetrics, SearchType};

use cce_storage_qdrant::QdrantRetrieval;

use cce_storage_bm25::Bm25Client;
use cce_storage_qdrant::QdrantClient;
use cce_storage_sqlite::SqliteClient;

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
    /// Optional relation graph boost contributor
    pub(crate) relation_boost: Option<Arc<RelationBoost>>,
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
    /// Optional assembly handler for SPSR-Graph assembly
    pub(crate) assembly_handler: Option<Arc<AssemblyHandler>>,
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

use super::search_builder::SearcherBuilder;

impl Searcher {
    /// Create a new searcher builder with a required project scope
    ///
    /// # Example
    ///
    /// ```ignore
    /// let searcher = Searcher::builder(qdrant, embedder, bm25, scope)
    ///     .with_sqlite(sqlite)
    ///     .with_assembler(assembler)
    ///     .with_rerank(rerank_handler)
    ///     .with_relation_boost(relation_searcher)
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
    /// Supports different search strategies based on SearchSources:
    ///    - Bm25Recall: Pure BM25 keyword recall (independent path)
    ///    - HybridRecall: Vector + BM25 parallel recall with weighted normalization fusion
    ///    - DenseRecall: Pure dense vector recall
    /// - WithRelationExpansion: Search with relation expansion
    /// - WithAssembly: Search with SPSR-Graph assembly
    pub async fn search(&self, options: &QueryOptions) -> Result<QueryResult> {
        let project_id = self.scope.project_id();
        if project_id != options.project_id {
            return Err(QueryError::config(&format!(
                "Searcher is bound to project {project_id}, but query requested project {}",
                options.project_id
            )));
        }
        let start = std::time::Instant::now();
        async {
            // Apply query rewriting (QueryRewrite capability) before strategy
            // determination. Plugins chain by priority; on failure the previous
            // query text is kept. The original query is always preserved as the
            // final recall fallback via `QueryOptions::query` rewriting below.
            let mut options = options.clone();
            if options.config.plugin.rewrite_enabled {
                options = self.apply_query_rewrite(options).await?;
            }

            // Determine execution strategy from sources
            let strategy = options.execution_strategy();

            // Execute search flow (retrieval + fusion + ranking)
            let mut results = self.execute_search_flow(&options, &strategy).await?;

            // Apply assembly after search flow if enabled
            if let ExecutionStrategy::WithAssembly {
                depth,
                strategy: expansion_strategy,
                ..
            } = &strategy
            {
                if let Some(ref handler) = self.assembly_handler {
                    tracing::trace!(
                        depth = depth,
                        strategy = ?expansion_strategy,
                        "Applying SPSR-Graph assembly"
                    );
                    let assembly_start = std::time::Instant::now();
                    results = handler
                        .assemble_results(results, *depth, *expansion_strategy)
                        .await?;
                    let assembly_elapsed = assembly_start.elapsed();
                    tracing::trace!(
                        elapsed_ms = assembly_elapsed.as_millis(),
                        "SPSR-Graph assembly completed"
                    );
                }
            }

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
            })
        }
        .await
    }

    /// Execute the complete search flow: retrieval → fusion → ranking
    async fn execute_search_flow(
        &self,
        options: &QueryOptions,
        strategy: &ExecutionStrategy,
    ) -> Result<Vec<SearchResult>> {
        use crate::query::retrieval::post_processing::{
            HybridFusionConfig, fuse_hybrid_results_with_stats,
        };
        use crate::query::retrieval::post_processing::{enrich_from_chunk, get_chunk_records};
        use crate::query::retrieval::strategies::RecallAlgorithm;

        // Get active epoch for version-aware filtering
        let query_filter = self.load_query_filter(options.project_id)?;
        tracing::trace!(
            epoch = query_filter.epoch_value(),
            "Using query filter with epoch"
        );

        // ============================================================================
        // Handle Bm25Recall: pure BM25 keyword recall (no vector dependency)
        // ============================================================================
        if let ExecutionStrategy::Bm25Recall = strategy {
            tracing::trace!("Starting pure BM25 recall");
            let retrieval_start = std::time::Instant::now();

            // BM25 now uses native project_id filtering via the index itself
            let bm25_strategy =
                crate::query::retrieval::strategies::bm25::Bm25Strategy::new(self.bm25.clone());
            let mut results = bm25_strategy.retrieve(options, &query_filter).await?;

            let retrieval_elapsed = retrieval_start.elapsed();
            tracing::trace!(
                count = results.len(),
                elapsed_ms = retrieval_elapsed.as_millis(),
                "BM25 recall completed"
            );

            // Entity enrichment (SQLite lookup for start_line/end_line)
            if let Some(ref sqlite_db) = self.sqlite {
                let point_ids: Vec<String> = results.iter().map(|r| r.id.clone()).collect();
                if !point_ids.is_empty() {
                    match sqlite_db.read_connection() {
                        Ok(conn) => {
                            match get_chunk_records(
                                &conn,
                                &point_ids,
                                options.project_id,
                                &query_filter,
                            ) {
                                Ok(Some(records)) => {
                                    let project_root =
                                        cce_storage_sqlite::source_reader::resolve_project_root(
                                            &conn,
                                            options.project_id,
                                        );
                                    for result in &mut results {
                                        enrich_from_chunk(
                                            result,
                                            &records,
                                            project_root.as_deref(),
                                        );
                                    }
                                }
                                Ok(None) => {}
                                Err(e) => {
                                    tracing::warn!("Chunk enrichment failed: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Failed to get SQLite connection: {}", e);
                        }
                    }
                }
            }

            // Glob filter (include/exclude pattern filtering)
            let results = self.glob_filter.apply(
                results,
                &options.include_patterns,
                &options.exclude_patterns,
            )?;

            // Post-processing (skip BM25 boost step for Bm25Recall)
            return self.post_process_results(results, options).await;
        }

        // ============================================================================
        // Handle HybridRecall: vector + BM25 parallel recall + weighted fusion
        // ============================================================================
        if let ExecutionStrategy::HybridRecall {
            vector_weight,
            bm25_weight,
        } = strategy
        {
            tracing::trace!(
                vector_weight = vector_weight,
                bm25_weight = bm25_weight,
                "Starting hybrid recall (vector + BM25 parallel)"
            );

            let (vector_attempt, bm25_attempt) = tokio::join!(
                // Vector path
                async {
                    let algo = RecallAlgorithm::Dense;
                    let retrieval = algo.create_strategy(self);
                    tracing::trace!("Hybrid: executing vector recall path");
                    let start = std::time::Instant::now();
                    let results = retrieval.retrieve(options, &query_filter).await;
                    let elapsed = start.elapsed();
                    match &results {
                        Ok(r) => tracing::trace!(
                            count = r.len(),
                            elapsed_ms = elapsed.as_millis(),
                            "Hybrid: vector recall path completed"
                        ),
                        Err(e) => tracing::warn!(
                            error = %e,
                            elapsed_ms = elapsed.as_millis(),
                            "Hybrid: vector recall path failed"
                        ),
                    }
                    results
                },
                // BM25 path
                async {
                    let algo = RecallAlgorithm::Bm25;
                    let retrieval = algo.create_strategy(self);
                    tracing::trace!("Hybrid: executing BM25 recall path");
                    let start = std::time::Instant::now();
                    let results = retrieval.retrieve(options, &query_filter).await;
                    let elapsed = start.elapsed();
                    match &results {
                        Ok(r) => tracing::trace!(
                            count = r.len(),
                            elapsed_ms = elapsed.as_millis(),
                            "Hybrid: BM25 recall path completed"
                        ),
                        Err(e) => tracing::warn!(
                            error = %e,
                            elapsed_ms = elapsed.as_millis(),
                            "Hybrid: BM25 recall path failed"
                        ),
                    }
                    results
                }
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

            // Enrich vector results with SQLite data (line numbers, content)
            let mut vector_results = vector_results;
            if !vector_results.is_empty() {
                if let Some(ref sqlite_db) = self.sqlite {
                    let point_ids: Vec<String> =
                        vector_results.iter().map(|r| r.id.clone()).collect();
                    if !point_ids.is_empty() {
                        match sqlite_db.read_connection() {
                            Ok(conn) => {
                                match get_chunk_records(
                                    &conn,
                                    &point_ids,
                                    options.project_id,
                                    &query_filter,
                                ) {
                                    Ok(Some(records)) => {
                                        let project_root =
                                            cce_storage_sqlite::source_reader::resolve_project_root(
                                                &conn,
                                                options.project_id,
                                            );
                                        for result in &mut vector_results {
                                            enrich_from_chunk(
                                                result,
                                                &records,
                                                project_root.as_deref(),
                                            );
                                        }
                                    }
                                    Ok(None) => {}
                                    Err(e) => {
                                        tracing::warn!("Chunk enrichment failed: {}", e);
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!("Failed to get SQLite connection: {}", e);
                            }
                        }
                    }
                }
            }

            // Enrich BM25 results with SQLite data (line numbers, content)
            let mut bm25_results = bm25_results;
            if !bm25_results.is_empty() {
                if let Some(ref sqlite_db) = self.sqlite {
                    let bm25_point_ids: Vec<String> =
                        bm25_results.iter().map(|r| r.id.clone()).collect();
                    if !bm25_point_ids.is_empty() {
                        match sqlite_db.read_connection() {
                            Ok(conn) => {
                                match get_chunk_records(
                                    &conn,
                                    &bm25_point_ids,
                                    options.project_id,
                                    &query_filter,
                                ) {
                                    Ok(Some(records)) => {
                                        let project_root =
                                            cce_storage_sqlite::source_reader::resolve_project_root(
                                                &conn,
                                                options.project_id,
                                            );
                                        for result in &mut bm25_results {
                                            enrich_from_chunk(
                                                result,
                                                &records,
                                                project_root.as_deref(),
                                            );
                                        }
                                    }
                                    Ok(None) => {}
                                    Err(e) => {
                                        tracing::warn!("Chunk enrichment failed: {}", e);
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!("Failed to get SQLite connection: {}", e);
                            }
                        }
                    }
                }
            }

            // Expand multi-entity results before fusion for entity-level alignment
            let vector_results = expand_multi_entity_results(vector_results);
            let bm25_results = expand_multi_entity_results(bm25_results);

            // Fuse results using weighted normalization (before filtering).
            // The alignment coverage stats are produced by fusion and recorded
            // here so the metric and the internal log observe a single pass.
            tracing::trace!("Fusing hybrid results");
            let fusion_start = std::time::Instant::now();
            // include_single_path/min_score/dedup_by_chunk are reserved tuning
            // switches pending the retrieval-method benchmark; the production
            // defaults keep single-path recall included, unbounded, and collapse
            // multi-entity duplicates per physical chunk (dedup_by_chunk).
            let fusion_config = HybridFusionConfig {
                vector_weight: *vector_weight,
                bm25_weight: *bm25_weight,
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
            let (fused_results, alignment_stats) =
                fuse_hybrid_results_with_stats(vector_results, bm25_results, &fusion_config);
            if let Some(metrics) = &self.search_metrics {
                metrics.record_hybrid_alignment(
                    alignment_stats.vector_keys,
                    alignment_stats.bm25_keys,
                    alignment_stats.matched_keys,
                );
            }
            let fusion_elapsed = fusion_start.elapsed();
            tracing::trace!(
                count = fused_results.len(),
                elapsed_ms = fusion_elapsed.as_millis(),
                "Hybrid fusion completed"
            );

            // Apply glob filter to fused results
            let fused_results = self.glob_filter.apply(
                fused_results,
                &options.include_patterns,
                &options.exclude_patterns,
            )?;

            // Post-processing (skip BM25 boost step for hybrid recall)
            return self.post_process_results(fused_results, options).await;
        }

        // ============================================================================
        // Strategy: DenseRecall (pure dense vector recall)
        // ============================================================================
        let recall_algo = match strategy {
            ExecutionStrategy::DenseRecall => RecallAlgorithm::Dense,
            ExecutionStrategy::WithAssembly { base, .. }
            | ExecutionStrategy::WithRelationExpansion { base, .. } => {
                return Box::pin(self.execute_search_flow(options, base)).await;
            }
            ExecutionStrategy::Bm25Recall | ExecutionStrategy::HybridRecall { .. } => {
                unreachable!("Bm25Recall and HybridRecall are handled above")
            }
            ExecutionStrategy::SummaryRecall => RecallAlgorithm::Summary,
        };

        // Step 1: Retrieval phase (pure recall path, no enrichment)
        let retrieval = recall_algo.create_strategy(self);
        tracing::trace!(algorithm = %recall_algo, "Starting retrieval");
        let retrieval_start = std::time::Instant::now();
        let mut results = retrieval
            .retrieve(options, &query_filter)
            .await
            .map_err(|e| {
                QueryError::retryable(
                    &recall_algo.to_string(),
                    format!("{} retrieval failed: {}", recall_algo, e),
                )
            })?;
        let retrieval_elapsed = retrieval_start.elapsed();
        tracing::trace!(
            count = results.len(),
            elapsed_ms = retrieval_elapsed.as_millis(),
            "Retrieval completed"
        );

        // Step 2: Entity enrichment phase (SQLite chunk content lookup)
        if let Some(ref sqlite_db) = self.sqlite {
            let point_ids: Vec<String> = results.iter().map(|r| r.id.clone()).collect();
            if !point_ids.is_empty() {
                match sqlite_db.read_connection() {
                    Ok(conn) => match get_chunk_records(
                        &conn,
                        &point_ids,
                        options.project_id,
                        &query_filter,
                    ) {
                        Ok(Some(records)) => {
                            let project_root =
                                cce_storage_sqlite::source_reader::resolve_project_root(
                                    &conn,
                                    options.project_id,
                                );
                            for result in &mut results {
                                enrich_from_chunk(result, &records, project_root.as_deref());
                            }
                        }
                        Ok(None) => {}
                        Err(e) => {
                            tracing::warn!("Chunk enrichment failed: {}", e);
                        }
                    },
                    Err(e) => {
                        tracing::warn!("Failed to get SQLite connection for enrichment: {}", e);
                    }
                }
            }
        }

        // Glob filter (include/exclude pattern filtering), applied after
        // enrichment (file_path available) but before normalization/boost.
        results = self.glob_filter.apply(
            results,
            &options.include_patterns,
            &options.exclude_patterns,
        )?;

        // Step 3: Score Normalization phase (optional, pre-boost normalization)
        if options.config.score.enable {
            tracing::trace!(
                strategy = ?options.config.score.strategy,
                "Applying pre-boost score normalization"
            );
            let mut scores: Vec<f32> = results.iter().map(|r| r.score).collect();
            if let Err(e) =
                crate::query::boost::normalize_scores(&mut scores, &options.config.score.strategy)
            {
                tracing::warn!(error = %e, "Score normalization failed, using unnormalized scores");
            } else {
                for (result, new_score) in results.iter_mut().zip(scores) {
                    result.score = new_score;
                    result.vector_score = new_score;
                }
            }
        }

        // Step 4: Collect boost contributions from all sources
        let boost_config = &options.config.boost;
        let mut all_contributions = Vec::new();

        // 4a: Summary relevance boost (optional, skip in SummaryRecall mode)
        if !matches!(strategy, ExecutionStrategy::SummaryRecall) && boost_config.enabled {
            if let Some(ref booster) = self.summary_boost {
                if options.sources.summary && options.config.summary.enable_boost {
                    tracing::trace!("Collecting summary relevance boost contributions");
                    match booster
                        .collect(&results, &options.query, &options.config, boost_config)
                        .await
                    {
                        Ok(contribs) => {
                            tracing::trace!(
                                count = contribs.len(),
                                "Summary boost contributions collected"
                            );
                            all_contributions.extend(contribs);
                        }
                        Err(e) => {
                            tracing::warn!("Summary boost collection failed, skipping: {}", e);
                        }
                    }
                }
            }
        }

        // 4b: Relation graph boost (optional, only when WithRelationExpansion strategy)
        if boost_config.enabled {
            if let ExecutionStrategy::WithRelationExpansion { .. } = strategy {
                if let Some(ref booster) = self.relation_boost {
                    if options.sources.relation {
                        tracing::trace!("Collecting relation graph boost contributions");
                        match booster.collect(&results, options, boost_config).await {
                            Ok(contribs) => {
                                tracing::trace!(
                                    count = contribs.len(),
                                    "Relation boost contributions collected"
                                );
                                all_contributions.extend(contribs);
                            }
                            Err(e) => {
                                tracing::warn!("Relation boost collection failed, skipping: {}", e);
                            }
                        }
                    }
                }
            }
        }

        // Step 5: Apply unified boost aggregation
        {
            let boost_start = std::time::Instant::now();
            apply_boosts(&mut results, all_contributions, boost_config);
            let boost_elapsed = boost_start.elapsed();
            tracing::trace!(
                elapsed_ms = boost_elapsed.as_millis(),
                "Unified boost aggregation completed"
            );
        }

        // Step 6: Post-processing phase (rerank, sort, filter)
        self.post_process_results(results, options).await
    }
}
