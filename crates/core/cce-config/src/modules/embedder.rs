//! Embedder configuration
//!
//! Provides configuration types for the HTTP-based embedder.
//!
//! # Model Registry Design
//!
//! The embedder uses the unified provider and model configuration from `llm_models` module:
//! - Providers are defined in `[llm.providers]` section
//! - Embedding models are defined in `[llm.embedding_models]` section
//! - Projects reference models by name, inheriting provider settings

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::serde_helpers::empty_string_as_none;
use crate::validation::{Validate, ValidationResult};
use cce_types::error::config::ConfigValidationError;

// Re-use shared default value functions
use super::defaults::{
    default_embedder_max_retries as default_max_retries, default_retry_delay, default_timeout,
    default_true,
};

// Re-export types from llm_models module
pub use super::llm_models::EmbeddingModelConfig;

/// Embedder configuration - runtime settings only
///
/// Model definitions and providers are managed in [llm] section.
/// This struct only contains embedder-specific runtime configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EmbedderConfig {
    /// Default model to use (references a model in llm.embedding_models)
    pub default_model: String,

    /// Maximum tokens per batch request
    #[serde(default = "default_max_batch_tokens")]
    pub max_batch_tokens: usize,

    /// Maximum tokens per single text item
    #[serde(default = "default_max_item_tokens")]
    pub max_item_tokens: usize,

    /// Request timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,

    /// Maximum retry attempts
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,

    /// Initial retry delay in milliseconds
    #[serde(default = "default_retry_delay")]
    pub retry_delay_ms: u64,

    /// Proxy URL (optional)
    #[serde(deserialize_with = "empty_string_as_none", default)]
    pub proxy_url: Option<String>,

    /// Extra HTTP headers
    #[serde(default)]
    pub extra_headers: HashMap<String, String>,

    /// Extra request parameters
    #[serde(default)]
    pub extra_params: HashMap<String, serde_json::Value>,

    /// Use base64 encoding for embeddings (default: true)
    #[serde(default = "default_true")]
    pub use_base64: bool,

    /// Path to file containing API key (alternative to api_keys)
    #[serde(deserialize_with = "empty_string_as_none", default)]
    pub api_key_file: Option<String>,
}

/// Preprocessor configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PreprocessorConfig {
    /// No preprocessing (default)
    #[default]
    None,
    /// Simple prefix
    Prefix { prefix: String },
    /// Template with {text} placeholder
    Template { template: String },
    /// Nomic-Embed task type
    Nomic { task_type: String },
    /// Stella task type
    Stella { task_type: String },
}

fn default_max_batch_tokens() -> usize {
    8192
}
fn default_max_item_tokens() -> usize {
    8192
}

impl Default for EmbedderConfig {
    fn default() -> Self {
        Self {
            default_model: String::new(),
            max_batch_tokens: default_max_batch_tokens(),
            max_item_tokens: default_max_item_tokens(),
            timeout_secs: default_timeout(),
            max_retries: default_max_retries(),
            retry_delay_ms: default_retry_delay(),
            proxy_url: None,
            extra_headers: HashMap::new(),
            extra_params: HashMap::new(),
            use_base64: true,
            api_key_file: None,
        }
    }
}

impl Validate for EmbedderConfig {
    fn validate_structured(&self) -> ValidationResult {
        let mut errors = Vec::new();

        if self.default_model.is_empty() {
            errors.push(ConfigValidationError::invalid_field(
                "default_model",
                "cannot be empty",
            ));
        }
        if self.max_batch_tokens == 0 {
            errors.push(ConfigValidationError::invalid_field(
                "max_batch_tokens",
                "must be greater than 0",
            ));
        }
        if self.max_item_tokens == 0 {
            errors.push(ConfigValidationError::invalid_field(
                "max_item_tokens",
                "must be greater than 0",
            ));
        }
        if self.timeout_secs == 0 {
            errors.push(ConfigValidationError::invalid_field(
                "timeout_secs",
                "must be greater than 0",
            ));
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(ConfigValidationError::multiple(errors))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = EmbedderConfig::default();
        assert_eq!(config.default_model, "");
        assert_eq!(config.max_batch_tokens, 8192);
        assert_eq!(config.max_item_tokens, 8192);
        assert_eq!(config.timeout_secs, 30);
        assert!(config.use_base64);
    }
}
