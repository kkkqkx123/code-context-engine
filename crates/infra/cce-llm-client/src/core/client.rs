//! Core LLM HTTP client

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::Instant;
use tracing::{trace, warn};

use cce_config::modules::ServiceType;

use crate::core::config::LlmConfig;
use crate::core::error::LlmError;
use crate::core::http_service::{HttpRequestConfig, HttpRequestService};
use crate::core::rate_limiter::ConfigurableRateLimiter;
use crate::core::retry::RetryPolicy;
use cce_circuit_breaker::CircuitBreaker;
use cce_metrics::LlmRetryMetrics;

/// LLM HTTP client - core functionality only
pub struct HttpLlmClient {
    /// HTTP request service
    http_service: Arc<HttpRequestService>,
    /// Base configuration
    config: LlmConfig,
    /// Provider ID
    provider_id: String,
    /// Maximum input tokens
    max_input_tokens: usize,
    /// Rate limit (requests per minute)
    rate_limit: u32,
    /// Consecutive failure threshold for health tracking
    failure_threshold: u32,
    /// Consecutive failure count for health tracking
    consecutive_failures: Arc<AtomicU32>,
    /// Consecutive success count for recovery tracking
    consecutive_successes: Arc<AtomicU32>,
    /// Total requests made
    total_requests: Arc<AtomicU64>,
    /// Total failures
    total_failures: Arc<AtomicU64>,
}

impl Clone for HttpLlmClient {
    fn clone(&self) -> Self {
        Self {
            http_service: Arc::clone(&self.http_service),
            config: self.config.clone(),
            provider_id: self.provider_id.clone(),
            max_input_tokens: self.max_input_tokens,
            rate_limit: self.rate_limit,
            failure_threshold: self.failure_threshold,
            consecutive_failures: Arc::clone(&self.consecutive_failures),
            consecutive_successes: Arc::clone(&self.consecutive_successes),
            total_requests: Arc::clone(&self.total_requests),
            total_failures: Arc::clone(&self.total_failures),
        }
    }
}

impl std::fmt::Debug for HttpLlmClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpLlmClient")
            .field("provider_id", &self.provider_id)
            .field("base_url", &self.config.base_url)
            .field("max_input_tokens", &self.max_input_tokens)
            .finish_non_exhaustive()
    }
}

/// Builder for HttpLlmClient
#[derive(Debug, Default)]
pub struct HttpLlmClientBuilder {
    config: Option<LlmConfig>,
    rate_limiter: Option<Arc<ConfigurableRateLimiter>>,
    retry_policy: Option<RetryPolicy>,
    timeout_secs: Option<u64>,
    provider_id: Option<String>,
    max_input_tokens: Option<usize>,
    rate_limit: Option<u32>,
    failure_threshold: Option<u32>,
    circuit_breaker: Option<Option<Arc<std::sync::Mutex<CircuitBreaker>>>>,
    retry_metrics: Option<Arc<LlmRetryMetrics>>,
}

impl HttpLlmClientBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set LLM configuration
    pub fn with_config(mut self, config: LlmConfig) -> Self {
        self.config = Some(config);
        self
    }

    /// Set rate limiter
    pub fn with_rate_limiter(mut self, limiter: Arc<ConfigurableRateLimiter>) -> Self {
        self.rate_limiter = Some(limiter);
        self
    }

    /// Set retry policy
    pub fn with_retry_policy(mut self, policy: RetryPolicy) -> Self {
        self.retry_policy = Some(policy);
        self
    }

    /// Set timeout
    pub fn with_timeout(mut self, timeout_secs: u64) -> Self {
        self.timeout_secs = Some(timeout_secs);
        self
    }

    /// Set provider ID
    pub fn with_provider_id(mut self, provider_id: String) -> Self {
        self.provider_id = Some(provider_id);
        self
    }

    /// Set max input tokens
    pub fn with_max_input_tokens(mut self, max_input_tokens: usize) -> Self {
        self.max_input_tokens = Some(max_input_tokens);
        self
    }

    /// Set rate limit
    pub fn with_rate_limit(mut self, rate_limit: u32) -> Self {
        self.rate_limit = Some(rate_limit);
        self
    }

    /// Set failure threshold
    pub fn with_failure_threshold(mut self, failure_threshold: u32) -> Self {
        self.failure_threshold = Some(failure_threshold);
        self
    }

    /// Set the circuit breaker shared per upstream base URL (None = disabled)
    pub fn with_circuit_breaker(
        mut self,
        circuit_breaker: Option<Arc<std::sync::Mutex<CircuitBreaker>>>,
    ) -> Self {
        self.circuit_breaker = Some(circuit_breaker);
        self
    }

    /// Attach registry-backed retry/circuit-breaker metrics (None = disabled)
    pub fn with_retry_metrics(mut self, retry_metrics: Option<Arc<LlmRetryMetrics>>) -> Self {
        self.retry_metrics = retry_metrics;
        self
    }

    /// Build the client
    pub fn build(self) -> Result<HttpLlmClient, LlmError> {
        let config = self
            .config
            .ok_or_else(|| LlmError::config("No configuration provided"))?;

        if config.base_url.is_empty() {
            return Err(LlmError::config("base_url is required"));
        }

        let timeout_secs = self.timeout_secs.unwrap_or(config.timeout_secs);
        let rate_limit = self.rate_limit.unwrap_or(60);
        let rate_limiter = self
            .rate_limiter
            .unwrap_or_else(|| Arc::new(ConfigurableRateLimiter::new(rate_limit)));
        let retry_policy = self
            .retry_policy
            .unwrap_or_else(|| RetryPolicy::new(config.max_retries, config.retry_delay_ms));

        let circuit_breaker = self.circuit_breaker.unwrap_or(None);
        let retry_metrics = self.retry_metrics;

        let http_config = HttpRequestConfig {
            base_url: config.base_url.clone(),
            api_keys: config.api_keys.clone(),
            timeout_secs,
            proxy_url: config.proxy_url.clone(),
            extra_headers: config.extra_headers.clone(),
            extra_params: config.extra_params.clone(),
        };

        let http_service = Arc::new(HttpRequestService::new(
            http_config,
            rate_limiter,
            retry_policy,
            circuit_breaker,
            retry_metrics,
        )?);

        Ok(HttpLlmClient {
            http_service,
            config,
            provider_id: self.provider_id.unwrap_or_else(|| "default".to_string()),
            max_input_tokens: self.max_input_tokens.unwrap_or(128000),
            rate_limit,
            failure_threshold: self.failure_threshold.unwrap_or(3),
            consecutive_failures: Arc::new(AtomicU32::new(0)),
            consecutive_successes: Arc::new(AtomicU32::new(0)),
            total_requests: Arc::new(AtomicU64::new(0)),
            total_failures: Arc::new(AtomicU64::new(0)),
        })
    }
}

