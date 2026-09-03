//! Circuit breaker pattern for handling repeated failures
//!
//! Provides a generic circuit breaker that can be used with any error type
//! implementing the `CircuitBreakerRejected` trait.
//!
//! # Metrics
//!
//! Use [`CircuitBreakerMetrics`] to monitor circuit breaker state transitions,
//! rejections, successes, and failures.

use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tracing::{debug, warn};

/// Trait for errors that support circuit breaker rejection.
///
/// Allows the generic `CircuitBreaker` to work with any error type
/// that can represent a "circuit is open" rejection.
pub trait CircuitBreakerRejected: Sized {
    /// Create an error representing a circuit breaker rejection
    fn circuit_open(message: impl Into<String>) -> Self;
}

/// Metrics for monitoring circuit breaker behavior.
///
/// Uses atomic counters for lock-free concurrent updates from the
/// circuit breaker's hot path.
#[derive(Debug)]
pub struct CircuitBreakerMetrics {
    state_changes_total: AtomicU64,
    rejections_total: AtomicU64,
    successes_total: AtomicU64,
    failures_total: AtomicU64,
}

impl CircuitBreakerMetrics {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            state_changes_total: AtomicU64::new(0),
            rejections_total: AtomicU64::new(0),
            successes_total: AtomicU64::new(0),
            failures_total: AtomicU64::new(0),
        })
    }

    pub fn record_state_change(&self) {
        self.state_changes_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_rejection(&self) {
        self.rejections_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_success(&self) {
        self.successes_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_failure(&self) {
        self.failures_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn state_changes_total(&self) -> u64 {
        self.state_changes_total.load(Ordering::Relaxed)
    }

    pub fn rejections_total(&self) -> u64 {
        self.rejections_total.load(Ordering::Relaxed)
    }
}

impl Default for CircuitBreakerMetrics {
    fn default() -> Self {
        Self {
            state_changes_total: AtomicU64::new(0),
            rejections_total: AtomicU64::new(0),
            successes_total: AtomicU64::new(0),
            failures_total: AtomicU64::new(0),
        }
    }
}

/// Circuit breaker pattern for handling repeated failures
#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    /// Failure threshold before opening circuit
    failure_threshold: u32,
    /// Success threshold to close circuit
    success_threshold: u32,
    /// Timeout before attempting to close circuit
    timeout: Duration,
    /// Current state
    state: CircuitState,
    /// Current failure count
    failure_count: u32,
    /// Current success count
    success_count: u32,
    /// Last failure time
    last_failure: Option<std::time::Instant>,
    /// Optional metrics collector
    metrics: Option<Arc<CircuitBreakerMetrics>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

impl CircuitBreaker {
    /// Create a new circuit breaker
    pub fn new(failure_threshold: u32, timeout: Duration) -> Self {
        Self {
            failure_threshold,
            success_threshold: 2,
            timeout,
            state: CircuitState::Closed,
            failure_count: 0,
            success_count: 0,
            last_failure: None,
            metrics: None,
        }
    }

    /// Attach metrics collector
    pub fn with_metrics(mut self, metrics: Arc<CircuitBreakerMetrics>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Execute with circuit breaker protection
    ///
    /// Works with any error type that implements `CircuitBreakerRejected`.
    pub async fn execute<F, Fut, T, E>(&mut self, f: F) -> Result<T, E>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, E>>,
        E: CircuitBreakerRejected,
    {
        self.try_acquire()?;

        match f().await {
            Ok(result) => {
                self.on_success();
                Ok(result)
            }
            Err(err) => {
                self.on_failure();
                Err(err)
            }
        }
    }

    /// Check whether a request may proceed; returns the rejection error when
    /// the circuit is open and the recovery timeout has not elapsed.
    ///
    /// Half-open requests are allowed through as recovery probes. This is the
    /// non-future variant of the guard inside [`Self::execute`], intended for
    /// callers that run the operation through their own retry loop and only
    /// want the fast-fail gate.
    pub fn try_acquire<E: CircuitBreakerRejected>(&mut self) -> Result<(), E> {
        match self.state {
            CircuitState::Open => {
                if let Some(last) = self.last_failure {
                    if last.elapsed() >= self.timeout {
                        debug!("Circuit breaker entering half-open state");
                        self.transition_to(CircuitState::HalfOpen);
                    } else {
                        if let Some(ref metrics) = self.metrics {
                            metrics.record_rejection();
                        }
                        return Err(E::circuit_open("Circuit breaker is open"));
                    }
                }
            }
            CircuitState::HalfOpen | CircuitState::Closed => {}
        }
        Ok(())
    }

    /// Record a successful execution
    pub fn record_success(&mut self) {
        self.on_success();
    }

    /// Record a failed execution
    pub fn record_failure(&mut self) {
        self.on_failure();
    }

    fn transition_to(&mut self, new_state: CircuitState) {
        if self.state != new_state {
            if let Some(ref metrics) = self.metrics {
                metrics.record_state_change();
            }
            self.state = new_state;
            if new_state == CircuitState::HalfOpen {
                self.success_count = 0;
            }
        }
    }

    fn on_success(&mut self) {
        match self.state {
            CircuitState::HalfOpen => {
                self.success_count += 1;
                if self.success_count >= self.success_threshold {
                    debug!("Circuit breaker closed");
                    self.transition_to(CircuitState::Closed);
                    self.failure_count = 0;
                }
            }
            CircuitState::Closed => {
                if self.failure_count > 0 {
                    self.failure_count = 0;
                }
            }
            CircuitState::Open => {}
        }
        if let Some(ref metrics) = self.metrics {
            metrics.record_success();
        }
    }

    fn on_failure(&mut self) {
        self.failure_count += 1;
        self.last_failure = Some(std::time::Instant::now());

        if self.state == CircuitState::HalfOpen || self.failure_count >= self.failure_threshold {
            warn!(
                failure_count = self.failure_count,
                "Circuit breaker opened due to failures"
            );
            self.transition_to(CircuitState::Open);
        }
        if let Some(ref metrics) = self.metrics {
            metrics.record_failure();
        }
    }

    /// Get current state
    pub fn state(&self) -> &str {
        match self.state {
            CircuitState::Closed => "closed",
            CircuitState::Open => "open",
            CircuitState::HalfOpen => "half-open",
        }
    }

    /// Numeric circuit state for metrics: 0=closed, 0.5=half-open, 1=open
    pub fn state_value(&self) -> f64 {
        match self.state {
            CircuitState::Closed => 0.0,
            CircuitState::HalfOpen => 0.5,
            CircuitState::Open => 1.0,
        }
    }

    /// Check if circuit is open
    pub fn is_open(&self) -> bool {
        self.state == CircuitState::Open
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(dead_code)]
    struct TestError(String);

    impl CircuitBreakerRejected for TestError {
        fn circuit_open(message: impl Into<String>) -> Self {
            TestError(message.into())
        }
    }

    #[tokio::test]
    async fn test_circuit_breaker_initial_state() {
        let breaker = CircuitBreaker::new(2, Duration::from_millis(100));
        assert_eq!(breaker.state(), "closed");
        assert!(!breaker.is_open());
    }

    #[tokio::test]
    async fn test_circuit_breaker_state_transitions() {
        let mut breaker = CircuitBreaker::new(2, Duration::from_millis(50));

        assert_eq!(breaker.state(), "closed");

        breaker
            .execute(|| async { Err::<(), _>(TestError("error".to_string())) })
            .await
            .ok();
        breaker
            .execute(|| async { Err::<(), _>(TestError("error".to_string())) })
            .await
            .ok();
        assert_eq!(breaker.state(), "open");
        assert!(breaker.is_open());

        let result = breaker.execute(|| async { Ok::<(), TestError>(()) }).await;
        assert!(result.is_err());

        tokio::time::sleep(Duration::from_millis(60)).await;

        let result = breaker.execute(|| async { Ok::<(), TestError>(()) }).await;
        assert!(result.is_ok());

        let result = breaker.execute(|| async { Ok::<(), TestError>(()) }).await;
        assert!(result.is_ok());

        assert_eq!(breaker.state(), "closed");
        assert!(!breaker.is_open());
    }

    #[tokio::test]
    async fn test_circuit_breaker_half_open_to_open() {
        let mut breaker = CircuitBreaker::new(2, Duration::from_millis(50));

        breaker
            .execute(|| async { Err::<(), _>(TestError("error".to_string())) })
            .await
            .ok();
        breaker
            .execute(|| async { Err::<(), _>(TestError("error".to_string())) })
            .await
            .ok();
        assert!(breaker.is_open());

        tokio::time::sleep(Duration::from_millis(60)).await;

        breaker
            .execute(|| async { Err::<(), _>(TestError("error".to_string())) })
            .await
            .ok();
        assert!(breaker.is_open());
        assert_eq!(breaker.state(), "open");
    }

    #[tokio::test]
    async fn test_circuit_breaker_half_open_success_threshold() {
        let mut breaker = CircuitBreaker::new(1, Duration::from_millis(50));

        breaker
            .execute(|| async { Err::<(), _>(TestError("error".to_string())) })
            .await
            .ok();
        assert_eq!(breaker.state(), "open");

        tokio::time::sleep(Duration::from_millis(60)).await;

        // Half-open: one success should close (success_threshold = 2)
        breaker
            .execute(|| async { Ok::<(), TestError>(()) })
            .await
            .ok();
        // Should still be half-open after 1 success
        assert_eq!(breaker.state(), "half-open");

        breaker
            .execute(|| async { Ok::<(), TestError>(()) })
            .await
            .ok();
        assert_eq!(breaker.state(), "closed");
    }
}
