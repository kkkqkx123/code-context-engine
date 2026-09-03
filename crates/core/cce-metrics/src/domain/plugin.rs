//! Plugin/Lua runtime metrics
//!
//! Tracks plugin lifecycle, execution performance, and error rates
//! for both Lua script plugins and native dynamic library plugins.

use std::sync::Arc;

use dashmap::DashMap;

use crate::{LabeledCounter, LabeledGauge, LabeledHistogram, MetricsRegistry};

/// Plugin runtime monitoring metrics
///
/// Tracks plugin loading, execution latency, errors, and memory usage.
#[derive(Debug)]
pub struct PluginMetrics {
    /// Total number of plugin loads (successful)
    pub loads_total: LabeledCounter,
    /// Total number of plugin load failures
    pub load_failures_total: LabeledCounter,
    /// Total number of plugin executions
    pub executions_total: LabeledCounter,
    /// Plugin execution latency distribution (in milliseconds)
    pub execution_latency_ms: LabeledHistogram,
    /// Total number of plugin execution errors
    pub execution_errors_total: LabeledCounter,
    /// Total number of plugin unloads
    pub unloads_total: LabeledCounter,
    /// Per-plugin-name error counters
    pub errors_by_plugin: Arc<DashMap<String, LabeledCounter>>,
    /// Per-capability execution counters (text_gen/format_parse/entity_extract/
    /// group/group_override/chunk/rerank/relation_extract/query_rewrite/fusion/
    /// result_filter/file_filter), labeled by capability.
    pub capability_executions: Arc<DashMap<String, LabeledCounter>>,
    /// Per-capability error counters.
    pub capability_errors: Arc<DashMap<String, LabeledCounter>>,
    /// Registry for lazy counter creation
    registry: MetricsRegistry,
}

impl PluginMetrics {
    /// Create new plugin metrics with the given registry
    pub fn new(registry: &MetricsRegistry) -> Arc<Self> {
        Arc::new(Self {
            loads_total: registry.counter("plugin_loads_total", &[]),
            load_failures_total: registry.counter("plugin_load_failures_total", &[]),
            executions_total: registry.counter("plugin_executions_total", &[]),
            execution_latency_ms: registry.histogram(
                "plugin_execution_latency_ms",
                crate::LATENCY_BUCKETS.to_vec(),
                &[],
            ),
            execution_errors_total: registry.counter("plugin_execution_errors_total", &[]),
            unloads_total: registry.counter("plugin_unloads_total", &[]),
            errors_by_plugin: Arc::new(DashMap::new()),
            capability_executions: Arc::new(DashMap::new()),
            capability_errors: Arc::new(DashMap::new()),
            registry: registry.clone(),
        })
    }

    /// Record a successful plugin load
    pub fn record_load(&self) {
        self.loads_total.increment();
    }

    /// Record a plugin load failure
    pub fn record_load_failure(&self) {
        self.load_failures_total.increment();
    }

    /// Record a completed plugin execution
    pub fn record_execution(&self, latency_ms: f64, success: bool) {
        self.executions_total.increment();
        self.execution_latency_ms.observe(latency_ms);
        if !success {
            self.execution_errors_total.increment();
        }
    }

    /// Record a completed plugin execution broken down by capability facet.
    ///
    /// `capability` is one of `text_gen | format_parse | entity_extract |
    /// group | group_override | chunk | rerank | relation_extract |
    /// query_rewrite | fusion | result_filter | file_filter | unknown`.
    pub fn record_capability_execution(&self, capability: &str, latency_ms: f64, success: bool) {
        self.record_execution(latency_ms, success);
        let registry = self.registry.clone();
        let counter = self
            .capability_executions
            .entry(capability.to_string())
            .or_insert_with(|| {
                registry.counter(
                    "plugin_capability_executions_total",
                    &[("capability", capability)],
                )
            });
        counter.increment();
        if !success {
            let err_counter = self
                .capability_errors
                .entry(capability.to_string())
                .or_insert_with(|| {
                    registry.counter(
                        "plugin_capability_errors_total",
                        &[("capability", capability)],
                    )
                });
            err_counter.increment();
        }
    }

