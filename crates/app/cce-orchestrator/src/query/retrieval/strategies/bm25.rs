//! BM25 retrieval strategy
//!
//! Pure BM25 keyword-based recall without vector search dependency.
//! Executes tantivy BM25 search and maps results to the unified SearchResult type.
//!
//! This strategy is suitable for:
//! - Precise combined keyword queries
//! - Environments without embedder/vector DB
//! - BM25-only search mode
//!
//! # Project Isolation
//!
//! BM25 uses native project_id field filtering for multi-tenant isolation.
//! This eliminates dependency on external SQLite queries for project scoping,
//! improving query performance by reducing round-trips to external databases.

use std::collections::HashMap;
use std::sync::Arc;

use crate::query::error::{QueryError, Result};
use crate::query::filter::QueryFilter;
use crate::query::types::{QueryOptions, SearchResult};
use cce_storage_bm25::Bm25Client;
use cce_storage_bm25::{Bm25Retrieval, Bm25SearchOptions};

/// BM25 retrieval strategy — pure keyword-based recall with native project isolation
///
/// Performs native project isolation using the project_id field stored in the BM25 index.
/// This eliminates the need for external SQLite verification and improves query performance.
#[derive(Clone)]
pub struct Bm25Strategy {
    bm25_retrieval: Bm25Retrieval,
    bm25_client: Arc<tokio::sync::Mutex<Bm25Client>>,
}

impl Bm25Strategy {
    pub fn new(bm25_client: Arc<tokio::sync::Mutex<Bm25Client>>) -> Self {
        Self {
            bm25_retrieval: Bm25Retrieval::new(),
            bm25_client,
        }
    }

    /// Create BM25 strategy (alias for new)
    pub fn with_client(bm25_client: Arc<tokio::sync::Mutex<Bm25Client>>) -> Self {
        Self::new(bm25_client)
    }

    /// Execute BM25 keyword retrieval with native project isolation
    ///
    /// Returns raw search results with fields populated from BM25 index.
    /// Unlike vector retrieval, BM25 results include content directly,
    /// so start_line/end_line are not available unless enriched by SQLite.
    ///
    /// Project isolation is performed natively in the BM25 index using the
    /// project_id field, eliminating dependency on SQLite for query filtering.
    /// This improves performance by reducing external lookups.
    pub async fn retrieve(
        &self,
        options: &QueryOptions,
        query_filter: &QueryFilter,
    ) -> Result<Vec<SearchResult>> {
        // Step 1: Acquire BM25 index resources
        let client = self.bm25_client.lock().await;
        let manager = match client.index_manager() {
            Some(m) => m,
            None => {
                return Err(QueryError::Config(
                    "BM25 index manager not available".to_string(),
                ));
            }
        };
        let manager_guard = manager.read().await;
        let schema = manager_guard.schema();

        // Step 2: Build retrieval options with native project_id and epoch filters
        let limit = options.config.vector.top_k.max(options.config.result.limit);
        let retrieval_options = Bm25SearchOptions {
            limit,
            offset: 0,
            field_weights: options.config.bm25.field_weights.clone(),
            highlight: false,
            project_id: options.project_id,
            epochs: query_filter.epochs(),
            excluded_files: if query_filter.excluded_files().is_empty() {
                None
            } else {
                Some(query_filter.excluded_files().to_vec())
            },
            exclude_test: options
                .exclude_content_types
                .iter()
                .any(|t| matches!(t, crate::query::types::ExcludableContentType::Test)),
            include_categories: options.include_categories.clone(),
            exclude_categories: options.exclude_categories.clone(),
            term_operator: options.config.bm25.term_operator,
        };

        // Step 3: Execute BM25 search with project and epoch isolation
        let bm25_results = self
            .bm25_retrieval
            .search(&manager_guard, schema, &options.query, &retrieval_options)
            .map_err(classify_bm25_error)?;

        // Step 4: Convert BM25 results to unified SearchResult
        let search_results: Vec<SearchResult> = bm25_results
            .into_iter()
            .map(|r| {
                let chunk_id = r
                    .fields
                    .get("chunk_id")
                    .cloned()
                    .unwrap_or_else(|| r.document_id.clone());
                let file_path = r.fields.get("file_path").cloned().unwrap_or_default();
                let title = r.fields.get("title").cloned().unwrap_or_default();
                // Decode the entity list via the shared chunk-entity codec
                let entity_ids: Vec<cce_types::EntityId> = r
                    .fields
                    .get("entity_id")
                    .map(|s| cce_types::ChunkEntityRefs::parse_bm25_csv(s))
                    .unwrap_or_default();
                // content is an index-only BM25 field (not stored); it is
                // enriched from SQLite by the post-processing pipeline.
                let content = String::new();
                let bm25_score = r.score;

                SearchResult {
                    id: chunk_id,
                    entity_ids,
                    segment_id: r.fields.get("segment_id").cloned(),
                    kind: String::new(),
                    name: title,
                    file_path,
                    score: r.score,
                    original_score: r.score,
                    vector_score: 0.0,
                    bm25_score: Some(bm25_score),
                    sources: vec!["bm25".to_string()],
                    snippet: None,
                    content,
                    start_line: 0,
                    end_line: 0,
                    is_boosted: false,
                    boost_reason: None,
                    relations: None,
                    metadata: HashMap::new(),
                    pattern_info: None,
                    category: None,
                }
            })
            .collect();

        Ok(search_results)
    }
}

