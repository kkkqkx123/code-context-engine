//! Advanced serialization support for metrics
//!
//! This module provides enhanced serialization formats for metrics export,
//! including JSON with labels support and Prometheus exposition format.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::HistogramStats;

/// A single metric value with optional labels
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MetricValue {
    /// Metric name
    pub name: String,
    /// Labels (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<HashMap<String, String>>,
    /// Metric data
    pub value: MetricData,
}

/// Metric data types
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", content = "value")]
pub enum MetricData {
    /// Counter value
    Counter(u64),
    /// Gauge value (integer)
    Gauge(u64),
    /// Float gauge value (floating-point)
    FloatGauge(f64),
    /// Histogram statistics
    Histogram(HistogramStats),
}

/// Complete metrics snapshot for API responses
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MetricsSnapshot {
    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// All metrics
    pub metrics: Vec<MetricValue>,
    /// Summary statistics
    pub summary: SnapshotSummary,
}

/// Summary of a metrics snapshot
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SnapshotSummary {
    pub total_counters: usize,
    pub total_gauges: usize,
    pub total_float_gauges: usize,
    pub total_histograms: usize,
}

impl MetricsSnapshot {
    /// Create a new metrics snapshot from the registry
    ///
    /// The snapshot is deduplicated by `(name, labels)`: a metric registered
    /// under multiple types emits a single entry. On conflict the type with
    /// the higher snapshot priority wins (`Gauge` supersedes `Counter`;
    /// `FloatGauge` and `Histogram` supersede both) and a warning is logged,
    /// keeping Prometheus/JSON export free of duplicate series.
    pub fn from_registry(registry: &crate::MetricsRegistry) -> Self {
        let mut metrics = Vec::new();
        let mut index_by_key: std::collections::HashMap<(String, String), usize> =
            std::collections::HashMap::new();

        fn canonical_labels(labels: &HashMap<String, String>) -> String {
            let mut pairs: Vec<(&String, &String)> = labels.iter().collect();
            pairs.sort_by(|a, b| a.0.cmp(b.0));
            pairs
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join(",")
        }

        fn ensure_unique(
            metrics: &mut Vec<MetricValue>,
            index_by_key: &mut std::collections::HashMap<(String, String), usize>,
            name: String,
            labels: Option<HashMap<String, String>>,
            value: MetricData,
            rank: u8,
        ) {
            let key = (
                name.clone(),
                labels.as_ref().map(canonical_labels).unwrap_or_default(),
            );
            match index_by_key.get(&key) {
                None => {
                    index_by_key.insert(key, metrics.len());
                    metrics.push(MetricValue {
                        name,
                        labels,
                        value,
                    });
                }
                Some(&existing) => {
                    let existing_rank = match metrics[existing].value {
                        MetricData::Counter(_) => 1,
                        MetricData::Gauge(_) => 2,
                        MetricData::FloatGauge(_) => 3,
                        MetricData::Histogram(_) => 4,
                    };
                    if rank > existing_rank {
                        tracing::warn!(
                            metric_name = %name,
                            "Duplicate metric (name+labels) under multiple types; keeping \
                             higher-priority type"
                        );
                        metrics[existing] = MetricValue {
                            name,
                            labels,
                            value,
                        };
                        index_by_key.insert(key, existing);
                    } else {
                        tracing::debug!(
                            metric_name = %name,
                            "Duplicate metric (name+labels) under multiple types; keeping \
                             existing lower-priority registration"
                        );
                    }
                }
            }
        }

        // Export counters
        let counters = registry.get_all_counters_with_keys();
        for (key, value) in counters {
            ensure_unique(
                &mut metrics,
                &mut index_by_key,
                key.name.clone(),
                metric_labels(&key),
                MetricData::Counter(value),
                1,
            );
        }

        // Export gauges
        let gauges = registry.get_all_gauges_with_keys();
        for (key, value) in gauges {
            ensure_unique(
                &mut metrics,
                &mut index_by_key,
                key.name.clone(),
                metric_labels(&key),
                MetricData::Gauge(value),
                2,
            );
        }

        // Export float gauges
        let float_gauges = registry.get_all_float_gauges_with_keys();
        for (key, value) in float_gauges {
            ensure_unique(
                &mut metrics,
                &mut index_by_key,
                key.name.clone(),
                metric_labels(&key),
                MetricData::FloatGauge(value),
                3,
            );
        }

        // Export histograms
        let histograms = registry.get_all_histograms_with_keys();
        for (key, stats) in histograms {
            ensure_unique(
                &mut metrics,
                &mut index_by_key,
                key.name.clone(),
                metric_labels(&key),
                MetricData::Histogram(stats),
                4,
            );
        }

        let counter_count = metrics
            .iter()
            .filter(|m| matches!(m.value, MetricData::Counter(_)))
            .count();
        let gauge_count = metrics
            .iter()
            .filter(|m| matches!(m.value, MetricData::Gauge(_)))
            .count();
        let float_gauge_count = metrics
            .iter()
            .filter(|m| matches!(m.value, MetricData::FloatGauge(_)))
            .count();
        let histogram_count = metrics
            .iter()
            .filter(|m| matches!(m.value, MetricData::Histogram(_)))
            .count();

        MetricsSnapshot {
            timestamp: chrono::Utc::now(),
            metrics,
            summary: SnapshotSummary {
                total_counters: counter_count,
                total_gauges: gauge_count,
                total_float_gauges: float_gauge_count,
                total_histograms: histogram_count,
            },
        }
    }
}

