//! LLM Client
//!
//! llm-suite-backed embedding, chat and rerank providers plus the factory
//! assembling them from the global config.

pub mod core;
pub mod factory;
pub mod services;
pub mod suite;

pub use crate::core::{
    config::{ChatConfig, EmbeddingConfig, ResponseFormat},
    error::{LlmConfigError, LlmError},
};

pub use crate::factory::{
    ChatClientHandle, build_chat_client, build_cohere_rerank_provider,
    build_generative_rerank_provider,
};

pub use crate::services::chat::handler::ChatRequestHandler;
pub use crate::services::chat::types::{ChatResult, Message, MessageRole};
pub use crate::services::embedding::handler::{EmbeddingRequestHandler, SuiteEmbeddingTransport};
pub use crate::services::embedding::provider::OpenAICompatibleProvider;
pub use crate::services::rerank::{
    CohereRerankProvider, DelegatingRerankProvider, GenerativeRerankProvider,
    GenerativeRerankRequestHandler, ProductionRerankHandler, RerankCandidate, RerankFusionStrategy,
    RerankRequest, RerankResult, RerankRuntimeConfig, RerankedCandidate,
};
pub use cce_llm::{EmbeddingResult, RerankProvider};
pub use crate::suite::{GatewayMetricsSink, SuiteChatClient, init_global_token_metrics};
