//! Retry strategies for LLM requests
//!
//! Provides configurable retry policies with exponential backoff

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, warn};

use cce_llm::LlmRetryErrorClass;

use super::error::LlmError;
use cce_metrics::LlmRetryMetrics;

/// Retry policy configuration
#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    /// Maximum retry attempts
    max_retries: u32,
    /// Initial retry delay in milliseconds
    initial_delay_ms: u64,
    /// Maximum delay between retries
    max_delay_ms: u64,
    /// Backoff multiplier
    backoff_multiplier: f64,
    /// Retry on rate limit
    retry_on_rate_limit: bool,
    /// Random jitter ratio applied to delays
    jitter_ratio: f64,
    /// Maximum retry attempts for rate limit errors (429)
    rate_limit_max_retries: u32,
    /// Upper bound (ms) for the retry-after driven delay of rate limit errors
    rate_limit_max_delay_ms: u64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 5,
            initial_delay_ms: 1000,
            max_delay_ms: 30000,
            backoff_multiplier: 2.0,
            retry_on_rate_limit: true,
            jitter_ratio: 0.2,
            rate_limit_max_retries: 20,
            rate_limit_max_delay_ms: 60000,
        }
    }
}

impl RetryPolicy {
    /// Create a new retry policy
    pub fn new(max_retries: u32, initial_delay_ms: u64) -> Self {
        Self {
            max_retries,
            initial_delay_ms,
            ..Default::default()
        }
    }

    /// Set maximum delay
    pub fn with_max_delay(mut self, max_delay_ms: u64) -> Self {
        self.max_delay_ms = max_delay_ms;
        self
    }

    /// Set backoff multiplier
    pub fn with_backoff_multiplier(mut self, multiplier: f64) -> Self {
        self.backoff_multiplier = multiplier;
        self
    }

    /// Disable retry on rate limit
    pub fn without_rate_limit_retry(mut self) -> Self {
        self.retry_on_rate_limit = false;
        self
    }

    /// Set the random jitter ratio applied on top of computed delays
    pub fn with_jitter_ratio(mut self, ratio: f64) -> Self {
        self.jitter_ratio = ratio.max(0.0);
        self
    }

    /// Configure an independent retry budget for rate limit errors
    pub fn with_rate_limit_budget(mut self, max_retries: u32, max_delay_ms: u64) -> Self {
        self.rate_limit_max_retries = max_retries;
        self.rate_limit_max_delay_ms = max_delay_ms;
        self
    }

    /// Execute a function with retry logic
    pub async fn execute<F, Fut, T>(&self, f: F) -> Result<T, LlmError>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, LlmError>>,
    {
        self.execute_observed(None, f).await
    }

    /// Execute a function with retry logic and per-attempt metrics recording
    pub async fn execute_observed<F, Fut, T>(
        &self,
        metrics: Option<&Arc<LlmRetryMetrics>>,
        mut f: F,
    ) -> Result<T, LlmError>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, LlmError>>,
    {
        for attempt in 0..=self.max_attempts() {
            match f().await {
                Ok(result) => return Ok(result),
                Err(err) => {
                    let error_class = LlmRetryErrorClass::from_error(&err);

                    if !self.should_retry(&err) {
                        if let Some(metrics) = metrics {
                            metrics.record_failure(error_class.as_str());
                        }
                        return Err(err);
                    }

                    if attempt >= self.retry_budget(&err) {
                        if let Some(metrics) = metrics {
                            metrics.record_exhausted(error_class.as_str());
                        }
                        return Err(err);
                    }

                    let delay = self.calculate_delay_for(attempt, &err);
                    let delay_ms = delay.as_millis() as u64;

                    if let Some(metrics) = metrics {
                        metrics.record_retry(error_class.as_str(), delay_ms);
                    }

                    debug!(
                        attempt = attempt + 1,
                        max_retries = self.max_retries,
                        delay_ms = delay_ms,
                        "Retrying after error"
                    );

                    tokio::time::sleep(delay).await;
                }
            }
        }

        Err(LlmError::api("Max retries exceeded"))
    }

    /// Execute a function with retry and custom error handler
    pub async fn execute_with_handler<F, Fut, T, H>(
        &self,
        mut f: F,
        mut error_handler: H,
    ) -> Result<T, LlmError>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, LlmError>>,
        H: FnMut(&LlmError, u32),
    {
        for attempt in 0..=self.max_attempts() {
            match f().await {
                Ok(result) => return Ok(result),
                Err(err) => {
                    error_handler(&err, attempt);

                    if !self.should_retry(&err) {
                        return Err(err);
                    }

                    if attempt >= self.retry_budget(&err) {
                        return Err(err);
                    }

                    let delay = self.calculate_delay_for(attempt, &err);
                    tokio::time::sleep(delay).await;
                }
            }
        }

        Err(LlmError::api("Max retries exceeded"))
    }

    fn retry_budget(&self, error: &LlmError) -> u32 {
        match error {
            LlmError::RateLimitExceeded(_) => self.rate_limit_max_retries,
            _ => self.max_retries,
        }
    }

    fn max_attempts(&self) -> u32 {
        self.max_retries.max(self.rate_limit_max_retries)
    }

    fn should_retry(&self, error: &LlmError) -> bool {
        match error {
            LlmError::RateLimitExceeded(_) => self.retry_on_rate_limit,
            LlmError::Http(_) => true,
            LlmError::HttpStatus { status, .. } => (500..=599).contains(status),
            LlmError::Config(_) => false,
            LlmError::InvalidInput(_) => false,
            LlmError::Auth(_) => false,
            LlmError::ModelNotFound(_) => false,
            LlmError::Api(_) => false,
            LlmError::InvalidResponse(_) => true,
            LlmError::TokenLimitExceeded(_, _) => false,
            LlmError::Timeout(_) => true,
            LlmError::Internal(_) => false,
        }
    }

