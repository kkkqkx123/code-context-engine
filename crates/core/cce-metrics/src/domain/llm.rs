//! LLM retry and circuit breaker metrics
//!
//! Tracks retry behavior against LLM upstreams: per-error-class retry counts,
//! accumulated retry waiting time, retries exhausted, and circuit breaker
//! state/transitions. All metrics are labeled by provider.
//!
//! # Design Principles
//!
//! - **Provider Labeling**: Every metric carries the `provider` label so
//!   upstreams can be compared side by side.
//! - **Lazy Counters**: Per-error-type counters are created on first use and
//!   cached in a `DashMap` (same pattern as `EmbeddingMetrics`).
//! - **Type Safety**: Named methods prevent metric naming inconsistencies.

use std::sync::Arc;

use dashmap::DashMap;

use crate::{LabeledCounter, LabeledFloatGauge, MetricsRegistry};

/// Retry and circuit breaker metrics for LLM upstreams, labeled by provider.
#[derive(Debug)]
pub struct LlmRetryMetrics {
    /// Per-error-type retry counters (`llm_retry_total{provider, error_type}`)
    retry_total: Arc<DashMap<String, LabeledCounter>>,
    /// Accumulated waiting time for rate-limit retries (`llm_retry_wait_ms_total{provider}`)
    retry_wait_ms_total: LabeledCounter,
    /// Per-error-type counters for retries exhausted (`llm_retry_exhausted_total{provider, error_type}`)
    exhausted_total: Arc<DashMap<String, LabeledCounter>>,
    /// Per-error-type counters for final non-retried failures (`llm_retry_failures_total{provider, error_type}`)
    failures_total: Arc<DashMap<String, LabeledCounter>>,
    /// Circuit breaker state gauge: 0=closed, 0.5=half-open, 1=open (`llm_circuit_breaker_state{provider}`)
    circuit_breaker_state: LabeledFloatGauge,
    /// Circuit breaker state transitions (`llm_circuit_breaker_transitions_total{provider}`)
    circuit_breaker_transitions_total: LabeledCounter,
    /// Circuit breaker rejections (`llm_circuit_breaker_rejections_total{provider}`)
    circuit_breaker_rejections_total: LabeledCounter,
    /// Provider label for counter labels
    provider_label: String,
    /// Registry for lazy counter creation
    registry: MetricsRegistry,
}

impl LlmRetryMetrics {
    /// Create LLM retry metrics with the given registry and provider label
    ///
    /// # Arguments
    ///
    /// * `registry` - The global metrics registry
    /// * `provider_label` - Label identifying the LLM provider (e.g., "openai", "deepseek")
    pub fn new(registry: &MetricsRegistry, provider_label: &str) -> Arc<Self> {
        let prov = provider_label.to_string();
        Arc::new(Self {
            retry_total: Arc::new(DashMap::new()),
            retry_wait_ms_total: registry
                .counter("llm_retry_wait_ms_total", &[("provider", provider_label)]),
            exhausted_total: Arc::new(DashMap::new()),
            failures_total: Arc::new(DashMap::new()),
            circuit_breaker_state: registry
                .float_gauge("llm_circuit_breaker_state", &[("provider", provider_label)]),
            circuit_breaker_transitions_total: registry.counter(
                "llm_circuit_breaker_transitions_total",
                &[("provider", provider_label)],
            ),
            circuit_breaker_rejections_total: registry.counter(
                "llm_circuit_breaker_rejections_total",
                &[("provider", provider_label)],
            ),
            provider_label: prov,
            registry: registry.clone(),
        })
    }

    fn get_or_create_counter(
        map: &Arc<DashMap<String, LabeledCounter>>,
        registry: &MetricsRegistry,
        provider: &str,
        name: &str,
        error_type: &str,
    ) -> LabeledCounter {
        let entry = map.entry(error_type.to_string()).or_insert_with(|| {
            registry.counter(name, &[("provider", provider), ("error_type", error_type)])
        });
        entry.clone()
    }

