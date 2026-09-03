//! LLM configuration
//!
//! Provides configuration types for LLM clients supporting both
//! embedding and chat/completion APIs.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use cce_config::modules::ServiceType;

pub use cce_types::llm::ProviderType;

/// LLM configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// Resolved API keys loaded from config, environment, or key files.
    #[serde(default)]
    pub api_keys: Vec<String>,

    /// Base URL for API endpoint
    pub base_url: String,

    /// Per-service endpoint path overrides (service type -> path).
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
    #[serde(default = "default_retry_jitter")]
    pub retry_jitter: f64,

    /// Independent retry budget (attempts) for rate limit (429) errors
    #[serde(default = "default_rate_limit_max_retries")]
    pub rate_limit_max_retries: u32,

    /// Upper bound (ms) for the retry-after driven delay of rate limit errors
    #[serde(default = "default_rate_limit_max_delay_ms")]
    pub rate_limit_max_delay_ms: u64,

    /// Circuit breaker settings for this provider's upstream
    #[serde(default)]
    pub circuit_breaker: cce_config::modules::CircuitBreakerConfig,

    /// Proxy URL (optional)
    #[serde(default)]
    pub proxy_url: Option<String>,

    /// Extra HTTP headers
    #[serde(default)]
    pub extra_headers: HashMap<String, String>,

    /// Extra query parameters included in every request.
    #[serde(default)]
    pub extra_params: HashMap<String, serde_json::Value>,
}

/// Embedding-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    /// Model to use for embeddings
    pub model: String,

    /// Maximum tokens per batch request
    #[serde(default = "default_max_batch_tokens")]
    pub max_batch_tokens: usize,

    /// Maximum tokens per single text item
    #[serde(default = "default_max_item_tokens")]
    pub max_item_tokens: usize,

    /// Vector dimension (if known)
    #[serde(default)]
    pub vector_dimension: Option<usize>,

    /// Use base64 encoding for embeddings
    #[serde(default = "default_true")]
    pub use_base64: bool,
}

/// Chat/Completion-specific configuration (moved to `cce_llm`)
pub use cce_llm::{ChatConfig, ResponseFormat};

// Default value functions
fn default_timeout() -> u64 {
    60
}

fn default_max_retries() -> u32 {
    3
}

fn default_retry_delay() -> u64 {
    1000
}

fn default_retry_jitter() -> f64 {
    0.2
}

fn default_rate_limit_max_retries() -> u32 {
    5
}

fn default_rate_limit_max_delay_ms() -> u64 {
    60000
}

fn default_max_batch_tokens() -> usize {
    8192
}

fn default_max_item_tokens() -> usize {
    8192
}

fn default_true() -> bool {
    true
}

impl LlmConfig {
    /// Create OpenAI configuration
    pub fn openai(api_key: String) -> Self {
        Self {
            api_keys: vec![api_key],
            base_url: "https://api.openai.com/v1".to_string(),
            ..Default::default()
        }
    }

    /// Create Gemini configuration
    pub fn gemini(api_key: String) -> Self {
        Self {
            api_keys: vec![api_key],
            base_url: "https://generativelanguage.googleapis.com/v1beta/openai/".to_string(),
            ..Default::default()
        }
    }

    /// Create Ollama configuration
    pub fn ollama() -> Self {
        Self {
            api_keys: vec!["ollama".to_string()],
            base_url: "http://localhost:11434/v1".to_string(),
            ..Default::default()
        }
    }

    /// Create Azure OpenAI configuration
    pub fn azure(api_key: String, resource_name: String, api_version: Option<String>) -> Self {
        let version = api_version.unwrap_or_else(|| "2024-02-01".to_string());
        Self {
            api_keys: vec![api_key],
            base_url: format!(
                "https://{}.openai.azure.com/openai/deployments",
                resource_name
            ),
            extra_params: {
                let mut params = HashMap::new();
                params.insert("api-version".to_string(), serde_json::json!(version));
                params
            },
            ..Default::default()
        }
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<(), crate::core::LlmConfigError> {
        if self.base_url.is_empty() {
            return Err(crate::core::LlmConfigError::missing_base_url("unknown"));
        }
        Ok(())
    }

    /// Validate configuration with provider ID for better error messages
    pub fn validate_with_provider(
        &self,
        provider_id: &str,
    ) -> Result<(), crate::core::LlmConfigError> {
        if self.base_url.is_empty() {
            return Err(crate::core::LlmConfigError::missing_base_url(provider_id));
        }
        Ok(())
    }

    /// Build full URL for endpoint
    pub fn build_url(&self, endpoint: &str) -> String {
        format!("{}/{}", self.base_url.trim_end_matches('/'), endpoint)
    }

    /// Resolve the full URL for a service, honoring per-service endpoint
    /// path overrides (`endpoints`) and falling back to the default path.
    pub fn endpoint_url(&self, service: ServiceType) -> String {
        let path = self.endpoints.get(&service).cloned().unwrap_or_else(|| {
            cce_config::modules::ProviderConfig::default_endpoint_path(service).to_string()
        });
        self.build_url(&path)
    }
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            api_keys: Vec::new(),
            base_url: String::new(),
            endpoints: HashMap::new(),
            timeout_secs: default_timeout(),
            max_retries: default_max_retries(),
            retry_delay_ms: default_retry_delay(),
            retry_jitter: default_retry_jitter(),
            rate_limit_max_retries: default_rate_limit_max_retries(),
            rate_limit_max_delay_ms: default_rate_limit_max_delay_ms(),
            circuit_breaker: cce_config::modules::CircuitBreakerConfig::default(),
            proxy_url: None,
            extra_headers: HashMap::new(),
            extra_params: HashMap::new(),
        }
    }
}

