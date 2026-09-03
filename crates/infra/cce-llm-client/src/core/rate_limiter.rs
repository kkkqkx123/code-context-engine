//! Rate limiter for LLM requests
//!
//! Provides token bucket and retry-after based rate limiting
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, warn};

/// Rate limit state
#[derive(Debug)]
struct RateLimitState {
    /// Whether currently rate limited
    is_limited: AtomicBool,
    /// Reset time for rate limit
    reset_time: RwLock<Option<Instant>>,
    /// Consecutive error count
    consecutive_errors: AtomicU64,
}

impl Default for RateLimitState {
    fn default() -> Self {
        Self {
            is_limited: AtomicBool::new(false),
            reset_time: RwLock::new(None),
            consecutive_errors: AtomicU64::new(0),
        }
    }
}

/// Rate limiter for LLM API requests
#[derive(Debug, Clone)]
pub struct RateLimiter {
    state: Arc<RateLimitState>,
    /// Maximum consecutive errors before cooling down
    max_consecutive_errors: u64,
    /// Cool down duration after max errors
    cool_down_duration: Duration,
    /// Maximum random stagger added when a rate-limit window expires.
    max_stagger_ms: u64,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self {
            state: Arc::new(RateLimitState::default()),
            max_consecutive_errors: 5,
            cool_down_duration: Duration::from_secs(30),
            max_stagger_ms: 1000,
        }
    }
}

impl RateLimiter {
    /// Create a new rate limiter with custom settings
    pub fn new(max_consecutive_errors: u64, cool_down_seconds: u64) -> Self {
        Self {
            state: Arc::new(RateLimitState::default()),
            max_consecutive_errors,
            cool_down_duration: Duration::from_secs(cool_down_seconds),
            max_stagger_ms: 1000,
        }
    }

    /// Set the maximum random stagger added on rate-limit window release
    pub fn with_stagger(mut self, max_stagger_ms: u64) -> Self {
        self.max_stagger_ms = max_stagger_ms;
        self
    }

    /// Wait for rate limit to clear
    pub async fn wait(&self) {
        loop {
            if !self.state.is_limited.load(Ordering::Relaxed) {
                break;
            }

            let reset_time = *self.state.reset_time.read().await;
            let Some(reset) = reset_time else {
                break;
            };

            let now = Instant::now();
            if now < reset {
                let wait_duration = reset - now;
                let stagger = Duration::from_millis(fastrand::u64(0..=self.max_stagger_ms));
                debug!(
                    wait_ms = wait_duration.as_millis(),
                    stagger_ms = stagger.as_millis(),
                    "Waiting for rate limit"
                );
                tokio::time::sleep(wait_duration + stagger).await;
                continue;
            }

            let mut guard = self.state.reset_time.write().await;
            if self.state.is_limited.load(Ordering::Relaxed) {
                if let Some(current) = *guard {
                    if Instant::now() >= current {
                        self.state.is_limited.store(false, Ordering::Relaxed);
                        *guard = None;
                        debug!("Rate limit window expired");
                    }
                }
            }
        }
    }

    /// Set rate limit from retry-after header
    pub async fn set_rate_limit(&self, retry_after_ms: u64) {
        let reset_time = Instant::now() + Duration::from_millis(retry_after_ms);
        let mut guard = self.state.reset_time.write().await;
        self.state.is_limited.store(true, Ordering::Relaxed);
        *guard = Some(reset_time);
        let errors = self
            .state
            .consecutive_errors
            .fetch_add(1, Ordering::Relaxed)
            + 1;
        warn!(
            retry_after_ms = retry_after_ms,
            consecutive_errors = errors,
            "Rate limit activated"
        );
    }

    /// Reset rate limit state on successful request
    pub async fn reset(&self) {
        if self.state.is_limited.load(Ordering::Relaxed) {
            let mut guard = self.state.reset_time.write().await;
            self.state.is_limited.store(false, Ordering::Relaxed);
            *guard = None;
            self.state.consecutive_errors.store(0, Ordering::Relaxed);
            debug!("Rate limit reset");
        }
    }

    /// Check if should cool down due to too many errors
    pub async fn check_cool_down(&self) -> Option<Duration> {
        let errors = self.state.consecutive_errors.load(Ordering::Relaxed);
        if errors >= self.max_consecutive_errors {
            warn!(
                consecutive_errors = errors,
                cool_down_seconds = self.cool_down_duration.as_secs(),
                "Entering cool down due to consecutive errors"
            );
            self.state.consecutive_errors.store(0, Ordering::Relaxed);
            Some(self.cool_down_duration)
        } else {
            None
        }
    }

