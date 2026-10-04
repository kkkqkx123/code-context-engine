//! Embedding result type (shared by the workspace)
//!
//! The concrete embedder implementation lives in
//! `cce_llm_client::services::embedding::OpenAICompatibleProvider`.

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
