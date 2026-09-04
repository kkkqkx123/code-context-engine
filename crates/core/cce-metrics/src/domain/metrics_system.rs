//! Metrics-system self-monitoring.
//!
//! Tracks the health of the metrics pipeline itself: registry size,
//! aggregation latency/throughput, cleanup volume, and export activity.

use std::sync::Arc;

use crate::{LabeledCounter, LabeledFloatGauge, LabeledGauge, LabeledHistogram, MetricsRegistry};

/// Self-monitoring metrics for the metrics subsystem.
#[derive(Debug)]
pub struct MetricsSystemMetrics {
    /// Number of metric series currently held in memory.
    pub registry_size: LabeledGauge,
    /// Aggregation cycle latency distribution (milliseconds).
    pub aggregation_latency_ms: LabeledHistogram,
    /// Total aggregated records written to SQLite.
    pub aggregation_records_total: LabeledCounter,
    /// Total records removed by cleanup.
    pub cleanup_records_total: LabeledCounter,
    /// Export latency distribution (milliseconds).
    pub export_latency_ms: LabeledHistogram,
    /// Total exports by format.
    pub export_formats_total: Arc<dashmap::DashMap<String, LabeledCounter>>,
    /// Average export payload size in bytes.
    pub export_size_bytes: LabeledFloatGauge,
    /// Registry for lazy per-format counter creation.
    registry: MetricsRegistry,
}

impl MetricsSystemMetrics {
    /// Create self-monitoring metrics bound to the given registry.
    pub fn new(registry: &MetricsRegistry) -> Arc<Self> {
        Arc::new(Self {
            registry_size: registry.gauge("metrics_registry_size", &[("component", "metrics")]),
            aggregation_latency_ms: registry.histogram(
                "metrics_aggregation_latency_ms",
                crate::LATENCY_BUCKETS.to_vec(),
                &[("component", "metrics")],
            ),
            aggregation_records_total: registry.counter(
                "metrics_aggregation_records_total",
                &[("component", "metrics")],
            ),
            cleanup_records_total: registry
                .counter("metrics_cleanup_records_total", &[("component", "metrics")]),
            export_latency_ms: registry.histogram(
                "metrics_export_latency_ms",
                crate::LATENCY_BUCKETS.to_vec(),
                &[("component", "metrics")],
            ),
            export_formats_total: Arc::new(dashmap::DashMap::new()),
            export_size_bytes: registry
                .float_gauge("metrics_export_size_bytes", &[("component", "metrics")]),
            registry: registry.clone(),
        })
    }

    /// Record a completed aggregation cycle.
    pub fn record_aggregation(&self, count: usize, latency_ms: f64) {
        self.aggregation_latency_ms.observe(latency_ms);
        self.aggregation_records_total.add(count as u64);
    }

    /// Record a cleanup cycle that removed `count` records.
    pub fn record_cleanup(&self, count: usize) {
        self.cleanup_records_total.add(count as u64);
    }

    /// Record a completed metrics export.
    pub fn record_export(&self, format: &str, latency_ms: f64, size_bytes: usize) {
        self.export_latency_ms.observe(latency_ms);
        self.export_size_bytes.set(size_bytes as f64);
        let registry = self.registry.clone();
        let counter = self
            .export_formats_total
            .entry(format.to_string())
            .or_insert_with(|| {
                registry.counter("metrics_export_formats_total", &[("format", format)])
            });
        counter.increment();
    }

    /// Refresh the registry-size gauge from the live registry.
    pub fn update_registry_size(&self, size: usize) {
        self.registry_size.set(size as u64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_metrics_creation() {
        let registry = MetricsRegistry::new();
        let metrics = MetricsSystemMetrics::new(&registry);
        assert_eq!(metrics.registry_size.get(), 0);
        assert_eq!(metrics.aggregation_records_total.get(), 0);
        assert_eq!(metrics.cleanup_records_total.get(), 0);
    }

    #[test]
    fn test_record_aggregation() {
        let registry = MetricsRegistry::new();
        let metrics = MetricsSystemMetrics::new(&registry);
        metrics.record_aggregation(42, 15.0);
        assert_eq!(metrics.aggregation_records_total.get(), 42);
        assert_eq!(metrics.aggregation_latency_ms.get_count(), 1);
    }

    #[test]
    fn test_record_cleanup() {
        let registry = MetricsRegistry::new();
        let metrics = MetricsSystemMetrics::new(&registry);
        metrics.record_cleanup(7);
        assert_eq!(metrics.cleanup_records_total.get(), 7);
    }

    #[test]
    fn test_record_export() {
        let registry = MetricsRegistry::new();
        let metrics = MetricsSystemMetrics::new(&registry);
        metrics.record_export("prometheus", 3.0, 1024);
        metrics.record_export("json", 4.0, 2048);
        assert_eq!(metrics.export_latency_ms.get_count(), 2);
        assert_eq!(metrics.export_size_bytes.get(), 2048.0);
        let snapshot = registry.export_all();
        let total: u64 = snapshot
            .metrics
            .iter()
            .filter(|m| m.name == "metrics_export_formats_total")
            .filter_map(|m| match m.value {
                crate::MetricData::Counter(v) => Some(v),
                _ => None,
            })
            .sum();
        assert_eq!(total, 2);
    }

    #[test]
    fn test_update_registry_size() {
        let registry = MetricsRegistry::new();
        let metrics = MetricsSystemMetrics::new(&registry);
        metrics.update_registry_size(128);
        assert_eq!(metrics.registry_size.get(), 128);
    }
}
