//! Metrics primitives for the code context engine

pub mod buckets;
pub mod config;
pub mod descriptions;
pub mod domain;
pub mod labels;
pub mod serialization;
pub mod types;

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use dashmap::DashMap;

pub use buckets::{EMBEDDING_BUCKETS, LATENCY_BUCKETS, THROUGHPUT_BUCKETS};
pub use config::{MetricsLabelConfig, MetricsMemoryConfig};
pub use descriptions::metric_description;
pub use domain::{
    BackgroundTaskMetrics, Bm25Metrics, EmbeddingMetrics, FileProcessingMetrics, HotUpdateMetrics,
    HotUpdateStorageMetrics, HttpMetrics, LlmRetryMetrics, MetricsSystemMetrics, ParserMetrics,
    PipelineStageMetrics, PluginMetrics, QdrantMetrics, QueryMetrics, QueueMetrics,
    RelationMetrics, RerankMetrics, RuntimeMetrics, ScannerMetrics, SearchMetrics, SqliteMetrics,
    SummaryMetrics, SystemMetrics, WatchMetrics,
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
    label_config: Arc<RwLock<MetricsLabelConfig>>,
    last_access: Arc<DashMap<MetricKey, Instant>>,
    memory_config: Arc<RwLock<MetricsMemoryConfig>>,
}

impl Default for MetricsRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsRegistry {
    pub fn new() -> Self {
        Self::with_label_config(MetricsLabelConfig::default())
    }

    pub fn with_label_config(config: MetricsLabelConfig) -> Self {
        Self {
            counters: Arc::new(DashMap::new()),
            gauges: Arc::new(DashMap::new()),
            float_gauges: Arc::new(DashMap::new()),
            histograms: Arc::new(DashMap::new()),
            snapshot_lock: Arc::new(RwLock::new(())),
            label_config: Arc::new(RwLock::new(config)),
            last_access: Arc::new(DashMap::new()),
            memory_config: Arc::new(RwLock::new(MetricsMemoryConfig::default())),
        }
    }

    /// Replace the label validation config at runtime without a restart.
    pub fn update_label_config(&self, config: MetricsLabelConfig) {
        let mut guard = self.label_config.write().unwrap_or_else(|e| e.into_inner());
        *guard = config;
    }

