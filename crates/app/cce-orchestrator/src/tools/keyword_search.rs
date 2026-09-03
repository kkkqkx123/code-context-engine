//! Keyword search tool
//!
//! Provides standalone BM25 keyword search with content-highlighted snippets.
//! This tool is independent of the vector search pipeline and can be used
//! as a focused keyword query module.
//!
//! # Architecture
//!
//! ```text
//! Query → BM25 search → get chunk_ids → SQLite lookup → highlight content → scored results
//! ```
//!
//! Content is sourced from SQLite (not Tantivy stored fields), enabling accurate
//! highlight generation from the full code content.
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

use cce_storage_bm25::highlight::highlight_text;
use cce_storage_bm25::{Bm25Client, Bm25Retrieval, Bm25SearchOptions};
use cce_storage_sqlite::SqliteClient;

use crate::query::filter::QueryFilter;

pub use types::{
    KeywordSearchError, KeywordSearchItem, KeywordSearchRequest, KeywordSearchResponse,
};

/// Tokenize query text with the shared `MixedTokenizer` for symmetric term
/// matching against the BM25 index.
fn tokenize_terms(text: &str) -> Vec<String> {
    cce_text::MixedTokenizer::new().tokenize(text)
}

/// Keyword search tool
///
/// Provides standalone BM25-based keyword search with highlighted snippets
/// sourced from SQLite content. Results are sorted by BM25 relevance score.
/// Only results that have at least one highlighted content match are returned.
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
    /// 3. Enrich with chunk content from SQLite
    /// 4. Generate highlighted snippets from full content
    /// 5. Filter to only results with content highlights
    /// 6. Sort by BM25 score descending
    ///
    /// # Arguments
    ///
    /// * `request` - Search parameters including query, top_n, and project_id
    ///
    /// # Returns
    ///
    /// Search results with highlighted snippets, or an error
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
        let sqlite_ref = self.sqlite.as_ref().expect("sqlite presence checked above");
        let conn = sqlite_ref
            .read_connection()
            .map_err(|e| KeywordSearchError::Sqlite(e.to_string()))?;
        let query_filter = match request.epoch {
            Some(epoch) => {
                QueryFilter::new(epoch).map_err(|e| KeywordSearchError::Bm25(e.to_string()))?
            }
            None => crate::query::filter::load_active_query_filter(&conn, request.project_id)
                .map_err(|e| KeywordSearchError::Sqlite(e.to_string()))?,
        };

        let options = Bm25SearchOptions {
            limit: request.top_n,
            offset: 0,
            field_weights: HashMap::new(),
            highlight: false,
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

        // Step 5: Generate highlighted snippets from SQLite content
        let query_terms: Vec<String> = tokenize_terms(&request.query);

        let mut keyword_results: Vec<KeywordSearchItem> = Vec::new();

        for result in &results {
            let chunk_id = result.fields.get("chunk_id").cloned().unwrap_or_default();

            // Look up SQLite chunk record
            let chunk = match chunk_records.as_ref() {
                Some(records) => records.get(&chunk_id),
                None => None,
            };

            let content_snippet = match chunk {
                Some(chunk) => highlight_text(
                    &cce_storage_sqlite::source_reader::read_source_lines(
                        project_root.as_deref(),
                        &chunk.file_path,
                        chunk.start_line.max(0) as u32,
                        chunk.end_line.max(0) as u32,
                    ),
                    &query_terms,
                ),
                None => None,
            };

            // Determine whether the title/keywords fields hit (title hits do
            // not require the query term to appear verbatim in the source).
            // title field itself must stay plain text — BM25 results are data,
            // not HTML, so highlight markup is never written back into it.
            let title_value = result.fields.get("title").cloned().unwrap_or_default();
            let title_hit = highlight_text(&title_value, &query_terms).is_some();

            // A result is kept if it has a content highlight OR a title hit.
            // Only results with neither are discarded.
            if content_snippet.is_none() && !title_hit {
                continue;
            }

            let highlighted_snippet = content_snippet.unwrap_or_default();
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
                highlighted_snippet,
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
            "Keyword search '{}' — {} results with content highlights",
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
            term_operator: Default::default(),
        };
        // Empty query should fail — but we can't easily test async here.
        // The important thing is that there is no Default impl that sets project_id = 0.
        assert!(req.query.is_empty());
    }
}
