//! LLM Client
//!
//! Provides unified HTTP client infrastructure for LLM APIs including
//! embeddings, chat/completions, and reranking.

pub mod core;
pub mod factory;
pub mod rate_limiter_registry;
pub mod services;

pub use crate::core::{
    client::{HttpLlmClient, HttpLlmClientBuilder},
    config::{ChatConfig, EmbeddingConfig, LlmConfig, ProviderType, ResponseFormat},
    error::{LlmConfigError, LlmError},
    rate_limiter::{RateLimiter, TokenBucket},
    retry::{FixedIntervalPolicy, NoRetry, RetryPolicy},
};

pub use crate::factory::{
    ChatClientHandle, build_chat_client, build_llm_client, build_rerank_client,
};

pub use crate::services::chat::handler::ChatRequestHandler;
pub use crate::services::chat::types::{ChatResult, Message, MessageRole};
pub use crate::services::embedding::handler::EmbeddingRequestHandler;
pub use crate::services::embedding::provider::OpenAICompatibleProvider;
pub use crate::services::rerank::{
    CohereRerankProvider, GenerativeRerankProvider, GenerativeRerankRequestHandler,
    ProductionRerankHandler, RerankCandidate, RerankRequest, RerankResult, RerankRuntimeConfig,
    RerankedCandidate, ScoreFusionStrategy,
};
pub use cce_llm::{Embedder, EmbeddingResult, RerankProvider};
