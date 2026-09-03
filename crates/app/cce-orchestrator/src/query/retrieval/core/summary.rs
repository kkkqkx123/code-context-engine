//! Summary vector retrieval implementation
//!
//! Provides low-level summary vector search operations against Qdrant.
//! This layer handles direct Qdrant interaction and result mapping for
//! file-level summary vectors, distinguished from chunk vectors by
//! the `type = "summary"` payload field.

use std::collections::HashMap;
use std::sync::Arc;

use crate::query::error::{QueryError, Result};
use crate::query::types::SearchResult;
use cce_storage_common::{DenseSearchQuery, SearchFilter};
use cce_storage_qdrant::QdrantRetrieval;

/// Summary vector retrieval handler
///
/// Provides low-level vector search operations against the summary index.
/// This is a stateless implementation focused on file-level summary search.
#[derive(Clone)]
pub struct SummaryRetrieval {
    qdrant_retrieval: Arc<QdrantRetrieval>,
}

impl SummaryRetrieval {
    /// Create a new summary retrieval instance
    pub fn new(qdrant_retrieval: Arc<QdrantRetrieval>) -> Self {
        Self { qdrant_retrieval }
    }

    /// Search summary vectors in Qdrant with pre-computed embedding
    ///
    /// Returns file-level results from the summary index.
    pub async fn search(
        &self,
        query_embedding: Vec<f32>,
        top_k: usize,
        min_score: f32,
        hnsw_ef: Option<usize>,
        filter: SearchFilter,
    ) -> Result<Vec<SearchResult>> {
        let mut dense_query = DenseSearchQuery::new(query_embedding, top_k);

        if min_score > 0.0 {
            dense_query = dense_query.with_score_threshold(min_score);
        }

        if let Some(ef) = hnsw_ef {
            dense_query = dense_query.with_hnsw_ef(ef as u64);
        }

        dense_query = dense_query.with_filter(filter);

        let results = self
            .qdrant_retrieval
            .search_dense(dense_query)
            .await
            .map_err(|e| {
                QueryError::Vector(cce_llm_client::LlmError::Http(
                    cce_types::error::common::HttpError::new(e.to_string()),
                ))
            })?;

        let search_results: Vec<SearchResult> = results
            .into_iter()
            .map(|r| SearchResult {
                id: r.id.clone(),
                entity_ids: Vec::new(),
                segment_id: None,
                kind: "summary".to_string(),
                name: extract_name(&r.id),
                file_path: r.payload.file_path.clone(),
                score: r.score,
                original_score: r.score,
                vector_score: r.score,
                bm25_score: None,
                sources: vec!["summary".to_string()],
                snippet: None,
                content: String::new(),
                start_line: 0,
                end_line: 0,
                is_boosted: false,
                boost_reason: None,
                relations: None,
                metadata: HashMap::new(),
                pattern_info: None,
                category: None,
            })
            .collect();

        Ok(search_results)
    }
}

fn extract_name(id: &str) -> String {
    id.split("::").last().unwrap_or(id).to_string()
}
