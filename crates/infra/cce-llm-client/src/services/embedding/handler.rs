//! Embedding Request Handler

use std::time::Duration;

use crate::config::EmbeddingConfig;
use crate::suite::{full_endpoint_url, map_embedding_error, query_string};
use cce_config::global::ResolvedEmbeddingConfig;
use cce_llm::{EmbeddingResult, LlmError};
use cce_types::error::common::ErrorClassify;
use cce_utils::token_estimation::estimate_tokens;
use llm_embedding::EmbeddingProvider;

/// Upper bound for one computed retry delay, so a long retry budget cannot
/// stall a caller for minutes on a single batch.
const MAX_RETRY_DELAY_MS: u64 = 30_000;

/// Retry budget for embedding requests against one provider.
///
/// Only transient failures are retried: request timeouts, transport
/// failures, 5xx responses and rate limits. A permanent rejection (auth,
/// context length, invalid request) surfaces on the first attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Attempts made after the first failed one.
    pub max_retries: u32,
    /// Delay before the first retry; doubles per attempt.
    pub retry_delay_ms: u64,
}

impl RetryPolicy {
    /// No retries: a transient failure propagates to the caller at once.
    pub fn fail_fast() -> Self {
        Self {
            max_retries: 0,
            retry_delay_ms: 0,
        }
    }

    /// Delay before `attempt` (1-based) in ms: exponential with saturation,
    /// capped by [`MAX_RETRY_DELAY_MS`].
    pub fn backoff_ms(&self, attempt: u32) -> u64 {
        let growth = 1u64 << attempt.saturating_sub(1).min(32);
        self.retry_delay_ms
            .saturating_mul(growth)
            .min(MAX_RETRY_DELAY_MS)
    }
}

/// Transport for one embedding batch: an llm-suite provider.
///
/// Retries are owned here rather than inside llm-suite: the provider config
/// carries the retry budget (`max_retries`, `retry_delay_ms`) which the
/// embedding transport is the only component able to apply. A batch that
/// still fails after the budget is exhausted propagates to the caller, which
/// defers it to its own outer retry pass.
pub struct SuiteEmbeddingTransport<P = llm_embedding::OpenAICompatibleProvider> {
    provider: P,
    retries: RetryPolicy,
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
        Ok(Self {
            provider,
            retries: RetryPolicy {
                max_retries: resolved.max_retries,
                retry_delay_ms: resolved.retry_delay_ms,
            },
        })
    }
}

impl<P: EmbeddingProvider> SuiteEmbeddingTransport<P> {
    /// Sends one batch, converting errors into the CCE contract.
    ///
    /// A transient failure is retried within the provider's retry budget with
    /// exponential backoff; a rate limit waits at least the window the
    /// provider reported. Permanent failures surface on the first attempt.
    pub async fn embed_batch(&self, batch: Vec<String>) -> Result<EmbeddingResult, LlmError> {
        let mut attempt = 0u32;
        loop {
            let outcome = self
                .provider
                .embed(&batch)
                .await
                .map(|result| EmbeddingResult {
                    embeddings: result.embeddings,
                    prompt_tokens: result.prompt_tokens,
                    total_tokens: result.total_tokens,
                })
                .map_err(map_embedding_error);
            match outcome {
                Ok(result) => return Ok(result),
                Err(error) => {
                    if attempt >= self.retries.max_retries || !error.is_retryable() {
                        return Err(error);
                    }
                    attempt += 1;
                    let delay_ms = self.retry_delay(&error, attempt);
                    tracing::warn!(
                        attempt,
                        max_retries = self.retries.max_retries,
                        delay_ms,
                        error = %error,
                        "Embedding attempt failed transiently; retrying"
                    );
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                }
            }
        }
    }

    /// Delay before the next attempt: the exponential backoff, or the
    /// provider's retry-after window for a rate limit, whichever is longer.
    fn retry_delay(&self, error: &LlmError, attempt: u32) -> u64 {
        let backoff = self.retries.backoff_ms(attempt);
        match error {
            LlmError::RateLimitExceeded(retry_after_ms) => backoff.max(*retry_after_ms),
            _ => backoff,
        }
    }

    /// Builds a transport around an injected provider for tests.
    pub fn for_testing(provider: P) -> Self {
        Self {
            provider,
            retries: RetryPolicy::fail_fast(),
        }
    }

    /// Sets the retry budget (test-only; production reads it from the
    /// resolved provider configuration).
    #[cfg(test)]
    pub fn with_retry_policy(mut self, retries: RetryPolicy) -> Self {
        self.retries = retries;
        self
    }

