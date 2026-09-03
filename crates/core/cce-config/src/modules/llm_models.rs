//! LLM Provider and Model Configuration
//!
//! Provides unified provider registration and model configuration for all LLM services.
//!
//! # Architecture
//!
//! - **Providers**: Define connection details (base_url, api_keys, endpoints) once
//! - **Models**: Define service-specific metadata (dimension, temperature, etc.)
//! - Each model references exactly one provider by ID; provider routing/failover is not handled here
//!
//! # Example
//!
//! ```toml
//! [llm.providers.openai]
//! name = "OpenAI"
//! base_url = "https://api.openai.com/v1"
//! api_keys = ["${OPENAI_API_KEY}"]
//!
//! [llm.embedding_models.text-embedding-3-small]
//! provider_id = "openai"
//! model = "text-embedding-3-small"
//! vector_dimension = 1536
//!
//! [llm.chat_models.gpt-4o]
//! provider_id = "openai"
//! model = "gpt-4o"
//! temperature = 0.7
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::serde_helpers::empty_string_as_none;
use crate::validation::{Validate, ValidationResult};
use cce_types::error::config::ConfigValidationError;
use cce_types::llm::ProviderType;

// Re-use shared default value functions
use super::defaults::{
    default_llm_max_retries as default_max_retries, default_retry_delay, default_timeout,
};

/// Service type for endpoint routing
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceType {
    Embedding,
    Chat,
    Rerank,
    Completion,
}

/// Provider configuration - defines connection details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// Provider ID (unique identifier)
    #[serde(skip)]
    pub id: String,

    /// Display name
    pub name: String,

    /// Whether this provider is enabled (default: true)
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Provider type (local or remote)
    #[serde(default)]
    pub provider_type: ProviderType,

    /// API keys (can be empty when auth is handled elsewhere)
    #[serde(default)]
    pub api_keys: Vec<String>,

    /// Base URL for API endpoint
    pub base_url: String,

    /// Optional endpoint overrides (service_type -> endpoint_path)
    /// If not specified, uses default paths:
    /// - embedding: "embeddings"
    /// - chat: "chat/completions"
    /// - rerank: "rerank"
    #[serde(default)]
    pub endpoints: HashMap<ServiceType, String>,

    /// Request timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,

    /// Maximum retry attempts
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,

    /// Initial retry delay in milliseconds
    #[serde(default = "default_retry_delay")]
    pub retry_delay_ms: u64,

    /// Random jitter ratio applied on top of computed retry delays
    /// (default: 0.2 = up to +20%).
    #[serde(default = "default_retry_jitter")]
    pub retry_jitter: f64,

    /// Independent retry budget (attempts) for rate limit (429) errors
    /// (default: 5).
    #[serde(default = "default_rate_limit_max_retries")]
    pub rate_limit_max_retries: u32,

    /// Upper bound (ms) for the retry-after driven delay of rate limit errors
    /// (default: 60000).
    #[serde(default = "default_rate_limit_max_delay_ms")]
    pub rate_limit_max_delay_ms: u64,

    /// Maximum requests per minute sent to this provider (0 = no limit).
    ///
    /// Shared by all models of this provider: embedding, chat and rerank
    /// clients throttle against the same bucket so the combined request rate
    /// stays below the upstream limit.
    #[serde(default = "default_rate_limit")]
    pub rate_limit: u32,

    /// Circuit breaker settings protecting this provider's upstream.
    #[serde(default)]
    pub circuit_breaker: CircuitBreakerConfig,

    /// Proxy URL (optional)
    #[serde(deserialize_with = "empty_string_as_none", default)]
    pub proxy_url: Option<String>,

    /// Extra HTTP headers
    #[serde(default)]
    pub extra_headers: HashMap<String, String>,

    /// Path to file containing API key (alternative to api_keys)
    #[serde(deserialize_with = "empty_string_as_none", default)]
    pub api_key_file: Option<String>,
}

/// Circuit breaker configuration for an LLM provider
///
/// The breaker is shared per upstream base URL (same granularity as the rate
/// limiter); its settings are taken from the first provider that registers
/// the upstream.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    /// Whether circuit breaking is enabled (default: true)
    #[serde(default = "default_circuit_breaker_enabled")]
    pub enabled: bool,
    /// Consecutive failures (5xx/timeout/network/invalid response) that open
    /// the circuit (default: 5)
    #[serde(default = "default_circuit_breaker_failure_threshold")]
    pub failure_threshold: u32,
    /// Recovery timeout in seconds before the circuit goes half-open and
    /// allows a probe request (default: 60)
    #[serde(default = "default_circuit_breaker_recovery_timeout_secs")]
    pub recovery_timeout_secs: u64,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            enabled: default_circuit_breaker_enabled(),
            failure_threshold: default_circuit_breaker_failure_threshold(),
            recovery_timeout_secs: default_circuit_breaker_recovery_timeout_secs(),
        }
    }
}

