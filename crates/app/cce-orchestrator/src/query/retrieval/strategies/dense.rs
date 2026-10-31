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

use crate::query::error::Result;
use crate::query::filter::QueryFilter;
use crate::query::retrieval::core::dense::DenseRetrieval;
use crate::query::types::{QueryOptions, SearchResult};
use cce_storage_common::SearchFilter;
use cce_types::PointKind;

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
    pub async fn retrieve(
        &self,
        options: &QueryOptions,
        query_filter: &QueryFilter,
    ) -> Result<Vec<SearchResult>> {
        // Step 1: Compute query embedding
        let query_embedding = self
            .searcher
            .embedder
            .embed_one(&options.query)
            .await
            .map_err(crate::query::error::QueryError::Vector)?;

        // Step 2: Build filter from options and apply epoch filtering
        let mut filter = build_search_filter(options, self.searcher.scope.project_group_id());
        filter = query_filter.apply_to_search_filter(filter);

        // Step 3: Execute core dense search with over-fetch when glob
        // patterns are present, then filter locally inside the strategy.
        let config = &options.config.vector;
        let has_glob = !options.include_patterns.is_empty() || !options.exclude_patterns.is_empty();
        let fetch_k = if has_glob {
            (config.top_k.saturating_mul(GLOB_OVER_FETCH_FACTOR)).max(config.top_k + 20)
        } else {
            config.top_k
        };
        let raw = self
            .dense_retrieval
            .search(
                query_embedding,
                fetch_k,
                config.min_score,
                Some(config.hnsw_ef as usize),
                filter,
            )
            .await?;
        Ok(apply_strategy_filters(raw, options, config.top_k))
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
        exclude_test: options.exclude_content_types.iter().any(|t| {
            matches!(
                t,
                crate::query::types::ExcludableContentType::Test
                    | crate::query::types::ExcludableContentType::Generated
                    | crate::query::types::ExcludableContentType::Dependency
            )
        }),
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

/// Local glob + generated/dependency filtering for dense candidates.
///
/// Runs inside the strategy so the searcher no longer pays a post-fusion
/// filter pass. Keeps the configured ordering and truncates to `limit`.
fn apply_strategy_filters(
    results: Vec<crate::query::types::SearchResult>,
    options: &QueryOptions,
    limit: usize,
) -> Vec<crate::query::types::SearchResult> {
    use crate::query::retrieval::post_processing::GlobFilter;
    let filter = GlobFilter::new();
    let filtered = filter
        .apply(results, &options.include_patterns, &options.exclude_patterns)
        .unwrap_or_default();
    let mut kept = Vec::with_capacity(filtered.len().min(limit));
    for item in filtered {
        if crate::query::types::query_options::path_excluded_by_types(
            &item.file_path,
            &options.exclude_content_types,
        ) {
            continue;
        }
        kept.push(item);
        if kept.len() >= limit {
            break;
        }
    }
    kept
}
