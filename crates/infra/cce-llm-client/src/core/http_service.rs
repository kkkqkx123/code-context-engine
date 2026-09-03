//! HTTP Request Service for LLM operations

use reqwest::Client;
use serde::{Serialize, de::DeserializeOwned};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, Instant};
use tracing::{debug, error, warn};

use cce_llm::LlmRetryErrorClass;

use crate::core::error::LlmError;
use crate::core::rate_limiter::ConfigurableRateLimiter;
use crate::core::retry::RetryPolicy;
use cce_circuit_breaker::CircuitBreaker;
use cce_metrics::LlmRetryMetrics;

/// HTTP request service configuration
#[derive(Debug, Clone)]
pub struct HttpRequestConfig {
    /// Base URL for API requests
    pub base_url: String,
    /// Resolved API keys for authentication.
    pub api_keys: Vec<String>,
    /// Timeout in seconds
    pub timeout_secs: u64,
    /// Proxy URL (optional)
    pub proxy_url: Option<String>,
    /// Extra headers to include in all requests
    pub extra_headers: std::collections::HashMap<String, String>,
    /// Query parameters to include in all requests.
    pub extra_params: std::collections::HashMap<String, serde_json::Value>,
}

/// HTTP request service - provides unified HTTP operations for LLM APIs
pub struct HttpRequestService {
    /// HTTP client
    client: Client,
    /// Configuration
    config: HttpRequestConfig,
    /// Rate limiter (configurable with token bucket)
    rate_limiter: Arc<ConfigurableRateLimiter>,
    /// Retry policy
    retry_policy: RetryPolicy,
    /// Circuit breaker shared per upstream base URL (None = disabled)
    circuit_breaker: Option<Arc<Mutex<CircuitBreaker>>>,
    /// LLM retry metrics (None = not exported)
    retry_metrics: Option<Arc<LlmRetryMetrics>>,
    /// Last circuit state observed for the state gauge
    last_circuit_state: AtomicU8,
}

impl HttpRequestService {
    /// Create a new HTTP request service
    pub fn new(
        config: HttpRequestConfig,
        rate_limiter: Arc<ConfigurableRateLimiter>,
        retry_policy: RetryPolicy,
        circuit_breaker: Option<Arc<Mutex<CircuitBreaker>>>,
        retry_metrics: Option<Arc<LlmRetryMetrics>>,
    ) -> Result<Self, LlmError> {
        if config.base_url.is_empty() {
            return Err(LlmError::config("Base URL is required"));
        }

        let mut builder = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .pool_max_idle_per_host(10);

        if let Some(ref proxy_url) = config.proxy_url {
            if !proxy_url.is_empty() {
                let proxy = reqwest::Proxy::all(proxy_url)
                    .map_err(|e| LlmError::config(format!("Proxy error: {}", e)))?;
                builder = builder.proxy(proxy);
            }
        }

        let client = builder
            .build()
            .map_err(|e| LlmError::http(format!("Client build error: {}", e)))?;

        Ok(Self {
            client,
            config,
            rate_limiter,
            retry_policy,
            circuit_breaker,
            retry_metrics,
            last_circuit_state: AtomicU8::new(0),
        })
    }

    /// Fast-fail gate: rejects requests while the circuit breaker is open
    async fn check_circuit_breaker(&self) -> Result<(), LlmError> {
        let Some(breaker) = &self.circuit_breaker else {
            return Ok(());
        };
        let mut breaker = breaker.lock().expect("LLM circuit breaker mutex poisoned");
        let result = breaker.try_acquire::<LlmError>();
        self.sync_circuit_state_metrics(&breaker);
        match result {
            Ok(()) => Ok(()),
            Err(_) => {
                if let Some(metrics) = &self.retry_metrics {
                    metrics.record_circuit_rejection();
                }
                Err(LlmError::api("Circuit breaker is open"))
            }
        }
    }

    /// Record the outcome of a request against the circuit breaker.
    async fn record_circuit_outcome<T>(&self, result: &Result<T, LlmError>) {
        let Some(breaker) = &self.circuit_breaker else {
            return;
        };
        let mut breaker = breaker.lock().expect("LLM circuit breaker mutex poisoned");
        match result {
            Ok(_) => breaker.record_success(),
            Err(error) if LlmRetryErrorClass::from_error(error).counts_toward_circuit_failure() => {
                warn!(error = %error, "LLM upstream failure counted by circuit breaker");
                breaker.record_failure();
            }
            Err(_) => {}
        }
        self.sync_circuit_state_metrics(&breaker);
    }

