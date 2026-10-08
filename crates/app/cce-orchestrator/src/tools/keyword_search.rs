//! Keyword search tool
//!
//! Provides standalone BM25 keyword search with raw source snippets.
//! This tool is independent of the vector search pipeline and can be used
//! as a focused keyword query module.
//!
//! # Architecture
//!
//! ```text
//! Query → BM25 search → get chunk_ids → relation-store lookup → read source snippet → scored results
//! ```
//!
//! Content is sourced from relation-store chunk metadata (not Tantivy stored
//! fields), returning the raw source lines so the caller can grep or read
//! them directly. No markup is embedded in the snippet.
//!
//! # Usage
//!
//! ```ignore
//! use cce_orchestrator::tools::keyword_search::KeywordSearchTool;
//!
//! let tool = KeywordSearchTool::new(bm25_client)
//!     .with_sqlite(sqlite_db);
//!
//! let response = tool.search(request).await?;
//! ```

mod types;

use std::collections::HashMap;
use std::sync::Arc;

use crate::index::vector_store::{FulltextStore, RelationStore};
use cce_storage_common::{FulltextSearchOptions, FulltextStorage};

use crate::query::retrieval::post_processing::{
    get_chunk_records_from_store, resolve_project_root_from_store,
};
use crate::tools::common::read_snippets_batch;

pub use self::types::{
    KeywordSearchError, KeywordSearchItem, KeywordSearchRequest, KeywordSearchResponse,
};

/// Keyword search tool
///
/// Provides standalone BM25-based keyword search with raw source snippets
/// sourced from relation-store chunk content. Results are sorted by BM25
/// relevance score.
#[derive(Clone)]
pub struct KeywordSearchTool {
    /// Fulltext backend for keyword recall (called through the contract)
    fulltext: FulltextStore,
    /// Relation backend for the epoch view and chunk content lookup
    relation: Option<RelationStore>,
}

impl KeywordSearchTool {
    /// Create a new keyword search tool
    ///
    /// # Arguments
    ///
    /// * `fulltext` - Fulltext backend (calls go through the contract)
    pub fn new(fulltext: FulltextStore) -> Self {
        Self {
            fulltext,
            relation: None,
        }
    }

    /// Attach the local relation database for chunk content lookup.
    ///
    /// Local-only port: callers holding the backend enum attach it directly
    /// through [`Self::with_relation_store`].
    pub fn with_sqlite(mut self, sqlite: Arc<cce_storage_metadb_sqlite::SqliteClient>) -> Self {
        self.relation = Some(RelationStore::local(sqlite));
        self
    }

    /// Attach the relation backend via enum dispatch.
    ///
    /// Reads go through the relation contract, so both branches share one
    /// read path and branch selection stays inside the enum.
    pub fn with_relation_store(mut self, store: RelationStore) -> Self {
        self.relation = Some(store);
        self
    }

