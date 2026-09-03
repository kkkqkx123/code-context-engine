//! Qdrant backend error types
//!
//! `QdrantError` lives in `cce_core` so that `StorageError` can wrap it in a
//! structured variant (`StorageError::Qdrant(QdrantError)`) without a
//! cce_core → cce_infrastructure dependency. The type only depends on common
//! cce_core error types and strings, so it has no transport coupling.

use super::common::{ErrorClassify, HttpError, IoError, JsonError, NotFoundError, TimeoutError};
use super::config::ConfigError;
use thiserror::Error;

/// Qdrant error type
#[derive(Error, Debug)]
pub enum QdrantError {
    /// Connection error
    #[error("Connection error: {0}")]
    Connection(String),

    /// Connection refused
    #[error("Connection refused to Qdrant server at {url}: {message}")]
    ConnectionRefused { url: String, message: String },

    /// Connection timeout
    #[error("Connection timeout: {0}")]
    ConnectionTimeout(String),

    /// Collection not found - uses common NotFoundError
    #[error("{0}")]
    CollectionNotFound(#[from] NotFoundError),

    /// Collection already exists
    #[error("Collection '{0}' already exists")]
    CollectionAlreadyExists(String),

    /// Invalid vector dimension
    #[error("Invalid vector dimension: expected {expected}, got {actual}")]
    DimensionMismatch { expected: usize, actual: usize },

    /// Invalid URL
    #[error("Invalid URL: {0}")]
    InvalidUrl(String),

    /// API error — Qdrant server returned a non-success HTTP status (4xx, 5xx)
    #[error("API error: {0}")]
    Api(String),

    /// Request error — transport-level failure (connection reset, DNS, TLS, etc.)
    #[error("Request error: {0}")]
    Request(String),

    /// Response parse error
    #[error("Failed to parse response: {0}")]
    ResponseParse(String),

    /// Payload error
    #[error("Payload error: {0}")]
    Payload(String),

    /// Index error
    #[error("Index error: {0}")]
    Index(String),

    /// Configuration error - uses common ConfigError
    #[error("{0}")]
    Config(#[from] ConfigError),

    /// Invalid configuration value
    #[error("Invalid configuration value: {field} - {reason}")]
    InvalidConfig { field: String, reason: String },

    /// Missing required configuration value
    #[error("Missing required configuration: {field}")]
    MissingConfig { field: String },

    /// Operation timeout - uses common TimeoutError
    #[error("{0}")]
    OperationTimeout(#[from] TimeoutError),

    /// Client not connected
    #[error("Qdrant client is not connected")]
    NotConnected,

    /// Client disabled
    #[error("Qdrant client is disabled")]
    Disabled,

    /// IO error - uses common IoError
    #[error("{0}")]
    Io(#[from] IoError),

    /// JSON error - uses common JsonError
    #[error("{0}")]
    Json(#[from] JsonError),

    /// HTTP error - uses common HttpError
    #[error("{0}")]
    Http(#[from] HttpError),

    /// Circuit breaker is open - request rejected due to repeated failures
    #[error("Circuit breaker is open: {0}")]
    CircuitBreakerOpen(String),
}

impl QdrantError {
    /// Create a connection error
    pub fn connection(message: impl Into<String>) -> Self {
        Self::Connection(message.into())
    }

    /// Create an API error
    pub fn api(message: impl Into<String>) -> Self {
        Self::Api(message.into())
    }

    /// Create a request error
    pub fn request(message: impl Into<String>) -> Self {
        Self::Request(message.into())
    }

    /// Create a config error
    pub fn config(message: impl Into<String>) -> Self {
        Self::Config(ConfigError::Other(message.into()))
    }

    /// Check if this is a connection error
    pub fn is_connection_error(&self) -> bool {
        matches!(
            self,
            Self::Connection(_) | Self::ConnectionRefused { .. } | Self::NotConnected
        )
    }

    /// Check if this is a not found error
    pub fn is_not_found(&self) -> bool {
        matches!(self, Self::CollectionNotFound(_))
    }

    /// Check if this is retryable
    ///
    /// A retryable error is one that may succeed on a subsequent attempt:
    /// - Connection-level failures (refused, timeout, DNS)
    /// - Circuit breaker open (indicating transient overload)
    /// - Operation timeout (server may recover)
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Connection(_)
                | Self::ConnectionRefused { .. }
                | Self::ConnectionTimeout(_)
                | Self::OperationTimeout(_)
                | Self::CircuitBreakerOpen(_)
        )
    }

    /// Get error code for programmatic error handling
    pub fn error_code(&self) -> &'static str {
        match self {
            Self::Connection(_) => "QDRANT_CONNECTION_ERROR",
            Self::ConnectionRefused { .. } => "QDRANT_CONNECTION_REFUSED_ERROR",
            Self::ConnectionTimeout(_) => "QDRANT_CONNECTION_TIMEOUT_ERROR",
            Self::CollectionNotFound(_) => "QDRANT_COLLECTION_NOT_FOUND_ERROR",
            Self::CollectionAlreadyExists(_) => "QDRANT_COLLECTION_ALREADY_EXISTS_ERROR",
            Self::DimensionMismatch { .. } => "QDRANT_DIMENSION_MISMATCH_ERROR",
            Self::InvalidUrl(_) => "QDRANT_INVALID_URL_ERROR",
            Self::Api(_) => "QDRANT_API_ERROR",
            Self::Request(_) => "QDRANT_REQUEST_ERROR",
            Self::ResponseParse(_) => "QDRANT_RESPONSE_PARSE_ERROR",
            Self::Payload(_) => "QDRANT_PAYLOAD_ERROR",
            Self::Index(_) => "QDRANT_INDEX_ERROR",
            Self::Config(_) => "QDRANT_CONFIG_ERROR",
            Self::InvalidConfig { .. } => "QDRANT_INVALID_CONFIG_ERROR",
            Self::MissingConfig { .. } => "QDRANT_MISSING_CONFIG_ERROR",
            Self::OperationTimeout(_) => "QDRANT_OPERATION_TIMEOUT_ERROR",
            Self::NotConnected => "QDRANT_NOT_CONNECTED_ERROR",
            Self::Disabled => "QDRANT_DISABLED_ERROR",
            Self::Io(_) => "QDRANT_IO_ERROR",
            Self::Json(_) => "QDRANT_JSON_ERROR",
            Self::Http(_) => "QDRANT_HTTP_ERROR",
            Self::CircuitBreakerOpen(_) => "QDRANT_CIRCUIT_BREAKER_OPEN_ERROR",
        }
    }
}

impl ErrorClassify for QdrantError {
    fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Connection(_)
                | Self::ConnectionRefused { .. }
                | Self::ConnectionTimeout(_)
                | Self::OperationTimeout(_)
                | Self::CircuitBreakerOpen(_)
        )
    }

    fn is_transient(&self) -> bool {
        self.is_retryable() || matches!(self, Self::Api(_) | Self::Request(_))
    }

    fn is_permanent(&self) -> bool {
        matches!(
            self,
            Self::CollectionNotFound(_)
                | Self::CollectionAlreadyExists(_)
                | Self::DimensionMismatch { .. }
                | Self::InvalidUrl(_)
                | Self::Config(_)
                | Self::InvalidConfig { .. }
                | Self::MissingConfig { .. }
                | Self::Disabled
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_creation() {
        let err = QdrantError::connection("test connection error");
        assert!(err.is_connection_error());
        assert!(err.is_retryable());
        assert!(!err.is_not_found());
    }

    #[test]
    fn test_not_found_error() {
        let err = QdrantError::CollectionNotFound(NotFoundError::new("test_collection"));
        assert!(err.is_not_found());
        assert!(!err.is_connection_error());
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_dimension_mismatch() {
        let err = QdrantError::DimensionMismatch {
            expected: 1024,
            actual: 512,
        };
        assert!(!err.is_connection_error());
        assert!(!err.is_not_found());
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_error_display() {
        let err = QdrantError::CollectionNotFound(NotFoundError::new("my_collection"));
        assert!(err.to_string().contains("my_collection"));

        let err = QdrantError::DimensionMismatch {
            expected: 1024,
            actual: 512,
        };
        assert!(err.to_string().contains("1024"));
        assert!(err.to_string().contains("512"));
    }

    #[test]
    fn test_qdrant_error_to_storage_error() {
        use crate::types::StorageError;

        let err: StorageError = QdrantError::ConnectionTimeout("timed out".into()).into();
        assert!(matches!(
            err,
            StorageError::Qdrant(QdrantError::ConnectionTimeout(_))
        ));
        assert!(err.is_retryable());

        let err: StorageError = QdrantError::Api("bad request".into()).into();
        assert!(matches!(err, StorageError::Qdrant(QdrantError::Api(_))));
        // Api errors are transient at the backend level but not retryable
        // (they reflect a server-side rejection, not a recoverable outage).
        assert!(!err.is_retryable());
        assert!(err.is_transient());

        let err: StorageError = QdrantError::CollectionNotFound(NotFoundError::new("coll")).into();
        assert!(matches!(
            err,
            StorageError::Qdrant(QdrantError::CollectionNotFound(_))
        ));
        assert!(err.is_permanent());

        let err: StorageError = QdrantError::Disabled.into();
        assert!(matches!(err, StorageError::Qdrant(QdrantError::Disabled)));
        assert!(err.is_permanent());

        let err: StorageError = QdrantError::ConnectionRefused {
            url: "localhost:6333".into(),
            message: "refused".into(),
        }
        .into();
        assert!(matches!(
            err,
            StorageError::Qdrant(QdrantError::ConnectionRefused { .. })
        ));
    }
}
