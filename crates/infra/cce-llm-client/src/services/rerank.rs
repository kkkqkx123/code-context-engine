//! Rerank Service Module

pub mod handler;
pub mod provider;

use std::sync::Arc;

use cce_llm::{LlmError, RerankProvider};

pub use cce_config::modules::search::RerankFusionStrategy;
pub use cce_llm::{RerankRequest, RerankRuntimeConfig};
pub use cce_types::{RerankCandidate, RerankResult, RerankedCandidate};
pub use handler::RerankRequestHandler;
pub use provider::{CohereRerankProvider, DelegatingRerankProvider, GenerativeRerankProvider};

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