    /// Get current consecutive error count
    pub fn consecutive_errors(&self) -> u64 {
        self.state.consecutive_errors.load(Ordering::Relaxed)
    }

    /// Check if currently rate limited
    pub fn is_limited(&self) -> bool {
        self.state.is_limited.load(Ordering::Relaxed)
    }
}

/// Token bucket rate limiter for more fine-grained control
#[derive(Debug)]
pub struct TokenBucket {
    state: std::sync::Mutex<TokenBucketState>,
}

#[derive(Debug)]
struct TokenBucketState {
    /// Current tokens
    tokens: f64,
    /// Maximum tokens
    max_tokens: f64,
    /// Tokens per second refill rate
    refill_rate: f64,
    /// Last refill time
    last_refill: Instant,
}

impl Clone for TokenBucket {
    fn clone(&self) -> Self {
        let state = self.state.lock().expect("token bucket mutex poisoned");
        Self {
            state: std::sync::Mutex::new(TokenBucketState {
                tokens: state.tokens,
                max_tokens: state.max_tokens,
                refill_rate: state.refill_rate,
                last_refill: state.last_refill,
            }),
        }
    }
}

impl TokenBucket {
    /// Create a new token bucket
    pub fn new(max_tokens: f64, refill_rate: f64) -> Self {
        Self {
            state: std::sync::Mutex::new(TokenBucketState {
                tokens: max_tokens,
                max_tokens,
                refill_rate,
                last_refill: Instant::now(),
            }),
        }
    }

    /// Try to consume tokens, returns wait time if not enough
    pub fn try_consume(&self, tokens: f64) -> Option<Duration> {
        self.refill();

        let mut state = self.state.lock().expect("token bucket mutex poisoned");
        if state.tokens >= tokens {
            state.tokens -= tokens;
            None
        } else {
            let needed = tokens - state.tokens;
            let wait_seconds = needed / state.refill_rate;
            Some(Duration::from_secs_f64(wait_seconds))
        }
    }

    /// Refill tokens based on elapsed time
    fn refill(&self) {
        let mut state = self.state.lock().expect("token bucket mutex poisoned");
        let now = Instant::now();
        let elapsed = now.duration_since(state.last_refill).as_secs_f64();

        if elapsed > 0.0 {
            state.tokens = (state.tokens + elapsed * state.refill_rate).min(state.max_tokens);
            state.last_refill = now;
        }
    }

    /// Update the bucket capacity and refill rate at runtime.
    pub fn update_rate(&self, max_tokens: f64, refill_rate: f64) {
        let mut state = self.state.lock().expect("token bucket mutex poisoned");
        state.max_tokens = max_tokens;
        state.refill_rate = refill_rate;
        state.tokens = state.tokens.min(max_tokens);
    }

    /// Get the current maximum token count
    pub fn max_tokens(&self) -> f64 {
        self.state
            .lock()
            .expect("token bucket mutex poisoned")
            .max_tokens
    }

    /// Get the current refill rate (tokens per second)
    pub fn refill_rate(&self) -> f64 {
        self.state
            .lock()
            .expect("token bucket mutex poisoned")
            .refill_rate
    }

    /// Get current token count
    pub fn tokens(&self) -> f64 {
        self.refill();
        self.state
            .lock()
            .expect("token bucket mutex poisoned")
            .tokens
    }
}

/// Configurable rate limiter combining proactive token bucket and reactive adaptive limiting
#[derive(Debug)]
pub struct ConfigurableRateLimiter {
    /// Token bucket for proactive rate limiting
    token_bucket: std::sync::Mutex<Option<TokenBucket>>,

    /// Adaptive rate limiter for server responses (429 handling)
    adaptive_limiter: RateLimiter,
}

impl Clone for ConfigurableRateLimiter {
    fn clone(&self) -> Self {
        let bucket = self
            .token_bucket
            .lock()
            .expect("token bucket mutex poisoned");
        Self {
            token_bucket: std::sync::Mutex::new(bucket.clone()),
            adaptive_limiter: self.adaptive_limiter.clone(),
        }
    }
}

impl ConfigurableRateLimiter {
    /// Create a new configurable rate limiter
    pub fn new(rate_limit_per_minute: u32) -> Self {
        let token_bucket = if rate_limit_per_minute > 0 {
            let max_tokens = rate_limit_per_minute as f64;
            let refill_rate = rate_limit_per_minute as f64 / 60.0;
            Some(TokenBucket::new(max_tokens, refill_rate))
        } else {
            None
        };

        Self {
            token_bucket: std::sync::Mutex::new(token_bucket),
            adaptive_limiter: RateLimiter::default(),
        }
    }

