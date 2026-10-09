//! Embedding Request Handler

use crate::config::EmbeddingConfig;
use crate::suite::{full_endpoint_url, map_embedding_error, query_string};
use cce_config::global::ResolvedEmbeddingConfig;
use cce_llm::{EmbeddingResult, LlmError};
use cce_utils::token_estimation::estimate_tokens;
use llm_embedding::EmbeddingProvider;

/// Transport for one embedding batch: an llm-suite provider.
///
/// Retries live inside llm-suite (gateway profiles for chat, provider
/// timeouts for embedding); the transport performs a single attempt so a
/// rate-limit error always propagates to the caller's deferred-retry queue
/// instead of being absorbed here.
pub struct SuiteEmbeddingTransport<P = llm_embedding::OpenAICompatibleProvider> {
    provider: P,
}

impl SuiteEmbeddingTransport<llm_embedding::OpenAICompatibleProvider> {
    /// Builds the transport from a resolved embedding model.
    pub fn from_resolved(resolved: &ResolvedEmbeddingConfig) -> Result<Self, LlmError> {
        let mut config = llm_embedding::EmbeddingConfig::new(
            full_endpoint_url(&resolved.base_url, &resolved.endpoint_path),
            resolved.model.clone(),
        )
        .with_dimension(resolved.vector_dimension)
        .with_timeout(resolved.timeout_secs);
        if let Some(request_dimensions) = resolved.request_dimensions {
            config = config.with_request_dimensions(request_dimensions);
        }
        if let Some(api_key) = resolved.api_keys.first() {
            config = config.with_api_key(api_key.clone());
        }
        if let Some(proxy) = resolved.proxy_url.as_deref() {
            config = config.with_proxy(proxy);
        }
        config.headers = resolved.extra_headers.clone();
        config.query_params = resolved
            .extra_params
            .iter()
            .map(|(key, value)| (key.clone(), query_string(value)))
            .collect();
        let provider =
            llm_embedding::OpenAICompatibleProvider::new(config).map_err(map_embedding_error)?;
        Ok(Self { provider })
    }
}

impl<P: EmbeddingProvider> SuiteEmbeddingTransport<P> {
    /// Sends one batch, converting errors into the CCE contract.
    pub async fn embed_batch(&self, batch: Vec<String>) -> Result<EmbeddingResult, LlmError> {
        self.provider
            .embed(&batch)
            .await
            .map(|result| EmbeddingResult {
                embeddings: result.embeddings,
                prompt_tokens: result.prompt_tokens,
                total_tokens: result.total_tokens,
            })
            .map_err(map_embedding_error)
    }

    /// Builds a transport around an injected provider for tests.
    pub fn for_testing(provider: P) -> Self {
        Self { provider }
    }

    /// Accesses the wrapped provider (used by tests to inspect mocks).
    pub fn provider(&self) -> &P {
        &self.provider
    }
}

/// Embedding Request Handler - handles batching and request orchestration
pub struct EmbeddingRequestHandler<P = llm_embedding::OpenAICompatibleProvider> {
    /// Suite-backed batch transport
    inner: SuiteEmbeddingTransport<P>,
}

impl<P: EmbeddingProvider> EmbeddingRequestHandler<P> {
    /// Create a new request handler
    pub fn new(transport: SuiteEmbeddingTransport<P>) -> Self {
        Self { inner: transport }
    }

    /// Accesses the underlying transport (used by tests to inspect mocks).
    pub fn transport(&self) -> &SuiteEmbeddingTransport<P> {
        &self.inner
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

        let mut idx = 0;
        while idx < batches.len() {
            let batch: Vec<String> = batches[idx]
                .iter()
                .map(|text| (*text).to_string())
                .collect();
            let result = match self.inner.embed_batch(batch).await {
                Ok(result) => result,
                Err(error)
                    if !all_embeddings.is_empty()
                        && cce_types::error::common::ErrorClassify::is_transient(&error) =>
                {
                    // A later failed sub-batch must not discard the already
                    // embedded ones; replay only this sub-batch once and let a
                    // second failure propagate.
                    tracing::warn!(
                        sub_batch = idx,
                        sub_batch_count = batches.len(),
                        error = %error,
                        "Embedding sub-batch failed after partial progress; replaying once"
                    );
                    let batch: Vec<String> = batches[idx]
                        .iter()
                        .map(|text| (*text).to_string())
                        .collect();
                    self.inner.embed_batch(batch).await?
                }
                Err(error) => return Err(error),
            };
            all_embeddings.extend(result.embeddings);
            total_prompt_tokens += result.prompt_tokens;
            total_tokens += result.total_tokens;
            idx += 1;
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn test_handler() -> EmbeddingRequestHandler {
        let resolved = ResolvedEmbeddingConfig {
            base_url: "http://localhost:1".to_string(),
            api_keys: Vec::new(),
            model: "test-model".to_string(),
            vector_dimension: 8,
            request_dimensions: None,
            preprocessor: cce_config::PreprocessorConfig::None,
            max_batch_tokens: 8192,
            max_item_tokens: 2048,
            timeout_secs: 5,
            max_retries: 0,
            retry_delay_ms: 1,
            proxy_url: None,
            extra_headers: HashMap::new(),
            api_key_file: None,
            use_base64: false,
            extra_params: HashMap::new(),
            endpoint_path: "embeddings".to_string(),
        };
        let transport =
            SuiteEmbeddingTransport::from_resolved(&resolved).expect("test transport should build");
        EmbeddingRequestHandler::new(transport)
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
}
