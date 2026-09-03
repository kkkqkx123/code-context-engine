//! LLM chat contract types shared by the whole workspace
//!
//! The parser's summary pipeline consumes LLM chat through the [`LlmClient`]
//! port; the concrete HTTP implementation lives in
//! `cce_infrastructure::llm::HttpLlmClient` and is injected as a generic
//! parameter (no trait objects).

use thiserror::Error;

use cce_types::error::ConfigError;
use cce_types::error::common::{HttpError, TimeoutError};

/// Configuration error type for LLM
#[derive(Error, Debug, PartialEq)]
pub enum LlmConfigError {
    /// No API key provided for a specific provider
    #[error(
        "No API key provided for provider '{provider_id}'. Configure it in one of these ways:\n  1. Config file: [llm.providers.{provider_id}].api_keys = [\"${{CCE_LLM_API_KEY_{env_var_name}}}\"]\n  2. Environment variable: CCE_LLM_API_KEY_{env_var_name}\n  3. .env file: CCE_LLM_API_KEY_{env_var_name}=your_key\n  4. File path: [llm.providers.{provider_id}].api_key_file = \"/path/to/key\""
    )]
    MissingApiKey {
        /// Provider ID (e.g., "openai", "siliconflow")
        provider_id: String,
        /// Environment variable name (uppercase provider ID)
        env_var_name: String,
    },

    /// Base URL is required
    #[error("base_url is required for provider '{0}'")]
    MissingBaseUrl(String),

    /// Model is required
    #[error("model is required for provider '{0}'")]
    MissingModel(String),

    /// Invalid max batch tokens
    #[error("max_batch_tokens must be > 0")]
    InvalidMaxBatchTokens,

    /// Invalid max item tokens
    #[error("max_item_tokens must be > 0")]
    InvalidMaxItemTokens,

    /// Invalid URL
    #[error("Invalid URL: {0}")]
    InvalidUrl(String),
}

impl LlmConfigError {
    /// Create a missing API key error
    pub fn missing_api_key(provider_id: impl Into<String>) -> Self {
        let provider_id = provider_id.into();
        let env_var_name = provider_id.to_uppercase().replace('-', "_");
        Self::MissingApiKey {
            provider_id,
            env_var_name,
        }
    }

    /// Create a missing base URL error
    pub fn missing_base_url(provider_id: impl Into<String>) -> Self {
        Self::MissingBaseUrl(provider_id.into())
    }

    /// Create a missing model error
    pub fn missing_model(provider_id: impl Into<String>) -> Self {
        Self::MissingModel(provider_id.into())
    }

    /// Create an invalid max batch tokens error
    pub fn invalid_max_batch_tokens() -> Self {
        Self::InvalidMaxBatchTokens
    }

    /// Create an invalid max item tokens error
    pub fn invalid_max_item_tokens() -> Self {
        Self::InvalidMaxItemTokens
    }

    /// Create an invalid URL error
    pub fn invalid_url(url: impl Into<String>) -> Self {
        Self::InvalidUrl(url.into())
    }
}

// Convert LlmConfigError to common ConfigError
impl From<LlmConfigError> for ConfigError {
    fn from(err: LlmConfigError) -> Self {
        Self::Other(err.to_string())
    }
}

