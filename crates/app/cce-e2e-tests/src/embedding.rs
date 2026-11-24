//! Embedding provider configuration for E2E tests

/// Embedding provider type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbeddingProviderType {
    /// Use local llama.cpp server
    LlamaCppServer,
    /// Use mock provider (returns fixed vectors)
    Mock,
}

/// Embedding service configuration.
///
/// This struct is a plain data container.  Callers must populate every field
/// from their own configuration (environment variables, config files, etc.)
/// — no provider, model, or endpoint is assumed.
#[derive(Debug, Clone)]
pub struct EmbeddingConfig {
    /// Provider type
    pub provider_type: EmbeddingProviderType,
    /// Service endpoint
    pub endpoint: String,
    /// Model name
    pub model: String,
    /// Vector dimension (for mock provider)
    pub dimension: usize,
    /// API key for embedding service
    pub api_key: String,
}

impl EmbeddingConfig {
    /// Create a mock embedding config for testing
    pub fn mock() -> Self {
        Self {
            provider_type: EmbeddingProviderType::Mock,
            endpoint: "http://localhost:8080/mock-embedding".to_string(),
            model: "mock-model-v1".to_string(),
            dimension: 384,
            api_key: "mock-api-key".to_string(),
        }
    }

    /// Check if this is a mock provider
    pub fn is_mock(&self) -> bool {
        self.provider_type == EmbeddingProviderType::Mock
    }

    /// Get vector dimension
    pub fn dimension(&self) -> usize {
        self.dimension
    }
}