    /// Record a plugin execution error with plugin name classification
    pub fn record_execution_error(&self, plugin_name: &str) {
        self.execution_errors_total.increment();
        let registry = self.registry.clone();
        let counter = self
            .errors_by_plugin
            .entry(plugin_name.to_string())
            .or_insert_with(|| {
                registry.counter(
                    "plugin_execution_errors_total",
                    &[("component", plugin_name)],
                )
            });
        counter.increment();
    }

    /// Record a plugin unload
    pub fn record_unload(&self) {
        self.unloads_total.increment();
    }
}

/// Rerank service metrics
///
/// Tracks cross-encoder reranking operations for result reordering.
#[derive(Debug)]
pub struct RerankMetrics {
    /// Total number of rerank requests
    pub requests_total: LabeledCounter,
    /// Request latency distribution (in milliseconds)
    pub latency_ms: LabeledHistogram,
    /// Total number of candidates processed
    pub candidates_total: LabeledCounter,
    /// Total number of errors
    pub errors_total: LabeledCounter,
    /// Retry count for rerank requests
    pub retries_total: LabeledCounter,
}

impl RerankMetrics {
    /// Create new rerank metrics with the given registry and provider label
    pub fn new(registry: &MetricsRegistry, provider_label: &str) -> Arc<Self> {
        Arc::new(Self {
            requests_total: registry
                .counter("rerank_requests_total", &[("provider", provider_label)]),
            latency_ms: registry.histogram(
                "rerank_latency_ms",
                crate::LATENCY_BUCKETS.to_vec(),
                &[("provider", provider_label)],
            ),
            candidates_total: registry
                .counter("rerank_candidates_total", &[("provider", provider_label)]),
            errors_total: registry.counter("rerank_errors_total", &[("provider", provider_label)]),
            retries_total: registry
                .counter("rerank_retries_total", &[("provider", provider_label)]),
        })
    }

    /// Record a completed rerank request
    pub fn record_request(&self, latency_ms: f64, candidate_count: usize, success: bool) {
        self.requests_total.increment();
        self.latency_ms.observe(latency_ms);
        self.candidates_total.add(candidate_count as u64);
        if !success {
            self.errors_total.increment();
        }
    }

    /// Record a retry attempt
    pub fn record_retry(&self) {
        self.retries_total.increment();
    }
}

/// Background task liveness metrics
///
/// Tracks the last successful execution timestamp of critical background tasks
/// to detect stalls or silent failures.
#[derive(Debug)]
pub struct BackgroundTaskMetrics {
    /// Last successful aggregation timestamp (Unix epoch seconds)
    pub last_aggregation_timestamp: LabeledGauge,
    /// Last successful cleanup timestamp (Unix epoch seconds)
    pub last_cleanup_timestamp: LabeledGauge,
    /// Aggregation cycles completed total
    pub aggregation_cycles_total: LabeledCounter,
    /// Aggregation errors total
    pub aggregation_errors_total: LabeledCounter,
    /// Records aggregated per cycle
    pub aggregated_records_total: LabeledCounter,
    /// Aggregation cycle latency in milliseconds
    pub aggregation_cycle_latency_ms: LabeledHistogram,
}

impl BackgroundTaskMetrics {
    /// Create new background task metrics with the given registry
    pub fn new(registry: &MetricsRegistry) -> Arc<Self> {
        Arc::new(Self {
            last_aggregation_timestamp: registry.gauge("bg_last_aggregation_timestamp", &[]),
            last_cleanup_timestamp: registry.gauge("bg_last_cleanup_timestamp", &[]),
            aggregation_cycles_total: registry.counter("bg_aggregation_cycles_total", &[]),
            aggregation_errors_total: registry.counter("bg_aggregation_errors_total", &[]),
            aggregated_records_total: registry.counter("bg_aggregated_records_total", &[]),
            aggregation_cycle_latency_ms: registry
                .histogram_default("bg_aggregation_cycle_latency_ms", &[]),
        })
    }

    /// Record a successful aggregation cycle
    pub fn record_aggregation(&self, record_count: usize, latency_ms: f64, timestamp: u64) {
        self.aggregation_cycles_total.increment();
        self.aggregated_records_total.add(record_count as u64);
        self.aggregation_cycle_latency_ms.observe(latency_ms);
        self.last_aggregation_timestamp.set(timestamp);
    }

