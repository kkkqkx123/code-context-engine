//! Rerank provider implementations

use crate::core::client::HttpLlmClient;
use crate::core::config::ChatConfig;
use crate::core::error::LlmError;
use crate::services::chat::handler::ChatRequestHandler;
use crate::services::chat::types::Message;
use crate::services::rerank::types::{RerankRequest, RerankResult, RerankedCandidate};
use cce_config::modules::ServiceType;
use cce_llm::RerankProvider;
use serde::Deserialize;
use std::collections::HashSet;
use std::sync::Arc;

/// Generative LLM reranking provider.
pub struct GenerativeRerankProvider {
    client: Arc<HttpLlmClient>,
    chat_handler: ChatRequestHandler,
    model_name: String,
}

impl GenerativeRerankProvider {
    pub fn new(client: Arc<HttpLlmClient>, model_name: String) -> Self {
        let chat_handler = ChatRequestHandler::new(Arc::clone(&client));
        Self {
            client,
            chat_handler,
            model_name,
        }
    }

    /// Constructing the cross-coder prompt
    fn build_cross_encoder_prompt(&self, request: &RerankRequest) -> String {
        let query = &request.query;
        let candidates = &request.candidates;

        let mut prompt = format!(
            "You are a code search relevance evaluator. Given a query and multiple code snippets, \
             evaluate the relevance of each snippet to the query on a scale of 0.0 to 1.0.\n\n\
             Query: {}\n\n\
             Code Snippets:\n",
            query
        );

        for (i, candidate) in candidates.iter().enumerate() {
            prompt.push_str(&format!(
                "[{}] ID: {}\nFile: {}\nType: {}\nContent:\n{}\n\n",
                i,
                candidate.id,
                candidate.file_path,
                candidate.entity_type.as_deref().unwrap_or("unknown"),
                truncate_content(&candidate.content, 500)
            ));
        }

        prompt.push_str(
            "Please output a JSON array with the following structure for each candidate:\n\
             [{\"id\": \"...\", \"score\": 0.0-1.0, \"reasoning\": \"...\"}]\n\
             Sort by score in descending order.",
        );

        prompt
    }

