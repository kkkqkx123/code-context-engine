//! Embedding port contract (shared by the workspace)
//!
//! The orchestration layer consumes embedding through the [`Embedder`] port
//! instead of a concrete provider, so the domain layer stays free of
//! infrastructure dependencies. The concrete adapter lives in
//! `cce_infrastructure::llm::services::embedding::OpenAICompatibleProvider`.
//!
//! Unlike the chat port (`crate::llm::LlmClient`, a deterministic generic
//! bound with RPITIT methods), embedding is a cross-cutting dependency
//! consumed by the storage coordinator, the searcher, hot-update processors
//! and summary boosts; parameterizing every consumer would ripple the generic
//! type through the whole orchestration layer. `Embedder` therefore uses
//! `#[async_trait]` (boxed futures, dyn-compatible) and is injected as
//! `Arc<dyn Embedder>`; this usage is recorded in `docs/archive/dynamic.md`.

use crate::error::LlmError;

/// Result of an embedding operation
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct EmbeddingResult {
    /// The generated embeddings
    pub embeddings: Vec<Vec<f32>>,
    /// Number of prompt tokens used
    pub prompt_tokens: u64,
    /// Total number of tokens used
    pub total_tokens: u64,
}

/// Port for embedding capability
///
/// The only production implementation is the infrastructure
/// `OpenAICompatibleProvider`.
#[async_trait::async_trait]
pub trait Embedder: Send + Sync {
    /// Create embeddings for texts (batch API)
    async fn embed(&self, texts: &[&str]) -> Result<EmbeddingResult, LlmError>;

    /// Embed a single text (convenience method)
    async fn embed_one(&self, text: &str) -> Result<Vec<f32>, LlmError>;

    /// Embed texts and return only the dense vectors (convenience method)
    async fn embed_vectors(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, LlmError>;

    /// Get the embedding dimension of this provider
    fn dimension(&self) -> usize;

    /// Get the model name
    fn model_name(&self) -> &str;

    /// Check if the embedding provider is healthy
    fn is_healthy(&self) -> bool;
}
