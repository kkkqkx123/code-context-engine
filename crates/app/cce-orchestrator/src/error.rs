//! Error types for orchestrator operations

use cce_types::error::ConfigError;
use cce_types::error::common::{IoError, NotFoundError, TimeoutError};
use thiserror::Error;

/// Orchestrator error type with type-safe variants
///
/// This error type wraps domain errors and provides orchestrator-specific errors.
/// It maintains type safety by preserving the original error types rather than
/// converting everything to strings.
#[derive(Error, Debug)]
pub enum OrchestratorError {
    /// Query error - preserves QueryError details
    #[error("Query error: {0}")]
    Query(#[from] crate::query::QueryError),

    /// Parse error - preserves ParseError details
    #[error("Parse error: {0}")]
    Parse(#[from] cce_types::error::ParseError),

    /// LLM error - preserves LlmError details (includes embedding operations)
    #[error("LLM error: {0}")]
    Llm(#[from] cce_llm_client::LlmError),

    /// Storage error - preserves StorageError details
    #[error("Storage error: {0}")]
    Storage(#[from] cce_types::error::StorageError),

    /// Scanner error - preserves ScannerError details
    #[error("Scanner error: {0}")]
    Scanner(#[from] cce_scanner::ScannerError),

    /// Index error - orchestrator-specific indexing failures
    #[error("Index error: {operation} - {reason}")]
    Index { operation: String, reason: String },

    /// Configuration error - uses common ConfigError
    #[error("{0}")]
    Config(#[from] ConfigError),

    /// Not found - uses common NotFoundError
    #[error("{0}")]
    NotFound(#[from] NotFoundError),

    /// Timeout - uses common TimeoutError
    #[error("{0}")]
    Timeout(#[from] TimeoutError),

    /// Merge error - result merge failures
    #[error("Result merge error: {reason}")]
    Merge { reason: String },

    /// Cache error - cache operation failures
    #[error("Cache error: {operation} - {reason}")]
    Cache { operation: String, reason: String },

    /// Hot update error - hot update operation failures
    #[error("Hot update error: {operation} - {reason}")]
    HotUpdate { operation: String, reason: String },

    /// Scan error - file scanning failures
    #[error("Scan error: {path} - {reason}")]
    Scan { path: String, reason: String },
}

impl OrchestratorError {
    /// Create an index error
    pub fn index(operation: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Index {
            operation: operation.into(),
            reason: reason.into(),
        }
    }

    /// Create a merge error
    pub fn merge(reason: impl Into<String>) -> Self {
        Self::Merge {
            reason: reason.into(),
        }
    }

    /// Create a cache error
    pub fn cache(operation: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Cache {
            operation: operation.into(),
            reason: reason.into(),
        }
    }

    /// Create a hot update error
    pub fn hot_update(operation: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::HotUpdate {
            operation: operation.into(),
            reason: reason.into(),
        }
    }

    /// Create a scan error
    pub fn scan(path: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Scan {
            path: path.into(),
            reason: reason.into(),
        }
    }

    /// Get error type for metrics collection
    pub fn error_type(&self) -> &'static str {
        match self {
            Self::Query(_) => "query_error",
            Self::Parse(_) => "parse_error",
            Self::Llm(_) => "llm_error",
            Self::Storage(_) => "storage_error",
            Self::Scanner(_) => "scanner_error",
            Self::Index { .. } => "index_error",
            Self::Config(_) => "config_error",
            Self::NotFound(_) => "not_found_error",
            Self::Timeout(_) => "timeout_error",
            Self::Merge { .. } => "merge_error",
            Self::Cache { .. } => "cache_error",
            Self::HotUpdate { .. } => "hot_update_error",
            Self::Scan { .. } => "scan_error",
        }
    }
}

// Convert module-specific errors to domain errors
impl From<cce_storage_qdrant::QdrantError> for OrchestratorError {
    fn from(e: cce_storage_qdrant::QdrantError) -> Self {
        OrchestratorError::Storage(cce_types::error::StorageError::from(e))
    }
}

impl From<cce_storage_bm25::Bm25Error> for OrchestratorError {
    fn from(e: cce_storage_bm25::Bm25Error) -> Self {
        OrchestratorError::Storage(cce_types::error::StorageError::from(e))
    }
}

impl From<std::io::Error> for OrchestratorError {
    fn from(e: std::io::Error) -> Self {
        // Convert IO errors to StorageError via common IoError
        OrchestratorError::Storage(cce_types::error::StorageError::Io(IoError::from(e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_creation() {
        let err = OrchestratorError::index("insert", "test reason");
        assert!(matches!(err, OrchestratorError::Index { .. }));
        assert!(err.to_string().contains("insert"));
        assert!(err.to_string().contains("test reason"));

        let err = OrchestratorError::merge("test reason");
        assert!(matches!(err, OrchestratorError::Merge { .. }));
        assert!(err.to_string().contains("test reason"));

        let err = OrchestratorError::cache("read", "test reason");
        assert!(matches!(err, OrchestratorError::Cache { .. }));
        assert!(err.to_string().contains("read"));
        assert!(err.to_string().contains("test reason"));

        let err = OrchestratorError::hot_update("update", "test reason");
        assert!(matches!(err, OrchestratorError::HotUpdate { .. }));
        assert!(err.to_string().contains("update"));
        assert!(err.to_string().contains("test reason"));

        let err = OrchestratorError::scan("/test/path", "test reason");
        assert!(matches!(err, OrchestratorError::Scan { .. }));
        assert!(err.to_string().contains("/test/path"));
        assert!(err.to_string().contains("test reason"));
    }

    #[test]
    fn test_error_type() {
        assert_eq!(
            OrchestratorError::index("test", "test").error_type(),
            "index_error"
        );
        assert_eq!(OrchestratorError::merge("test").error_type(), "merge_error");
        assert_eq!(
            OrchestratorError::cache("test", "test").error_type(),
            "cache_error"
        );
        assert_eq!(
            OrchestratorError::hot_update("test", "test").error_type(),
            "hot_update_error"
        );
        assert_eq!(
            OrchestratorError::scan("test", "test").error_type(),
            "scan_error"
        );
    }
}
