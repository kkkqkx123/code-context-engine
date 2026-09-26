//! Error types for orchestrator operations

use cce_types::error::ConfigError;
use cce_types::error::common::{ErrorClassify, IoError, NotFoundError, TimeoutError};
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
}

/// Identifier for an embedding stage that exceeded its wall-clock
/// deadline. Kept here so the module-failure projection can preserve
/// it instead of collapsing to the generic index error code.
pub(crate) const EMBEDDING_STAGE_TIMEOUT_CODE: &str = "EMBEDDING_STAGE_TIMEOUT";

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

    /// Whether this error means a backend rejected the operation because its
    /// circuit breaker is open (vector store or LLM provider). Surfaced on the
    /// index result so callers can distinguish an outage from per-file failures.
    pub fn is_circuit_open(&self) -> bool {
        matches!(
            self,
            Self::Storage(cce_types::error::StorageError::Qdrant(
                cce_types::error::QdrantError::CircuitBreakerOpen(_)
            )) | Self::Llm(cce_llm_client::LlmError::CircuitBreakerOpen(_))
        )
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
        }
    }

    /// Project this error into a classified module failure for the state
    /// tracker. Retryability matches the file-level transient/permanent split
    /// (`is_transient`), and typed inner errors keep their stable error code
    /// so dead-letter reports can aggregate by code and recovery routing can
    /// act on it (the token limit code drives truncate eligibility).
    pub fn as_module_failure(&self) -> crate::index_state::TrackerFailure {
        use crate::index_state::TrackerFailure;
        TrackerFailure {
            message: self.to_string(),
            code: match self {
                Self::Llm(err) => Some(err.error_code().to_string()),
                Self::Parse(err) => Some(err.error_code().to_string()),
                Self::Storage(err) => Some(err.error_code().to_string()),
                Self::Scanner(err) => Some(err.error_code().to_string()),
                Self::Query(err) => Some(err.error_code().to_string()),
                Self::Config(err) => Some(err.error_code().to_string()),
                Self::NotFound(_) => Some("ORCH_NOT_FOUND_ERROR".to_string()),
                Self::Timeout(_) => Some("ORCH_TIMEOUT_ERROR".to_string()),
                Self::Index { operation, .. } if operation == EMBEDDING_STAGE_TIMEOUT_CODE => {
                    Some(EMBEDDING_STAGE_TIMEOUT_CODE.to_string())
                }
                Self::Index { .. } => Some("ORCH_INDEX_ERROR".to_string()),
                Self::Cache { .. } => Some("ORCH_CACHE_ERROR".to_string()),
                Self::HotUpdate { .. } => Some("ORCH_HOT_UPDATE_ERROR".to_string()),
                Self::Merge { .. } => Some("ORCH_MERGE_ERROR".to_string()),
            },
            retryable: self.is_transient(),
        }
    }
}

impl ErrorClassify for OrchestratorError {
    fn is_retryable(&self) -> bool {
        self.is_transient()
    }

    fn is_transient(&self) -> bool {
        match self {
            Self::Parse(err) => err.is_transient(),
            Self::Llm(err) => err.is_transient(),
            Self::Scanner(err) => err.is_transient(),
            Self::Storage(err) => err.is_transient(),
            Self::Query(err) => err.is_transient(),
            Self::Timeout(_) => true,
            Self::Config(_) | Self::NotFound(_) => false,
            // String-reason orchestrator variants are infrastructure faults
            // recorded during batch bookkeeping (checkpoint writes, spool I/O,
            // merge passes), never deterministic content failures, so a file
            // is only skipped when its own content caused the failure.
            Self::Index { .. }
            | Self::Cache { .. }
            | Self::HotUpdate { .. }
            | Self::Merge { .. } => true,
        }
    }

    fn is_permanent(&self) -> bool {
        !self.is_transient()
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

        let err = OrchestratorError::Scanner(cce_scanner::ScannerError::scan(
            "/test/path",
            "test reason",
        ));
        assert!(matches!(err, OrchestratorError::Scanner(_)));
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
            OrchestratorError::Scanner(cce_scanner::ScannerError::scan("test", "test"))
                .error_type(),
            "scanner_error"
        );
    }

    #[test]
    fn test_classification() {
        // Deterministic parse failures delegate to permanent.
        assert!(
            OrchestratorError::Parse(cce_types::error::ParseError::unsupported_language("foo"))
                .is_permanent()
        );
        // IO-backed parse failures depend on the error kind.
        let transient_io = std::io::Error::new(std::io::ErrorKind::Interrupted, "test");
        assert!(
            OrchestratorError::Parse(cce_types::error::ParseError::from(transient_io))
                .is_transient()
        );
        // Transient storage faults stay retryable at file level.
        let plain = OrchestratorError::Storage(cce_types::error::StorageError::sqlite("locked"));
        assert!(plain.is_transient());
        // Regression: a content-level NotFound surfacing on a per-file path
        // must classify as permanent, not burn retries before dead-lettering.
        let not_found = OrchestratorError::Storage(cce_types::error::StorageError::not_found(
            "missing work unit",
        ));
        assert!(not_found.is_permanent());
        assert!(!not_found.is_transient());
        assert!(!not_found.as_module_failure().retryable);
        // String-reason orchestrator variants stay retryable by default.
        assert!(OrchestratorError::index("op", "reason").is_transient());
    }

    #[test]
    fn circuit_open_detection() {
        let err = OrchestratorError::Storage(cce_types::error::StorageError::Qdrant(
            cce_types::error::QdrantError::CircuitBreakerOpen("open".into()),
        ));
        assert!(err.is_circuit_open());

        let other = OrchestratorError::Storage(cce_types::error::StorageError::Qdrant(
            cce_types::error::QdrantError::request("connection reset"),
        ));
        assert!(!other.is_circuit_open());

        let llm = OrchestratorError::Llm(cce_llm_client::LlmError::circuit_breaker_open("open"));
        assert!(llm.is_circuit_open());

        let llm_other = OrchestratorError::Llm(cce_llm_client::LlmError::http("connection reset"));
        assert!(!llm_other.is_circuit_open());
    }

    #[test]
    fn module_failure_projection_preserves_classification() {
        let err =
            OrchestratorError::Llm(cce_llm_client::LlmError::token_limit_exceeded(9000, 8192));
        let failure = err.as_module_failure();
        assert_eq!(
            failure.code.as_deref(),
            Some("LLM_TOKEN_LIMIT_EXCEEDED_ERROR")
        );
        assert!(!failure.retryable);

        let storage = OrchestratorError::Storage(cce_types::error::StorageError::sqlite("locked"));
        let failure = storage.as_module_failure();
        assert!(failure.retryable);
        assert_eq!(failure.code.as_deref(), Some("STORAGE_SQLITE_ERROR"));
    }

    #[test]
    fn module_failure_projection_covers_all_variants() {
        let query = OrchestratorError::Query(crate::query::QueryError::not_found("missing".into()));
        let failure = query.as_module_failure();
        assert_eq!(failure.code.as_deref(), Some("QUERY_NOT_FOUND"));
        assert!(!failure.retryable);

        let config = OrchestratorError::Config(ConfigError::missing_env_var("CCE_KEY"));
        let failure = config.as_module_failure();
        assert_eq!(failure.code.as_deref(), Some("CONFIG_MISSING_ENV_VAR"));
        assert!(!failure.retryable);

        let index = OrchestratorError::index("op", "reason");
        let failure = index.as_module_failure();
        assert_eq!(failure.code.as_deref(), Some("ORCH_INDEX_ERROR"));
        assert!(failure.retryable);
    }
}