impl TryFrom<LlmConfig> for HttpLlmClient {
    type Error = LlmError;

    fn try_from(config: LlmConfig) -> Result<Self, Self::Error> {
        HttpLlmClient::new(config)
    }
}

impl HttpLlmClient {
    /// Create a new client with configuration
    pub fn new(config: LlmConfig) -> Result<Self, LlmError> {
        HttpLlmClientBuilder::new().with_config(config).build()
    }

    /// Create a builder
    pub fn builder() -> HttpLlmClientBuilder {
        HttpLlmClientBuilder::new()
    }

    /// Send HTTP request with retry and rate limiting
    pub async fn request<T: serde::Serialize, R: serde::de::DeserializeOwned>(
        &self,
        endpoint: &str,
        body: &T,
    ) -> Result<R, LlmError> {
        let started_at = Instant::now();
        let result = self.http_service.post_json(endpoint, body).await;
        self.record_request_result(result.is_ok(), started_at.elapsed().as_millis() as u64);
        result
    }

    /// Send HTTP request and return raw response body (for custom parsing)
    pub async fn request_raw<T: serde::Serialize>(
        &self,
        endpoint: &str,
        body: &T,
    ) -> Result<String, LlmError> {
        let started_at = Instant::now();
        let result = self.http_service.post_raw(endpoint, body).await;
        self.record_request_result(result.is_ok(), started_at.elapsed().as_millis() as u64);
        result
    }

    /// Resolve the endpoint path for a service
    pub fn endpoint_path(&self, service: ServiceType) -> String {
        self.config
            .endpoints
            .get(&service)
            .cloned()
            .unwrap_or_else(|| {
                cce_config::modules::ProviderConfig::default_endpoint_path(service).to_string()
            })
    }

    fn record_request_result(&self, success: bool, latency_ms: u64) {
        if success {
            self.record_success(latency_ms, 0);
        } else {
            self.record_failure();
        }
    }

    /// Get provider ID
    pub fn provider_id(&self) -> &str {
        &self.provider_id
    }

    /// Get max input tokens
    pub fn max_input_tokens(&self) -> usize {
        self.max_input_tokens
    }

    /// Get max tokens (alias for max_input_tokens)
    pub fn max_tokens(&self) -> u32 {
        (self.max_input_tokens / 4) as u32
    }

    /// Get rate limit (requests per minute)
    pub fn rate_limit(&self) -> u32 {
        self.rate_limit
    }

    /// Check if can handle given token count
    pub fn can_handle_tokens(&self, tokens: usize) -> bool {
        tokens <= self.max_input_tokens
    }

    /// Record success with health tracking
    pub fn record_success(&self, latency_ms: u64, tokens: u64) {
        self.consecutive_failures.store(0, Ordering::Relaxed);
        self.consecutive_successes.fetch_add(1, Ordering::Relaxed);
        self.total_requests.fetch_add(1, Ordering::Relaxed);

        trace!(
            provider_id = %self.provider_id,
            latency_ms = latency_ms,
            tokens = tokens,
            "Request succeeded"
        );
    }

    /// Record failure with health tracking
    pub fn record_failure(&self) {
        self.consecutive_failures.fetch_add(1, Ordering::Relaxed);
        self.consecutive_successes.store(0, Ordering::Relaxed);
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        self.total_failures.fetch_add(1, Ordering::Relaxed);

        let failures = self.consecutive_failures.load(Ordering::Relaxed);
        warn!(
            provider_id = %self.provider_id,
            consecutive_failures = failures,
            "Request failed"
        );
    }

