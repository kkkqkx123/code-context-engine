//! Hot update error types
//!
//! This module defines error types specific to hot update operations.

use thiserror::Error;

/// Error type for hot update operations
#[derive(Error, Debug)]
pub enum HotUpdateError {
    /// Scan operation failed
    #[error("Scan failed: {reason}")]
    Scan {
        reason: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// File operation failed
    #[error("File operation failed: {path:?}: {reason}")]
    File {
        path: Option<String>,
        reason: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// Parse operation failed
    #[error("Parse failed: {file}: {reason}")]
    Parse {
        file: String,
        reason: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// Hot update operation failed
    #[error("Hot update failed: {reason}")]
    HotUpdate {
        reason: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// Relation update failed
    #[error("Relation update failed: {reason}")]
    Relation {
        reason: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// Summary update failed
    #[error("Summary update failed: {reason}")]
    Summary {
        reason: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// Embedding update failed
    #[error("Embedding update failed: {reason}")]
    Embedding {
        reason: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// BM25 update failed
    #[error("BM25 update failed: {reason}")]
    Bm25 {
        reason: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// Export update failed
    #[error("Export update failed: {reason}")]
    Export {
        reason: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// State tracker error
    #[error("State tracker error: {reason}")]
    StateTracker {
        reason: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// Configuration error
    #[error("Configuration error: {reason}")]
    Config {
        reason: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// Permission denied error
    #[error("Permission denied: {reason}")]
    PermissionDenied {
        reason: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
}

/// Result type alias for hot update operations
pub type Result<T> = std::result::Result<T, HotUpdateError>;

impl HotUpdateError {
    /// Create a scan error
    pub fn scan(reason: impl Into<String>) -> Self {
        Self::Scan {
            reason: reason.into(),
            source: None,
        }
    }

    /// Create a scan error with source
    pub fn scan_with_source<E>(reason: impl Into<String>, source: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self::Scan {
            reason: reason.into(),
            source: Some(Box::new(source)),
        }
    }

    /// Create a file error
    pub fn file(reason: impl Into<String>) -> Self {
        Self::File {
            path: None,
            reason: reason.into(),
            source: None,
        }
    }

    /// Create a file error with path
    pub fn file_with_path(path: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::File {
            path: Some(path.into()),
            reason: reason.into(),
            source: None,
        }
    }

    /// Create a file error with source
    pub fn file_with_source<E>(reason: impl Into<String>, source: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self::File {
            path: None,
            reason: reason.into(),
            source: Some(Box::new(source)),
        }
    }

    /// Create a parse error
    pub fn parse(file: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Parse {
            file: file.into(),
            reason: reason.into(),
            source: None,
        }
    }

    /// Create a hot update error
    pub fn hot_update(reason: impl Into<String>) -> Self {
        Self::HotUpdate {
            reason: reason.into(),
            source: None,
        }
    }

    /// Create a relation error
    pub fn relation(reason: impl Into<String>) -> Self {
        Self::Relation {
            reason: reason.into(),
            source: None,
        }
    }

    /// Create a summary error
    pub fn summary(reason: impl Into<String>) -> Self {
        Self::Summary {
            reason: reason.into(),
            source: None,
        }
    }

    /// Create an embedding error
    pub fn embedding(reason: impl Into<String>) -> Self {
        Self::Embedding {
            reason: reason.into(),
            source: None,
        }
    }

    /// Create a BM25 error
    pub fn bm25(reason: impl Into<String>) -> Self {
        Self::Bm25 {
            reason: reason.into(),
            source: None,
        }
    }

    /// Create an export error
    pub fn export(reason: impl Into<String>) -> Self {
        Self::Export {
            reason: reason.into(),
            source: None,
        }
    }

    /// Create a state tracker error
    pub fn state_tracker(reason: impl Into<String>) -> Self {
        Self::StateTracker {
            reason: reason.into(),
            source: None,
        }
    }

    /// Create a config error
    pub fn config(reason: impl Into<String>) -> Self {
        Self::Config {
            reason: reason.into(),
            source: None,
        }
    }

    /// Create a permission denied error
    pub fn permission_denied(reason: impl Into<String>) -> Self {
        Self::PermissionDenied {
            reason: reason.into(),
            source: None,
        }
    }
}

impl From<crate::error::OrchestratorError> for HotUpdateError {
    fn from(e: crate::error::OrchestratorError) -> Self {
        Self::HotUpdate {
            reason: e.to_string(),
            source: Some(Box::new(e)),
        }
    }
}

impl From<std::io::Error> for HotUpdateError {
    fn from(e: std::io::Error) -> Self {
        Self::File {
            path: None,
            reason: e.to_string(),
            source: Some(Box::new(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_scan() {
        let error = HotUpdateError::scan("Scan failed");
        assert!(error.to_string().contains("Scan failed"));
    }

    #[test]
    fn test_error_file() {
        let error = HotUpdateError::file("File not found");
        assert!(error.to_string().contains("File not found"));
    }

    #[test]
    fn test_error_parse() {
        let error = HotUpdateError::parse("test.rs", "Syntax error");
        assert!(error.to_string().contains("test.rs"));
        assert!(error.to_string().contains("Syntax error"));
    }

    #[test]
    fn test_error_hot_update() {
        let error = HotUpdateError::hot_update("Update failed");
        assert!(error.to_string().contains("Update failed"));
    }

    #[test]
    fn test_error_relation() {
        let error = HotUpdateError::relation("Relation update failed");
        assert!(error.to_string().contains("Relation update failed"));
    }

    #[test]
    fn test_error_summary() {
        let error = HotUpdateError::summary("Summary generation failed");
        assert!(error.to_string().contains("Summary generation failed"));
    }

    #[test]
    fn test_error_embedding() {
        let error = HotUpdateError::embedding("Embedding failed");
        assert!(error.to_string().contains("Embedding failed"));
    }

    #[test]
    fn test_error_bm25() {
        let error = HotUpdateError::bm25("BM25 update failed");
        assert!(error.to_string().contains("BM25 update failed"));
    }

    #[test]
    fn test_error_state_tracker() {
        let error = HotUpdateError::state_tracker("State tracking failed");
        assert!(error.to_string().contains("State tracking failed"));
    }

    #[test]
    fn test_error_config() {
        let error = HotUpdateError::config("Invalid configuration");
        assert!(error.to_string().contains("Invalid configuration"));
    }

    #[test]
    fn test_error_permission_denied() {
        let error = HotUpdateError::permission_denied("Access denied");
        assert!(error.to_string().contains("Access denied"));
    }
}
