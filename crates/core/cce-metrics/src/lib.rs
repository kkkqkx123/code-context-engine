//! Metrics primitives for the code context engine

pub mod buckets;
pub mod descriptions;
pub mod domain;
pub mod labels;
pub mod serialization;
pub mod types;

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use dashmap::DashMap;

pub use buckets::{EMBEDDING_BUCKETS, LATENCY_BUCKETS, THROUGHPUT_BUCKETS};
pub use descriptions::metric_description;
pub use domain::{
    BackgroundTaskMetrics, Bm25Metrics, EmbeddingMetrics, FileProcessingMetrics, HotUpdateMetrics,
    HotUpdateStorageMetrics, HttpMetrics, LlmRetryMetrics, ParserMetrics, PipelineStageMetrics,
    PluginMetrics, QdrantMetrics, QueryMetrics, QueueMetrics, RelationMetrics, RerankMetrics,
    RuntimeMetrics, ScannerMetrics, SearchMetrics, SqliteMetrics, SummaryMetrics, SystemMetrics,
    WatchMetrics,
};
pub use labels::{Label, Labels, MetricKey};
pub use serialization::{MetricData, MetricValue, MetricsSnapshot};
pub use types::{
    EmbeddingErrorType, LabeledCounter, LabeledFloatGauge, LabeledGauge, LabeledHistogram,
    PipelineStage, ProgressPhase, SearchType,
};

/// A scoped metrics registry that prefixes all metric names
#[derive(Debug, Clone)]
pub struct ScopedRegistry {
    registry: MetricsRegistry,
    prefix: String,
}

impl ScopedRegistry {
    pub fn new(registry: MetricsRegistry, prefix: impl Into<String>) -> Self {
        Self {
            registry,
            prefix: prefix.into(),
        }
    }

    pub fn counter(&self, name: &str, labels: &[(&str, &str)]) -> LabeledCounter {
        self.registry
            .counter(&format!("{}_{}", self.prefix, name), labels)
    }

    pub fn gauge(&self, name: &str, labels: &[(&str, &str)]) -> LabeledGauge {
        self.registry
            .gauge(&format!("{}_{}", self.prefix, name), labels)
    }

    pub fn float_gauge(&self, name: &str, labels: &[(&str, &str)]) -> LabeledFloatGauge {
        self.registry
            .float_gauge(&format!("{}_{}", self.prefix, name), labels)
    }

    pub fn histogram(
        &self,
        name: &str,
        buckets: Vec<f64>,
        labels: &[(&str, &str)],
    ) -> LabeledHistogram {
        self.registry
            .histogram(&format!("{}_{}", self.prefix, name), buckets, labels)
    }

    pub fn histogram_default(&self, name: &str, labels: &[(&str, &str)]) -> LabeledHistogram {
        self.registry
            .histogram_default(&format!("{}_{}", self.prefix, name), labels)
    }
}

/// Statistics for a histogram metric
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HistogramStats {
    pub count: u64,
    pub average: f64,
    pub sum_microseconds: u64,
    pub max_ms: f64,
    pub p50: f64,
    pub p90: f64,
    pub p95: f64,
    pub p99: f64,
    pub buckets: Vec<f64>,
    pub bucket_counts: Vec<u64>,
    pub overflow_count: u64,
}

/// Global metrics registry
#[derive(Debug, Clone)]
pub struct MetricsRegistry {
    counters: Arc<DashMap<MetricKey, LabeledCounter>>,
    gauges: Arc<DashMap<MetricKey, LabeledGauge>>,
    float_gauges: Arc<DashMap<MetricKey, LabeledFloatGauge>>,
    histograms: Arc<DashMap<MetricKey, LabeledHistogram>>,
    snapshot_lock: Arc<RwLock<()>>,
}