    /// Record a retry attempt after an error of the given class
    ///
    /// * `error_type` - Classified error class (e.g. "rate_limited", "http")
    /// * `wait_ms` - Delay applied before the retry attempt (0 for immediate)
    pub fn record_retry(&self, error_type: &str, wait_ms: u64) {
        let counter = Self::get_or_create_counter(
            &self.retry_total,
            &self.registry,
            &self.provider_label,
            "llm_retry_total",
            error_type,
        );
        counter.increment();
        self.retry_wait_ms_total.add(wait_ms);
    }

    /// Record that retries for an error of the given class were exhausted
    pub fn record_exhausted(&self, error_type: &str) {
        let counter = Self::get_or_create_counter(
            &self.exhausted_total,
            &self.registry,
            &self.provider_label,
            "llm_retry_exhausted_total",
            error_type,
        );
        counter.increment();
    }

    /// Record a final failure that was not retried (or whose retries were exhausted)
    pub fn record_failure(&self, error_type: &str) {
        let counter = Self::get_or_create_counter(
            &self.failures_total,
            &self.registry,
            &self.provider_label,
            "llm_retry_failures_total",
            error_type,
        );
        counter.increment();
    }

    /// Record a circuit breaker state observation
    ///
    /// * `state` - 0.0 = closed, 0.5 = half-open, 1.0 = open
    pub fn record_circuit_state(&self, state: f64) {
        self.circuit_breaker_state.set(state);
    }

    /// Record a circuit breaker state transition (open/half-open/closed change)
    pub fn record_circuit_transition(&self) {
        self.circuit_breaker_transitions_total.increment();
    }

    /// Record a request rejected by an open circuit breaker
    pub fn record_circuit_rejection(&self) {
        self.circuit_breaker_rejections_total.increment();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MetricData, MetricsRegistry};

    fn counter_value(snapshot: &crate::MetricsSnapshot, name: &str) -> u64 {
        snapshot
            .metrics
            .iter()
            .filter(|m| m.name == name)
            .filter_map(|m| match m.value {
                MetricData::Counter(v) => Some(v),
                _ => None,
            })
            .sum()
    }

    #[test]
    fn test_record_retry_creates_labeled_counters() {
        let registry = MetricsRegistry::new();
        let metrics = LlmRetryMetrics::new(&registry, "test-provider");

        metrics.record_retry("rate_limited", 500);
        metrics.record_retry("http", 0);

        let snapshot = registry.export_all();
        assert_eq!(counter_value(&snapshot, "llm_retry_total"), 2);
        assert_eq!(counter_value(&snapshot, "llm_retry_wait_ms_total"), 500);
    }

    #[test]
    fn test_record_exhausted_and_failure() {
        let registry = MetricsRegistry::new();
        let metrics = LlmRetryMetrics::new(&registry, "test-provider");

        metrics.record_exhausted("timeout");
        metrics.record_failure("auth");

        let snapshot = registry.export_all();
        assert_eq!(counter_value(&snapshot, "llm_retry_exhausted_total"), 1);
        assert_eq!(counter_value(&snapshot, "llm_retry_failures_total"), 1);
    }

    #[test]
    fn test_circuit_breaker_gauges() {
        let registry = MetricsRegistry::new();
        let metrics = LlmRetryMetrics::new(&registry, "test-provider");

        metrics.record_circuit_state(1.0);
        metrics.record_circuit_transition();
        metrics.record_circuit_rejection();

        let snapshot = registry.export_all();
        assert!(snapshot.metrics.iter().any(|m| {
            m.name == "llm_circuit_breaker_state"
                && matches!(m.value, MetricData::FloatGauge(v) if (v - 1.0).abs() < f64::EPSILON)
        }));
        assert_eq!(
            counter_value(&snapshot, "llm_circuit_breaker_transitions_total"),
            1
        );
        assert_eq!(
            counter_value(&snapshot, "llm_circuit_breaker_rejections_total"),
            1
        );
    }
}