    /// Check if the client is healthy based on consecutive failures
    pub fn is_healthy(&self) -> bool {
        let failures = self.consecutive_failures.load(Ordering::Relaxed);
        failures < self.failure_threshold
    }
}

/// Port adapter: exposes the HTTP client through the core `LlmClient` chat port
impl cce_llm::LlmClient for HttpLlmClient {
    #[allow(clippy::manual_async_fn)]
    fn chat(
        &self,
        messages: &[cce_llm::Message],
        config: &cce_llm::ChatConfig,
    ) -> impl std::future::Future<Output = Result<cce_llm::ChatResult, cce_llm::LlmError>> + Send
    {
        async move {
            let handler =
                crate::services::chat::handler::ChatRequestHandler::new(Arc::new(self.clone()));
            handler.chat(messages, config).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llm_client_builder() {
        let config = LlmConfig::openai("sk-test".to_string());
        let client = HttpLlmClient::builder()
            .with_config(config)
            .with_timeout(30)
            .build();

        assert!(client.is_ok());
    }

    #[test]
    fn test_llm_client_new() {
        let config = LlmConfig::openai("sk-test".to_string());
        let client = HttpLlmClient::new(config);
        assert!(client.is_ok());
    }

    #[test]
    fn test_llm_client_builder_without_config() {
        let result = HttpLlmClient::builder().build();
        assert!(result.is_err());
    }

    #[test]
    fn test_llm_client_with_invalid_config() {
        let config = LlmConfig::default();
        let result = HttpLlmClient::new(config);
        assert!(result.is_err());
    }

    #[test]
    fn test_llm_client_clone() {
        let config = LlmConfig::openai("sk-test".to_string());
        let client = HttpLlmClient::new(config).expect("client should build");

        let cloned = client.clone();
        assert_eq!(cloned.provider_id(), client.provider_id());
        assert_eq!(cloned.max_input_tokens(), client.max_input_tokens());
    }

    #[test]
    fn test_llm_client_debug_format() {
        let config = LlmConfig::openai("sk-test".to_string());
        let client = HttpLlmClient::new(config).expect("client should build");

        let debug_str = format!("{:?}", client);
        assert!(debug_str.contains("HttpLlmClient"));
        assert!(debug_str.contains("provider_id"));
    }

    #[test]
    fn test_llm_client_properties() {
        let config = LlmConfig::openai("sk-test".to_string());
        let client = HttpLlmClient::builder()
            .with_config(config)
            .with_provider_id("test-provider".to_string())
            .with_max_input_tokens(8192)
            .with_rate_limit(30)
            .build()
            .expect("client should build");

        assert_eq!(client.provider_id(), "test-provider");
        assert_eq!(client.max_input_tokens(), 8192);
        assert_eq!(client.max_tokens(), 2048);
        assert_eq!(client.rate_limit(), 30);
        assert!(client.can_handle_tokens(4096));
        assert!(!client.can_handle_tokens(16384));
    }

    #[test]
    fn test_configured_retry_policy_is_used() {
        let mut config = LlmConfig::openai("sk-test".to_string());
        config.max_retries = 7;
        config.retry_delay_ms = 25;
        let client = HttpLlmClient::new(config).expect("client should build");

        assert_eq!(client.http_service.max_retries(), 7);
    }

    #[test]
    fn test_api_key_configuration_accepted() {
        let config = LlmConfig {
            api_keys: vec!["key1".to_string(), "key2".to_string(), "key3".to_string()],
            base_url: "https://api.example.com".to_string(),
            ..Default::default()
        };
        let client = HttpLlmClient::new(config);
        assert!(client.is_ok());
    }

    #[tokio::test]
    async fn test_health_tracking_initial_state() {
        let config = LlmConfig::openai("sk-test".to_string());
        let client = HttpLlmClient::new(config).expect("client should build");
        assert!(client.is_healthy());
    }

    #[tokio::test]
    async fn test_health_tracking_record_success() {
        let config = LlmConfig::openai("sk-test".to_string());
        let client = HttpLlmClient::new(config).expect("client should build");

        client.record_success(100, 50);
        assert!(client.is_healthy());

        client.record_success(50, 30);
        client.record_success(75, 40);
        assert!(client.is_healthy());
    }

    #[tokio::test]
    async fn test_health_tracking_record_failure() {
        let config = LlmConfig::openai("sk-test".to_string());
        let client = HttpLlmClient::new(config).expect("client should build");

        client.record_failure();
        assert!(client.is_healthy());

        client.record_failure();
        assert!(client.is_healthy());

        client.record_failure();
        assert!(!client.is_healthy());
    }

    #[tokio::test]
    async fn test_health_tracking_recovery_after_failure() {
        let config = LlmConfig::openai("sk-test".to_string());
        let client = HttpLlmClient::new(config).expect("client should build");

        client.record_failure();
        client.record_failure();
        client.record_failure();
        assert!(!client.is_healthy());

        client.record_success(100, 50);
        assert!(client.is_healthy());
    }
}