fn default_circuit_breaker_enabled() -> bool {
    true
}

fn default_circuit_breaker_failure_threshold() -> u32 {
    5
}

fn default_circuit_breaker_recovery_timeout_secs() -> u64 {
    60
}

impl Validate for ProviderConfig {
    fn validate_structured(&self) -> ValidationResult {
        if !self.enabled {
            return Ok(());
        }

        let mut errors = Vec::new();

        if self.id.is_empty() {
            errors.push(ConfigValidationError::missing_field("provider.id"));
        }
        if self.base_url.is_empty() {
            errors.push(ConfigValidationError::invalid_field(
                "base_url",
                format!("Provider {} has empty base_url", self.id),
            ));
        }
        if self.rate_limit > MAX_RATE_LIMIT {
            errors.push(ConfigValidationError::out_of_range(
                "rate_limit",
                self.rate_limit.to_string(),
                "0",
                MAX_RATE_LIMIT.to_string(),
            ));
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(ConfigValidationError::multiple(errors))
        }
    }
}

impl ProviderConfig {
    /// Default endpoint path for a service type
    pub fn default_endpoint_path(service: ServiceType) -> &'static str {
        match service {
            ServiceType::Embedding => "embeddings",
            ServiceType::Chat => "chat/completions",
            ServiceType::Rerank => "rerank",
            ServiceType::Completion => "completions",
        }
    }

    /// Get the endpoint path for a specific service type, honoring the
    /// `endpoints` override map when present.
    pub fn get_endpoint_path(&self, service: ServiceType) -> String {
        self.endpoints
            .get(&service)
            .cloned()
            .unwrap_or_else(|| Self::default_endpoint_path(service).to_string())
    }

    /// Get the full endpoint URL for a specific service type
    pub fn get_endpoint(&self, service: ServiceType) -> String {
        format!(
            "{}/{}",
            self.base_url.trim_end_matches('/'),
            self.get_endpoint_path(service)
        )
    }
}

/// Upper bound for the configured rate limit (requests per minute), guarding
/// against misconfiguration.
const MAX_RATE_LIMIT: u32 = 10_000;

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            enabled: default_enabled(),
            provider_type: ProviderType::default(),
            api_keys: Vec::new(),
            base_url: String::new(),
            endpoints: HashMap::new(),
            timeout_secs: default_timeout(),
            max_retries: default_max_retries(),
            retry_delay_ms: default_retry_delay(),
            retry_jitter: default_retry_jitter(),
            rate_limit_max_retries: default_rate_limit_max_retries(),
            rate_limit_max_delay_ms: default_rate_limit_max_delay_ms(),
            rate_limit: default_rate_limit(),
            circuit_breaker: CircuitBreakerConfig::default(),
            proxy_url: None,
            extra_headers: HashMap::new(),
            api_key_file: None,
        }
    }
}

fn default_retry_jitter() -> f64 {
    0.2
}

fn default_rate_limit_max_retries() -> u32 {
    20
}

fn default_rate_limit_max_delay_ms() -> u64 {
    60000
}

fn default_rate_limit() -> u32 {
    60
}

fn default_enabled() -> bool {
    true
}

/// Embedding model configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingModelConfig {
    /// Reference to provider ID
    pub provider_id: String,

    /// Model name
    pub model: String,

    /// Vector dimension (required)
    pub vector_dimension: usize,

    /// API model name (if different from model ID)
    #[serde(default)]
    pub api_model_name: Option<String>,

    /// Maximum tokens per batch request
    #[serde(default = "default_max_batch_tokens")]
    pub max_batch_tokens: usize,

    /// Maximum tokens per single text item
    #[serde(default = "default_max_item_tokens")]
    pub max_item_tokens: usize,

    /// Preprocessor configuration
    #[serde(default)]
    pub preprocessor: PreprocessorConfig,
}

impl Default for EmbeddingModelConfig {
    fn default() -> Self {
        Self {
            provider_id: String::new(),
            model: String::new(),
            vector_dimension: 0,
            api_model_name: None,
            max_batch_tokens: default_max_batch_tokens(),
            max_item_tokens: default_max_item_tokens(),
            preprocessor: PreprocessorConfig::default(),
        }
    }
}

