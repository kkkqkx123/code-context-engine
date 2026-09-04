//! Metrics configuration types

use serde::{Deserialize, Serialize};

use cce_types::error::config::ConfigValidationError;

/// Metrics configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetricsConfig {
    /// Aggregation configuration
    #[serde(default)]
    pub aggregation: MetricsAggregationConfig,
    /// Label validation configuration
    #[serde(default)]
    pub labels: cce_metrics::config::MetricsLabelConfig,
    /// Memory optimization configuration
    #[serde(default)]
    pub memory: cce_metrics::config::MetricsMemoryConfig,
}

/// Metrics aggregation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsAggregationConfig {
    /// Whether to enable automatic aggregation (default: true)
    #[serde(default = "default_aggregation_enabled")]
    pub enabled: bool,
    /// Aggregation interval in seconds (default: 300 = 5 minutes)
    #[serde(default = "default_aggregation_interval")]
    pub interval_secs: u64,
    /// Retention period in seconds for aggregated data (default: 604800 = 7 days)
    #[serde(default = "default_aggregation_retention")]
    pub retention_seconds: u64,
    /// Cleanup interval in seconds (default: 3600 = 1 hour)
    #[serde(default = "default_aggregation_cleanup_interval")]
    pub cleanup_interval_secs: u64,
    /// Whether to aggregate counter deltas (default: true)
    #[serde(default = "default_aggregate_counters")]
    pub aggregate_counters: bool,
    /// Whether to aggregate gauge snapshots (default: true)
    #[serde(default = "default_aggregate_gauges")]
    pub aggregate_gauges: bool,
    /// Rows per SQLite write transaction (default: 100)
    #[serde(default = "default_aggregation_batch_size")]
    pub batch_size: usize,
    /// Default interval applied to metrics without an override (seconds).
    /// Falls back to `interval_secs` when zero.
    #[serde(default)]
    pub default_interval_secs: u64,
    /// Per-metric aggregation overrides keyed by metric name.
    #[serde(default)]
    pub metric_overrides:
        std::collections::HashMap<String, cce_metrics::config::MetricAggregationOverride>,
}

impl Default for MetricsAggregationConfig {
    fn default() -> Self {
        Self {
            enabled: default_aggregation_enabled(),
            interval_secs: default_aggregation_interval(),
            retention_seconds: default_aggregation_retention(),
            cleanup_interval_secs: default_aggregation_cleanup_interval(),
            aggregate_counters: default_aggregate_counters(),
            aggregate_gauges: default_aggregate_gauges(),
            batch_size: default_aggregation_batch_size(),
            default_interval_secs: 0,
            metric_overrides: std::collections::HashMap::new(),
        }
    }
}

impl MetricsAggregationConfig {
    /// Effective default interval for metrics without an override.
    pub fn effective_default_interval_secs(&self) -> u64 {
        if self.default_interval_secs > 0 {
            self.default_interval_secs
        } else {
            self.interval_secs
        }
    }

    /// Validate aggregation tuning parameters.
    pub fn validate_metrics_aggregation(&self) -> Result<(), ConfigValidationError> {
        if self.enabled && self.interval_secs == 0 {
            return Err(ConfigValidationError::invalid_field(
                "metrics.aggregation.interval_secs",
                "must be greater than 0 when aggregation is enabled",
            ));
        }
        if self.batch_size == 0 {
            return Err(ConfigValidationError::invalid_field(
                "metrics.aggregation.batch_size",
                "must be greater than 0",
            ));
        }
        for (name, metric_override) in &self.metric_overrides {
            if let Some(interval) = metric_override.interval_secs {
                if interval == 0 {
                    return Err(ConfigValidationError::invalid_field(
                        format!("metrics.aggregation.metric_overrides.{name}.interval_secs"),
                        "must be greater than 0",
                    ));
                }
            }
        }
        Ok(())
    }
}

fn default_aggregation_enabled() -> bool {
    true
}

fn default_aggregation_interval() -> u64 {
    300
}

fn default_aggregation_retention() -> u64 {
    604800
}

fn default_aggregation_cleanup_interval() -> u64 {
    3600
}

fn default_aggregate_counters() -> bool {
    true
}

fn default_aggregate_gauges() -> bool {
    true
}

fn default_aggregation_batch_size() -> usize {
    100
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_metrics_labels_config_toml_loading() {
        let toml_str = r#"
[metrics.labels]
allowed_keys = ["operation", "custom_key"]
strict = true

[metrics.labels.static_enums]
search_type = ["bm25", "keyword"]
"#;
        let config: crate::AppConfig = toml::from_str(toml_str).expect("metrics labels must parse");
        assert_eq!(
            config.metrics.labels.allowed_keys,
            vec!["operation".to_string(), "custom_key".to_string()]
        );
        assert!(config.metrics.labels.strict);
        assert_eq!(
            config.metrics.labels.static_enums.get("search_type"),
            Some(&vec!["bm25".to_string(), "keyword".to_string()])
        );
        assert!(
            config
                .metrics
                .aggregation
                .validate_metrics_aggregation()
                .is_ok()
        );
    }

    #[test]
    fn test_metrics_aggregation_batch_and_overrides() {
        let toml_str = r#"
[metrics.aggregation]
enabled = true
interval_secs = 300
batch_size = 50
default_interval_secs = 120

[metrics.aggregation.metric_overrides]
embedding_latency_ms = { interval_secs = 60 }
tokio_workers_total = { enabled = false }
"#;
        let config: crate::AppConfig = toml::from_str(toml_str).expect("aggregation must parse");
        assert_eq!(config.metrics.aggregation.batch_size, 50);
        assert_eq!(config.metrics.aggregation.default_interval_secs, 120);
        assert_eq!(
            config
                .metrics
                .aggregation
                .metric_overrides
                .get("embedding_latency_ms")
                .and_then(|o| o.interval_secs),
            Some(60)
        );
        assert_eq!(
            config
                .metrics
                .aggregation
                .metric_overrides
                .get("tokio_workers_total")
                .and_then(|o| o.enabled),
            Some(false)
        );
        assert!(
            config
                .metrics
                .aggregation
                .validate_metrics_aggregation()
                .is_ok()
        );
    }

    #[test]
    fn test_metrics_aggregation_validation_rejects_bad_config() {
        let mut config = crate::AppConfig::default();
        config.metrics.aggregation.batch_size = 0;
        assert!(
            config
                .metrics
                .aggregation
                .validate_metrics_aggregation()
                .is_err()
        );

        let mut config = crate::AppConfig::default();
        config.metrics.aggregation.metric_overrides.insert(
            "bad".to_string(),
            cce_metrics::config::MetricAggregationOverride {
                interval_secs: Some(0),
                retention_seconds: None,
                enabled: None,
            },
        );
        assert!(
            config
                .metrics
                .aggregation
                .validate_metrics_aggregation()
                .is_err()
        );
    }

    #[test]
    fn test_metrics_memory_config_defaults() {
        let config = crate::AppConfig::default();
        assert!(!config.metrics.memory.eviction_enabled);
        assert_eq!(config.metrics.memory.retention_seconds, 3600);
        assert_eq!(config.metrics.aggregation.batch_size, 100);
        assert!(config.metrics.aggregation.aggregate_counters);
        assert!(config.metrics.aggregation.aggregate_gauges);
    }
}
