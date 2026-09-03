//! Common error types used across the codebase
//!
//! This module defines base error types that are shared across multiple modules
//! to avoid duplication and provide consistent error handling.

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

/// Aggregate error for batch operations
///
/// This error type is used when multiple errors occur during a batch operation.
/// It collects all errors and provides summary information.
#[derive(Debug)]
pub enum AggregateError<E: std::fmt::Display + std::fmt::Debug> {
    /// Multiple errors occurred during batch operation
    Multiple {
        /// All errors that occurred
        errors: Vec<E>,
        /// Total number of items processed
        total: usize,
        /// Number of items that failed
        failed: usize,
    },
}

impl<E: std::fmt::Display + std::fmt::Debug> std::fmt::Display for AggregateError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Multiple {
                errors,
                total,
                failed,
            } => {
                write!(
                    f,
                    "Failed to process {}/{} items:\n{}",
                    failed,
                    total,
                    errors
                        .iter()
                        .enumerate()
                        .map(|(i, e)| format!("  {}. {}", i + 1, e))
                        .collect::<Vec<_>>()
                        .join("\n")
                )
            }
        }
    }
}

impl<E: std::fmt::Display + std::fmt::Debug> std::error::Error for AggregateError<E> {}

impl<E: std::fmt::Display + std::fmt::Debug> AggregateError<E> {
    /// Create a new aggregate error from a list of errors
    ///
    /// If there's only one error, it's returned directly. Otherwise,
    /// an aggregate error is created.
    pub fn from_errors(errors: Vec<E>, total: usize) -> Self {
        let failed = errors.len();
        if failed == 1 {
            // For single error, we can't return it directly due to type constraints
            // The caller should handle this case separately
            Self::Multiple {
                errors,
                total,
                failed,
            }
        } else {
            Self::Multiple {
                errors,
                total,
                failed,
            }
        }
    }

    /// Get the number of failed items
    pub fn failed_count(&self) -> usize {
        match self {
            Self::Multiple { failed, .. } => *failed,
        }
    }

    /// Get the total number of items processed
    pub fn total_count(&self) -> usize {
        match self {
            Self::Multiple { total, .. } => *total,
        }
    }

    /// Get a reference to all errors
    pub fn errors(&self) -> &[E] {
        match self {
            Self::Multiple { errors, .. } => errors,
        }
    }
}

/// Common IO wrapper error
///
/// This type wraps std::io::Error and can be used across modules
/// that need IO error handling without duplicating the definition.
#[derive(Error, Debug)]
#[error("IO error: {0}")]
pub struct IoError(pub std::io::Error);

impl From<std::io::Error> for IoError {
    fn from(err: std::io::Error) -> Self {
        Self(err)
    }
}

impl IoError {
    /// Get a reference to the underlying IO error
    pub fn inner(&self) -> &std::io::Error {
        &self.0
    }
}

impl Clone for IoError {
    fn clone(&self) -> Self {
        IoError(std::io::Error::new(self.0.kind(), self.0.to_string()))
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

    #[test]
    fn test_aggregate_error_display() {
        let errors = vec!["Error 1", "Error 2", "Error 3"];
        let aggregate = AggregateError::from_errors(errors, 10);

        let display = format!("{}", aggregate);
        assert!(display.contains("Failed to process 3/10 items"));
        assert!(display.contains("1. Error 1"));
        assert!(display.contains("2. Error 2"));
        assert!(display.contains("3. Error 3"));
    }

    #[test]
    fn test_aggregate_error_counts() {
        let errors = vec!["Error 1", "Error 2"];
        let aggregate = AggregateError::from_errors(errors, 5);

        assert_eq!(aggregate.failed_count(), 2);
        assert_eq!(aggregate.total_count(), 5);
        assert_eq!(aggregate.errors().len(), 2);
    }
}
