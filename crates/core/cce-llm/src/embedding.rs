//! Embedding result type and provider port (shared by the workspace)
//!
//! The concrete embedder implementation lives in
//! `cce_llm_client::services::embedding`. The port mirrors the
//! [`crate::rerank::RerankProvider`] style: RPITIT futures, no trait objects,
//! so orchestrator generics bind to a deterministic type.

use std::future::Future;

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

/// Port for the LLM embedding capability
///
/// Implementations are the infrastructure adapters in `cce-llm-client`; the
/// orchestrator's cache binds to this trait as a generic bound.
pub trait EmbeddingProvider: Send + Sync {
    /// Embed a batch of texts, returning vectors in input order.
    fn embed(
        &self,
        texts: &[String],
    ) -> impl Future<Output = Result<EmbeddingResult, LlmError>> + Send;

    /// Embed a single text.
    fn embed_one(&self, text: &str) -> impl Future<Output = Result<Vec<f32>, LlmError>> + Send {
        async move {
            let result = self.embed(&[text.to_string()]).await?;
            result
                .embeddings
                .into_iter()
                .next()
                .ok_or_else(|| LlmError::invalid_response("provider returned no embedding"))
        }
    }

    /// Expected vector dimension of this provider.
    fn dimension(&self) -> usize;

    /// Model name served by this provider.
    fn model_name(&self) -> &str;

    /// Whether the provider is currently usable (e.g. its circuit breaker is
    /// closed); a `false` value lets callers fail fast before dispatching.
    fn is_healthy(&self) -> bool;
}
