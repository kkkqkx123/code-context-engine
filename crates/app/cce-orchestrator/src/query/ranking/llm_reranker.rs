//! LLM-based reranker for search results
//!
//! Provides optional reranking of search results using a language model.
//! Whether reranking actually runs is decided by handler availability (the
//! config layer builds the handler only when `[rerank] enabled = true`); the
//! per-request `QueryOptions::enable_rerank` override can force it on/off.

use super::common::{build_candidates, merge_rerank_results, select_candidate_count};
use crate::query::error::Result;
use crate::query::types::{SearchConfig, SearchResult};
use cce_llm_client::{
    ProductionRerankHandler, RerankCandidate, RerankProvider, RerankRequest, RerankRuntimeConfig,
};

/// LLM reranker that applies LLM-based reranking to search results
#[derive(Clone)]
pub struct LlmReranker {
    handler: Option<std::sync::Arc<ProductionRerankHandler>>,
}

impl LlmReranker {
    /// Create a new reranker with optional handler
    pub fn new(handler: Option<std::sync::Arc<ProductionRerankHandler>>) -> Self {
        Self { handler }
    }

    /// Apply reranking to search results
    ///
    /// If the handler is not available, returns original results.
    pub async fn rerank(
        &self,
        results: Vec<SearchResult>,
        query: &str,
        config: &SearchConfig,
    ) -> Result<Vec<SearchResult>> {
        // Check if handler is available
        let handler = match &self.handler {
            Some(h) => h,
            None => return Ok(results),
        };

        if results.is_empty() {
            return Ok(results);
        }

        // Skip reranking when the underlying provider is unhealthy (e.g.
        // consecutive failures tripped the circuit breaker).
        if !handler.is_available() {
            tracing::debug!(
                provider = handler.provider_name(),
                "Rerank provider unavailable, keeping original order"
            );
            return Ok(results);
        }

        // Dynamically select candidate count based on score distribution
        let candidate_count = select_candidate_count(
            &results,
            config.rerank.max_candidates,
            config.rerank.min_candidates,
            config.rerank.score_drop_threshold,
            config.rerank.min_score,
            config.rerank.drop_detection_start,
        );

        if candidate_count == 0 {
            tracing::trace!("No candidates meet the minimum score threshold for reranking");
            return Ok(results);
        }

        // Take only the selected candidates
        let candidates: Vec<RerankCandidate> = build_candidates(&results, candidate_count);

        // Build rerank config
        let rerank_config = RerankRuntimeConfig {
            max_candidates: candidate_count,
            temperature: config.rerank.temperature,
            return_reasoning: config.rerank.return_reasoning,
            score_fusion_strategy: config.rerank.score_fusion_strategy,
            timeout_ms: config.rerank.timeout_ms,
        };

        // Build rerank request
        let request = RerankRequest {
            query: query.to_string(),
            candidates,
            config: rerank_config,
        };

        // Execute reranking with fallback on error
        match handler.rerank(&request).await {
            Ok(rerank_result) => {
                let elapsed_ms = rerank_result.elapsed_ms;

                // Merge rerank results back to SearchResult
                let reranked_results = merge_rerank_results(results, rerank_result);

                tracing::trace!(
                    "Reranking completed: {} candidates (dynamic selection) in {}ms",
                    reranked_results.len(),
                    elapsed_ms
                );

                Ok(reranked_results)
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "Reranking failed, falling back to original results"
                );
                Ok(results)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rerank_without_handler_keeps_original() {
        let reranker = LlmReranker::new(None);
        let results = vec![SearchResult {
            id: "1".to_string(),
            score: 0.9,
            ..Default::default()
        }];

        let out = reranker
            .rerank(results, "query", &SearchConfig::default())
            .await
            .expect("rerank must succeed");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].id, "1");
        assert!(!out[0].metadata.contains_key("rerank_score"));
    }

    #[tokio::test]
    async fn test_rerank_empty_results_passthrough() {
        let reranker = LlmReranker::new(None);
        let out = reranker
            .rerank(vec![], "query", &SearchConfig::default())
            .await
            .expect("empty rerank must succeed");
        assert!(out.is_empty());
    }
}
