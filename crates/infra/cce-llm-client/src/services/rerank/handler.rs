//! Rearranging request processors

use crate::core::error::LlmError;
use crate::services::rerank::types::{RerankRequest, RerankResult};
use cce_llm::RerankProvider;
use cce_metrics::RerankMetrics;
use std::sync::Arc;

/// Rearrangement of request processors
pub struct RerankRequestHandler<P> {
    /// rescheduling provider
    provider: Arc<P>,
    /// Rerank metrics (optional)
    metrics: Option<Arc<RerankMetrics>>,
}

impl<P: RerankProvider> RerankRequestHandler<P> {
    pub fn new(provider: Arc<P>) -> Self {
        Self {
            provider,
            metrics: None,
        }
    }

    /// Attach rerank metrics
    pub fn with_rerank_metrics(mut self, metrics: Arc<RerankMetrics>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Get the provider name
    pub fn provider_name(&self) -> &str {
        self.provider.provider_name()
    }

    /// Check whether the underlying provider is available
    pub fn is_available(&self) -> bool {
        self.provider.is_available()
    }

    /// executable reordering
    pub async fn rerank(&self, request: &RerankRequest) -> Result<RerankResult, LlmError> {
        self.validate_basic(request)?;

        let limited_request = self.limit_candidates(request);

        if limited_request.candidates.is_empty() {
            return Err(LlmError::invalid_input(
                "No valid candidates after filtering".to_string(),
            ));
        }

        tracing::trace!(
            query_length = limited_request.query.len(),
            candidate_count = limited_request.candidates.len(),
            timeout_ms = limited_request.config.timeout_ms,
            "Starting reranking"
        );

        let start = std::time::Instant::now();

        let timeout_duration = std::time::Duration::from_millis(limited_request.config.timeout_ms);
        let outcome =
            tokio::time::timeout(timeout_duration, self.provider.rerank(&limited_request)).await;

        let elapsed = start.elapsed();
        let elapsed_ms = elapsed.as_secs_f64() * 1000.0;
        let candidate_count = limited_request.candidates.len();

        match outcome {
            Ok(Ok(result)) => {
                tracing::trace!(
                    elapsed_ms = elapsed.as_millis(),
                    candidates_processed = result.reranked_candidates.len(),
                    prompt_tokens = result.prompt_tokens,
                    total_tokens = result.total_tokens,
                    "Reranking completed"
                );

                if let Some(metrics) = &self.metrics {
                    metrics.record_request(elapsed_ms, candidate_count, true);
                }

                Ok(result)
            }
            Ok(Err(e)) => {
                if let Some(metrics) = &self.metrics {
                    metrics.record_request(elapsed_ms, candidate_count, false);
                }
                Err(e)
            }
            Err(_) => {
                tracing::warn!(
                    elapsed_ms = elapsed.as_millis(),
                    timeout_ms = limited_request.config.timeout_ms,
                    "Reranking timed out"
                );

                if let Some(metrics) = &self.metrics {
                    metrics.record_request(elapsed_ms, candidate_count, false);
                }

                Err(LlmError::Timeout(
                    cce_types::error::common::TimeoutError::new(format!(
                        "Rerank timeout after {}ms",
                        limited_request.config.timeout_ms
                    )),
                ))
            }
        }
    }

    /// Basic validation (excluding number of candidates)
    fn validate_basic(&self, request: &RerankRequest) -> Result<(), LlmError> {
        if request.query.is_empty() {
            return Err(LlmError::invalid_input("Query cannot be empty".to_string()));
        }

        Ok(())
    }

    /// Limiting the number of candidates
    fn limit_candidates(&self, request: &RerankRequest) -> RerankRequest {
        if request.candidates.len() <= request.config.max_candidates {
            return request.clone();
        }

        let mut sorted_candidates = request.candidates.clone();
        sorted_candidates.sort_by(|a, b| {
            b.initial_score
                .partial_cmp(&a.initial_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let limited_candidates = sorted_candidates
            .into_iter()
            .take(request.config.max_candidates)
            .collect();

        RerankRequest {
            query: request.query.clone(),
            candidates: limited_candidates,
            config: request.config.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::rerank::types::{RerankCandidate, RerankRuntimeConfig};
    use std::collections::HashMap;

    struct MockProvider;

    impl RerankProvider for MockProvider {
        async fn rerank(&self, request: &RerankRequest) -> Result<RerankResult, LlmError> {
            let reranked = request
                .candidates
                .iter()
                .enumerate()
                .map(|(i, c)| crate::services::rerank::types::RerankedCandidate {
                    id: c.id.clone(),
                    rerank_score: 1.0 - (i as f32 * 0.1),
                    initial_score: c.initial_score,
                    final_score: c.initial_score,
                    rank_change: 0,
                    reasoning: None,
                })
                .collect();

            Ok(RerankResult {
                reranked_candidates: reranked,
                prompt_tokens: 100,
                total_tokens: 150,
                elapsed_ms: 50,
            })
        }

        fn provider_name(&self) -> &str {
            "mock"
        }

        fn is_available(&self) -> bool {
            true
        }
    }

    fn create_test_candidate(id: &str, score: f32) -> RerankCandidate {
        RerankCandidate {
            id: id.to_string(),
            content: format!("content for {}", id),
            file_path: format!("file_{}.rs", id),
            initial_score: score,
            entity_type: Some("function".to_string()),
            metadata: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn test_validate_request_empty_query() {
        let provider = Arc::new(MockProvider);
        let handler = RerankRequestHandler::new(provider);

        let request = RerankRequest {
            query: "".to_string(),
            candidates: vec![create_test_candidate("1", 0.9)],
            config: RerankRuntimeConfig::default(),
        };

        let result = handler.rerank(&request).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_rerank_success() {
        let provider = Arc::new(MockProvider);
        let handler = RerankRequestHandler::new(provider);

        let candidates = vec![
            create_test_candidate("1", 0.9),
            create_test_candidate("2", 0.8),
        ];

        let request = RerankRequest {
            query: "test query".to_string(),
            candidates,
            config: RerankRuntimeConfig::default(),
        };

        let result = handler.rerank(&request).await;
        assert!(result.is_ok());

        let rerank_result = result.expect("reranking should succeed");
        assert_eq!(rerank_result.reranked_candidates.len(), 2);
        assert!(rerank_result.elapsed_ms > 0);
    }
}
