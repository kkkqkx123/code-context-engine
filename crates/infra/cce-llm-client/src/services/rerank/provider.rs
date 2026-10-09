//! Rerank provider implementations backed by llm-suite.
//!
//! The CCE providers keep the workspace-facing request/response contract
//! and delegate scoring to llm-suite, converting types at the boundary.
//! Each boundary type has a single conversion helper so field mappings stay
//! in one place.

use crate::suite::map_rerank_error;
use cce_config::modules::search::RerankFusionStrategy;
use cce_llm::{LlmError, RerankProvider, RerankRequest, RerankRuntimeConfig};
use cce_types::{RerankCandidate, RerankResult, RerankedCandidate};
use llm_rerank::RerankProvider as SuiteRerankProvider;

fn to_suite_candidate(candidate: &RerankCandidate) -> llm_rerank::RerankCandidate {
    llm_rerank::RerankCandidate {
        id: candidate.id.clone(),
        content: candidate.content.clone(),
        file_path: candidate.file_path.clone(),
        initial_score: candidate.initial_score,
        entity_type: candidate.entity_type.clone(),
        metadata: candidate.metadata.clone(),
    }
}

fn to_suite_fusion(strategy: &RerankFusionStrategy) -> llm_rerank::RerankFusionStrategy {
    match strategy {
        RerankFusionStrategy::RerankOnly => llm_rerank::RerankFusionStrategy::RerankOnly,
        RerankFusionStrategy::LinearWeighted { alpha } => {
            llm_rerank::RerankFusionStrategy::LinearWeighted { alpha: *alpha }
        }
        RerankFusionStrategy::Multiplicative => llm_rerank::RerankFusionStrategy::Multiplicative,
        RerankFusionStrategy::ReciprocalRankFusion { k } => {
            llm_rerank::RerankFusionStrategy::ReciprocalRankFusion { k: *k }
        }
    }
}

fn to_suite_config(config: &RerankRuntimeConfig) -> llm_rerank::RerankRuntimeConfig {
    llm_rerank::RerankRuntimeConfig {
        max_candidates: config.max_candidates,
        temperature: config.temperature,
        return_reasoning: config.return_reasoning,
        score_fusion_strategy: to_suite_fusion(&config.score_fusion_strategy),
        timeout_ms: config.timeout_ms,
    }
}

fn to_suite_request(request: &RerankRequest) -> llm_rerank::RerankRequest {
    llm_rerank::RerankRequest {
        query: request.query.clone(),
        candidates: request.candidates.iter().map(to_suite_candidate).collect(),
        config: to_suite_config(&request.config),
    }
}

fn from_suite_result(result: llm_rerank::RerankResult) -> RerankResult {
    RerankResult {
        reranked_candidates: result
            .reranked_candidates
            .into_iter()
            .map(|candidate| RerankedCandidate {
                id: candidate.id,
                rerank_score: candidate.rerank_score,
                initial_score: candidate.initial_score,
                final_score: candidate.final_score,
                rank_change: candidate.rank_change,
                reasoning: candidate.reasoning,
            })
            .collect(),
        prompt_tokens: result.prompt_tokens,
        total_tokens: result.total_tokens,
        elapsed_ms: result.elapsed_ms,
    }
}

/// Adapter exposing any llm-suite rerank provider through the CCE
/// [`RerankProvider`] port with CCE request/response types.
///
/// Production code instantiates it with the concrete generative or
/// cross-encoder suite providers (see the `GenerativeRerankProvider` and
/// `CohereRerankProvider` aliases); tests inject scripted suite mocks (for
/// example `llm_rerank::mock::MockRerankProvider`) so handler behavior is
/// verified without network stubs.
pub struct DelegatingRerankProvider<P> {
    inner: P,
}

impl<P> DelegatingRerankProvider<P> {
    /// Wraps an already-configured llm-suite rerank provider.
    pub fn new(inner: P) -> Self {
        Self { inner }
    }

    /// Accesses the wrapped suite provider.
    pub fn inner(&self) -> &P {
        &self.inner
    }
}

impl DelegatingRerankProvider<llm_rerank::GenerativeRerankProvider> {
    /// Returns the chat endpoint configuration the provider was built with.
    pub fn endpoint(&self) -> &llm_rerank::GenerativeChatEndpoint {
        self.inner.endpoint()
    }
}