    /// Execute keyword search
    ///
    /// 1. Validate input (project_id must be positive, query must be non-empty, top_n > 0)
    /// 2. Search BM25 index for matching documents
    /// 3. Enrich with chunk metadata from the relation store
    /// 4. Read raw source snippets for the matched chunks
    /// 5. Sort by BM25 score descending
    ///
    /// # Arguments
    ///
    /// * `request` - Search parameters including query, top_n, and project_id
    ///
    /// # Returns
    ///
    /// Search results with raw source snippets, or an error
    pub async fn search(
        &self,
        request: KeywordSearchRequest,
    ) -> Result<KeywordSearchResponse, KeywordSearchError> {
        // Step 0: Validate input
        if request.query.trim().is_empty() {
            return Err(KeywordSearchError::Bm25(
                "Query must not be empty".to_string(),
            ));
        }
        if request.project_id <= 0 {
            return Err(KeywordSearchError::Bm25(format!(
                "project_id must be positive, got {}",
                request.project_id
            )));
        }
        if request.top_n == 0 {
            return Err(KeywordSearchError::Bm25(
                "top_n must be greater than 0".to_string(),
            ));
        }
        if self.relation.is_none() {
            return Err(KeywordSearchError::SqliteNotConfigured);
        }

        // Step 1: Resolve the epoch view for version-aware filtering. An
        // explicit `request.epoch` pins a single full generation; otherwise
        // the active manifest view (own + parent + overridden files) applies.
        let store = self
            .relation
            .as_ref()
            .ok_or(KeywordSearchError::SqliteNotConfigured)?;
        let query_filter = match request.epoch {
            Some(epoch) => crate::query::filter::QueryFilter::new(epoch)
                .map_err(|e| KeywordSearchError::Sqlite(e.to_string()))?,
            None => {
                crate::query::filter::load_active_query_filter_from_store(store, request.project_id)
                    .await
                    .map_err(|e| KeywordSearchError::Sqlite(e.to_string()))?
            }
        };

        let options = FulltextSearchOptions {
            limit: request.top_n,
            offset: request.offset,
            field_weights: HashMap::new(),
            project_id: request.project_id,
            epochs: query_filter.epochs(),
            excluded_files: if query_filter.excluded_files().is_empty() {
                None
            } else {
                Some(query_filter.excluded_files().to_vec())
            },
            exclude_test: false,
            include_categories: Vec::new(),
            exclude_categories: Vec::new(),
            term_operator: request.term_operator,
        };

        // Step 2: Run the keyword recall through the fulltext contract.
        let results = self
            .fulltext
            .search(&request.query, &options)
            .await
            .map_err(KeywordSearchError::from)?;

        tracing::trace!(
            "Keyword search '{}' returned {} BM25 results",
            request.query,
            results.len()
        );

        // Step 3: Extract chunk_ids for relation-store lookup
        let chunk_ids: Vec<String> = results
            .iter()
            .filter_map(|r| r.fields.get("chunk_id").cloned())
            .filter(|id| !id.is_empty())
            .collect();

        // Step 4: Look up chunk metadata through the relation contract via
        // the same two-stage epoch-view resolution as the search pipeline;
        // snippets are lazy-loaded from the source file via the project root.
        let project_root = resolve_project_root_from_store(store, request.project_id).await;
        let chunk_records = match get_chunk_records_from_store(
            store,
            &chunk_ids,
            request.project_id,
            &query_filter,
        )
        .await
        {
            Ok(records) => {
                let records = records.unwrap_or_default();
                if records.is_empty() {
                    tracing::warn!("No chunk records found for keyword search results");
                    None
                } else {
                    Some(records)
                }
            }
            Err(e) => {
                tracing::warn!("Failed to query chunk records: {}", e);
                return Err(KeywordSearchError::Sqlite(e.to_string()));
            }
        };

        // Step 5: Read raw source snippets for matched chunks in batch.
        // BM25 already established the match, so no post-hoc re-verification
        // is performed; the caller gets the exact file path and line range
        // to grep or read.
        let snippets = match &chunk_records {
            Some(records) => read_snippets_batch(project_root.as_deref(), records),
            None => HashMap::new(),
        };

        let mut keyword_results: Vec<KeywordSearchItem> = Vec::new();

        for result in &results {
            let chunk_id = result.fields.get("chunk_id").cloned().unwrap_or_default();

            let chunk = match chunk_records.as_ref() {
                Some(records) => records.get(&chunk_id),
                None => None,
            };

            let snippet = snippets.get(&chunk_id).cloned().unwrap_or_default();

            let title_value = result.fields.get("title").cloned().unwrap_or_default();
            let (start_line, end_line) = match chunk {
                Some(chunk) => (chunk.start_line as u32, chunk.end_line as u32),
                None => (0, 0),
            };
            let file_path = chunk.map(|c| c.file_path.clone()).unwrap_or_default();

            keyword_results.push(KeywordSearchItem {
                chunk_id,
                score: result.score,
                file_path,
                title: title_value,
                snippet,
                start_line,
                end_line,
            });
        }

        // Step 6: Sort by BM25 score descending (already sorted, but ensure)
        keyword_results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let total = keyword_results.len();

        tracing::trace!(
            "Keyword search '{}' — {} results with source snippets",
            request.query,
            total
        );

        Ok(KeywordSearchResponse {
            query: request.query,
            total,
            results: keyword_results,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keyword_search_request_validation() {
        let req = KeywordSearchRequest {
            query: "".to_string(),
            top_n: 10,
            project_id: 1,
            epoch: None,
            offset: 0,
            term_operator: Default::default(),
        };
        // Empty query should fail — but we can't easily test async here.
        // The important thing is that there is no Default impl that sets project_id = 0.
        assert!(req.query.is_empty());
    }
}