    /// The active retry budget.
    pub fn retry_policy(&self) -> RetryPolicy {
        self.retries
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
            let result = self.inner.embed_batch(batch).await?;
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
    use llm_embedding::mock::{MockEmbeddingProvider, MockEmbeddingStep};
    use std::collections::HashMap;

    fn test_resolved() -> ResolvedEmbeddingConfig {
        ResolvedEmbeddingConfig {
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
        }
    }

    fn test_handler() -> EmbeddingRequestHandler {
        let transport = SuiteEmbeddingTransport::from_resolved(&test_resolved())
            .expect("test transport should build");
        EmbeddingRequestHandler::new(transport)
    }

    #[test]
    fn provider_retry_budget_reaches_the_transport() {
        let resolved = ResolvedEmbeddingConfig {
            max_retries: 4,
            retry_delay_ms: 250,
            ..test_resolved()
        };

        let transport =
            SuiteEmbeddingTransport::from_resolved(&resolved).expect("test transport should build");

        assert_eq!(transport.retry_policy().max_retries, 4);
        assert_eq!(transport.retry_policy().retry_delay_ms, 250);
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

    fn retrying_transport(
        provider: MockEmbeddingProvider,
    ) -> SuiteEmbeddingTransport<MockEmbeddingProvider> {
        SuiteEmbeddingTransport::for_testing(provider).with_retry_policy(RetryPolicy {
            max_retries: 2,
            retry_delay_ms: 1,
        })
    }

    #[tokio::test]
    async fn retries_transient_failure_until_success() {
        let mock = MockEmbeddingProvider::with_steps(vec![
            MockEmbeddingStep::Fail(llm_embedding::EmbeddingError::Timeout),
            MockEmbeddingStep::Respond,
        ]);
        let transport = retrying_transport(mock);

        let result = transport
            .embed_batch(vec!["first".to_string(), "second".to_string()])
            .await
            .expect("second attempt succeeds");

        assert_eq!(result.embeddings.len(), 2);
        assert_eq!(
            transport.provider().recorded_batch_sizes(),
            vec![2, 2],
            "the same batch is replayed, never split"
        );
    }

    #[tokio::test]
    async fn retry_budget_exhaustion_reports_the_failure() {
        let mock = MockEmbeddingProvider::with_steps(
            (0..3)
                .map(|_| MockEmbeddingStep::Fail(llm_embedding::EmbeddingError::Timeout))
                .collect(),
        );
        let transport = retrying_transport(mock);

        let error = transport
            .embed_batch(vec!["text".to_string()])
            .await
            .expect_err("budget of two retries is exhausted");

        assert!(matches!(error, LlmError::Timeout(_)));
        assert_eq!(transport.provider().recorded_batch_sizes(), vec![1, 1, 1]);
    }

    #[tokio::test]
    async fn permanent_failure_is_not_retried() {
        let mock = MockEmbeddingProvider::with_steps(
            (0..3)
                .map(|_| {
                    MockEmbeddingStep::Fail(llm_embedding::EmbeddingError::InvalidRequest(
                        "model does not support this input".to_string(),
                    ))
                })
                .collect(),
        );
        let transport = retrying_transport(mock);

        let error = transport
            .embed_batch(vec!["text".to_string()])
            .await
            .expect_err("invalid request is permanent");

        assert!(matches!(error, LlmError::InvalidInput(_)));
        assert_eq!(transport.provider().recorded_batch_sizes(), vec![1]);
    }

    #[test]
    fn rate_limit_retry_after_sets_the_delay_floor() {
        let transport = retrying_transport(MockEmbeddingProvider::new("model", 8));

        assert_eq!(
            transport.retry_delay(&LlmError::RateLimitExceeded(400), 1),
            400,
            "a retry-after window longer than the backoff wins"
        );
        assert_eq!(
            transport.retry_delay(&LlmError::RateLimitExceeded(1), 1),
            1,
            "a retry-after window shorter than the backoff is raised to it"
        );
    }

    #[test]
    fn backoff_doubles_per_attempt_and_stays_capped() {
        let policy = RetryPolicy {
            max_retries: 8,
            retry_delay_ms: 500,
        };

        assert_eq!(policy.backoff_ms(1), 500);
        assert_eq!(policy.backoff_ms(2), 1000);
        assert_eq!(policy.backoff_ms(3), 2000);
        assert_eq!(
            policy.backoff_ms(64),
            MAX_RETRY_DELAY_MS,
            "a long retry budget cannot produce an unbounded delay"
        );

        let fail_fast = RetryPolicy::fail_fast();
        assert_eq!(fail_fast.max_retries, 0);
        assert_eq!(fail_fast.backoff_ms(1), 0);
    }
}
