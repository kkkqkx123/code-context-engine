//! Registry of shared rate limiters

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use cce_config::modules::CircuitBreakerConfig;

use crate::core::rate_limiter::ConfigurableRateLimiter;
use cce_circuit_breaker::CircuitBreaker;

/// Shared rate limiter registry keyed by upstream base URL
#[derive(Debug, Default)]
pub struct LlmRateLimiterRegistry {
    limiters: Mutex<HashMap<String, Arc<ConfigurableRateLimiter>>>,
    circuit_breakers: Mutex<HashMap<String, Arc<Mutex<CircuitBreaker>>>>,
}

impl LlmRateLimiterRegistry {
    /// Get (or create) the rate limiter for an upstream base URL.
    pub fn limiter_for(&self, base_url: &str, rate_limit: u32) -> Arc<ConfigurableRateLimiter> {
        let mut guard = self
            .limiters
            .lock()
            .expect("rate limiter registry mutex poisoned");
        if let Some(existing) = guard.get(base_url) {
            let current = existing.rate_limit_per_minute();
            let effective = match (current, rate_limit) {
                (current, 0) => current,
                (0, new) => new,
                (current, new) => current.min(new),
            };
            existing.update_rate_limit(effective);
            return existing.clone();
        }

        let limiter = Arc::new(ConfigurableRateLimiter::new(rate_limit));
        guard.insert(base_url.to_string(), limiter.clone());
        limiter
    }

    /// Get (or create) the circuit breaker for an upstream base URL.
    pub fn circuit_breaker_for(
        &self,
        base_url: &str,
        config: &CircuitBreakerConfig,
    ) -> Option<Arc<Mutex<CircuitBreaker>>> {
        if !config.enabled {
            return None;
        }

        let mut guard = self
            .circuit_breakers
            .lock()
            .expect("circuit breaker registry mutex poisoned");
        if let Some(existing) = guard.get(base_url) {
            return Some(existing.clone());
        }

        let breaker = Arc::new(Mutex::new(CircuitBreaker::new(
            config.failure_threshold,
            Duration::from_secs(config.recovery_timeout_secs),
        )));
        guard.insert(base_url.to_string(), breaker.clone());
        Some(breaker)
    }
}

static GLOBAL: OnceLock<LlmRateLimiterRegistry> = OnceLock::new();

/// Process-wide registry shared by all client factories
pub fn global_rate_limiter_registry() -> &'static LlmRateLimiterRegistry {
    GLOBAL.get_or_init(LlmRateLimiterRegistry::default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_same_base_url_returns_same_limiter() {
        let registry = LlmRateLimiterRegistry::default();
        let first = registry.limiter_for("https://api.example.com", 60);
        let second = registry.limiter_for("https://api.example.com", 30);

        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(second.rate_limit_per_minute(), 30);
    }

    #[test]
    fn test_rate_converges_to_stricter_rate() {
        let registry = LlmRateLimiterRegistry::default();
        let limiter = registry.limiter_for("https://api.example.com", 60);

        registry.limiter_for("https://api.example.com", 30);
        assert_eq!(limiter.rate_limit_per_minute(), 30);

        registry.limiter_for("https://api.example.com", 90);
        assert_eq!(limiter.rate_limit_per_minute(), 30);
    }

    #[test]
    fn test_zero_rate_limit_never_relaxes_existing_bucket() {
        let registry = LlmRateLimiterRegistry::default();
        let limiter = registry.limiter_for("https://api.example.com", 60);

        registry.limiter_for("https://api.example.com", 0);
        assert_eq!(limiter.rate_limit_per_minute(), 60);
    }

    #[test]
    fn test_different_base_urls_have_distinct_limiters() {
        let registry = LlmRateLimiterRegistry::default();
        let first = registry.limiter_for("https://api-a.example.com", 60);
        let second = registry.limiter_for("https://api-b.example.com", 60);

        assert!(!Arc::ptr_eq(&first, &second));
    }

    #[test]
    fn test_same_base_url_returns_same_circuit_breaker() {
        let registry = LlmRateLimiterRegistry::default();
        let config = CircuitBreakerConfig::default();
        let first = registry.circuit_breaker_for("https://api.example.com", &config);
        let second = registry.circuit_breaker_for("https://api.example.com", &config);

        let first = first.expect("breaker should exist");
        let second = second.expect("breaker should exist");
        assert!(Arc::ptr_eq(&first, &second));
    }

    #[test]
    fn test_disabled_circuit_breaker_returns_none() {
        let registry = LlmRateLimiterRegistry::default();
        let config = CircuitBreakerConfig {
            enabled: false,
            ..Default::default()
        };
        assert!(
            registry
                .circuit_breaker_for("https://api.example.com", &config)
                .is_none()
        );
    }

    #[test]
    fn test_circuit_breaker_settings_from_first_registration() {
        let registry = LlmRateLimiterRegistry::default();
        let strict = CircuitBreakerConfig {
            failure_threshold: 2,
            recovery_timeout_secs: 30,
            ..Default::default()
        };
        let breaker = registry
            .circuit_breaker_for("https://api.example.com", &strict)
            .expect("breaker should exist");

        let mut breaker = breaker.lock().expect("breaker mutex poisoned");
        for _ in 0..2 {
            breaker.record_failure();
        }
        assert_eq!(breaker.state(), "open");
    }
}
