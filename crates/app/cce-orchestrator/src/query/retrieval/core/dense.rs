//! Dense vector retrieval implementation
//!
//! Provides low-level dense vector search operations.
//! This layer handles direct Qdrant interaction and result mapping,
//! separated from high-level orchestration concerns.

use std::collections::HashMap;
use std::sync::Arc;

use crate::query::error::{QueryError, Result};
use crate::query::types::SearchResult;
use cce_storage_common::{DenseSearchQuery, SearchFilter};
use cce_storage_qdrant::QdrantRetrieval;

/// Dense vector retrieval handler
///
/// Provides low-level dense search operations against Qdrant.
/// This is a stateless implementation focused on direct vector search.
#[derive(Clone)]
pub struct DenseRetrieval {
    qdrant_retrieval: Arc<QdrantRetrieval>,
}

impl DenseRetrieval {
    /// Create a new dense retrieval instance
    pub fn new(qdrant_retrieval: Arc<QdrantRetrieval>) -> Self {
        Self { qdrant_retrieval }
    }

    /// Search vectors in Qdrant with pre-computed embedding
    ///
    /// # Arguments
    ///
    /// * `query_embedding` - Pre-computed query embedding vector
    /// * `top_k` - Number of results to return
    /// * `min_score` - Minimum score threshold (0.0 to disable)
    /// * `hnsw_ef` - HNSW search parameter (optional)
    /// * `filter` - Payload filter for result filtering
    ///
    /// # Returns
    ///
    /// Returns the list of search results mapped to SearchResult format
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
                entity_ids: r
                    .payload
                    .entity_ids
                    .clone()
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|&id| id >= 0)
                    .map(|id| cce_types::EntityId(id as u64))
                    .collect(),
                segment_id: r.payload.segment_id.clone(),
                kind: String::new(),
                name: extract_name(&r.id),
                file_path: r.payload.file_path.clone(),
                score: r.score,
                original_score: r.score,
                vector_score: r.score,
                bm25_score: None,
                sources: vec!["vector".to_string()],
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

/// Best-effort name derived from the point id before SQLite enrichment.
///
/// This is a temporary placeholder: `enrich_from_chunk` overwrites `name` with
/// the chunk's `entity_names` whenever the SQLite record is available. Kept so
/// unenriched results (no metadata store) still carry a readable label.
fn extract_name(id: &str) -> String {
    id.split(':').nth(1).unwrap_or(id).to_string()
}