impl EmbeddingConfig {
    /// Create OpenAI embedding configuration
    pub fn openai(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            vector_dimension: Some(1536),
            ..Default::default()
        }
    }

    /// Create small embedding configuration (OpenAI text-embedding-3-small)
    pub fn openai_small() -> Self {
        Self {
            model: "text-embedding-3-small".to_string(),
            vector_dimension: Some(1536),
            ..Default::default()
        }
    }

    /// Create large embedding configuration (OpenAI text-embedding-3-large)
    pub fn openai_large() -> Self {
        Self {
            model: "text-embedding-3-large".to_string(),
            vector_dimension: Some(3072),
            ..Default::default()
        }
    }
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            model: String::new(),
            max_batch_tokens: default_max_batch_tokens(),
            max_item_tokens: default_max_item_tokens(),
            vector_dimension: None,
            use_base64: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_config() {
        let config = LlmConfig::openai("sk-test".to_string());
        assert!(config.validate().is_ok());
        assert_eq!(config.base_url, "https://api.openai.com/v1");
    }

    #[test]
    fn test_ollama_config() {
        let config = LlmConfig::ollama();
        assert!(config.validate().is_ok());
        assert_eq!(config.base_url, "http://localhost:11434/v1");
    }

    #[test]
    fn test_invalid_config() {
        let config = LlmConfig::default();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_embedding_config() {
        let config = EmbeddingConfig::openai_small();
        assert_eq!(config.model, "text-embedding-3-small");
        assert_eq!(config.vector_dimension, Some(1536));
    }

    #[test]
    fn test_build_url() {
        let config = LlmConfig::openai("key".to_string());
        assert_eq!(
            config.build_url("embeddings"),
            "https://api.openai.com/v1/embeddings"
        );
    }

    #[test]
    fn test_endpoint_url_default_paths() {
        let config = LlmConfig::openai("key".to_string());
        assert_eq!(
            config.endpoint_url(ServiceType::Embedding),
            "https://api.openai.com/v1/embeddings"
        );
        assert_eq!(
            config.endpoint_url(ServiceType::Chat),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            config.endpoint_url(ServiceType::Rerank),
            "https://api.openai.com/v1/rerank"
        );
    }

    #[test]
    fn test_endpoint_url_honors_override() {
        let mut config = LlmConfig::openai("key".to_string());
        config.endpoints = HashMap::from([
            (
                ServiceType::Embedding,
                "embeddings?api-version=2024-02-01".to_string(),
            ),
            (
                ServiceType::Chat,
                "chat/completions?api-version=2024-02-01".to_string(),
            ),
        ]);
        assert_eq!(
            config.endpoint_url(ServiceType::Embedding),
            "https://api.openai.com/v1/embeddings?api-version=2024-02-01"
        );
        assert_eq!(
            config.endpoint_url(ServiceType::Chat),
            "https://api.openai.com/v1/chat/completions?api-version=2024-02-01"
        );
        assert_eq!(
            config.endpoint_url(ServiceType::Rerank),
            "https://api.openai.com/v1/rerank"
        );
    }

    #[test]
    fn test_gemini_config() {
        let config = LlmConfig::gemini("test-key".to_string());
        assert!(config.validate().is_ok());
        assert_eq!(
            config.base_url,
            "https://generativelanguage.googleapis.com/v1beta/openai/"
        );
    }

    #[test]
    fn test_azure_config() {
        let config = LlmConfig::azure("test-key".to_string(), "my-resource".to_string(), None);
        assert!(config.validate().is_ok());
        assert!(config.base_url.contains("my-resource"));
        assert_eq!(
            config
                .extra_params
                .get("api-version")
                .expect("Azure API version should be configured"),
            &serde_json::json!("2024-02-01")
        );
    }

    #[test]
    fn test_azure_config_with_version() {
        let config = LlmConfig::azure(
            "test-key".to_string(),
            "my-resource".to_string(),
            Some("2023-05-15".to_string()),
        );
        assert!(config.validate().is_ok());
        assert_eq!(
            config
                .extra_params
                .get("api-version")
                .expect("Azure API version should be configured"),
            &serde_json::json!("2023-05-15")
        );
    }

    #[test]
    fn test_provider_type_is_local() {
        assert!(!ProviderType::Remote.is_local());
        assert!(ProviderType::Local.is_local());
    }

    #[test]
    fn test_chat_config_default() {
        let config = ChatConfig::default();
        assert_eq!(config.temperature, 0.3);
        assert_eq!(config.top_p, 1.0);
        assert_eq!(config.max_tokens, 1024);
        assert!(config.frequency_penalty.is_none());
        assert!(config.presence_penalty.is_none());
    }

    #[test]
    fn test_response_format() {
        let format = ResponseFormat {
            format_type: "json_object".to_string(),
        };
        assert_eq!(format.format_type, "json_object");
    }
}
