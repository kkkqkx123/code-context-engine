//! Dense retrieval strategy
//!
//! Orchestrates dense vector semantic search by combining low-level DenseRetrieval
//! (core layer) with embedder and searcher dependencies from the high-level layer.
//!
//! This strategy is suitable for:
//! - Semantic similarity search via vector embeddings
//! - Environments with access to embedder and Qdrant
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
            dense_retrieval: DenseRetrieval::new(searcher.qdrant_retrieval.clone()),
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

        // Step 3: Execute core dense search
        let config = &options.config.vector;
        self.dense_retrieval
            .search(
                query_embedding,
                config.top_k,
                config.min_score,
                Some(config.hnsw_ef as usize),
                filter,
            )
            .await
    }
}

fn build_search_filter(options: &QueryOptions, group_id: &str) -> SearchFilter {
    SearchFilter {
        // The epoch view is applied by DenseStrategy.retrieve() via QueryFilter.
        epochs: Vec::new(),
        excluded_files: None,
        group_id: Some(group_id.to_string()),
        point_type: Some(PointKind::Chunk),
        directory_prefix: options.directory_prefix.clone(),
        exclude_test: options
            .exclude_content_types
            .iter()
            .any(|t| matches!(t, crate::query::types::ExcludableContentType::Test)),
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