    /// Parse Rearrangement Response
    fn parse_rerank_response(
        &self,
        response: &str,
        request: &RerankRequest,
    ) -> Result<Vec<RerankedCandidate>, LlmError> {
        let json_str = extract_json_from_response(response);

        let parsed: Vec<RerankResponseItem> = serde_json::from_str(&json_str).map_err(|e| {
            let preview: String = response.chars().take(200).collect();
            LlmError::invalid_response(format!(
                "Failed to parse rerank response: {e}. Response preview: {preview}"
            ))
        })?;

        if parsed.len() != request.candidates.len() {
            return Err(LlmError::invalid_response(format!(
                "Rerank response count mismatch: expected {}, received {}",
                request.candidates.len(),
                parsed.len()
            )));
        }

        let mut seen_ids = HashSet::with_capacity(parsed.len());
        let mut reranked = Vec::with_capacity(parsed.len());
        for (new_rank, item) in parsed.iter().enumerate() {
            if !seen_ids.insert(item.id.as_str()) {
                return Err(LlmError::invalid_response(format!(
                    "Rerank response contains duplicate candidate ID '{}'",
                    item.id
                )));
            }
            if !item.score.is_finite() || !(0.0..=1.0).contains(&item.score) {
                return Err(LlmError::invalid_response(format!(
                    "Rerank score for candidate '{}' must be finite and between 0 and 1",
                    item.id
                )));
            }

            let candidate = request
                .candidates
                .iter()
                .find(|candidate| candidate.id == item.id)
                .ok_or_else(|| {
                    LlmError::invalid_response(format!(
                        "Rerank response contains unknown candidate ID '{}'",
                        item.id
                    ))
                })?;

            reranked.push(RerankedCandidate {
                id: item.id.clone(),
                rerank_score: item.score,
                initial_score: candidate.initial_score,
                final_score: request.config.score_fusion_strategy.calculate(
                    item.score,
                    candidate.initial_score,
                    new_rank,
                ),
                rank_change: 0,
                reasoning: request
                    .config
                    .return_reasoning
                    .then(|| item.reasoning.clone()),
            });
        }

        reranked.sort_by(|a, b| {
            b.final_score
                .partial_cmp(&a.final_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for (new_rank, candidate) in reranked.iter_mut().enumerate() {
            let initial_rank = request
                .candidates
                .iter()
                .position(|item| item.id == candidate.id)
                .ok_or_else(|| LlmError::internal("Validated rerank candidate disappeared"))?;
            candidate.rank_change = initial_rank as i32 - new_rank as i32;
        }

        Ok(reranked)
    }
}

impl RerankProvider for GenerativeRerankProvider {
    async fn rerank(&self, request: &RerankRequest) -> Result<RerankResult, LlmError> {
        let prompt = self.build_cross_encoder_prompt(request);

        let messages = vec![Message::user(prompt)];
        let chat_config = ChatConfig {
            model: self.model_name.clone(),
            temperature: request.config.temperature,
            max_tokens: 2000,
            ..Default::default()
        };

        let start = std::time::Instant::now();
        let result = self.chat_handler.chat(&messages, &chat_config).await?;
        let elapsed_ms = start.elapsed().as_millis() as u64;

        let reranked_candidates = self.parse_rerank_response(&result.content, request)?;

        Ok(RerankResult {
            reranked_candidates,
            prompt_tokens: result.prompt_tokens,
            total_tokens: result.total_tokens,
            elapsed_ms,
        })
    }

    fn provider_name(&self) -> &str {
        "generative-llm"
    }

    fn is_available(&self) -> bool {
        self.client.is_healthy()
    }
}

#[derive(Debug, Deserialize)]
struct RerankResponseItem {
    id: String,
    score: f32,
    #[serde(default)]
    reasoning: String,
}

/// Truncate content to avoid exceeding token limits
pub fn truncate_content(content: &str, max_chars: usize) -> String {
    if content.chars().count() <= max_chars {
        content.to_string()
    } else {
        let truncated: String = content.chars().take(max_chars).collect();
        format!("{truncated}...")
    }
}

/// Extracting the JSON part from the response
fn extract_json_from_response(response: &str) -> String {
    if let Some(start) = response.find('[') {
        if let Some(end) = response.rfind(']') {
            return response[start..=end].to_string();
        }
    }
    response.to_string()
}

/// Cross-encoder rerank provider using a dedicated `/rerank` endpoint.
pub struct CohereRerankProvider {
    client: Arc<HttpLlmClient>,
    model_name: String,
}

impl CohereRerankProvider {
    pub fn new(client: Arc<HttpLlmClient>, model_name: String) -> Self {
        Self { client, model_name }
    }

    /// Build the Cohere-compatible rerank request body.
    fn build_request_body(&self, request: &RerankRequest) -> serde_json::Value {
        let documents: Vec<serde_json::Value> = request
            .candidates
            .iter()
            .map(|candidate| {
                serde_json::json!({
                    "id": candidate.id,
                    "text": truncate_content(&candidate.content, 500),
                })
            })
            .collect();

        serde_json::json!({
            "model": self.model_name,
            "query": request.query,
            "documents": documents,
            "top_n": request.candidates.len(),
            "return_documents": false,
        })
    }

    /// Parse a Cohere-compatible rerank response.
    fn parse_rerank_response(
        &self,
        response: &str,
        request: &RerankRequest,
    ) -> Result<Vec<RerankedCandidate>, LlmError> {
        #[derive(Debug, Deserialize)]
        struct CohereRerankResponse {
            results: Vec<CohereRerankResult>,
        }

        #[derive(Debug, Deserialize)]
        struct CohereRerankResult {
            index: usize,
            relevance_score: f32,
        }

        let parsed: CohereRerankResponse = serde_json::from_str(response).map_err(|e| {
            let preview: String = response.chars().take(200).collect();
            LlmError::invalid_response(format!(
                "Failed to parse rerank response: {e}. Response preview: {preview}"
            ))
        })?;

        if parsed.results.len() != request.candidates.len() {
            return Err(LlmError::invalid_response(format!(
                "Rerank response count mismatch: expected {}, received {}",
                request.candidates.len(),
                parsed.results.len()
            )));
        }

        let mut seen_indexes = HashSet::with_capacity(parsed.results.len());
        let mut reranked = Vec::with_capacity(parsed.results.len());
        for (new_rank, item) in parsed.results.iter().enumerate() {
            if !seen_indexes.insert(item.index) {
                return Err(LlmError::invalid_response(format!(
                    "Rerank response contains duplicate candidate index '{}'",
                    item.index
                )));
            }
            if !item.relevance_score.is_finite() || !(0.0..=1.0).contains(&item.relevance_score) {
                return Err(LlmError::invalid_response(format!(
                    "Rerank score for candidate index '{}' must be finite and between 0 and 1",
                    item.index
                )));
            }

            let candidate = request.candidates.get(item.index).ok_or_else(|| {
                LlmError::invalid_response(format!(
                    "Rerank response contains out-of-range candidate index '{}'",
                    item.index
                ))
            })?;

            reranked.push(RerankedCandidate {
                id: candidate.id.clone(),
                rerank_score: item.relevance_score,
                initial_score: candidate.initial_score,
                final_score: request.config.score_fusion_strategy.calculate(
                    item.relevance_score,
                    candidate.initial_score,
                    new_rank,
                ),
                rank_change: 0,
                reasoning: request
                    .config
                    .return_reasoning
                    .then(|| format!("cross-encoder relevance score {:.3}", item.relevance_score)),
            });
        }

        reranked.sort_by(|a, b| {
            b.final_score
                .partial_cmp(&a.final_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for (new_rank, candidate) in reranked.iter_mut().enumerate() {
            let initial_rank = request
                .candidates
                .iter()
                .position(|item| item.id == candidate.id)
                .ok_or_else(|| LlmError::internal("Validated rerank candidate disappeared"))?;
            candidate.rank_change = initial_rank as i32 - new_rank as i32;
        }

        Ok(reranked)
    }
}

impl RerankProvider for CohereRerankProvider {
    async fn rerank(&self, request: &RerankRequest) -> Result<RerankResult, LlmError> {
        let body = self.build_request_body(request);

        let start = std::time::Instant::now();
        let response = self
            .client
            .request_raw(&self.client.endpoint_path(ServiceType::Rerank), &body)
            .await?;
        let elapsed_ms = start.elapsed().as_millis() as u64;

        let total_tokens = serde_json::from_str::<serde_json::Value>(&response)
            .ok()
            .and_then(|value| value.get("usage").cloned())
            .and_then(|usage| usage.get("total_tokens").cloned())
            .and_then(|value| value.as_u64())
            .unwrap_or(0);

        let reranked_candidates = self.parse_rerank_response(&response, request)?;

        Ok(RerankResult {
            reranked_candidates,
            prompt_tokens: 0,
            total_tokens,
            elapsed_ms,
        })
    }

    fn provider_name(&self) -> &str {
        "cross-encoder"
    }

    fn is_available(&self) -> bool {
        self.client.is_healthy()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::rerank::types::RerankCandidate;

    #[test]
    fn test_truncate_content_short() {
        let short = "short text";
        assert_eq!(truncate_content(short, 100), short);
    }

    #[test]
    fn test_truncate_content_long() {
        let long = "a".repeat(1000);
        let truncated = truncate_content(&long, 100);
        assert_eq!(truncated.len(), 103);
        assert!(truncated.ends_with("..."));
    }

    #[test]
    fn test_truncate_content_preserves_utf8_boundaries() {
        let content = "中".repeat(600);
        let truncated = truncate_content(&content, 500);
        assert_eq!(truncated.chars().count(), 503);
        assert!(truncated.ends_with("..."));
    }

    #[test]
    fn test_extract_json_complete() {
        let response = r#"[{"id": "1", "score": 0.9}]"#;
        let extracted = extract_json_from_response(response);
        assert_eq!(extracted, response);
    }

    #[test]
    fn test_extract_json_with_prefix() {
        let response = r#"Here is the result: [{"id": "1", "score": 0.9}]"#;
        let extracted = extract_json_from_response(response);
        assert_eq!(extracted, r#"[{"id": "1", "score": 0.9}]"#);
    }

    #[test]
    fn test_extract_json_no_array() {
        let response = "No JSON here";
        let extracted = extract_json_from_response(response);
        assert_eq!(extracted, response);
    }

    fn test_request() -> RerankRequest {
        use std::collections::HashMap;
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
            config: crate::services::rerank::types::RerankRuntimeConfig::default(),
        }
    }

    fn test_provider() -> CohereRerankProvider {
        let config = crate::core::config::LlmConfig::openai("sk-test".to_string());
        let client = Arc::new(HttpLlmClient::new(config).expect("client must build"));
        CohereRerankProvider::new(client, "BAAI/bge-reranker-v2-m3".to_string())
    }

    #[test]
    fn test_build_request_body_shape() {
        let body = test_provider().build_request_body(&test_request());
        assert_eq!(body["model"], "BAAI/bge-reranker-v2-m3");
        assert_eq!(body["query"], "how to start the app");
        assert_eq!(body["top_n"], 2);
        let documents = body["documents"].as_array().expect("documents array");
        assert_eq!(documents.len(), 2);
        assert_eq!(documents[0]["id"], "c1");
        assert!(documents[0]["text"].as_str().unwrap().contains("fn main()"));
    }

    #[test]
    fn test_parse_rerank_response_success() {
        let response =
            r#"{"results":[{"index":1,"relevance_score":0.9},{"index":0,"relevance_score":0.6}]}"#;
        let reranked = test_provider()
            .parse_rerank_response(response, &test_request())
            .expect("parse must succeed");

        assert_eq!(reranked.len(), 2);
        assert_eq!(reranked[0].id, "c2");
        assert!((reranked[0].rerank_score - 0.9).abs() < f32::EPSILON);
        assert_eq!(reranked[0].rank_change, 1);
        assert_eq!(reranked[1].id, "c1");
        assert_eq!(reranked[1].rank_change, -1);
    }

    #[test]
    fn test_parse_rerank_response_count_mismatch() {
        let response = r#"{"results":[{"index":0,"relevance_score":0.9}]}"#;
        let result = test_provider().parse_rerank_response(response, &test_request());
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_rerank_response_out_of_range_index() {
        let response =
            r#"{"results":[{"index":0,"relevance_score":0.9},{"index":5,"relevance_score":0.8}]}"#;
        let result = test_provider().parse_rerank_response(response, &test_request());
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_rerank_response_duplicate_index() {
        let response =
            r#"{"results":[{"index":0,"relevance_score":0.9},{"index":0,"relevance_score":0.8}]}"#;
        let result = test_provider().parse_rerank_response(response, &test_request());
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_rerank_response_invalid_score() {
        let response =
            r#"{"results":[{"index":0,"relevance_score":2.5},{"index":1,"relevance_score":0.8}]}"#;
        let result = test_provider().parse_rerank_response(response, &test_request());
        assert!(result.is_err());
    }
}
