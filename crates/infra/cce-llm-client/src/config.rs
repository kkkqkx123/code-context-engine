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

// Default value functions
fn default_max_batch_tokens() -> usize {
    8192
}

fn default_max_item_tokens() -> usize {
    8192
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
    fn test_embedding_config_default() {
        let config = EmbeddingConfig::default();
        assert_eq!(config.max_batch_tokens, 8192);
        assert_eq!(config.max_item_tokens, 8192);
        assert!(config.vector_dimension.is_none());
    }
}
