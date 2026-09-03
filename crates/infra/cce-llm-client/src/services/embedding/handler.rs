//! Embedding Request Handler

use crate::core::client::HttpLlmClient;
use crate::core::config::EmbeddingConfig;
use crate::core::error::LlmError;
use crate::services::embedding::response_parser::StandardEmbeddingData;
use crate::services::embedding::types::EmbeddingResult;
use crate::services::request_builder::RequestBuilder;
use cce_config::modules::ServiceType;
use cce_utils::token_estimation::estimate_tokens;
use std::sync::Arc;

/// Embedding Request Handler - handles batching and request orchestration
pub struct EmbeddingRequestHandler {
    /// Underlying HTTP client
    inner: Arc<HttpLlmClient>,
}

impl EmbeddingRequestHandler {
    /// Create a new request handler
    pub fn new(client: Arc<HttpLlmClient>) -> Self {
        Self { inner: client }
    }

    /// Generate embeddings with batching
    pub async fn embed(
        &self,
        texts: &[&str],
        config: &EmbeddingConfig,
    ) -> Result<EmbeddingResult, LlmError> {
        if texts.is_empty() {
            return Ok(EmbeddingResult::default());
        }

        let batches = self.create_batches(texts, config)?;

        let mut all_embeddings = Vec::new();
        let mut total_prompt_tokens = 0u64;
        let mut total_tokens = 0u64;

        for batch in batches {
            let result = self.embed_batch(&batch, config).await?;
            all_embeddings.extend(result.embeddings);
            total_prompt_tokens += result.prompt_tokens;
            total_tokens += result.total_tokens;
        }

        Ok(EmbeddingResult {
            embeddings: all_embeddings,
            prompt_tokens: total_prompt_tokens,
            total_tokens,
        })
    }

    /// Create batches based on token limits.
    fn create_batches<'a>(
        &self,
        texts: &[&'a str],
        config: &EmbeddingConfig,
    ) -> Result<Vec<Vec<&'a str>>, LlmError> {
        if config.max_batch_tokens == 0 {
            return Err(LlmError::invalid_input(
                "max_batch_tokens must be greater than zero",
            ));
        }
        let mut batches: Vec<Vec<&str>> = Vec::new();
        let mut current_batch: Vec<&str> = Vec::new();
        let mut current_tokens = 0usize;

        for text in texts {
            let tokens = estimate_tokens(text);

            if tokens > config.max_item_tokens && config.max_item_tokens > 0 {
                return Err(LlmError::token_limit_exceeded(
                    tokens,
                    config.max_item_tokens,
                ));
            }

            if current_tokens + tokens > config.max_batch_tokens && !current_batch.is_empty() {
                batches.push(std::mem::take(&mut current_batch));
                current_tokens = 0;
            }

            current_batch.push(text);
            current_tokens += tokens;
        }

        if !current_batch.is_empty() {
            batches.push(current_batch);
        }

        Ok(batches)
    }

    /// Embed a single batch
    async fn embed_batch(
        &self,
        batch: &[&str],
        config: &EmbeddingConfig,
    ) -> Result<EmbeddingResult, LlmError> {
        let mut builder = RequestBuilder::new(&config.model).with_input(batch);

        if let Some(dimension) = config.vector_dimension {
            if dimension > 0 {
                builder = builder.with_dimensions(dimension);
            }
        }

        if config.use_base64 {
            builder = builder.with_encoding_format("base64");
        }

        let request_body = builder.build();

        let response: crate::services::embedding::response_parser::StandardEmbeddingResponse = self
            .inner
            .request(
                &self.inner.endpoint_path(ServiceType::Embedding),
                &request_body,
            )
            .await?;

        let mut data = response.data;
        data.sort_by_key(|d| d.index);

        validate_embedding_data(&data, batch.len(), config.vector_dimension)?;

        let embeddings: Vec<Vec<f32>> = data.into_iter().map(|d| d.embedding).collect();

        let usage = response.usage.unwrap_or_default();

        Ok(EmbeddingResult {
            embeddings,
            prompt_tokens: usage.prompt_tokens,
            total_tokens: usage.total_tokens,
        })
    }
}

fn validate_embedding_data(
    data: &[StandardEmbeddingData],
    expected_count: usize,
    expected_dimension: Option<usize>,
) -> Result<(), LlmError> {
    if data.len() != expected_count {
        return Err(LlmError::invalid_response(format!(
            "Embedding response count mismatch: expected {expected_count}, received {}",
            data.len()
        )));
    }

    for (expected_index, item) in data.iter().enumerate() {
        if item.index != expected_index {
            return Err(LlmError::invalid_response(format!(
                "Embedding response index mismatch: expected {expected_index}, received {}",
                item.index
            )));
        }
        if let Some(expected_dimension) = expected_dimension
            && expected_dimension > 0
            && item.embedding.len() != expected_dimension
        {
            return Err(LlmError::invalid_response(format!(
                "Embedding dimension mismatch at index {expected_index}: expected {expected_dimension}, received {}",
                item.embedding.len()
            )));
        }
        if item.embedding.iter().any(|value| !value.is_finite()) {
            return Err(LlmError::invalid_response(format!(
                "Embedding at index {expected_index} contains a non-finite value"
            )));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::LlmConfig;

    fn test_handler() -> EmbeddingRequestHandler {
        let client = HttpLlmClient::new(LlmConfig::openai("sk-test".to_string()))
            .expect("test client should build");
        EmbeddingRequestHandler::new(Arc::new(client))
    }

    #[test]
    fn rejects_item_over_token_limit() {
        let config = EmbeddingConfig {
            max_item_tokens: 1,
            ..Default::default()
        };
        let result = test_handler().create_batches(&["this input is too long"], &config);
        assert!(matches!(result, Err(LlmError::TokenLimitExceeded(_, 1))));
    }

    #[test]
    fn rejects_zero_batch_limit() {
        let config = EmbeddingConfig {
            max_batch_tokens: 0,
            ..Default::default()
        };
        let result = test_handler().create_batches(&["text"], &config);
        assert!(matches!(result, Err(LlmError::InvalidInput(_))));
    }

    #[test]
    fn rejects_invalid_embedding_response_contract() {
        let duplicate_index = vec![
            StandardEmbeddingData {
                embedding: vec![1.0, 2.0],
                index: 0,
            },
            StandardEmbeddingData {
                embedding: vec![3.0, 4.0],
                index: 0,
            },
        ];
        assert!(validate_embedding_data(&duplicate_index, 2, Some(2)).is_err());

        let invalid_value = vec![StandardEmbeddingData {
            embedding: vec![f32::NAN, 2.0],
            index: 0,
        }];
        assert!(validate_embedding_data(&invalid_value, 1, Some(2)).is_err());
    }
}