/// Classify a BM25 retrieval error.
///
/// Only genuine configuration problems stay `Config`; runtime failures
/// (I/O, corrupted segments, schema/document errors) are transient and become
/// retryable so the query can be reprocessed once the service recovers.
fn classify_bm25_error(error: cce_storage_bm25::Bm25Error) -> QueryError {
    use cce_types::error::common::ErrorClassify;
    if error.is_permanent() {
        QueryError::config(&format!("BM25 search failed: {error}"))
    } else {
        QueryError::retryable("bm25", format!("BM25 search failed: {error}"))
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_parse_comma_separated_entity_ids() {
        // The parsing logic is delegated to the shared chunk-entity codec
        let parse_fn =
            |s: &str| -> Vec<cce_types::EntityId> { cce_types::ChunkEntityRefs::parse_bm25_csv(s) };

        // Multiple entity_ids
        let ids = parse_fn("100,200,300");
        assert_eq!(ids.len(), 3);
        assert_eq!(ids[0], cce_types::EntityId(100));
        assert_eq!(ids[1], cce_types::EntityId(200));
        assert_eq!(ids[2], cce_types::EntityId(300));

        // Single entity_id (backward compatible with old index)
        let ids = parse_fn("42");
        assert_eq!(ids.len(), 1);
        assert_eq!(ids[0], cce_types::EntityId(42));

        // Empty string
        let ids = parse_fn("");
        assert!(ids.is_empty());

        // Whitespace handling
        let ids = parse_fn(" 10 , 20 , 30 ");
        assert_eq!(ids.len(), 3);
    }

    #[test]
    fn test_classify_bm25_error_config_variants() {
        use super::classify_bm25_error;
        use crate::query::error::QueryError;
        use cce_storage_bm25::Bm25Error;

        let config_error = classify_bm25_error(Bm25Error::Disabled);
        assert!(matches!(config_error, QueryError::Config(_)));
        assert!(!config_error.is_retryable());
        assert!(config_error.is_config_error());
    }

    #[test]
    fn test_classify_bm25_error_runtime_variants() {
        use super::classify_bm25_error;
        use crate::query::error::QueryError;
        use cce_storage_bm25::Bm25Error;

        let runtime_error = classify_bm25_error(Bm25Error::Search("reader unavailable".into()));
        match &runtime_error {
            QueryError::Retryable { service, message } => {
                assert_eq!(service, "bm25");
                assert!(message.contains("BM25 search failed"));
            }
            other => panic!("expected retryable error, got {:?}", other),
        }
        assert!(runtime_error.is_retryable());
        assert!(!runtime_error.is_config_error());
    }
}