/// Chat model configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatModelConfig {
    /// Reference to provider ID
    pub provider_id: String,

    /// Model name
    pub model: String,

    /// Temperature parameter
    #[serde(default = "default_temperature")]
    pub temperature: f32,

    /// Maximum output tokens
    #[serde(default = "default_max_output_tokens")]
    pub max_tokens: u32,

    /// Top-p parameter
    #[serde(default = "default_top_p")]
    pub top_p: f32,

    /// Maximum input tokens
    #[serde(default = "default_max_input_tokens")]
    pub max_input_tokens: usize,

    /// Extra parameters (provider-specific)
    #[serde(default)]
    pub extra_params: HashMap<String, serde_json::Value>,
}

impl Default for ChatModelConfig {
    fn default() -> Self {
        Self {
            provider_id: String::new(),
            model: String::new(),
            temperature: default_temperature(),
            max_tokens: default_max_output_tokens(),
            top_p: default_top_p(),
            max_input_tokens: default_max_input_tokens(),
            extra_params: HashMap::new(),
        }
    }
}

/// Rerank model execution mode
///
/// `Generative` scores candidates through a chat-completion prompt; it works
/// with any chat-capable LLM. `CrossEncoder` calls a dedicated `/rerank`
/// endpoint (Cohere-compatible schema, e.g. BAAI/bge-reranker-* on SiliconFlow)
/// and requires the provider to expose that endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RerankMode {
    /// Score candidates with a chat-completions scoring prompt (default).
    #[serde(rename = "generative")]
    #[default]
    Generative,
    /// Score candidates with a dedicated `/rerank` (cross-encoder) endpoint.
    #[serde(rename = "cross_encoder")]
    CrossEncoder,
}

/// Rerank model configuration
///
/// Holds only the model resolution contract (`provider_id` + `model` + `mode`).
/// Runtime rerank parameters (candidates, temperature, timeout, ...) come
/// exclusively from the `[rerank]` section (`RerankConfig`).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RerankModelConfig {
    /// Reference to provider ID
    pub provider_id: String,

    /// Model name
    pub model: String,

    /// Execution mode: generative (chat prompt) or cross-encoder (dedicated endpoint)
    #[serde(default)]
    pub mode: RerankMode,
}

/// Preprocessor configuration - re-exported from embedder module
pub use super::embedder::PreprocessorConfig;

// Default value functions
fn default_max_batch_tokens() -> usize {
    8192
}

fn default_max_item_tokens() -> usize {
    2048
}

fn default_temperature() -> f32 {
    0.7
}

fn default_max_output_tokens() -> u32 {
    2048
}

fn default_top_p() -> f32 {
    1.0
}

fn default_max_input_tokens() -> usize {
    8192
}

