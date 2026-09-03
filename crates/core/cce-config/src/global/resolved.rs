use crate::modules::ServiceType;
use crate::validation::Validate;

use super::AppConfig;

/// Resolved embedding configuration - complete config for initializing an embedder
#[derive(Debug, Clone)]
pub struct ResolvedEmbeddingConfig {
    pub base_url: String,
    pub api_keys: Vec<String>,
    pub model: String,
    pub vector_dimension: usize,
    pub preprocessor: crate::modules::PreprocessorConfig,
    pub max_batch_tokens: usize,
    pub max_item_tokens: usize,
    pub timeout_secs: u64,
    pub max_retries: u32,
    pub retry_delay_ms: u64,
    pub proxy_url: Option<String>,
    pub extra_headers: std::collections::HashMap<String, String>,
    pub api_key_file: Option<String>,
    pub use_base64: bool,
    pub extra_params: std::collections::HashMap<String, serde_json::Value>,
    /// Resolved endpoint path for embedding requests (provider override or default)
    pub endpoint_path: String,
}

/// Resolved LLM connection - provider-level settings shared by all LLM services
///
/// Produced by [`AppConfig::resolve_llm_connection`] for a registered model.
#[derive(Debug, Clone)]
pub struct ResolvedLlmConnection {
    pub provider_id: String,
    pub api_keys: Vec<String>,
    pub api_key_file: Option<String>,
    pub base_url: String,
    /// Resolved endpoint path (provider override or service default)
    pub endpoint_path: String,
    pub timeout_secs: u64,
    pub max_retries: u32,
    pub retry_delay_ms: u64,
    /// Random jitter ratio applied on top of computed retry delays
    pub retry_jitter: f64,
    /// Independent retry budget (attempts) for rate limit (429) errors
    pub rate_limit_max_retries: u32,
    /// Upper bound (ms) for the retry-after driven delay of rate limit errors
    pub rate_limit_max_delay_ms: u64,
    /// Provider-wide request rate limit (requests per minute, 0 = unlimited)
    pub rate_limit: u32,
    /// Circuit breaker settings for this provider's upstream
    pub circuit_breaker: crate::modules::CircuitBreakerConfig,
    pub proxy_url: Option<String>,
    pub extra_headers: std::collections::HashMap<String, String>,
    /// Provider-/model-specific extra parameters (e.g. `extra_params` from a chat model)
    pub extra_params: std::collections::HashMap<String, serde_json::Value>,
}

/// Resolved chat configuration - complete config for initializing a chat client
#[derive(Debug, Clone)]
pub struct ResolvedChatConfig {
    pub provider_id: String,
    pub api_keys: Vec<String>,
    pub api_key_file: Option<String>,
    pub base_url: String,
    /// Resolved endpoint path for chat requests (provider override or default)
    pub endpoint_path: String,
    pub timeout_secs: u64,
    pub max_retries: u32,
    pub retry_delay_ms: u64,
    pub proxy_url: Option<String>,
    pub extra_headers: std::collections::HashMap<String, String>,
    /// Provider-/model-specific extra parameters
    pub extra_params: std::collections::HashMap<String, serde_json::Value>,
    /// Chat model name sent to the API
    pub model: String,
    pub temperature: f32,
    pub max_tokens: u32,
    pub top_p: f32,
    pub max_input_tokens: usize,
}

