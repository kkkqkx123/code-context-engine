//! Summary retrieval strategy
//!
//! Orchestrates file-level summary vector search by combining low-level SummaryRetrieval
//! (core layer) with embedder and searcher dependencies from the high-level layer.
//!
//! This strategy is suitable for:
//! - SummaryOnly search mode (query only summary vectors, not chunks)
//! - File-level relevance filtering before detailed chunk search
//! - Environments with access to embedder and Qdrant

use std::sync::Arc;

use crate::query::error::Result;
use crate::query::filter::QueryFilter;
use crate::query::retrieval::core::summary::SummaryRetrieval;
use crate::query::types::{QueryOptions, SearchResult};
use cce_storage_common::SearchFilter;
use cce_types::PointKind;

/// Summary retrieval strategy — orchestrates embedder + summary vector search
///
/// Combines embedder service with core SummaryRetrieval to provide
/// file-level semantic search against summary vectors.
#[derive(Clone)]
pub struct SummaryStrategy {
    summary_retrieval: SummaryRetrieval,
    searcher: Arc<crate::query::Searcher>,
}

impl SummaryStrategy {
    pub fn new(searcher: &crate::query::Searcher) -> Self {
        Self {
            summary_retrieval: SummaryRetrieval::new(searcher.qdrant_retrieval.clone()),
            searcher: Arc::new(searcher.clone()),
        }
    }

    /// Execute summary vector retrieval with embedding computation
    ///
    /// Returns file-level results from the summary index. Results contain
    /// file_path and score but no chunk-level details (content, snippet, line numbers).
    pub async fn retrieve(
        &self,
        options: &QueryOptions,
        query_filter: &QueryFilter,
    ) -> Result<Vec<SearchResult>> {
        let query_embedding = self
            .searcher
            .embedder
            .embed_one(&options.query)
            .await
            .map_err(crate::query::error::QueryError::Vector)?;

        let mut filter =
            build_summary_search_filter(options, self.searcher.scope.project_group_id());
        filter = query_filter.apply_to_search_filter(filter);

        let config = &options.config.vector;
        self.summary_retrieval
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

fn build_summary_search_filter(options: &QueryOptions, group_id: &str) -> SearchFilter {
    SearchFilter {
        // The epoch view is applied by SummaryStrategy.retrieve() via QueryFilter.
        epochs: Vec::new(),
        excluded_files: None,
        group_id: Some(group_id.to_string()),
        point_type: Some(PointKind::Summary),
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