fn metric_labels(key: &crate::labels::MetricKey) -> Option<HashMap<String, String>> {
    if key.labels.is_empty() {
        None
    } else {
        Some(
            key.labels
                .iter()
                .map(|l| (l.key.clone(), l.value.clone()))
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MetricsRegistry;

    #[test]
    fn test_metric_value_serialization() {
        let metric = MetricValue {
            name: "test_counter".to_string(),
            labels: Some(
                [("method".to_string(), "GET".to_string())]
                    .iter()
                    .cloned()
                    .collect(),
            ),
            value: MetricData::Counter(42),
        };

        let json = serde_json::to_string(&metric).unwrap();
        assert!(json.contains("test_counter"));
        assert!(json.contains("GET"));
    }

    #[test]
    fn test_metrics_snapshot_from_registry() {
        let registry = MetricsRegistry::new();

        // Add some metrics
        registry
            .counter("test_counter", &[("label", "value")])
            .increment();
        registry
            .histogram_default("test_histogram", &[])
            .observe(10.0);

        let snapshot = MetricsSnapshot::from_registry(&registry);

        assert!(!snapshot.metrics.is_empty());
        assert_eq!(snapshot.summary.total_counters, 1);
        assert_eq!(snapshot.summary.total_histograms, 1);
    }

    #[test]
    fn test_snapshot_json_roundtrip() {
        let registry = MetricsRegistry::new();
        registry.counter("test", &[]).increment();

        let snapshot = MetricsSnapshot::from_registry(&registry);
        let json = serde_json::to_string_pretty(&snapshot).unwrap();

        // Parse it back
        let parsed: MetricsSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.metrics.len(), snapshot.metrics.len());
    }

    #[test]
    fn test_metric_value_without_labels() {
        let metric = MetricValue {
            name: "simple_counter".to_string(),
            labels: None,
            value: MetricData::Counter(42),
        };

        let json = serde_json::to_string(&metric).unwrap();
        assert!(json.contains("simple_counter"));
        assert!(!json.contains("labels")); // Should be skipped
    }

    #[test]
    fn test_metric_data_types() {
        // Test Counter
        let counter_data = MetricData::Counter(100);
        let json = serde_json::to_string(&counter_data).unwrap();
        assert!(json.contains("Counter"));

        // Test Gauge
        let gauge_data = MetricData::Gauge(50);
        let json = serde_json::to_string(&gauge_data).unwrap();
        assert!(json.contains("Gauge"));

        // Test Histogram
        let hist_stats = crate::HistogramStats {
            count: 10,
            average: 25.5,
            sum_microseconds: 25_500,
            max_ms: 48.0,
            p50: 20.0,
            p90: 40.0,
            p95: 45.0,
            p99: 48.0,
            buckets: vec![10.0, 50.0, 100.0],
            bucket_counts: vec![3, 5, 2],
            overflow_count: 0,
        };
        let histogram_data = MetricData::Histogram(hist_stats);
        let json = serde_json::to_string(&histogram_data).unwrap();
        assert!(json.contains("Histogram"));
    }

    #[test]
    fn test_snapshot_summary_counts() {
        let registry = MetricsRegistry::new();

        // Add different types of metrics
        registry.counter("counter1", &[]).increment();
        registry.counter("counter2", &[]).increment();
        registry.gauge("gauge1", &[]).set(10);
        registry.histogram_default("hist1", &[]).observe(5.0);

        let snapshot = MetricsSnapshot::from_registry(&registry);

        assert_eq!(snapshot.summary.total_counters, 2);
        assert_eq!(snapshot.summary.total_gauges, 1);
        assert_eq!(snapshot.summary.total_histograms, 1);
    }

    #[test]
    fn test_snapshot_timestamp() {
        let registry = MetricsRegistry::new();
        registry.counter("test", &[]).increment();

        let snapshot = MetricsSnapshot::from_registry(&registry);

        // Timestamp should be recent (within last minute)
        let now = chrono::Utc::now();
        let diff = now.signed_duration_since(snapshot.timestamp);
        assert!(diff.num_seconds() < 60);
    }

    #[test]
    fn test_snapshot_dedup_counter_gauge_collision() {
        let registry = MetricsRegistry::new();

        // Same name + labels registered as both counter and gauge
        registry
            .counter("collision_metric", &[("project_id", "1")])
            .add(5);
        registry
            .gauge("collision_metric", &[("project_id", "1")])
            .set(42);

        let snapshot = MetricsSnapshot::from_registry(&registry);

        let matches = snapshot
            .metrics
            .iter()
            .filter(|m| m.name == "collision_metric")
            .collect::<Vec<_>>();
        // Deduplicated to a single entry; gauge supersedes counter
        assert_eq!(matches.len(), 1);
        match &matches[0].value {
            MetricData::Gauge(v) => assert_eq!(*v, 42),
            other => panic!("expected gauge, got {:?}", other),
        }
        assert_eq!(snapshot.summary.total_counters, 0);
        assert_eq!(snapshot.summary.total_gauges, 1);
    }

    #[test]
    fn test_snapshot_dedup_distinct_labels_kept() {
        let registry = MetricsRegistry::new();

        registry
            .counter("same_name", &[("project_id", "1")])
            .increment();
        registry
            .counter("same_name", &[("project_id", "2")])
            .increment();

        let snapshot = MetricsSnapshot::from_registry(&registry);
        let entries = snapshot
            .metrics
            .iter()
            .filter(|m| m.name == "same_name")
            .count();
        // Different label sets are distinct series, no dedup
        assert_eq!(entries, 2);
        assert_eq!(snapshot.summary.total_counters, 2);
    }

    #[test]
    fn test_snapshot_dedup_summary_consistent() {
        let registry = MetricsRegistry::new();

        registry.counter("a", &[]).add(1);
        registry.gauge("b", &[]).set(1);
        // Collision: gauge supersedes counter
        registry.counter("c", &[]).add(1);
        registry.gauge("c", &[]).set(1);
        registry.histogram_default("d", &[]).observe(10.0);

        let snapshot = MetricsSnapshot::from_registry(&registry);
        let counts = [
            snapshot.summary.total_counters,
            snapshot.summary.total_gauges,
            snapshot.summary.total_float_gauges,
            snapshot.summary.total_histograms,
        ];
        assert_eq!(counts.iter().sum::<usize>(), snapshot.metrics.len());
        assert_eq!(snapshot.summary.total_counters, 1);
        assert_eq!(snapshot.summary.total_gauges, 2);
        assert_eq!(snapshot.summary.total_histograms, 1);
    }
}
