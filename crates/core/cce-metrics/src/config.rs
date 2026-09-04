//! Configuration types for the metrics subsystem.
//!
//! These types are owned by `cce-metrics` so the registry can stay free of a
//! dependency on `cce-config`. `cce-config` re-uses them inside its
//! `MetricsConfig` section, keeping a single source of truth for defaults.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Default label keys accepted by the registry.
///
/// Covers the original hardcoded allowlist plus the keys already used by
/// built-in domain metrics (`capability` for plugins, `method`/`path`/
/// `status_class` for HTTP, `format` for export self-monitoring).
pub fn default_allowed_label_keys() -> Vec<String> {
    vec![
        "operation".to_string(),
        "component".to_string(),
        "status".to_string(),
        "project_id".to_string(),
        "language".to_string(),
        "provider".to_string(),
        "error_type".to_string(),
        "search_type".to_string(),
        "stage".to_string(),
        "reason".to_string(),
        "capability".to_string(),
        "method".to_string(),
        "path".to_string(),
        "status_class".to_string(),
        "format".to_string(),
    ]
}

fn default_strict_labels() -> bool {
    false
}

/// Label validation configuration for [`crate::MetricsRegistry`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MetricsLabelConfig {
    /// Label keys accepted by the registry.
    #[serde(default = "default_allowed_label_keys")]
    pub allowed_keys: Vec<String>,
    /// When true, an invalid label key panics; otherwise it only logs.
    #[serde(default = "default_strict_labels")]
    pub strict: bool,
    /// Optional per-key value allowlists (`key -> accepted values`).
    #[serde(default)]
    pub static_enums: HashMap<String, Vec<String>>,
}

impl Default for MetricsLabelConfig {
    fn default() -> Self {
        Self {
            allowed_keys: default_allowed_label_keys(),
            strict: default_strict_labels(),
            static_enums: HashMap::new(),
        }
    }
}

impl MetricsLabelConfig {
    /// Create a config with a custom key list (non-strict, no enums).
    pub fn with_allowed_keys(keys: Vec<String>) -> Self {
        Self {
            allowed_keys: keys,
            strict: false,
            static_enums: HashMap::new(),
        }
    }

    /// Check whether a label key is allowed.
    pub fn is_key_allowed(&self, key: &str) -> bool {
        self.allowed_keys.iter().any(|k| k == key)
    }

    /// Check whether a label value is accepted for a key.
    ///
    /// Keys without an entry in `static_enums` accept any value.
    pub fn is_value_allowed(&self, key: &str, value: &str) -> bool {
        match self.static_enums.get(key) {
            None => true,
            Some(allowed) => allowed.iter().any(|v| v == value),
        }
    }
}

/// Per-metric aggregation override (dynamic aggregation windows).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MetricAggregationOverride {
    /// Override the aggregation interval for this metric (seconds).
    pub interval_secs: Option<u64>,
    /// Override the retention period for this metric (seconds).
    pub retention_seconds: Option<u64>,
    /// Whether this metric participates in aggregation.
    pub enabled: Option<bool>,
}

/// Memory optimization configuration for the registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MetricsMemoryConfig {
    /// Enable eviction of inactive metrics (default false).
    #[serde(default = "default_eviction_enabled")]
    pub eviction_enabled: bool,
    /// Time without access before a metric becomes evictable (seconds).
    #[serde(default = "default_metric_retention")]
    pub retention_seconds: u64,
    /// How often the cleanup task scans for inactive metrics (seconds).
    #[serde(default = "default_memory_cleanup_interval")]
    pub cleanup_interval_secs: u64,
}

impl Default for MetricsMemoryConfig {
    fn default() -> Self {
        Self {
            eviction_enabled: default_eviction_enabled(),
            retention_seconds: default_metric_retention(),
            cleanup_interval_secs: default_memory_cleanup_interval(),
        }
    }
}

fn default_eviction_enabled() -> bool {
    false
}

fn default_metric_retention() -> u64 {
    3600
}

fn default_memory_cleanup_interval() -> u64 {
    300
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_allowed_keys_cover_builtin_usage() {
        let config = MetricsLabelConfig::default();
        for key in [
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
            "capability",
            "method",
            "path",
            "status_class",
            "format",
        ] {
            assert!(config.is_key_allowed(key), "missing default key {key}");
        }
        assert!(!config.strict);
    }

    #[test]
    fn test_static_enum_validation() {
        let mut config = MetricsLabelConfig::default();
        config.static_enums.insert(
            "search_type".to_string(),
            vec!["dense_recall".to_string(), "bm25".to_string()],
        );
        assert!(config.is_value_allowed("search_type", "bm25"));
        assert!(!config.is_value_allowed("search_type", "other"));
        assert!(config.is_value_allowed("provider", "anything"));
    }

    #[test]
    fn test_memory_config_defaults_are_conservative() {
        let config = MetricsMemoryConfig::default();
        assert!(!config.eviction_enabled);
        assert_eq!(config.retention_seconds, 3600);
        assert_eq!(config.cleanup_interval_secs, 300);
    }

    #[test]
    fn test_label_config_serde_roundtrip() {
        let config = MetricsLabelConfig::default();
        let json = serde_json::to_string(&config).expect("serialize label config");
        let parsed: MetricsLabelConfig =
            serde_json::from_str(&json).expect("deserialize label config");
        assert_eq!(parsed.allowed_keys, config.allowed_keys);
        assert_eq!(parsed.strict, config.strict);
    }
}
