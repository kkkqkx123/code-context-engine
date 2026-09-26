//! Common error types used across the codebase
//!
//! This module defines base error types that are shared across multiple modules
//! to avoid duplication and provide consistent error handling.

use std::sync::Arc;

use thiserror::Error;

/// Error classification trait
///
/// This trait provides methods to classify errors for retry logic,
/// monitoring, and alerting purposes.
pub trait ErrorClassify {
    /// Check if this error is retryable
    ///
    /// Retryable errors are typically transient failures that may succeed
    /// on retry (e.g., network timeouts, rate limits).
    fn is_retryable(&self) -> bool;

    /// Check if this error is transient
    ///
    /// Transient errors are temporary failures that may resolve themselves
    /// over time (e.g., rate limits, temporary unavailability).
    fn is_transient(&self) -> bool;

    /// Check if this error is permanent
    ///
    /// Permanent errors are unlikely to succeed on retry (e.g., not found,
    /// invalid configuration, permission denied).
    fn is_permanent(&self) -> bool;
}

/// Common IO wrapper error
///
/// This type wraps std::io::Error and can be used across modules
/// that need IO error handling without duplicating the definition.
/// The inner error is shared via `Arc` so cloning keeps the full
/// error (kind, OS code and source chain) intact.
#[derive(Error, Debug, Clone)]
#[error("IO error: {0}")]
pub struct IoError(pub Arc<std::io::Error>);

impl From<std::io::Error> for IoError {
    fn from(err: std::io::Error) -> Self {
        Self(Arc::new(err))
    }
}

impl IoError {
    /// Get a reference to the underlying IO error
    pub fn inner(&self) -> &std::io::Error {
        &self.0
    }

    /// Whether the failure is caused by exhausted storage space.
    ///
    /// Retrying cannot succeed until an operator frees disk space, so
    /// callers must treat this as permanent and surface an alert.
    pub fn is_storage_full(&self) -> bool {
        if self.0.kind() == std::io::ErrorKind::StorageFull {
            return true;
        }
        matches!(self.0.raw_os_error(), Some(28))
    }
}

impl ErrorClassify for IoError {
    fn is_retryable(&self) -> bool {
        self.is_transient()
    }

    fn is_transient(&self) -> bool {
        if self.is_storage_full() {
            return false;
        }
        // Deterministic local failures (missing file, permissions, invalid
        // data, wrong path type) never succeed on retry; interruptions,
        // resource pressure and connection faults may.
        !matches!(
            self.0.kind(),
            std::io::ErrorKind::NotFound
                | std::io::ErrorKind::PermissionDenied
                | std::io::ErrorKind::InvalidInput
                | std::io::ErrorKind::InvalidData
                | std::io::ErrorKind::Unsupported
                | std::io::ErrorKind::IsADirectory
                | std::io::ErrorKind::NotADirectory
        )
    }

    fn is_permanent(&self) -> bool {
        !self.is_transient()
    }
}

/// Common not found error
///
/// This type represents a resource not found error that can be used
/// across different modules consistently.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
#[error("Resource not found: {0}")]
pub struct NotFoundError(pub String);

impl NotFoundError {
    /// Create a new not found error
    pub fn new(resource: impl Into<String>) -> Self {
        Self(resource.into())
    }

    /// Get the resource identifier
    pub fn resource(&self) -> &str {
        &self.0
    }
}

/// Common timeout error
///
/// This type represents timeout errors that can be used
/// across different modules consistently.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
#[error("Operation timeout: {0}")]
pub struct TimeoutError(pub String);

impl TimeoutError {
    /// Create a new timeout error
    pub fn new(reason: impl Into<String>) -> Self {
        Self(reason.into())
    }

    /// Get the error reason
    pub fn reason(&self) -> &str {
        &self.0
    }
}

/// Common HTTP error
///
/// This type represents HTTP-related errors that can be used
/// across different modules consistently.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
#[error("HTTP error: {0}")]
pub struct HttpError(pub String);

impl HttpError {
    /// Create a new HTTP error
    pub fn new(reason: impl Into<String>) -> Self {
        Self(reason.into())
    }

    /// Get the error reason
    pub fn reason(&self) -> &str {
        &self.0
    }
}

/// Common JSON/serialization error
///
/// This type wraps serde_json::Error and can be used across modules
/// that need JSON error handling without duplicating the definition.
#[derive(Error, Debug)]
#[error("JSON error: {0}")]
pub struct JsonError(pub serde_json::Error);

impl From<serde_json::Error> for JsonError {
    fn from(err: serde_json::Error) -> Self {
        Self(err)
    }
}

impl JsonError {
    /// Get a reference to the underlying JSON error
    pub fn inner(&self) -> &serde_json::Error {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "test");
        let err = IoError::from(io_err);
        assert!(err.to_string().contains("IO error"));
    }

    #[test]
    fn test_not_found_error() {
        let err = NotFoundError::new("test_resource");
        assert_eq!(err.to_string(), "Resource not found: test_resource");
        assert_eq!(err.resource(), "test_resource");
    }

    #[test]
    fn test_timeout_error() {
        let err = TimeoutError::new("operation took too long");
        assert_eq!(
            err.to_string(),
            "Operation timeout: operation took too long"
        );
    }

    #[test]
    fn test_http_error() {
        let err = HttpError::new("connection refused");
        assert_eq!(err.to_string(), "HTTP error: connection refused");
    }

    #[test]
    fn test_json_error() {
        let json_err = serde_json::from_str::<serde_json::Value>("invalid json").unwrap_err();
        let err = JsonError::from(json_err);
        assert!(err.to_string().contains("JSON error"));
    }
}
