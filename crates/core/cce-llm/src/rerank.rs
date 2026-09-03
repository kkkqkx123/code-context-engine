//! Rerank port contract (shared by the workspace)
//!
//! The query reranker consumes reranking through the [`RerankProvider`] port
//! instead of a concrete provider, so the domain layer stays free of
//! infrastructure dependencies. The concrete adapters live in
//! `cce_infrastructure::llm::services::rerank`. `RerankProvider` is used as a
//! deterministic trait bound (RPITIT, no trait object), matching the
//! [`crate::llm::LlmClient`] chat port style.
//!
//! The cross-layer candidate/result contract (`RerankCandidate`,
//! `RerankedCandidate`, [`RerankResult`]) lives in
//! `crate::types::ast_to_nl::rerank` (referenced by the plugin rerank
//! capability) and is re-exported here.

use std::future::Future;

use crate::error::LlmError;
use cce_config::modules::search::ScoreFusionStrategy;
pub use cce_types::ast_to_nl::{RerankCandidate, RerankResult, RerankedCandidate};

/// Port for LLM rerank capability
///
/// The only production implementations are the infrastructure providers
/// (`GenerativeRerankProvider` / `CohereRerankProvider`). The method returns
/// `impl Future + Send` (RPITIT) so no trait object or `async-trait` macro is
/// needed.
pub trait RerankProvider: Send + Sync {
    /// Execute a rerank call
    fn rerank(
        &self,
        request: &RerankRequest,
    ) -> impl Future<Output = Result<RerankResult, LlmError>> + Send;

    /// Provider name for logging and metrics
    fn provider_name(&self) -> &str;

    /// Whether the provider is available (e.g. underlying client healthy)
    fn is_available(&self) -> bool;
}

/// Rerank request
#[derive(Debug, Clone)]
pub struct RerankRequest {
    /// Original query text
    pub query: String,
    /// List of candidate results to be rearranged
    pub candidates: Vec<RerankCandidate>,
    /// Per-request rerank parameters
    pub config: RerankRuntimeConfig,
}

/// Per-request runtime parameters for a rerank call.
///
/// Distinct from `crate::config::modules::rerank::RerankConfig` (the `[rerank]`
/// TOML section): this type carries the parameters of a single call.
#[derive(Debug, Clone)]
pub struct RerankRuntimeConfig {
    /// Maximum number of rearrangement candidates (to avoid too many calls to LLM)
    pub max_candidates: usize,
    /// Temperature parameters
    pub temperature: f32,
    /// Whether to return the reason for the rearrangement
    pub return_reasoning: bool,
    /// Score integration strategy
    pub score_fusion_strategy: ScoreFusionStrategy,
    /// Timeout time (milliseconds)
    pub timeout_ms: u64,
}

impl Default for RerankRuntimeConfig {
    fn default() -> Self {
        Self {
            max_candidates: 50,
            temperature: 0.0,
            return_reasoning: false,
            score_fusion_strategy: ScoreFusionStrategy::LinearWeighted { alpha: 0.7 },
            timeout_ms: 5000,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rerank_runtime_config_default() {
        let config = RerankRuntimeConfig::default();
        assert_eq!(config.max_candidates, 50);
        assert_eq!(config.temperature, 0.0);
        assert!(!config.return_reasoning);
        assert_eq!(config.timeout_ms, 5000);
    }
}