    /// Snapshot the current label validation config.
    pub fn label_config(&self) -> MetricsLabelConfig {
        self.label_config
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Replace the memory optimization config at runtime.
    ///
    /// When eviction is newly enabled, all existing metrics are marked as
    /// accessed now so they are not evicted before their first idle period.
    pub fn update_memory_config(&self, config: MetricsMemoryConfig) {
        let mut guard = self
            .memory_config
            .write()
            .unwrap_or_else(|e| e.into_inner());
        let newly_enabled = config.eviction_enabled && !guard.eviction_enabled;
        *guard = config;
        if newly_enabled {
            let now = Instant::now();
            for entry in self.counters.iter() {
                self.last_access.insert(entry.key().clone(), now);
            }
            for entry in self.gauges.iter() {
                self.last_access.insert(entry.key().clone(), now);
            }
            for entry in self.float_gauges.iter() {
                self.last_access.insert(entry.key().clone(), now);
            }
            for entry in self.histograms.iter() {
                self.last_access.insert(entry.key().clone(), now);
            }
        }
    }

    /// Snapshot the current memory optimization config.
    pub fn memory_config(&self) -> MetricsMemoryConfig {
        self.memory_config
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Total number of metric series held in memory.
    pub fn registry_size(&self) -> usize {
        self.counters.len() + self.gauges.len() + self.float_gauges.len() + self.histograms.len()
    }

    /// Record an access for eviction tracking (no-op unless eviction is enabled).
    fn touch(&self, key: &MetricKey) {
        let enabled = self
            .memory_config
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .eviction_enabled;
        if enabled {
            self.last_access.insert(key.clone(), Instant::now());
        }
    }

    /// Evict metrics idle for longer than the configured retention.
    ///
    /// Returns the number of series removed. No-op when eviction is disabled.
    pub fn cleanup_inactive_metrics(&self) -> usize {
        let (enabled, retention) = {
            let guard = self.memory_config.read().unwrap_or_else(|e| e.into_inner());
            (
                guard.eviction_enabled,
                Duration::from_secs(guard.retention_seconds),
            )
        };
        if !enabled {
            return 0;
        }
        let now = Instant::now();
        let stale: Vec<MetricKey> = self
            .last_access
            .iter()
            .filter(|entry| now.duration_since(*entry.value()) > retention)
            .map(|entry| entry.key().clone())
            .collect();
        if stale.is_empty() {
            return 0;
        }
        let _guard = self.snapshot_write_guard();
        let mut removed = 0;
        for key in &stale {
            if self.counters.remove(key).is_some() {
                removed += 1;
            }
            if self.gauges.remove(key).is_some() {
                removed += 1;
            }
            if self.float_gauges.remove(key).is_some() {
                removed += 1;
            }
            if self.histograms.remove(key).is_some() {
                removed += 1;
            }
            self.last_access.remove(key);
        }
        removed
    }

    /// Spawn a background task that periodically evicts inactive metrics.
    pub fn start_cleanup_task(&self) -> tokio::task::JoinHandle<()> {
        let registry = self.clone();
        let interval_secs = self.memory_config().cleanup_interval_secs.max(1);
        tokio::spawn(async move {
            let mut timer = tokio::time::interval(Duration::from_secs(interval_secs));
            loop {
                timer.tick().await;
                let removed = registry.cleanup_inactive_metrics();
                if removed > 0 {
                    tracing::debug!(removed_count = removed, "Evicted inactive metrics");
                }
            }
        })
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
        self.touch(&key);
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
        let config = self.label_config.read().unwrap_or_else(|e| e.into_inner());
        for (key, value) in labels {
            if !config.is_key_allowed(key) {
                let msg = format!(
                    "Disallowed metric label key '{}'. Allowed keys: {:?}",
                    key, config.allowed_keys
                );
                Self::handle_invalid_label(&msg, config.strict);
            } else if !config.is_value_allowed(key, value) {
                let msg = format!(
                    "Disallowed metric label value '{}' for key '{}'. Allowed values: {:?}",
                    value,
                    key,
                    config.static_enums.get(*key)
                );
                Self::handle_invalid_label(&msg, config.strict);
            }
        }
    }

    fn validate_labels_from_labels(&self, labels: &Labels) {
        let config = self.label_config.read().unwrap_or_else(|e| e.into_inner());
        for label in labels.iter() {
            if !config.is_key_allowed(&label.key) {
                let msg = format!(
                    "Disallowed metric label key '{}'. Allowed keys: {:?}",
                    label.key, config.allowed_keys
                );
                Self::handle_invalid_label(&msg, config.strict);
            } else if !config.is_value_allowed(&label.key, &label.value) {
                let msg = format!(
                    "Disallowed metric label value '{}' for key '{}'. Allowed values: {:?}",
                    label.value,
                    label.key,
                    config.static_enums.get(&label.key)
                );
                Self::handle_invalid_label(&msg, config.strict);
            }
        }
    }

    fn handle_invalid_label(msg: &str, strict: bool) {
        if strict || std::env::var("STRICT_METRICS_LABELS").is_ok() {
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
        self.touch(&key);
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
        self.touch(&key);
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
        self.touch(&key);
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
        let mut evicted_keys = Vec::new();
        self.counters.retain(|k, _| {
            let matched = k.name.starts_with(prefix);
            if matched {
                removed += 1;
                evicted_keys.push(k.clone());
            }
            !matched
        });
        self.gauges.retain(|k, _| {
            let matched = k.name.starts_with(prefix);
            if matched {
                removed += 1;
                evicted_keys.push(k.clone());
            }
            !matched
        });
        self.float_gauges.retain(|k, _| {
            let matched = k.name.starts_with(prefix);
            if matched {
                removed += 1;
                evicted_keys.push(k.clone());
            }
            !matched
        });
        self.histograms.retain(|k, _| {
            let matched = k.name.starts_with(prefix);
            if matched {
                removed += 1;
                evicted_keys.push(k.clone());
            }
            !matched
        });
        for key in evicted_keys {
            self.last_access.remove(&key);
        }
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
        self.last_access.retain(|k, _| !matches(k, label, value));
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_label_config_defaults_allow_builtin_keys() {
        let registry = MetricsRegistry::new();
        let config = registry.label_config();
        assert!(config.is_key_allowed("operation"));
        assert!(config.is_key_allowed("capability"));
        assert!(config.is_key_allowed("method"));
        assert!(!config.strict);
    }

    #[test]
    fn test_custom_label_keys_without_restart() {
        let registry = MetricsRegistry::new();
        registry.counter("test", &[("custom_key", "value")]);
        registry.update_label_config(MetricsLabelConfig::with_allowed_keys(vec![
            "custom_key".to_string(),
        ]));
        assert!(registry.label_config().is_key_allowed("custom_key"));
        assert!(!registry.label_config().is_key_allowed("operation"));
        registry.counter("test", &[("custom_key", "value")]);
    }

    #[test]
    fn test_strict_mode_panics_on_invalid_key() {
        let registry = MetricsRegistry::with_label_config(MetricsLabelConfig {
            allowed_keys: vec!["custom_key".to_string()],
            strict: true,
            static_enums: HashMap::new(),
        });
        registry.counter("test", &[("custom_key", "value")]);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            registry.counter("test", &[("invalid_key", "value")]);
        }));
        assert!(result.is_err());
    }

    #[test]
    fn test_non_strict_mode_only_logs_on_invalid_key() {
        let registry = MetricsRegistry::with_label_config(MetricsLabelConfig {
            allowed_keys: vec!["custom_key".to_string()],
            strict: false,
            static_enums: HashMap::new(),
        });
        registry.counter("test", &[("invalid_key", "value")]);
        assert_eq!(registry.get_all_counters().len(), 1);
    }

    #[test]
    fn test_static_enum_value_validation() {
        let mut static_enums = HashMap::new();
        static_enums.insert(
            "search_type".to_string(),
            vec!["bm25".to_string(), "keyword".to_string()],
        );
        let registry = MetricsRegistry::with_label_config(MetricsLabelConfig {
            allowed_keys: vec!["search_type".to_string()],
            strict: true,
            static_enums,
        });
        registry.counter("test", &[("search_type", "bm25")]);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            registry.counter("test", &[("search_type", "other")]);
        }));
        assert!(result.is_err());
    }

    #[test]
    fn test_eviction_disabled_by_default() {
        let registry = MetricsRegistry::new();
        registry.counter("test", &[]).increment();
        assert!(!registry.memory_config().eviction_enabled);
        assert_eq!(registry.cleanup_inactive_metrics(), 0);
        assert_eq!(registry.registry_size(), 1);
    }

    #[test]
    fn test_inactive_metrics_are_evicted() {
        let registry = MetricsRegistry::new();
        registry.update_memory_config(MetricsMemoryConfig {
            eviction_enabled: true,
            retention_seconds: 3600,
            cleanup_interval_secs: 300,
        });
        registry.counter("active_metric", &[]).increment();
        registry.counter("stale_metric", &[]).increment();
        assert_eq!(registry.registry_size(), 2);

        let stale_key = MetricKey::new("stale_metric", Labels::new());
        registry.last_access.insert(
            stale_key,
            std::time::Instant::now() - std::time::Duration::from_secs(7200),
        );

        assert_eq!(registry.cleanup_inactive_metrics(), 1);
        assert_eq!(registry.registry_size(), 1);
        assert_eq!(registry.get_all_counters().len(), 1);
    }

    #[test]
    fn test_active_metrics_survive_cleanup() {
        let registry = MetricsRegistry::new();
        registry.update_memory_config(MetricsMemoryConfig {
            eviction_enabled: true,
            retention_seconds: 3600,
            cleanup_interval_secs: 300,
        });
        registry.counter("a", &[]).increment();
        registry.gauge("b", &[]).set(1);
        registry.histogram_default("c", &[]).observe(5.0);
        assert_eq!(registry.cleanup_inactive_metrics(), 0);
        assert_eq!(registry.registry_size(), 3);
    }

    #[test]
    fn test_enabling_eviction_marks_existing_metrics() {
        let registry = MetricsRegistry::new();
        registry.counter("pre_existing", &[]).increment();
        registry.update_memory_config(MetricsMemoryConfig {
            eviction_enabled: true,
            retention_seconds: 3600,
            cleanup_interval_secs: 300,
        });
        assert_eq!(registry.cleanup_inactive_metrics(), 0);
    }
}
