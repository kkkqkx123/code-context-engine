//! Dense retrieval strategy
//!
//! Orchestrates dense vector semantic search by combining low-level DenseRetrieval
//! (core layer) with embedder and searcher dependencies from the high-level layer.
//!
//! This strategy is suitable for:
//! - Semantic similarity search via vector embeddings
//! - Environments with access to embedder and vector backend
//! - Dense-only search mode

use std::sync::Arc;
use std::time::Instant;

use crate::query::error::Result;
use crate::query::filter::QueryFilter;
use crate::query::retrieval::core::dense::DenseRetrieval;
use crate::query::types::{QueryOptions, SearchResult};
use cce_llm::EmbeddingProvider as _;
use cce_storage_common::SearchFilter;
use cce_types::PointKind;

/// Raw dense retrieval result with per-call timing.
pub(super) struct DenseRetrievalOutcome {
    pub results: Result<Vec<SearchResult>>,
    pub embedding_ms: u64,
    pub embedding_provider_ms: u64,
    pub embedding_overhead_ms: u64,
    pub vector_search_ms: u64,
}

/// Dense retrieval strategy — orchestrates embedder + vector search
///
/// Combines embedder service with core DenseRetrieval to provide
/// semantic search capabilities. This strategy layer handles:
/// - Query embedding computation
/// - Filter construction from options
/// - Result post-processing (if needed)
#[derive(Clone)]
pub struct DenseStrategy {
    dense_retrieval: DenseRetrieval,
    searcher: Arc<crate::query::Searcher>,
}

impl DenseStrategy {
    pub fn new(searcher: &crate::query::Searcher) -> Self {
        Self {
            dense_retrieval: DenseRetrieval::new(searcher.vector.clone()),
            searcher: Arc::new(searcher.clone()),
        }
    }

    /// Execute dense vector retrieval with embedding computation
    ///
    /// Returns raw search results with basic fields populated. Enrichment
    /// (content, snippet, line numbers, entity mapping) is applied later in
    /// the search pipeline.
    pub(super) async fn retrieve(
        &self,
        options: &QueryOptions,
        query_filter: &QueryFilter,
    ) -> DenseRetrievalOutcome {
        // Step 1: Compute query embedding
        let embedding_started = Instant::now();
        let query_embedding = match self.searcher.embedder.embed_one(&options.query).await {
            Ok(embedding) => embedding,
            Err(error) => {
                return DenseRetrievalOutcome {
                    results: Err(crate::query::error::QueryError::Vector(error)),
                    embedding_ms: embedding_started.elapsed().as_millis() as u64,
                    embedding_provider_ms: 0,
                    embedding_overhead_ms: 0,
                    vector_search_ms: 0,
                };
            }
        };
        let embedding_ms = embedding_started.elapsed().as_millis() as u64;
        let embedding_timing = self.searcher.embedder.last_embedding_timing();

        // Step 2: Build filter from options and apply epoch filtering
        let mut filter = build_search_filter(options, self.searcher.scope.project_group_id());
        filter = query_filter.apply_to_search_filter(filter);

        // Step 3: Execute core dense search with over-fetch only when glob
        // patterns may drop candidates. Content-type exclusions are indexed
        // markers pushed to storage, so they never need over-fetch.
        let config = &options.config.vector;
        let needs_over_fetch =
            !options.include_patterns.is_empty() || !options.exclude_patterns.is_empty();
        let fetch_k = if needs_over_fetch {
            (config.top_k.saturating_mul(GLOB_OVER_FETCH_FACTOR)).max(config.top_k + 20)
        } else {
            config.top_k
        };
        let search_started = Instant::now();
        let raw = match self
            .dense_retrieval
            .search(
                query_embedding,
                fetch_k,
                config.similarity_threshold,
                Some(config.hnsw_ef as usize),
                filter,
            )
            .await
        {
            Ok(raw) => raw,
            Err(error) => {
                return DenseRetrievalOutcome {
                    results: Err(error),
                    embedding_ms,
                    embedding_provider_ms: embedding_timing.provider_ms,
                    embedding_overhead_ms: embedding_timing.overhead_ms,
                    vector_search_ms: search_started.elapsed().as_millis() as u64,
                };
            }
        };
        let vector_search_ms = search_started.elapsed().as_millis() as u64;
        DenseRetrievalOutcome {
            results: Ok(apply_strategy_filters(raw, options, config.top_k)),
            embedding_ms,
            embedding_provider_ms: embedding_timing.provider_ms,
            embedding_overhead_ms: embedding_timing.overhead_ms,
            vector_search_ms,
        }
    }
}

fn build_search_filter(options: &QueryOptions, group_id: &str) -> SearchFilter {
    SearchFilter {
        epochs: Vec::new(),
        excluded_files: None,
        group_id: Some(group_id.to_string()),
        point_type: Some(PointKind::Chunk),
        file_path: None,
        directory_prefix: options.directory_prefix.clone(),
        exclude_test: options
            .exclude_content_types
            .contains(&crate::query::types::ExcludableContentType::Test),
        exclude_generated: options
            .exclude_content_types
            .contains(&crate::query::types::ExcludableContentType::Generated),
        exclude_dependency: options
            .exclude_content_types
            .contains(&crate::query::types::ExcludableContentType::Dependency),
        include_categories: if options.include_categories.is_empty() {
            None
        } else {
            Some(options.include_categories.clone())
        },
        exclude_categories: if options.exclude_categories.is_empty() {
            None
        } else {
            Some(options.exclude_categories.clone())
        },
        raw_filter: None,
    }
}

/// Over-recall factor applied when glob patterns are present.
///
/// Storage backends do not evaluate glob patterns, so the strategy fetches
/// extra candidates, filters them locally, and keeps the configured limit.
const GLOB_OVER_FETCH_FACTOR: usize = 3;

/// Local glob filtering for dense candidates.
///
/// Only glob patterns stay here: content-type exclusions are indexed markers
/// already enforced by storage pushdown. Keeps the configured ordering and
/// truncates to `limit`.
fn apply_strategy_filters(
    results: Vec<crate::query::types::SearchResult>,
    options: &QueryOptions,
    limit: usize,
) -> Vec<crate::query::types::SearchResult> {
    use crate::query::retrieval::primitives::GlobFilter;
    let filter = GlobFilter::new();
    let filtered = filter
        .apply(
            results,
            &options.include_patterns,
            &options.exclude_patterns,
        )
        .unwrap_or_default();
    if filtered.len() > limit {
        filtered.into_iter().take(limit).collect()
    } else {
        filtered
    }
}
