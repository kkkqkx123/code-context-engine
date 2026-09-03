//! Error types for Tree-sitter query operations
//!
//! This module defines error types specific to Tree-sitter query execution.

use thiserror::Error;

/// Error type for Tree-sitter query operations
#[derive(Error, Debug)]
pub enum QueryError {
    /// Error from tree-sitter query execution
    #[error("Tree-sitter query error: {0}")]
    TreeSitter(String),

    /// IO error during query execution
    #[error("IO error during query execution: {0}")]
    Io(#[from] std::io::Error),

    /// Language not supported
    #[error("Language not supported: {0}")]
    UnsupportedLanguage(String),

    /// Invalid query parameters or format
    #[error("Invalid query: {0}")]
    InvalidQuery(String),
}

impl QueryError {
    /// Create a tree-sitter error
    pub fn tree_sitter(msg: String) -> Self {
        QueryError::TreeSitter(msg)
    }

    /// Create an unsupported language error
    pub fn unsupported_language(lang: String) -> Self {
        QueryError::UnsupportedLanguage(lang)
    }
}

/// Result type alias for Tree-sitter query operations
pub type Result<T> = std::result::Result<T, QueryError>;
