//! LLM Client
//!
//! llm-suite-backed embedding, chat and rerank providers plus the factory
//! assembling them from the global config.

pub mod config;
pub mod factory;
pub mod services;
pub mod suite;

pub use crate::config::EmbeddingConfig;

pub use crate::factory::{
    ChatClientHandle, build_chat_client, build_cohere_rerank_provider,
    build_generative_rerank_provider,
};

pub use crate::services::embedding::provider::OpenAICompatibleProvider;
pub use crate::services::rerank::{
    CohereRerankProvider, DelegatingRerankProvider, GenerativeRerankProvider,
    GenerativeRerankRequestHandler, ProductionRerankHandler, RerankCandidate, RerankFusionStrategy,
    RerankRequest, RerankResult, RerankRuntimeConfig, RerankedCandidate,
};
pub use crate::suite::{
    GatewayMetricsSink, SuiteChatClient, init_gateway, init_global_token_metrics,
};
pub use cce_llm::{
    ChatConfig, ChatResult, EmbeddingResult, LlmConfigError, LlmError, Message, MessageRole,
    RerankProvider, ResponseFormat,
};
