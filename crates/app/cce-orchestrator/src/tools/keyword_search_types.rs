//! Types for keyword search tool
//!
//! Defines request/response types for the keyword search operation.
//! The tool performs BM25-based keyword search with content highlight snippets
//! sourced from SQLite (not from Tantivy stored fields).

use serde::{Deserialize, Serialize};

/// Request for keyword search
///
/// All fields including `project_id` must be explicitly provided.
/// There is no default instance — the caller must supply every field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeywordSearchRequest {
    /// Search query text (must be non-empty)
    pub query: String,
    /// Maximum number of results to return (must be > 0)
    pub top_n: usize,
    /// Project ID for SQLite chunk lookup and BM25 filtering (must be positive)
    pub project_id: i64,
    /// Optional epoch for version-aware filtering
    pub epoch: Option<i64>,
    /// Operator for combining multiple query terms (`or`/`and`)
    #[serde(default)]
    pub term_operator: cce_storage_bm25::TermOperator,
}

/// A single keyword search result with highlighted snippet
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeywordSearchItem {
    /// Chunk/document ID
    pub chunk_id: String,
    /// BM25 relevance score
    pub score: f32,
    /// File path containing the match
    pub file_path: String,
    /// Entity/function title
    pub title: String,
    /// Highlighted code snippet (HTML with <mark> tags)
    pub highlighted_snippet: String,
    /// Start line in the file
    pub start_line: u32,
    /// End line in the file
    pub end_line: u32,
}

/// Response for keyword search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeywordSearchResponse {
    /// The original query
    pub query: String,
    /// Total number of results returned
    pub total: usize,
    /// Search results with highlighted snippets
    pub results: Vec<KeywordSearchItem>,
}

/// Error type for keyword search operations
#[derive(Debug, Clone, thiserror::Error)]
pub enum KeywordSearchError {
    /// BM25 index error
    #[error("BM25 error: {0}")]
    Bm25(String),

    /// BM25 index not available
    #[error("BM25 index not available")]
    IndexNotAvailable,

    /// SQLite error
    #[error("SQLite error: {0}")]
    Sqlite(String),

    /// No SQLite connection available
    #[error("SQLite database not configured")]
    SqliteNotConfigured,
}

impl From<cce_storage_bm25::Bm25Error> for KeywordSearchError {
    fn from(e: cce_storage_bm25::Bm25Error) -> Self {
        KeywordSearchError::Bm25(e.to_string())
    }
}
