//! LLM configuration
//!
//! Batching configuration for embeddings; chat parameters live in `cce_llm`
//! and transport settings come from the resolved provider connection.

use serde::{Deserialize, Serialize};

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
}

/// Chat/Completion-specific configuration (moved to `cce_llm`)
pub use cce_llm::{ChatConfig, ResponseFormat};

// Default value functions
fn default_max_batch_tokens() -> usize {
    8192
}

fn default_max_item_tokens() -> usize {
    8192
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_config() {
        let config = EmbeddingConfig::openai_small();
        assert_eq!(config.model, "text-embedding-3-small");
        assert_eq!(config.vector_dimension, Some(1536));
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
