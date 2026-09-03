//! Query error types
//!
//! Defines error types for query operations with proper error chains.
//!
//! # Error Recovery
//!
//! Use `recovery_suggestion()` to get actionable recovery steps for errors:
//!
//! ```ignore
//! match coordinator.search(&options).await {
//!     Ok(result) => { /* handle result */ }
//!     Err(e) => {
//!         eprintln!("Error: {}", e);
//!         if let Some(suggestion) = e.recovery_suggestion() {
//!             eprintln!("Suggestion: {}", suggestion);
//!         }
//!     }
//! }
//! ```

use cce_llm_client::LlmError;
use cce_parser::tree_sitter_query::QueryError as TreeSitterQueryError;
use cce_relation::RelationQueryError;
use thiserror::Error;

/// Errors that can occur during query operations
#[derive(Error, Debug)]
pub enum QueryError {
    /// Error from vector retrieval (embedding or search)
    #[error("Vector operation failed")]
    Vector(#[source] LlmError),

    /// Error from BM25 retrieval
    #[error("BM25 retrieval error: {0}")]
    Bm25(String),

    /// Error from relation query
    #[error("Relation query failed")]
    Relation(#[source] RelationQueryError),

    /// Storage error (SQLite)
    #[error("Storage error: {0}")]
    Storage(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// Invalid query parameters
    #[error("Invalid query: {0}")]
    InvalidQuery(String),

    /// Resource not found
    #[error("Not found: {0}")]
    NotFound(String),

    /// Timeout error
    #[error("Operation timed out after {timeout_ms}ms")]
    Timeout { timeout_ms: u64 },

    /// Traversal error
    #[error("Traversal error: {0}")]
    Traversal(String),

    /// Path not found error
    #[error("Path not found: from {0} to {1} (max depth: {2})")]
    PathNotFound(String, String, usize),

    /// Invalid error
    #[error("Invalid: {0}")]
    Invalid(String),

    /// Index not available for the requested operation
    #[error("Index '{0}' is not available. Run 'cce index --{0}' to build it.")]
    IndexNotAvailable(String),

    /// Assembly error
    #[error("Assembly failed: {0}")]
    Assembly(String),

    /// Reranking error
    #[error("Reranking error: {0}")]
    Rerank(String),

    /// Retryable error — the operation can be retried later when services recover.
    ///
    /// Contains the service name (e.g. "qdrant", "bm25", "embedding") and the underlying cause.
    #[error("[retryable:{service}] {message}")]
    Retryable {
        /// Which service failed (e.g. "qdrant", "bm25", "embedding")
        service: String,
        /// Human-readable description of the failure
        message: String,
    },
}

impl From<RelationQueryError> for QueryError {
    fn from(err: RelationQueryError) -> Self {
        // Preserve the original error using the Relation variant
        QueryError::Relation(err)
    }
}

impl From<TreeSitterQueryError> for QueryError {
    fn from(err: TreeSitterQueryError) -> Self {
        // Map TreeSitterQueryError variants to appropriate QueryError variants
        match err {
            TreeSitterQueryError::TreeSitter(msg) => QueryError::InvalidQuery(msg),
            TreeSitterQueryError::Io(io_err) => {
                QueryError::InvalidQuery(format!("IO error: {}", io_err))
            }
            TreeSitterQueryError::UnsupportedLanguage(lang) => {
                QueryError::InvalidQuery(format!("Unsupported language: {}", lang))
            }
            TreeSitterQueryError::InvalidQuery(msg) => QueryError::InvalidQuery(msg),
        }
    }
}

impl QueryError {
    /// Create a configuration error
    pub fn config(msg: &str) -> Self {
        Self::Config(msg.to_string())
    }

    /// Create a traversal error
    pub fn traversal(msg: String) -> Self {
        Self::Traversal(msg)
    }

    /// Create a storage error
    pub fn storage(msg: &str) -> Self {
        Self::Storage(msg.to_string())
    }

    /// Create a not found error
    pub fn not_found(msg: String) -> Self {
        Self::NotFound(msg)
    }

    /// Create an invalid error
    pub fn invalid(msg: &str) -> Self {
        Self::Invalid(msg.to_string())
    }

    /// Create a path not found error
    pub fn path_not_found(from: String, to: String, max_depth: usize) -> Self {
        Self::PathNotFound(from, to, max_depth)
    }

    /// Create an index not available error
    pub fn index_not_available(index: &str) -> Self {
        Self::IndexNotAvailable(index.to_string())
    }

    /// Create a reranking error
    pub fn rerank(msg: String) -> Self {
        Self::Rerank(msg)
    }

    /// Create a retryable error with service label
    ///
    /// `service` identifies which service failed (e.g. "qdrant", "bm25", "embedding").
    pub fn retryable(service: &str, msg: impl Into<String>) -> Self {
        Self::Retryable {
            service: service.to_string(),
            message: msg.into(),
        }
    }

    /// Attach or override the service label for retryable errors
    ///
    /// Non-retryable errors are returned unchanged.
    pub fn with_service(self, service: &str) -> Self {
        match self {
            QueryError::Retryable { message, .. } => QueryError::Retryable {
                service: service.to_string(),
                message,
            },
            other => other,
        }
    }

    /// Get recovery suggestion for this error
    ///
    /// Returns actionable steps the user can take to resolve the error.
    pub fn recovery_suggestion(&self) -> Option<String> {
        match self {
            QueryError::IndexNotAvailable(index) => {
                Some(format!("Run 'cce index --{}' to build the missing index", index))
            }
            QueryError::Vector(llm_err) => {
                match llm_err {
                    LlmError::Config(_) => {
                        Some("Check your embedding API configuration in config.toml (api_keys, base_url, model)".to_string())
                    }
                    LlmError::Http(_) => {
                        Some("Check your network connection and embedding API endpoint availability".to_string())
                    }
                    LlmError::HttpStatus { status, .. } => {
                        Some(format!("Embedding API returned HTTP {status}; check endpoint availability and request validity"))
                    }
                    LlmError::Auth(msg) if msg.contains("authentication") || msg.contains("Unauthorized") => {
                        Some("Verify your API key is correct and has not expired".to_string())
                    }
                    LlmError::TokenLimitExceeded(_, _) => {
                        Some("Reduce query length or use a model with larger context window".to_string())
                    }
                    _ => Some("Check embedding service status and configuration".to_string()),
                }
            }
            QueryError::Bm25(msg) if msg.contains("not initialized") || msg.contains("not available") => {
                Some("Run 'cce index --bm25' to build the BM25 index".to_string())
            }
            QueryError::Timeout { timeout_ms } => {
                Some(format!("Consider increasing timeout or optimizing your query (current: {}ms)", timeout_ms))
            }
            QueryError::NotFound(msg) if msg.contains("entity") => {
                Some("The entity may have been deleted or the index needs rebuilding".to_string())
            }
            _ => None,
        }
    }

    /// Check if this error is retryable
    ///
    /// Returns true for transient errors that may succeed on retry.
    pub fn is_retryable(&self) -> bool {
        match self {
            QueryError::Vector(LlmError::Http(_))
            | QueryError::Bm25(_)
            | QueryError::Timeout { .. }
            | QueryError::Retryable { .. } => true,
            QueryError::Vector(LlmError::HttpStatus { status, .. }) => (500..=599).contains(status),
            _ => false,
        }
    }

    /// Check if this error indicates a configuration problem
    pub fn is_config_error(&self) -> bool {
        matches!(
            self,
            QueryError::Config(_)
                | QueryError::Vector(LlmError::Config(_))
                | QueryError::IndexNotAvailable(_)
        )
    }
}

/// Result type alias for query operations
pub type Result<T> = std::result::Result<T, QueryError>;