    /// Push the current breaker state to the metrics gauge
    fn sync_circuit_state_metrics(&self, breaker: &CircuitBreaker) {
        let Some(metrics) = &self.retry_metrics else {
            return;
        };
        let state_value = breaker.state_value();
        let encoded = match state_value {
            0.0 => 0u8,
            0.5 => 1,
            _ => 2,
        };
        metrics.record_circuit_state(state_value);
        let prev = self.last_circuit_state.swap(encoded, Ordering::Relaxed);
        if prev != encoded {
            metrics.record_circuit_transition();
        }
    }

    /// Send HTTP POST request with JSON body and deserialize response
    pub async fn post_json<T: Serialize, R: DeserializeOwned>(
        &self,
        endpoint: &str,
        body: &T,
    ) -> Result<R, LlmError> {
        self.check_circuit_breaker().await?;
        let result = self
            .retry_policy
            .execute_observed(self.retry_metrics.as_ref(), || {
                self.send_post_request(endpoint, body)
            })
            .await;
        self.record_circuit_outcome(&result).await;
        result
    }

    /// Send HTTP POST request and return raw response body
    pub async fn post_raw<T: Serialize>(
        &self,
        endpoint: &str,
        body: &T,
    ) -> Result<String, LlmError> {
        self.check_circuit_breaker().await?;
        let result = self
            .retry_policy
            .execute_observed(self.retry_metrics.as_ref(), || async {
                let response_text = self.send_post_request_raw(endpoint, body).await?;
                Ok(response_text)
            })
            .await;
        self.record_circuit_outcome(&result).await;
        result
    }

    /// Get the primary API key, if one is configured.
    fn primary_api_key(&self) -> Option<&str> {
        self.config.api_keys.first().map(String::as_str)
    }

    /// Send POST request with JSON body (single attempt, returns deserialized response)
    async fn send_post_request<T: Serialize, R: DeserializeOwned>(
        &self,
        endpoint: &str,
        body: &T,
    ) -> Result<R, LlmError> {
        let response_text = self.send_post_request_raw(endpoint, body).await?;
        serde_json::from_str(&response_text)
            .map_err(|e| LlmError::invalid_response(format!("Failed to parse response: {}", e)))
    }

    /// Send POST request with JSON body (single attempt, returns raw text)
    async fn send_post_request_raw<T: Serialize>(
        &self,
        endpoint: &str,
        body: &T,
    ) -> Result<String, LlmError> {
        self.rate_limiter.wait().await;

        let url = format!(
            "{}/{}",
            self.config.base_url.trim_end_matches('/'),
            endpoint.trim_start_matches('/')
        );
        let start_time = Instant::now();

        let mut req = self.client.post(&url);
        if !self.config.extra_params.is_empty() {
            req = req.query(&self.config.extra_params);
        }
        if let Some(api_key) = self.primary_api_key() {
            req = req.bearer_auth(api_key);
        }
        req = req.json(body);

        for (key, value) in &self.config.extra_headers {
            req = req.header(key, value);
        }

        debug!(
            url = %url,
            endpoint = endpoint,
            "Sending LLM request"
        );

        let response = req.send().await.map_err(|e| {
            error!(error = %e, "Request failed to send");
            LlmError::http(format!("Request failed: {}", e))
        })?;

        let status = response.status();
        let elapsed_ms = start_time.elapsed().as_millis();

        debug!(
            url = %url,
            status = %status,
            elapsed_ms = elapsed_ms,
            "Received LLM response"
        );

        if status == 429 {
            let retry_after_ms = parse_retry_after_ms(response.headers());
            self.rate_limiter.on_rate_limit(retry_after_ms).await;
            return Err(LlmError::rate_limit_exceeded(retry_after_ms));
        }

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            let body_preview: String = body.chars().take(1000).collect();
            error!(status = %status, response_length = body.len(), "LLM API error");

            return Err(match status.as_u16() {
                401 => LlmError::auth(format!("Authentication failed: {body_preview}")),
                403 => LlmError::auth(format!("Permission denied: {body_preview}")),
                404 => LlmError::model_not_found(format!("Resource not found: {body_preview}")),
                code => LlmError::http_status(code, body_preview),
            });
        }

