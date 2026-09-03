//! Rerank Service Module

pub mod handler;
pub mod provider;
pub mod types;

use std::sync::Arc;

use crate::core::error::LlmError;
use cce_llm::RerankProvider;

pub use handler::RerankRequestHandler;
pub use provider::{CohereRerankProvider, GenerativeRerankProvider};
pub use types::{
    RerankCandidate, RerankRequest, RerankResult, RerankRuntimeConfig, RerankedCandidate,
    ScoreFusionStrategy,
};

/// Rerank handler used by the production generative LLM provider.
pub type GenerativeRerankRequestHandler = RerankRequestHandler<GenerativeRerankProvider>;

/// Rerank handler used by the production cross-encoder provider.
pub type CohereRerankRequestHandler = RerankRequestHandler<CohereRerankProvider>;

/// Production rerank handler: either the generative (chat prompt) or the
/// cross-encoder (dedicated `/rerank` endpoint) provider.
#[derive(Clone)]
pub enum ProductionRerankHandler {
    Generative(Arc<GenerativeRerankRequestHandler>),
    CrossEncoder(Arc<CohereRerankRequestHandler>),
}

impl RerankProvider for ProductionRerankHandler {
    async fn rerank(&self, request: &RerankRequest) -> Result<RerankResult, LlmError> {
        match self {
            Self::Generative(handler) => handler.rerank(request).await,
            Self::CrossEncoder(handler) => handler.rerank(request).await,
        }
    }

    fn provider_name(&self) -> &str {
        match self {
            Self::Generative(handler) => handler.provider_name(),
            Self::CrossEncoder(handler) => handler.provider_name(),
        }
    }

    fn is_available(&self) -> bool {
        match self {
            Self::Generative(handler) => handler.is_available(),
            Self::CrossEncoder(handler) => handler.is_available(),
        }
    }
}