    /// Wait for rate limit clearance (both proactive and reactive)
    pub async fn wait(&self) {
        self.adaptive_limiter.wait().await;

        if let Some(cool_down) = self.adaptive_limiter.check_cool_down().await {
            warn!(
                cool_down_secs = cool_down.as_secs(),
                "Entering cool down due to consecutive errors"
            );
            tokio::time::sleep(cool_down).await;
        }

        loop {
            let wait_time = {
                let bucket = self
                    .token_bucket
                    .lock()
                    .expect("token bucket mutex poisoned");
                match bucket.as_ref() {
                    None => break,
                    Some(bucket) => bucket.try_consume(1.0),
                }
            };
            match wait_time {
                None => break,
                Some(wait_time) => {
                    debug!(
                        wait_ms = wait_time.as_millis(),
                        "Waiting for rate limit token"
                    );
                    tokio::time::sleep(wait_time).await;
                }
            }
        }
    }

    /// Call on successful request to reset adaptive limiter
    pub async fn on_success(&self) {
        self.adaptive_limiter.reset().await;
    }

    /// Call when receiving 429 rate limit response
    pub async fn on_rate_limit(&self, retry_after_ms: u64) {
        self.adaptive_limiter.set_rate_limit(retry_after_ms).await;
    }

    /// Check if currently rate limited (adaptive layer only)
    pub fn is_limited(&self) -> bool {
        self.adaptive_limiter.is_limited()
    }

    /// Update the proactive rate limit at runtime.
    pub fn update_rate_limit(&self, rate_per_minute: u32) {
        if rate_per_minute == 0 {
            return;
        }
        let max_tokens = rate_per_minute as f64;
        let refill_rate = rate_per_minute as f64 / 60.0;
        let mut bucket = self
            .token_bucket
            .lock()
            .expect("token bucket mutex poisoned");
        match bucket.as_ref() {
            Some(bucket) => bucket.update_rate(max_tokens, refill_rate),
            None => {
                *bucket = Some(TokenBucket::new(max_tokens, refill_rate));
            }
        }
    }

    /// Current configured rate limit (requests per minute), 0 when unlimited.
    pub fn rate_limit_per_minute(&self) -> u32 {
        let bucket = self
            .token_bucket
            .lock()
            .expect("token bucket mutex poisoned");
        match bucket.as_ref() {
            Some(bucket) => bucket.max_tokens() as u32,
            None => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rate_limiter() {
        let limiter = RateLimiter::default();
        let start = Instant::now();
        limiter.wait().await;
        assert!(start.elapsed() < Duration::from_millis(50));
    }

    #[tokio::test]
    async fn test_rate_limit() {
        let limiter = RateLimiter::default();
        limiter.set_rate_limit(100).await;
        assert!(limiter.is_limited());
        limiter.wait().await;
        assert!(!limiter.is_limited());
    }

    #[tokio::test]
    async fn test_reset() {
        let limiter = RateLimiter::default();
        limiter.set_rate_limit(1000).await;
        assert_eq!(limiter.consecutive_errors(), 1);
        limiter.reset().await;
        assert!(!limiter.is_limited());
        assert_eq!(limiter.consecutive_errors(), 0);
    }

    #[tokio::test]
    async fn test_token_bucket() {
        let bucket = TokenBucket::new(10.0, 5.0);
        assert!(bucket.try_consume(5.0).is_none());

        let started = Instant::now();
        let tokens = bucket.tokens();
        let elapsed = started.elapsed().as_secs_f64();

        let min_tokens = 5.0;
        let max_tokens = 5.0 + elapsed * bucket.refill_rate() + 0.001;
        assert!(
            (min_tokens..=max_tokens).contains(&tokens),
            "Expected tokens within [{min_tokens}, {max_tokens}] after {elapsed}s, got {tokens}"
        );

        let wait = bucket.try_consume(bucket.max_tokens() + 1.0);
        assert!(wait.is_some());
    }

    #[tokio::test]
    async fn test_configurable_rate_limiter_creation() {
        let limiter = ConfigurableRateLimiter::new(60);
        assert!(!limiter.is_limited());
        let limiter_no_limit = ConfigurableRateLimiter::new(0);
        assert!(!limiter_no_limit.is_limited());
    }
}