impl DelegatingRerankProvider<llm_rerank::CohereRerankProvider> {
    /// Returns the endpoint configuration the provider was built with.
    pub fn config(&self) -> &llm_rerank::RerankConfig {
        self.inner.config()
    }
}

impl<P: SuiteRerankProvider> RerankProvider for DelegatingRerankProvider<P> {
    async fn rerank(&self, request: &RerankRequest) -> Result<RerankResult, LlmError> {
        self.inner
            .rerank(&to_suite_request(request))
            .await
            .map(from_suite_result)
            .map_err(map_rerank_error)
    }

    fn provider_name(&self) -> &str {
        self.inner.provider_name()
    }

    fn is_available(&self) -> bool {
        self.inner.is_available()
    }
}

/// Production generative LLM reranking provider.
pub type GenerativeRerankProvider = DelegatingRerankProvider<llm_rerank::GenerativeRerankProvider>;
/// Production cross-encoder rerank provider using a dedicated endpoint.
pub type CohereRerankProvider = DelegatingRerankProvider<llm_rerank::CohereRerankProvider>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn test_request() -> RerankRequest {
        RerankRequest {
            query: "how to start the app".to_string(),
            candidates: vec![
                RerankCandidate {
                    id: "c1".to_string(),
                    content: "fn main() {}".to_string(),
                    file_path: "src/main.rs".to_string(),
                    initial_score: 0.7,
                    entity_type: Some("function".to_string()),
                    metadata: HashMap::new(),
                },
                RerankCandidate {
                    id: "c2".to_string(),
                    content: "pub fn start() {}".to_string(),
                    file_path: "src/app.rs".to_string(),
                    initial_score: 0.5,
                    entity_type: Some("function".to_string()),
                    metadata: HashMap::new(),
                },
            ],
            config: RerankRuntimeConfig::default(),
        }
    }

    #[test]
    fn request_conversion_preserves_candidates_and_config() {
        let converted = to_suite_request(&test_request());
        assert_eq!(converted.query, "how to start the app");
        assert_eq!(converted.candidates.len(), 2);
        assert_eq!(converted.candidates[0].id, "c1");
        assert_eq!(converted.candidates[1].file_path, "src/app.rs");
        assert_eq!(converted.config.max_candidates, 50);
    }

    #[test]
    fn fusion_conversion_covers_all_strategies() {
        let cases = [
            RerankFusionStrategy::RerankOnly,
            RerankFusionStrategy::LinearWeighted { alpha: 0.7 },
            RerankFusionStrategy::Multiplicative,
            RerankFusionStrategy::ReciprocalRankFusion { k: 60.0 },
        ];
        for strategy in &cases {
            let converted = to_suite_fusion(strategy);
            assert_eq!(
                std::mem::discriminant(&converted),
                std::mem::discriminant(&match strategy {
                    RerankFusionStrategy::RerankOnly =>
                        llm_rerank::RerankFusionStrategy::RerankOnly,
                    RerankFusionStrategy::LinearWeighted { .. } =>
                        llm_rerank::RerankFusionStrategy::LinearWeighted { alpha: 0.0 },
                    RerankFusionStrategy::Multiplicative =>
                        llm_rerank::RerankFusionStrategy::Multiplicative,
                    RerankFusionStrategy::ReciprocalRankFusion { .. } =>
                        llm_rerank::RerankFusionStrategy::ReciprocalRankFusion { k: 0.0 },
                })
            );
        }
    }

    #[test]
    fn result_conversion_preserves_scores_and_ranks() {
        let converted = from_suite_result(llm_rerank::RerankResult {
            reranked_candidates: vec![llm_rerank::RerankedCandidate {
                id: "c2".to_string(),
                rerank_score: 0.9,
                initial_score: 0.5,
                final_score: 0.8,
                rank_change: 1,
                reasoning: Some("relevant".to_string()),
            }],
            prompt_tokens: 10,
            total_tokens: 15,
            elapsed_ms: 7,
        });
        assert_eq!(converted.reranked_candidates.len(), 1);
        let candidate = &converted.reranked_candidates[0];
        assert_eq!(candidate.id, "c2");
        assert!((candidate.rerank_score - 0.9).abs() < f32::EPSILON);
        assert_eq!(candidate.rank_change, 1);
        assert_eq!(candidate.reasoning.as_deref(), Some("relevant"));
        assert_eq!(converted.prompt_tokens, 10);
        assert_eq!(converted.total_tokens, 15);
        assert_eq!(converted.elapsed_ms, 7);
    }
}