/// Type alias for backward compatibility
/// ModelDefinition is now EmbeddingModelConfig
pub type ModelDefinition = EmbeddingModelConfig;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_validate_allows_empty_api_keys() {
        let provider = ProviderConfig {
            id: "openai".to_string(),
            name: "OpenAI".to_string(),
            enabled: true,
            provider_type: cce_types::llm::ProviderType::Remote,
            api_keys: Vec::new(),
            base_url: "https://api.openai.com/v1".to_string(),
            endpoints: HashMap::new(),
            timeout_secs: default_timeout(),
            max_retries: default_max_retries(),
            retry_delay_ms: default_retry_delay(),
            retry_jitter: default_retry_jitter(),
            rate_limit_max_retries: default_rate_limit_max_retries(),
            rate_limit_max_delay_ms: default_rate_limit_max_delay_ms(),
            rate_limit: default_rate_limit(),
            circuit_breaker: CircuitBreakerConfig::default(),
            proxy_url: None,
            extra_headers: HashMap::new(),
            api_key_file: None,
        };

        assert!(provider.validate_structured().is_ok());
    }

    #[test]
    fn test_provider_validate_rejects_excessive_rate_limit() {
        let provider = ProviderConfig {
            id: "openai".to_string(),
            name: "OpenAI".to_string(),
            enabled: true,
            provider_type: cce_types::llm::ProviderType::Remote,
            api_keys: vec!["key".to_string()],
            base_url: "https://api.openai.com/v1".to_string(),
            endpoints: HashMap::new(),
            timeout_secs: 30,
            max_retries: 3,
            retry_delay_ms: 1000,
            retry_jitter: 0.2,
            rate_limit_max_retries: 5,
            rate_limit_max_delay_ms: 60000,
            rate_limit: 100_000,
            circuit_breaker: CircuitBreakerConfig::default(),
            proxy_url: None,
            extra_headers: HashMap::new(),
            api_key_file: None,
        };

        assert!(provider.validate_structured().is_err());
    }

    #[test]
    fn test_provider_rate_limit_serde_default_and_explicit() {
        // Default is 60/min when the field is absent
        let toml_without = "id = \"a\"\nname = \"A\"\nbase_url = \"https://a.example.com\"\n";
        let provider: ProviderConfig =
            toml::from_str(toml_without).expect("default rate_limit should deserialize");
        assert_eq!(provider.rate_limit, 60);

        // Explicit 0 means unlimited
        let toml_zero =
            "id = \"a\"\nname = \"A\"\nbase_url = \"https://a.example.com\"\nrate_limit = 0\n";
        let provider: ProviderConfig =
            toml::from_str(toml_zero).expect("rate_limit 0 should deserialize");
        assert_eq!(provider.rate_limit, 0);
    }

    #[test]
    fn test_provider_validate_rejects_empty_base_url() {
        let provider = ProviderConfig {
            id: "openai".to_string(),
            name: "OpenAI".to_string(),
            enabled: true,
            provider_type: cce_types::llm::ProviderType::Remote,
            api_keys: vec!["key".to_string()],
            base_url: String::new(),
            endpoints: HashMap::new(),
            timeout_secs: default_timeout(),
            max_retries: default_max_retries(),
            retry_delay_ms: default_retry_delay(),
            retry_jitter: default_retry_jitter(),
            rate_limit_max_retries: default_rate_limit_max_retries(),
            rate_limit_max_delay_ms: default_rate_limit_max_delay_ms(),
            rate_limit: default_rate_limit(),
            circuit_breaker: CircuitBreakerConfig::default(),
            proxy_url: None,
            extra_headers: HashMap::new(),
            api_key_file: None,
        };

        assert!(provider.validate_structured().is_err());
    }

    #[test]
    fn test_provider_get_endpoint_path_honors_overrides() {
        let provider = ProviderConfig {
            id: "azure".to_string(),
            name: "Azure".to_string(),
            enabled: true,
            provider_type: cce_types::llm::ProviderType::Remote,
            api_keys: vec!["key".to_string()],
            base_url: "https://res.openai.azure.com".to_string(),
            endpoints: HashMap::from([
                (
                    ServiceType::Embedding,
                    "embeddings?api-version=2024-02-01".to_string(),
                ),
                (
                    ServiceType::Chat,
                    "chat/completions?api-version=2024-02-01".to_string(),
                ),
            ]),
            timeout_secs: default_timeout(),
            max_retries: default_max_retries(),
            retry_delay_ms: default_retry_delay(),
            retry_jitter: default_retry_jitter(),
            rate_limit_max_retries: default_rate_limit_max_retries(),
            rate_limit_max_delay_ms: default_rate_limit_max_delay_ms(),
            rate_limit: default_rate_limit(),
            circuit_breaker: CircuitBreakerConfig::default(),
            proxy_url: None,
            extra_headers: HashMap::new(),
            api_key_file: None,
        };

        assert_eq!(
            provider.get_endpoint_path(ServiceType::Embedding),
            "embeddings?api-version=2024-02-01"
        );
        assert_eq!(
            provider.get_endpoint_path(ServiceType::Rerank),
            "rerank",
            "unoverridden service falls back to its default path"
        );
        assert_eq!(
            provider.get_endpoint(ServiceType::Chat),
            "https://res.openai.azure.com/chat/completions?api-version=2024-02-01"
        );
    }

    #[test]
    fn test_rerank_mode_serde() {
        use serde_json::json;

        let parsed: RerankMode = serde_json::from_value(json!("cross_encoder")).expect("parse");
        assert_eq!(parsed, RerankMode::CrossEncoder);

        let parsed: RerankMode = serde_json::from_value(json!("generative")).expect("parse");
        assert_eq!(parsed, RerankMode::Generative);

        // Unknown values must be rejected loudly instead of silently defaulting.
        assert!(serde_json::from_value::<RerankMode>(json!("unknown")).is_err());
        assert_eq!(
            serde_json::to_string(&RerankMode::default()).expect("serialize"),
            "\"generative\""
        );
    }
}