/// LLM error type for unified error handling
///
/// `Clone` supports fan-out scenarios where one error result is shared across
/// multiple awaiters (e.g. single-flight caches returning `Arc<LlmError>`).
#[derive(Error, Debug, Clone)]
pub enum LlmError {
    /// HTTP transport error (no HTTP response received) - uses common HttpError
    #[error("{0}")]
    Http(#[from] HttpError),

    /// HTTP response with a non-success status code
    #[error("HTTP status {status}: {message}")]
    HttpStatus { status: u16, message: String },

    /// API error from LLM provider
    #[error("API error: {0}")]
    Api(String),

    /// Configuration error - uses common ConfigError
    #[error("{0}")]
    Config(#[from] ConfigError),

    /// Invalid input
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    /// Rate limit exceeded with retry-after milliseconds
    #[error("Rate limit exceeded, retry after {0}ms")]
    RateLimitExceeded(u64),

    /// Request timeout - uses common TimeoutError
    #[error("{0}")]
    Timeout(#[from] TimeoutError),

    /// Invalid response from API
    #[error("Invalid response: {0}")]
    InvalidResponse(String),

    /// Token limit exceeded
    #[error("Token limit exceeded: {0} > {1}")]
    TokenLimitExceeded(usize, usize),

    /// Authentication error
    #[error("Authentication failed: {0}")]
    Auth(String),

    /// Model not found or unavailable
    #[error("Model not found: {0}")]
    ModelNotFound(String),

    /// Internal error
    #[error("Internal error: {0}")]
    Internal(String),
}

impl LlmError {
    /// Create an HTTP transport error (no HTTP response received)
    pub fn http(reason: impl Into<String>) -> Self {
        Self::Http(HttpError::new(reason))
    }

    /// Create an HTTP error carrying a typed status code
    pub fn http_status(status: u16, reason: impl Into<String>) -> Self {
        Self::HttpStatus {
            status,
            message: reason.into(),
        }
    }

    /// Create an API error
    pub fn api(reason: impl Into<String>) -> Self {
        Self::Api(reason.into())
    }

    /// Create a configuration error
    pub fn config(reason: impl Into<String>) -> Self {
        Self::Config(ConfigError::Other(reason.into()))
    }

    /// Create an invalid input error
    pub fn invalid_input(reason: impl Into<String>) -> Self {
        Self::InvalidInput(reason.into())
    }

    /// Create a rate limit error
    pub fn rate_limit_exceeded(retry_after_ms: u64) -> Self {
        Self::RateLimitExceeded(retry_after_ms)
    }

    /// Create an invalid response error
    pub fn invalid_response(reason: impl Into<String>) -> Self {
        Self::InvalidResponse(reason.into())
    }

    /// Create a token limit error
    pub fn token_limit_exceeded(actual: usize, limit: usize) -> Self {
        Self::TokenLimitExceeded(actual, limit)
    }

    /// Create an authentication error
    pub fn auth(reason: impl Into<String>) -> Self {
        Self::Auth(reason.into())
    }

    /// Create a model not found error
    pub fn model_not_found(model: impl Into<String>) -> Self {
        Self::ModelNotFound(model.into())
    }

    /// Create an internal error
    pub fn internal(reason: impl Into<String>) -> Self {
        Self::Internal(reason.into())
    }

    /// Get error code for programmatic error handling
    pub fn error_code(&self) -> &'static str {
        match self {
            Self::Http(_) => "LLM_HTTP_ERROR",
            Self::HttpStatus { .. } => "LLM_HTTP_STATUS_ERROR",
            Self::Api(_) => "LLM_API_ERROR",
            Self::Config(_) => "LLM_CONFIG_ERROR",
            Self::InvalidInput(_) => "LLM_INVALID_INPUT_ERROR",
            Self::RateLimitExceeded(_) => "LLM_RATE_LIMIT_EXCEEDED_ERROR",
            Self::Timeout(_) => "LLM_TIMEOUT_ERROR",
            Self::InvalidResponse(_) => "LLM_INVALID_RESPONSE_ERROR",
            Self::TokenLimitExceeded(_, _) => "LLM_TOKEN_LIMIT_EXCEEDED_ERROR",
            Self::Auth(_) => "LLM_AUTH_ERROR",
            Self::ModelNotFound(_) => "LLM_MODEL_NOT_FOUND_ERROR",
            Self::Internal(_) => "LLM_INTERNAL_ERROR",
        }
    }
}

impl cce_types::error::common::ErrorClassify for LlmError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::Http(_)
            | Self::RateLimitExceeded(_)
            | Self::Timeout(_)
            | Self::InvalidResponse(_) => true,
            Self::HttpStatus { status, .. } => (500..=599).contains(status),
            _ => false,
        }
    }

    fn is_transient(&self) -> bool {
        self.is_retryable() || matches!(self, Self::Api(_))
    }

    fn is_permanent(&self) -> bool {
        match self {
            Self::Config(_)
            | Self::InvalidInput(_)
            | Self::TokenLimitExceeded(_, _)
            | Self::Auth(_)
            | Self::ModelNotFound(_)
            | Self::Internal(_) => true,
            Self::HttpStatus { status, .. } => !(500..=599).contains(status),
            _ => false,
        }
    }
}

/// Convert LlmConfigError to LlmError
impl From<LlmConfigError> for LlmError {
    fn from(err: LlmConfigError) -> Self {
        Self::Config(ConfigError::from(err))
    }
}

/// Error classes used for LLM retry accounting and circuit breaker counting
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmRetryErrorClass {
    /// Rate limit (429) responses
    RateLimited,
    /// HTTP transport/server errors (5xx, network)
    Http,
    /// Request timeouts
    Timeout,
    /// Invalid/unparseable responses
    InvalidResponse,
    /// Any other error
    Other,
}

impl LlmRetryErrorClass {
    /// Stable label value used in metrics
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RateLimited => "rate_limited",
            Self::Http => "http",
            Self::Timeout => "timeout",
            Self::InvalidResponse => "invalid_response",
            Self::Other => "other",
        }
    }

    /// Classify an LLM error
    pub fn from_error(error: &LlmError) -> Self {
        match error {
            LlmError::RateLimitExceeded(_) => Self::RateLimited,
            // 5xx responses indicate an unhealthy upstream; 4xx (client
            // errors) are permanent and do not count toward the circuit.
            LlmError::HttpStatus { status, .. } if (500..=599).contains(status) => Self::Http,
            LlmError::Http(_) => Self::Http,
            LlmError::Timeout(_) => Self::Timeout,
            LlmError::InvalidResponse(_) => Self::InvalidResponse,
            _ => Self::Other,
        }
    }

    /// Whether errors of this class indicate an unhealthy upstream and should
    /// count toward opening the circuit breaker
    pub fn counts_toward_circuit_failure(self) -> bool {
        matches!(self, Self::Http | Self::Timeout | Self::InvalidResponse)
    }
}

impl cce_circuit_breaker::CircuitBreakerRejected for LlmError {
    fn circuit_open(message: impl Into<String>) -> Self {
        LlmError::api(message.into())
    }
}