    /// Record an aggregation error
    pub fn record_aggregation_error(&self) {
        self.aggregation_errors_total.increment();
    }

    /// Record a successful cleanup cycle
    pub fn record_cleanup(&self, timestamp: u64) {
        self.last_cleanup_timestamp.set(timestamp);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_metrics_creation() {
        let registry = MetricsRegistry::new();
        let metrics = PluginMetrics::new(&registry);

        assert_eq!(metrics.loads_total.get(), 0);
        assert_eq!(metrics.executions_total.get(), 0);
        assert_eq!(metrics.execution_errors_total.get(), 0);
    }

    #[test]
    fn test_plugin_metrics_record_execution() {
        let registry = MetricsRegistry::new();
        let metrics = PluginMetrics::new(&registry);

        metrics.record_execution(25.0, true);
        assert_eq!(metrics.executions_total.get(), 1);
        assert_eq!(metrics.execution_errors_total.get(), 0);
        assert_eq!(metrics.execution_latency_ms.get_count(), 1);

        metrics.record_execution(50.0, false);
        assert_eq!(metrics.executions_total.get(), 2);
        assert_eq!(metrics.execution_errors_total.get(), 1);
    }

    #[test]
    fn test_plugin_metrics_record_error_by_name() {
        let registry = MetricsRegistry::new();
        let metrics = PluginMetrics::new(&registry);

        metrics.record_execution_error("my_plugin");
        metrics.record_execution_error("my_plugin");
        metrics.record_execution_error("other_plugin");

        assert_eq!(metrics.execution_errors_total.get(), 3);
    }

    #[test]
    fn test_plugin_metrics_lifecycle() {
        let registry = MetricsRegistry::new();
        let metrics = PluginMetrics::new(&registry);

        metrics.record_load();
        metrics.record_load();
        metrics.record_load_failure();
        metrics.record_unload();

        assert_eq!(metrics.loads_total.get(), 2);
        assert_eq!(metrics.load_failures_total.get(), 1);
        assert_eq!(metrics.unloads_total.get(), 1);
    }

    #[test]
    fn test_rerank_metrics_creation() {
        let registry = MetricsRegistry::new();
        let metrics = RerankMetrics::new(&registry, "local");

        assert_eq!(metrics.requests_total.get(), 0);
        assert_eq!(metrics.candidates_total.get(), 0);
    }

    #[test]
    fn test_rerank_metrics_record_request() {
        let registry = MetricsRegistry::new();
        let metrics = RerankMetrics::new(&registry, "local");

        metrics.record_request(30.0, 10, true);
        assert_eq!(metrics.requests_total.get(), 1);
        assert_eq!(metrics.candidates_total.get(), 10);
        assert_eq!(metrics.errors_total.get(), 0);

        metrics.record_request(20.0, 5, false);
        assert_eq!(metrics.requests_total.get(), 2);
        assert_eq!(metrics.errors_total.get(), 1);
    }

    #[test]
    fn test_background_task_metrics_creation() {
        let registry = MetricsRegistry::new();
        let metrics = BackgroundTaskMetrics::new(&registry);

        assert_eq!(metrics.aggregation_cycles_total.get(), 0);
        assert_eq!(metrics.aggregation_errors_total.get(), 0);
    }

    #[test]
    fn test_background_task_metrics_record_aggregation() {
        let registry = MetricsRegistry::new();
        let metrics = BackgroundTaskMetrics::new(&registry);

        metrics.record_aggregation(42, 15.0, 1700000000);
        assert_eq!(metrics.aggregation_cycles_total.get(), 1);
        assert_eq!(metrics.aggregated_records_total.get(), 42);
        assert_eq!(metrics.aggregation_cycle_latency_ms.get_count(), 1);
        assert_eq!(metrics.last_aggregation_timestamp.get(), 1700000000);
    }

    #[test]
    fn test_background_task_metrics_record_error_and_cleanup() {
        let registry = MetricsRegistry::new();
        let metrics = BackgroundTaskMetrics::new(&registry);

        metrics.record_aggregation_error();
        metrics.record_cleanup(1700000100);

        assert_eq!(metrics.aggregation_errors_total.get(), 1);
        assert_eq!(metrics.last_cleanup_timestamp.get(), 1700000100);
    }
}