impl Default for MetricsRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsRegistry {
    const ALLOWED_LABEL_KEYS: &'static [&'static str] = &[
        "operation",
        "component",
        "status",
        "project_id",
        "language",
        "provider",
        "error_type",
        "search_type",
        "stage",
        "reason",
    ];

    pub fn new() -> Self {
        Self {
            counters: Arc::new(DashMap::new()),
            gauges: Arc::new(DashMap::new()),
            float_gauges: Arc::new(DashMap::new()),
            histograms: Arc::new(DashMap::new()),
            snapshot_lock: Arc::new(RwLock::new(())),
        }
    }

    pub fn scope(&self, prefix: impl Into<String>) -> ScopedRegistry {
        ScopedRegistry::new(self.clone(), prefix)
    }

    pub fn counter_simple(&self, name: &str) -> LabeledCounter {
        self.counter_with_labels(name, Labels::new())
    }

    pub fn counter_with_labels(&self, name: &str, labels: Labels) -> LabeledCounter {
        self.validate_labels_from_labels(&labels);
        let key = MetricKey::new(name, labels.clone());
        self.counters
            .entry(key)
            .or_insert_with(|| LabeledCounter::new(labels))
            .clone()
    }

    pub fn counter(&self, name: &str, labels: &[(&str, &str)]) -> LabeledCounter {
        self.validate_labels(labels);
        self.counter_with_labels(name, Labels::from_pairs(labels))
    }

    fn validate_labels(&self, labels: &[(&str, &str)]) {
        for (key, _value) in labels {
            if !Self::ALLOWED_LABEL_KEYS.contains(key) {
                let msg = format!(
                    "Disallowed metric label key '{}'. Allowed keys: {:?}",
                    key,
                    Self::ALLOWED_LABEL_KEYS
                );
                Self::handle_invalid_label(&msg);
            }
        }
    }

    fn validate_labels_from_labels(&self, labels: &Labels) {
        for label in labels.iter() {
            if !Self::ALLOWED_LABEL_KEYS.contains(&label.key.as_str()) {
                let msg = format!(
                    "Disallowed metric label key '{}'. Allowed keys: {:?}",
                    label.key,
                    Self::ALLOWED_LABEL_KEYS
                );
                Self::handle_invalid_label(&msg);
            }
        }
    }

    fn handle_invalid_label(msg: &str) {
        if std::env::var("STRICT_METRICS_LABELS").is_ok() {
            panic!("{}", msg);
        } else {
            tracing::error!("{}", msg);
        }
    }

    pub fn gauge_simple(&self, name: &str) -> LabeledGauge {
        self.gauge_with_labels(name, Labels::new())
    }

    pub fn gauge_with_labels(&self, name: &str, labels: Labels) -> LabeledGauge {
        self.validate_labels_from_labels(&labels);
        let key = MetricKey::new(name, labels.clone());
        self.gauges
            .entry(key)
            .or_insert_with(|| LabeledGauge::new(labels))
            .clone()
    }

    pub fn gauge(&self, name: &str, labels: &[(&str, &str)]) -> LabeledGauge {
        self.validate_labels(labels);
        self.gauge_with_labels(name, Labels::from_pairs(labels))
    }

    pub fn float_gauge_simple(&self, name: &str) -> LabeledFloatGauge {
        self.float_gauge_with_labels(name, Labels::new())
    }

    pub fn float_gauge_with_labels(&self, name: &str, labels: Labels) -> LabeledFloatGauge {
        self.validate_labels_from_labels(&labels);
        let key = MetricKey::new(name, labels.clone());
        self.float_gauges
            .entry(key)
            .or_insert_with(|| LabeledFloatGauge::new(labels))
            .clone()
    }

    pub fn float_gauge(&self, name: &str, labels: &[(&str, &str)]) -> LabeledFloatGauge {
        self.validate_labels(labels);
        self.float_gauge_with_labels(name, Labels::from_pairs(labels))
    }

    pub fn histogram_simple(&self, name: &str, buckets: Vec<f64>) -> LabeledHistogram {
        self.histogram_with_labels(name, buckets, Labels::new())
    }

    pub fn histogram_with_labels(
        &self,
        name: &str,
        buckets: Vec<f64>,
        labels: Labels,
    ) -> LabeledHistogram {
        self.validate_labels_from_labels(&labels);
        let key = MetricKey::new(name, labels.clone());
        self.histograms
            .entry(key)
            .or_insert_with(|| LabeledHistogram::new(buckets.clone(), labels))
            .clone()
    }

    pub fn histogram(
        &self,
        name: &str,
        buckets: Vec<f64>,
        labels: &[(&str, &str)],
    ) -> LabeledHistogram {
        self.validate_labels(labels);
        self.histogram_with_labels(name, buckets, Labels::from_pairs(labels))
    }

    pub fn histogram_default_simple(&self, name: &str) -> LabeledHistogram {
        self.histogram_default(name, &[])
    }

    pub fn histogram_default(&self, name: &str, labels: &[(&str, &str)]) -> LabeledHistogram {
        self.histogram_with_labels(
            name,
            vec![1.0, 5.0, 10.0, 50.0, 100.0, 500.0, 1000.0, 5000.0],
            Labels::from_pairs(labels),
        )
    }

    fn snapshot_read_guard(&self) -> std::sync::RwLockReadGuard<'_, ()> {
        self.snapshot_lock.read().unwrap_or_else(|e| e.into_inner())
    }

    fn snapshot_write_guard(&self) -> std::sync::RwLockWriteGuard<'_, ()> {
        self.snapshot_lock
            .write()
            .unwrap_or_else(|e| e.into_inner())
    }

    pub fn get_all_counters(&self) -> HashMap<String, u64> {
        let _guard = self.snapshot_read_guard();
        self.counters
            .iter()
            .map(|entry| (entry.key().to_storage_key(), entry.value().get()))
            .collect()
    }

    pub fn get_all_counters_with_keys(&self) -> Vec<(MetricKey, u64)> {
        let _guard = self.snapshot_read_guard();
        self.counters
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().get()))
            .collect()
    }

    pub fn get_all_gauges(&self) -> HashMap<String, u64> {
        let _guard = self.snapshot_read_guard();
        self.gauges
            .iter()
            .map(|entry| (entry.key().to_storage_key(), entry.value().get()))
            .collect()
    }

    pub fn get_all_gauges_with_keys(&self) -> Vec<(MetricKey, u64)> {
        let _guard = self.snapshot_read_guard();
        self.gauges
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().get()))
            .collect()
    }

    pub fn get_all_float_gauges(&self) -> HashMap<String, f64> {
        let _guard = self.snapshot_read_guard();
        self.float_gauges
            .iter()
            .map(|entry| (entry.key().to_storage_key(), entry.value().get()))
            .collect()
    }

    pub fn get_all_float_gauges_with_keys(&self) -> Vec<(MetricKey, f64)> {
        let _guard = self.snapshot_read_guard();
        self.float_gauges
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().get()))
            .collect()
    }

    pub fn export_all(&self) -> MetricsSnapshot {
        serialization::MetricsSnapshot::from_registry(self)
    }

    pub fn remove_by_prefix(&self, prefix: &str) -> usize {
        let _guard = self.snapshot_write_guard();
        let mut removed = 0;
        self.counters.retain(|k, _| {
            let matched = k.name.starts_with(prefix);
            if matched {
                removed += 1;
            }
            !matched
        });
        self.gauges.retain(|k, _| {
            let matched = k.name.starts_with(prefix);
            if matched {
                removed += 1;
            }
            !matched
        });
        self.float_gauges.retain(|k, _| {
            let matched = k.name.starts_with(prefix);
            if matched {
                removed += 1;
            }
            !matched
        });
        self.histograms.retain(|k, _| {
            let matched = k.name.starts_with(prefix);
            if matched {
                removed += 1;
            }
            !matched
        });
        removed
    }

    pub fn remove_by_label_value(&self, label: &str, value: &str) -> usize {
        fn matches(key: &MetricKey, label: &str, value: &str) -> bool {
            key.labels
                .iter()
                .any(|l| l.key == label && l.value == value)
        }
        fn count_matching<T>(map: &DashMap<MetricKey, T>, label: &str, value: &str) -> usize {
            map.iter()
                .filter(|e| matches(e.key(), label, value))
                .count()
        }
        let _guard = self.snapshot_write_guard();
        let removed = count_matching(&self.counters, label, value)
            + count_matching(&self.gauges, label, value)
            + count_matching(&self.float_gauges, label, value)
            + count_matching(&self.histograms, label, value);
        self.counters.retain(|k, _| !matches(k, label, value));
        self.gauges.retain(|k, _| !matches(k, label, value));
        self.float_gauges.retain(|k, _| !matches(k, label, value));
        self.histograms.retain(|k, _| !matches(k, label, value));
        removed
    }

    pub fn get_all_histograms_with_handles(&self) -> Vec<(MetricKey, LabeledHistogram)> {
        let _guard = self.snapshot_read_guard();
        self.histograms
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect()
    }

    pub fn get_all_histograms_with_keys(&self) -> Vec<(MetricKey, HistogramStats)> {
        let _guard = self.snapshot_read_guard();
        self.histograms
            .iter()
            .map(|entry| {
                let (key, hist) = entry.pair();
                (
                    key.clone(),
                    HistogramStats {
                        count: hist.get_count(),
                        average: hist.get_average(),
                        sum_microseconds: hist.get_sum(),
                        max_ms: hist.get_max_ms(),
                        p50: hist.p50(),
                        p90: hist.p90(),
                        p95: hist.p95(),
                        p99: hist.p99(),
                        buckets: hist.get_buckets().to_vec(),
                        bucket_counts: hist.get_bucket_counts(),
                        overflow_count: hist.get_overflow_count(),
                    },
                )
            })
            .collect()
    }
}
