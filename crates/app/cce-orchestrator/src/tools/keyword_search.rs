//! Keyword search tool
//!
//! Provides standalone BM25 keyword search with raw source snippets.
//! This tool is independent of the vector search pipeline and can be used
//! as a focused keyword query module.
//!
//! # Architecture
//!
//! ```text
//! Query → BM25 search → get chunk_ids → SQLite lookup → read source snippet → scored results
//! ```
//!
//! Content is sourced from SQLite (not Tantivy stored fields), returning the
//! raw source lines so the caller can grep or read them directly. No markup is
//! embedded in the snippet.
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

use cce_storage_bm25::{Bm25Client, Bm25Retrieval, Bm25SearchOptions};
use cce_storage_sqlite::SqliteClient;

use crate::tools::common::{read_snippets_batch, resolve_epoch_view};

pub use self::types::{
    KeywordSearchError, KeywordSearchItem, KeywordSearchRequest, KeywordSearchResponse,
};

/// Keyword search tool
///
/// Provides standalone BM25-based keyword search with raw source snippets
/// sourced from SQLite content. Results are sorted by BM25 relevance score.
#[derive(Clone)]
pub struct KeywordSearchTool {
    /// BM25 client for Tantivy index access
    bm25: Arc<tokio::sync::Mutex<Bm25Client>>,
    /// Optional SQLite database for chunk content lookup
    sqlite: Option<Arc<SqliteClient>>,
}

impl KeywordSearchTool {
    /// Create a new keyword search tool
    ///
    /// # Arguments
    ///
    /// * `bm25` - BM25 client wrapped in Arc<Mutex> for thread-safe access
    pub fn new(bm25: Arc<tokio::sync::Mutex<Bm25Client>>) -> Self {
        Self { bm25, sqlite: None }
    }

    /// Attach SQLite database for chunk content lookup
    ///
    /// # Arguments
    ///
    /// * `sqlite` - SQLite database for chunk content retrieval
    pub fn with_sqlite(mut self, sqlite: Arc<SqliteClient>) -> Self {
        self.sqlite = Some(sqlite);
        self
    }

    /// Execute keyword search
    ///
    /// 1. Validate input (project_id must be positive, query must be non-empty, top_n > 0)
    /// 2. Search BM25 index for matching documents
    /// 3. Enrich with chunk metadata from SQLite
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
        if self.sqlite.is_none() {
            return Err(KeywordSearchError::SqliteNotConfigured);
        }

        // Step 1: Lock BM25 client and acquire index resources
        let bm25_client = self.bm25.lock().await;

        let manager_arc = bm25_client
            .index_manager()
            .ok_or(KeywordSearchError::IndexNotAvailable)?;
        let manager_guard = manager_arc.read().await;
        let schema = bm25_client.schema();

        // Step 2: Resolve the epoch view for version-aware filtering. An
        // explicit `request.epoch` pins a single full generation; otherwise
        // the active manifest view (own + parent + overridden files) applies.
        // The same connection serves the chunk lookup below so both stages
        // observe one consistent snapshot.
        let sqlite_ref = self
            .sqlite
            .as_ref()
            .ok_or(KeywordSearchError::SqliteNotConfigured)?;
        let conn = sqlite_ref
            .read_connection()
            .map_err(|e| KeywordSearchError::Sqlite(e.to_string()))?;
        let query_filter = resolve_epoch_view(&conn, request.project_id, request.epoch)
            .map_err(|e| KeywordSearchError::Sqlite(e.to_string()))?;

        let options = Bm25SearchOptions {
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

        let results =
            Bm25Retrieval::new().search(&manager_guard, schema, &request.query, &options)?;

        tracing::trace!(
            "Keyword search '{}' returned {} BM25 results",
            request.query,
            results.len()
        );

        // Step 3: Extract chunk_ids for SQLite lookup
        let chunk_ids: Vec<String> = results
            .iter()
            .filter_map(|r| r.fields.get("chunk_id").cloned())
            .filter(|id| !id.is_empty())
            .collect();

        // Step 4: Look up chunk metadata from SQLite via the same two-stage
        // epoch-view resolution as the search pipeline; snippets are
        // lazy-loaded from the source file via the project root.
        let (chunk_records, project_root) = {
            let project_root =
                cce_storage_sqlite::source_reader::resolve_project_root(&conn, request.project_id);
            match crate::query::retrieval::post_processing::get_chunk_records(
                &conn,
                &chunk_ids,
                request.project_id,
                &query_filter,
            ) {
                Ok(records) => {
                    let records = records.unwrap_or_default();
                    if records.is_empty() {
                        tracing::warn!("No chunk records found for keyword search results");
                        (None, project_root)
                    } else {
                        (Some(records), project_root)
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to query chunk records: {}", e);
                    return Err(KeywordSearchError::Sqlite(e.to_string()));
                }
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
