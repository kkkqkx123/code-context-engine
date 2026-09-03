//! Error types for scanner operations

use cce_types::error::common::{IoError, NotFoundError};
use thiserror::Error;

/// Scanner error type
#[derive(Error, Debug)]
pub enum ScannerError {
    /// IO error - uses common IoError
    #[error("{0}")]
    Io(#[from] IoError),

    /// File not found - uses common NotFoundError
    #[error("{0}")]
    NotFound(#[from] NotFoundError),

    /// Invalid argument
    #[error("Invalid argument: {reason}")]
    InvalidArgument { reason: String },

    /// Scan error
    #[error("Scan error: {path} - {reason}")]
    Scan { path: String, reason: String },

    /// Path error
    #[error("Path error: {path} - {reason}")]
    Path { path: String, reason: String },

    /// Permission denied
    #[error("Permission denied: {path} - {reason}")]
    PermissionDenied { path: String, reason: String },
}

/// Result type alias for scanner operations
pub type Result<T> = std::result::Result<T, ScannerError>;

impl ScannerError {
    /// Create an invalid argument error
    pub fn invalid_argument(reason: impl Into<String>) -> Self {
        Self::InvalidArgument {
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

    /// Create a path error
    pub fn path(path: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Path {
            path: path.into(),
            reason: reason.into(),
        }
    }

    /// Create a permission denied error
    pub fn permission_denied(path: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::PermissionDenied {
            path: path.into(),
            reason: reason.into(),
        }
    }
}

// Implement From<std::io::Error> for ScannerError via IoError
impl From<std::io::Error> for ScannerError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(IoError::from(err))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_creation() {
        let err = ScannerError::invalid_argument("test reason");
        assert!(matches!(err, ScannerError::InvalidArgument { .. }));
        assert!(err.to_string().contains("test reason"));

        let err = ScannerError::scan("/test/path", "test reason");
        assert!(matches!(err, ScannerError::Scan { .. }));
        assert!(err.to_string().contains("/test/path"));
        assert!(err.to_string().contains("test reason"));

        let err = ScannerError::path("/test/path", "test reason");
        assert!(matches!(err, ScannerError::Path { .. }));
        assert!(err.to_string().contains("/test/path"));
        assert!(err.to_string().contains("test reason"));

        let err = ScannerError::permission_denied("/test/path", "test reason");
        assert!(matches!(err, ScannerError::PermissionDenied { .. }));
        assert!(err.to_string().contains("/test/path"));
        assert!(err.to_string().contains("test reason"));
    }

    #[test]
    fn test_io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "test");
        let err = ScannerError::from(io_err);
        assert!(matches!(err, ScannerError::Io(_)));
    }
}
