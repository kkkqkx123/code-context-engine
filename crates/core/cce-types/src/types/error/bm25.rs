//! BM25 backend error types
//!
//! `Bm25Error` lives in `cce_core` so that `StorageError` can wrap it in a
//! structured variant (`StorageError::Bm25(Bm25Error)`) without a
//! cce_core → cce_infrastructure dependency. The only external coupling is
//! `tantivy::TantivyError`, retained as a structured variant.

use super::common::{ErrorClassify, IoError};
use super::config::ConfigError;
use thiserror::Error;

/// BM25 client error
#[derive(Error, Debug)]
pub enum Bm25Error {
    /// Index operation error
    #[error("Index error: {0}")]
    Index(String),

    /// Search operation error
    #[error("Search error: {0}")]
    Search(String),

    /// Configuration error - uses common ConfigError
    #[error("{0}")]
    Config(#[from] ConfigError),

    /// BM25 service is disabled
    #[error("BM25 service is disabled")]
    Disabled,

    /// IO error - uses common IoError
    #[error("{0}")]
    Io(#[from] IoError),

    /// Tantivy error
    #[error("Tantivy error: {0}")]
    Tantivy(#[from] tantivy::TantivyError),

    /// Schema error
    #[error("Schema error: {0}")]
    Schema(String),

    /// Document error
    #[error("Document error: {0}")]
    Document(String),

    /// Writer error
    #[error("Writer error: {0}")]
    Writer(String),
}

impl Bm25Error {
    /// Create a new index error from string
    pub fn index<S: Into<String>>(msg: S) -> Self {
        Self::Index(msg.into())
    }

    /// Create a new search error from string
    pub fn search<S: Into<String>>(msg: S) -> Self {
        Self::Search(msg.into())
    }

    /// Create a new config error from string
    pub fn config<S: Into<String>>(msg: S) -> Self {
        Self::Config(ConfigError::Other(msg.into()))
    }
}

// Implement From<std::io::Error> for Bm25Error via IoError
impl From<std::io::Error> for Bm25Error {
    fn from(err: std::io::Error) -> Self {
        Self::Io(IoError::from(err))
    }
}

impl ErrorClassify for Bm25Error {
    fn is_retryable(&self) -> bool {
        // Tantivy/I/O failures and writer-level issues are transient; a retry
        // after the index recovers may succeed.
        matches!(
            self,
            Self::Tantivy(_)
                | Self::Io(_)
                | Self::Index(_)
                | Self::Document(_)
                | Self::Writer(_)
                | Self::Search(_)
        )
    }

    fn is_transient(&self) -> bool {
        self.is_retryable()
    }

    fn is_permanent(&self) -> bool {
        matches!(self, Self::Config(_) | Self::Disabled | Self::Schema(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bm25_error_classification() {
        assert!(Bm25Error::Search("reader unavailable".into()).is_retryable());
        assert!(
            Bm25Error::Tantivy(tantivy::TantivyError::InvalidArgument(
                "segment corrupted".into()
            ))
            .is_retryable()
        );
        assert!(!Bm25Error::Config(ConfigError::Other("bad".into())).is_retryable());
        assert!(Bm25Error::Config(ConfigError::Other("bad".into())).is_permanent());
        assert!(Bm25Error::Disabled.is_permanent());
    }

    #[test]
    fn test_bm25_error_to_storage_error() {
        use crate::types::StorageError;

        let err: StorageError = Bm25Error::Search("reader unavailable".into()).into();
        assert!(matches!(err, StorageError::Bm25(Bm25Error::Search(_))));

        let err: StorageError = Bm25Error::Disabled.into();
        assert!(matches!(err, StorageError::Bm25(Bm25Error::Disabled)));

        let err: StorageError = Bm25Error::Index("write failed".into()).into();
        assert!(matches!(err, StorageError::Bm25(Bm25Error::Index(_))));
    }
}