    fn calculate_delay_for(&self, attempt: u32, error: &LlmError) -> Duration {
        let base_ms = match error {
            LlmError::RateLimitExceeded(retry_after) => (self.calculate_delay(attempt).as_millis()
                as u64)
                .max(*retry_after)
                .min(self.rate_limit_max_delay_ms),
            _ => self.calculate_delay(attempt).as_millis() as u64,
        };

        let jitter = fastrand::f64() * self.jitter_ratio;
        Duration::from_millis((base_ms as f64 * (1.0 + jitter)) as u64)
    }

    fn calculate_delay(&self, attempt: u32) -> Duration {
        let delay_ms = (self.initial_delay_ms as f64 * self.backoff_multiplier.powi(attempt as i32))
            .min(self.max_delay_ms as f64) as u64;

        Duration::from_millis(delay_ms)
    }

    /// Get maximum retries
    pub fn max_retries(&self) -> u32 {
        self.max_retries
    }
}

/// Fixed interval retry policy
#[derive(Debug, Clone, Copy)]
pub struct FixedIntervalPolicy {
    max_retries: u32,
    interval_ms: u64,
}

impl FixedIntervalPolicy {
    /// Create a new fixed interval policy
    pub fn new(max_retries: u32, interval_ms: u64) -> Self {
        Self {
            max_retries,
            interval_ms,
        }
    }

    /// Execute with fixed interval retry
    pub async fn execute<F, Fut, T>(&self, mut f: F) -> Result<T, LlmError>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, LlmError>>,
    {
        for attempt in 0..=self.max_retries {
            match f().await {
                Ok(result) => return Ok(result),
                Err(err) => {
                    if attempt == self.max_retries {
                        return Err(err);
                    }
                    warn!(
                        attempt = attempt + 1,
                        max_retries = self.max_retries,
                        interval_ms = self.interval_ms,
                        "Retrying with fixed interval"
                    );
                    tokio::time::sleep(Duration::from_millis(self.interval_ms)).await;
                }
            }
        }

        Err(LlmError::api("Max retries exceeded"))
    }
}

/// No retry policy
#[derive(Debug, Clone, Copy)]
pub struct NoRetry;

impl NoRetry {
    /// Execute without retry
    pub async fn execute<F, Fut, T>(f: F) -> Result<T, LlmError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, LlmError>>,
    {
        f().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::error::common;

    #[tokio::test]
    async fn test_retry_policy_success() {
        let policy = RetryPolicy::default();
        let attempts = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let attempts_clone = attempts.clone();

        let result = policy
            .execute(|| async {
                attempts_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok::<_, LlmError>("success")
            })
            .await;

        assert!(result.is_ok());
        assert_eq!(attempts.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_retry_policy_eventual_success() {
        let policy = RetryPolicy::new(3, 10);
        let attempts = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let attempts_clone = attempts.clone();

        let result = policy
            .execute(|| async {
                let count = attempts_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if count < 2 {
                    Err(LlmError::Http(common::HttpError(
                        "temporary error".to_string(),
                    )))
                } else {
                    Ok("success")
                }
            })
            .await;

        assert!(result.is_ok());
        assert_eq!(attempts.load(std::sync::atomic::Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_policy_no_retry_on_config_error() {
        let policy = RetryPolicy::default();
        let attempts = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let attempts_clone = attempts.clone();

        let result = policy
            .execute(|| async {
                attempts_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Err::<(), _>(LlmError::config("invalid config"))
            })
            .await;

        assert!(result.is_err());
        assert_eq!(attempts.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_retry_policy_max_retries_exceeded() {
        let policy = RetryPolicy::new(2, 10);
        let attempts = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let attempts_clone = attempts.clone();

        let result = policy
            .execute(|| async {
                attempts_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Err::<(), _>(LlmError::Http(common::HttpError(
                    "500 Internal Server Error".to_string(),
                )))
            })
            .await;

        assert!(result.is_err());
        assert_eq!(attempts.load(std::sync::atomic::Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_no_retry_policy() {
        let attempts = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let attempts_clone = attempts.clone();

        let result = NoRetry::execute(|| async {
            attempts_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Err::<(), _>(LlmError::http("error"))
        })
        .await;

        assert!(result.is_err());
        assert_eq!(attempts.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_retry_policy_observed_metrics() {
        use cce_metrics::MetricData;
        use std::sync::Arc;

        let registry = cce_metrics::MetricsRegistry::new();
        let metrics = Arc::new(cce_metrics::LlmRetryMetrics::new(
            &registry,
            "test-provider",
        ));
        let attempts = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let attempts_clone = attempts.clone();

        let policy = RetryPolicy::new(2, 10);
        let result = policy
            .execute_observed(Some(&metrics), || async {
                attempts_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Err::<(), _>(LlmError::http("503 Service Unavailable"))
            })
            .await;

        assert!(result.is_err());
        assert_eq!(attempts.load(std::sync::atomic::Ordering::SeqCst), 3);

        let snapshot = registry.export_all();
        let retry_total: u64 = snapshot
            .metrics
            .iter()
            .filter(|m| m.name == "llm_retry_total")
            .filter_map(|m| match m.value {
                MetricData::Counter(v) => Some(v),
                _ => None,
            })
            .sum();
        assert_eq!(retry_total, 2);

        let exhausted_total: u64 = snapshot
            .metrics
            .iter()
            .filter(|m| m.name == "llm_retry_exhausted_total")
            .filter_map(|m| match m.value {
                MetricData::Counter(v) => Some(v),
                _ => None,
            })
            .sum();
        assert_eq!(exhausted_total, 1);
    }
}