impl AppConfig {
    /// Resolve the provider connection for a registered model of a given service.
    ///
    /// This method combines:
    /// - Model registration from the service-specific registry (`embedding_models`,
    ///   `chat_models`, `rerank_models`)
    /// - Provider connection details from `llm.providers`
    /// - Provider-level endpoint path overrides (`endpoints`)
    ///
    /// Returns the shared connection settings needed to build an HTTP client.
    pub fn resolve_llm_connection(
        &self,
        model_key: &str,
        service: ServiceType,
    ) -> Result<ResolvedLlmConnection, String> {
        let provider_id = match service {
            ServiceType::Embedding => {
                &self
                    .llm
                    .embedding_models
                    .get(model_key)
                    .ok_or_else(|| {
                        format!("Model '{model_key}' not found in llm.embedding_models")
                    })?
                    .provider_id
            }
            ServiceType::Chat => {
                &self
                    .llm
                    .chat_models
                    .get(model_key)
                    .ok_or_else(|| format!("Model '{model_key}' not found in llm.chat_models"))?
                    .provider_id
            }
            ServiceType::Rerank => {
                &self
                    .llm
                    .rerank_models
                    .get(model_key)
                    .ok_or_else(|| format!("Model '{model_key}' not found in llm.rerank_models"))?
                    .provider_id
            }
            ServiceType::Completion => {
                return Err("Completion service has no model registry".to_string());
            }
        };

        let provider =
            self.llm.providers.get(provider_id).ok_or_else(|| {
                format!("Provider '{provider_id}' not found for model '{model_key}'")
            })?;

        provider.validate_structured().map_err(|e| {
            format!("Provider '{provider_id}' validation failed for model '{model_key}': {e}")
        })?;

        let extra_params = match service {
            ServiceType::Chat => self
                .llm
                .chat_models
                .get(model_key)
                .map(|m| m.extra_params.clone())
                .unwrap_or_default(),
            _ => std::collections::HashMap::new(),
        };

        Ok(ResolvedLlmConnection {
            provider_id: provider_id.clone(),
            api_keys: provider.api_keys.clone(),
            api_key_file: provider.api_key_file.clone(),
            base_url: provider.base_url.clone(),
            endpoint_path: provider.get_endpoint_path(service),
            timeout_secs: provider.timeout_secs,
            max_retries: provider.max_retries,
            retry_delay_ms: provider.retry_delay_ms,
            retry_jitter: provider.retry_jitter,
            rate_limit_max_retries: provider.rate_limit_max_retries,
            rate_limit_max_delay_ms: provider.rate_limit_max_delay_ms,
            rate_limit: provider.rate_limit,
            circuit_breaker: provider.circuit_breaker.clone(),
            proxy_url: provider.proxy_url.clone(),
            extra_headers: provider.extra_headers.clone(),
            extra_params,
        })
    }

    /// Resolve embedding configuration for a specific model
    ///
    /// This method combines:
    /// - Model definition from llm.embedding_models
    /// - Provider configuration from llm.providers
    /// - Runtime settings from embedder config
    ///
    /// Returns a complete configuration for initializing an embedder.
    pub fn resolve_embedding_config(
        &self,
        model_name: &str,
    ) -> Result<ResolvedEmbeddingConfig, String> {
        let connection = self.resolve_llm_connection(model_name, ServiceType::Embedding)?;
        let model = self
            .llm
            .embedding_models
            .get(model_name)
            .ok_or_else(|| format!("Model '{model_name}' not found in llm.embedding_models"))?;

        Ok(ResolvedEmbeddingConfig {
            base_url: connection.base_url,
            api_keys: connection.api_keys,
            endpoint_path: connection.endpoint_path,
            api_key_file: connection.api_key_file,
            model: model
                .api_model_name
                .clone()
                .unwrap_or_else(|| model.model.clone()),
            vector_dimension: model.vector_dimension,
            preprocessor: model.preprocessor.clone(),
            max_batch_tokens: model.max_batch_tokens.min(self.embedder.max_batch_tokens),
            max_item_tokens: model.max_item_tokens.min(self.embedder.max_item_tokens),
            timeout_secs: connection.timeout_secs,
            max_retries: connection.max_retries,
            retry_delay_ms: connection.retry_delay_ms,
            proxy_url: connection.proxy_url,
            extra_headers: connection.extra_headers,
            use_base64: self.embedder.use_base64,
            extra_params: self.embedder.extra_params.clone(),
        })
    }

    /// Resolve chat configuration for a specific model
    ///
    /// This method combines:
    /// - Model definition from llm.chat_models
    /// - Provider configuration from llm.providers
    ///
    /// Returns a complete configuration for initializing a chat client.
    pub fn resolve_chat_config(&self, model_name: &str) -> Result<ResolvedChatConfig, String> {
        let connection = self.resolve_llm_connection(model_name, ServiceType::Chat)?;
        let model = self
            .llm
            .chat_models
            .get(model_name)
            .ok_or_else(|| format!("Model '{model_name}' not found in llm.chat_models"))?;

        Ok(ResolvedChatConfig {
            provider_id: connection.provider_id,
            api_keys: connection.api_keys,
            api_key_file: connection.api_key_file,
            base_url: connection.base_url,
            endpoint_path: connection.endpoint_path,
            timeout_secs: connection.timeout_secs,
            max_retries: connection.max_retries,
            retry_delay_ms: connection.retry_delay_ms,
            proxy_url: connection.proxy_url,
            extra_headers: connection.extra_headers,
            extra_params: connection.extra_params,
            model: model.model.clone(),
            temperature: model.temperature,
            max_tokens: model.max_tokens,
            top_p: model.top_p,
            max_input_tokens: model.max_input_tokens,
        })
    }
}
