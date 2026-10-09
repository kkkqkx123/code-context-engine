//! Embedding Request Handler

use crate::config::EmbeddingConfig;
use crate::suite::{embedding_resilience, full_endpoint_url, map_embedding_error, query_string};
use cce_config::global::ResolvedEmbeddingConfig;
use cce_llm::{EmbeddingResult, LlmError};
use cce_utils::token_estimation::estimate_tokens;
use llm_client::Resilience;
use llm_embedding::EmbeddingProvider;

/// Transport for one embedding batch: an llm-suite provider wrapped in the
/// shared resilience stack (limiter + breaker + retry injected from the
/// gateway's per-endpoint registries, so one upstream shares one set of
/// protection components across chat/embedding/rerank).
pub struct SuiteEmbeddingTransport<P = llm_embedding::OpenAICompatibleProvider> {
    provider: P,
    resilience: Resilience,
}

/// Translates the CCE preprocessor configuration into the suite's
/// [`llm_embedding::PreprocessorConfig`]. Nomic/Stella task types are mapped
/// to their concrete prefix/template strings here so the suite stays
/// model-agnostic.
fn suite_preprocessor(
    config: &cce_config::PreprocessorConfig,
) -> llm_embedding::PreprocessorConfig {
    match config {
        cce_config::PreprocessorConfig::None => llm_embedding::PreprocessorConfig::None,
        cce_config::PreprocessorConfig::Prefix { prefix } => {
            llm_embedding::PreprocessorConfig::Prefix {
                prefix: prefix.clone(),
            }
        }
        cce_config::PreprocessorConfig::Template { template } => {
            llm_embedding::PreprocessorConfig::Template {
                template: template.replace("{text}", "{{text}}"),
            }
        }
        cce_config::PreprocessorConfig::Nomic { task_type } => {
            llm_embedding::PreprocessorConfig::Prefix {
                prefix: nomic_prefix(task_type).to_string(),
            }
        }
        cce_config::PreprocessorConfig::Stella { task_type } => {
            llm_embedding::PreprocessorConfig::Template {
                template: stella_template(task_type).replace("{text}", "{{text}}"),
            }
        }
    }
}

/// Nomic-Embed task prefixes (document/query/clustering/classification).
fn nomic_prefix(task_type: &str) -> &'static str {
    match task_type {
        "search_query" => "search_query: ",
        "clustering" => "clustering: ",
        "classification" => "classification: ",
        _ => "search_document: ",
    }
}

/// Stella instruct templates for the two supported task types.
fn stella_template(task_type: &str) -> &'static str {
    match task_type {
        "s2s" => "Instruct: Retrieve semantically similar text.\nQuery: {text}",
        _ => {
            "Instruct: Given a web search query, retrieve relevant passages that answer the query.\nQuery: {text}"
        }
    }
}

impl SuiteEmbeddingTransport<llm_embedding::OpenAICompatibleProvider> {
    /// Builds the transport from a resolved embedding model.
    pub fn from_resolved(resolved: &ResolvedEmbeddingConfig) -> Result<Self, LlmError> {
        let mut config = llm_embedding::EmbeddingConfig::new(
            full_endpoint_url(&resolved.base_url, &resolved.endpoint_path),
            resolved.model.clone(),
        )
        .with_dimension(resolved.vector_dimension)
        .with_timeout(resolved.timeout_secs)
        .with_preprocessor(suite_preprocessor(&resolved.preprocessor));
        if let Some(request_dimensions) = resolved.request_dimensions {
            config = config.with_request_dimensions(request_dimensions);
        }
        if let Some(api_key) = resolved.api_keys.first() {
            config = config.with_api_key(api_key.clone());
        }
        if let Some(proxy) = resolved.proxy_url.as_deref() {
            config = config.with_proxy(proxy);
        }
        config.no_proxy = resolved.no_proxy.clone();
        config.headers = resolved.extra_headers.clone();
        config.query_params = resolved
            .extra_params
            .iter()
            .map(|(key, value)| (key.clone(), query_string(value)))
            .collect();
        let provider =
            llm_embedding::OpenAICompatibleProvider::new(config).map_err(map_embedding_error)?;

        let resilience = embedding_resilience(resolved);
        Ok(Self {
            provider,
            resilience,
        })
    }
}

impl<P: EmbeddingProvider> SuiteEmbeddingTransport<P> {
    /// Sends one batch, converting errors into the CCE contract.
    ///
    /// Rate limiting, circuit breaking and transient-failure retries are
    /// owned by the injected resilience stack (single suite-side
    /// implementation); a batch that still fails after the budget is
    /// exhausted propagates to the caller.
    pub async fn embed_batch(&self, batch: Vec<String>) -> Result<EmbeddingResult, LlmError> {
        self.resilience
            .execute(
                || llm_embedding::EmbeddingError::CircuitOpen,
                || self.provider.embed(&batch),
            )
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
        Self {
            provider,
            resilience: Resilience::new(),
        }
    }

    /// Overrides the resilience stack (test-only).
    #[cfg(test)]
    pub fn with_resilience(mut self, resilience: Resilience) -> Self {
        self.resilience = resilience;
        self
    }

    /// The active resilience stack (used by tests to inspect breakers).
    pub fn resilience(&self) -> &Resilience {
        &self.resilience
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
            provider_id: "test".to_string(),
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
            rate_limit: 0,
            circuit_breaker: cce_config::modules::CircuitBreakerConfig {
                enabled: false,
                ..Default::default()
            },
            proxy_url: None,
            no_proxy: Vec::new(),
            extra_headers: HashMap::new(),
            api_key_file: None,
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
        SuiteEmbeddingTransport::for_testing(provider).with_resilience(
            Resilience::new().with_retry(llm_common::retry::RetryPolicy {
                max_retries: 2,
                base_delay_ms: 1,
                exponential_backoff: true,
            }),
        )
    }

    #[test]
    fn retry_floor_honors_retry_after_hint() {
        let policy = llm_common::retry::RetryPolicy {
            max_retries: 3,
            base_delay_ms: 10,
            exponential_backoff: true,
        };
        assert_eq!(policy.delay_for_attempt_with_floor(1, 400), 400);
        assert_eq!(policy.delay_for_attempt_with_floor(1, 1), 10);
        assert_eq!(policy.delay_for_attempt_with_floor(2, 0), 20);
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

    #[tokio::test]
    async fn rate_limit_retry_after_is_propagated() {
        let mock = MockEmbeddingProvider::with_steps(vec![MockEmbeddingStep::Fail(
            llm_embedding::EmbeddingError::Provider {
                status: 429,
                message: "slow down".to_string(),
                retry_after_ms: Some(1200),
            },
        )]);
        let transport = SuiteEmbeddingTransport::for_testing(mock);

        let error = transport
            .embed_batch(vec!["text".to_string()])
            .await
            .expect_err("single attempt surfaces the limit");
        assert!(matches!(error, LlmError::RateLimitExceeded(1200)));
    }
}