        self.rate_limiter.on_success().await;

        let response_body = response.text().await.map_err(|e| {
            error!(error = %e, "Failed to read response body");
            LlmError::http(format!("Failed to read response: {}", e))
        })?;

        Ok(response_body)
    }

    /// Get base URL
    pub fn base_url(&self) -> &str {
        &self.config.base_url
    }

    /// Get timeout
    pub fn timeout_secs(&self) -> u64 {
        self.config.timeout_secs
    }

    #[cfg(test)]
    pub(crate) fn max_retries(&self) -> u32 {
        self.retry_policy.max_retries()
    }
}

/// Parse a `Retry-After` header value into milliseconds (default 5s).
fn parse_retry_after_ms(headers: &reqwest::header::HeaderMap) -> u64 {
    let retry_after_secs = headers
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(parse_retry_after_secs)
        .unwrap_or(5);
    retry_after_secs * 1000
}

/// Parse a `Retry-After` value into seconds remaining.
fn parse_retry_after_secs(value: &str) -> Option<u64> {
    let value = value.trim();
    if let Ok(secs) = value.parse::<u64>() {
        return Some(secs);
    }

    let date = chrono::DateTime::parse_from_rfc2822(value).ok()?;
    let remaining = date.signed_duration_since(chrono::Utc::now());
    Some(remaining.num_seconds().max(0) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_http_request_service_creation() {
        let config = HttpRequestConfig {
            base_url: "https://api.example.com".to_string(),
            api_keys: vec!["test-key".to_string()],
            timeout_secs: 30,
            proxy_url: None,
            extra_headers: std::collections::HashMap::new(),
            extra_params: std::collections::HashMap::new(),
        };

        let service = HttpRequestService::new(
            config,
            Arc::new(ConfigurableRateLimiter::new(60)),
            RetryPolicy::default(),
            None,
            None,
        );

        assert!(service.is_ok());
        assert_eq!(
            service.expect("service should build").primary_api_key(),
            Some("test-key")
        );
    }

    #[test]
    fn test_http_request_service_validation_no_keys() {
        let config = HttpRequestConfig {
            base_url: "https://api.example.com".to_string(),
            api_keys: vec![],
            timeout_secs: 30,
            proxy_url: None,
            extra_headers: std::collections::HashMap::new(),
            extra_params: std::collections::HashMap::new(),
        };

        let service = HttpRequestService::new(
            config,
            Arc::new(ConfigurableRateLimiter::new(60)),
            RetryPolicy::default(),
            None,
            None,
        );

        assert!(service.is_ok());
    }

    #[test]
    fn test_http_request_service_validation_no_url() {
        let config = HttpRequestConfig {
            base_url: "".to_string(),
            api_keys: vec!["test-key".to_string()],
            timeout_secs: 30,
            proxy_url: None,
            extra_headers: std::collections::HashMap::new(),
            extra_params: std::collections::HashMap::new(),
        };

        let service = HttpRequestService::new(
            config,
            Arc::new(ConfigurableRateLimiter::new(60)),
            RetryPolicy::default(),
            None,
            None,
        );

        assert!(service.is_err());
    }

    #[test]
    fn test_extra_params_are_encoded_as_query_string() {
        let params =
            std::collections::HashMap::from([("api-version".to_string(), json!("2024-02-01"))]);
        let request = Client::new()
            .post("https://example.com/embeddings")
            .query(&params)
            .build()
            .expect("request should build");

        assert_eq!(
            request.url().as_str(),
            "https://example.com/embeddings?api-version=2024-02-01"
        );
    }

    #[test]
    fn test_parse_retry_after_delta_seconds() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("retry-after", "30".parse().expect("valid header"));
        assert_eq!(parse_retry_after_ms(&headers), 30_000);
    }

    #[test]
    fn test_parse_retry_after_http_date() {
        let future = chrono::Utc::now() + chrono::Duration::seconds(90);
        let header_value = future.to_rfc2822();
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("retry-after", header_value.parse().expect("valid header"));

        let ms = parse_retry_after_ms(&headers);
        assert!((85_000..=95_000).contains(&ms), "expected ~90s, got {ms}ms");
    }

    #[test]
    fn test_parse_retry_after_missing_or_invalid_defaults() {
        let headers = reqwest::header::HeaderMap::new();
        assert_eq!(parse_retry_after_ms(&headers), 5_000);

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("retry-after", "not-a-date".parse().expect("valid header"));
        assert_eq!(parse_retry_after_ms(&headers), 5_000);
    }

    fn closed_port_service(
        circuit_breaker: Option<Arc<Mutex<CircuitBreaker>>>,
        retry_metrics: Option<Arc<cce_metrics::LlmRetryMetrics>>,
    ) -> HttpRequestService {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind test port");
        let addr = listener.local_addr().expect("local addr");
        drop(listener);

        let config = HttpRequestConfig {
            base_url: format!("http://{addr}"),
            api_keys: vec![],
            timeout_secs: 5,
            proxy_url: None,
            extra_headers: std::collections::HashMap::new(),
            extra_params: std::collections::HashMap::new(),
        };
        HttpRequestService::new(
            config,
            Arc::new(ConfigurableRateLimiter::new(60)),
            RetryPolicy::new(0, 10),
            circuit_breaker,
            retry_metrics,
        )
        .expect("service should build")
    }

    #[tokio::test]
    async fn test_circuit_breaker_opens_and_fast_fails() {
        let breaker = Arc::new(Mutex::new(CircuitBreaker::new(2, Duration::from_secs(60))));
        let service = closed_port_service(Some(breaker), None);

        let first = service
            .post_json::<_, serde_json::Value>("embeddings", &json!({"input": "x"}))
            .await;
        assert!(first.is_err());
        assert_eq!(
            service
                .circuit_breaker
                .as_ref()
                .expect("breaker present")
                .lock()
                .expect("breaker mutex poisoned")
                .state(),
            "closed",
        );

        let second = service
            .post_json::<_, serde_json::Value>("embeddings", &json!({"input": "x"}))
            .await;
        assert!(second.is_err());
        assert_eq!(
            service
                .circuit_breaker
                .as_ref()
                .expect("breaker present")
                .lock()
                .expect("breaker mutex poisoned")
                .state(),
            "open",
        );

        let rejected = service
            .post_json::<_, serde_json::Value>("embeddings", &json!({"input": "x"}))
            .await
            .expect_err("open circuit must reject the request");
        assert!(rejected.to_string().contains("Circuit breaker is open"));
    }

    #[tokio::test]
    async fn test_circuit_breaker_half_open_allows_probe() {
        let breaker = Arc::new(Mutex::new(CircuitBreaker::new(
            1,
            Duration::from_millis(50),
        )));
        let service = closed_port_service(Some(breaker), None);

        let _ = service
            .post_json::<_, serde_json::Value>("embeddings", &json!({"input": "x"}))
            .await;
        assert_eq!(
            service
                .circuit_breaker
                .as_ref()
                .expect("breaker present")
                .lock()
                .expect("breaker mutex poisoned")
                .state(),
            "open"
        );

        tokio::time::sleep(Duration::from_millis(60)).await;
        let probe = service
            .post_json::<_, serde_json::Value>("embeddings", &json!({"input": "x"}))
            .await;
        assert!(probe.is_err());
        let message = probe
            .expect_err("probe must fail against dead upstream")
            .to_string();
        assert!(!message.contains("Circuit breaker is open"));
    }

    #[tokio::test]
    async fn test_circuit_breaker_counts_metrics() {
        let registry = cce_metrics::MetricsRegistry::new();
        let metrics = cce_metrics::LlmRetryMetrics::new(&registry, "test-provider");
        let breaker = Arc::new(Mutex::new(CircuitBreaker::new(1, Duration::from_secs(60))));
        let service = closed_port_service(Some(breaker), Some(metrics));

        let _ = service
            .post_json::<_, serde_json::Value>("embeddings", &json!({"input": "x"}))
            .await;

        let snapshot = registry.export_all();
        let state = snapshot
            .metrics
            .iter()
            .find(|m| m.name == "llm_circuit_breaker_state")
            .expect("state gauge registered");
        assert!(matches!(
            state.value,
            cce_metrics::MetricData::FloatGauge(v) if (v - 1.0).abs() < f64::EPSILON
        ),);
    }
}
