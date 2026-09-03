//! LLM trait abstractions for the code context engine

pub mod config;
pub mod embedding;
pub mod error;
pub mod rerank;
pub mod types;

pub use config::{ChatConfig, ResponseFormat};
pub use embedding::{Embedder, EmbeddingResult};
pub use error::{LlmConfigError, LlmError, LlmRetryErrorClass};
pub use rerank::{RerankProvider, RerankRequest, RerankRuntimeConfig};
pub use types::{ChatResult, Message, MessageRole};

/// Port for LLM chat capability
pub trait LlmClient: Send + Sync {
    fn chat(
        &self,
        messages: &[Message],
        config: &ChatConfig,
    ) -> impl std::future::Future<Output = Result<ChatResult, LlmError>> + Send;
}
