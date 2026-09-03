//! Label system for metrics
//!
//! This module provides a simple but powerful label system for multi-dimensional metrics.
//! Labels allow you to track metrics across different dimensions (e.g., by provider, method, status).

use std::collections::BTreeMap;
use std::fmt;

/// A single label key-value pair
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Label {
    pub key: String,
    pub value: String,
}

impl Label {
    /// Create a new label
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
        }
    }
}

impl<K, V> From<(K, V)> for Label
where
    K: Into<String>,
    V: Into<String>,
{
    fn from((key, value): (K, V)) -> Self {
        Self::new(key, value)
    }
}

impl fmt::Display for Label {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}=\"{}\"", self.key, self.value)
    }
}

/// A collection of labels (sorted by key using BTreeMap for efficiency and determinism)
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Labels {
    labels: BTreeMap<String, String>,
}

impl Labels {
    /// Create an empty label collection
    pub fn new() -> Self {
        Self {
            labels: BTreeMap::new(),
        }
    }

    /// Create labels from key-value pairs
    pub fn from_pairs(pairs: &[(&str, &str)]) -> Self {
        let mut labels = BTreeMap::new();
        for (k, v) in pairs {
            labels.insert(k.to_string(), v.to_string());
        }
        Self { labels }
    }

    /// Add a single label
    pub fn add(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(key.into(), value.into());
        self
    }

    /// Merge with another label collection (other takes precedence on conflict)
    pub fn merge(mut self, other: Labels) -> Self {
        self.labels.extend(other.labels);
        self
    }

    /// Get the number of labels
    pub fn len(&self) -> usize {
        self.labels.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.labels.is_empty()
    }

    /// Iterate over labels
    pub fn iter(&self) -> impl Iterator<Item = Label> + '_ {
        self.labels.iter().map(|(k, v)| Label::new(k, v))
    }

    /// Convert to HashMap (for fast lookup)
    pub fn to_hashmap(&self) -> std::collections::HashMap<&str, &str> {
        self.labels
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect()
    }
}

impl fmt::Display for Labels {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let labels_str: Vec<String> = self
            .labels
            .iter()
            .map(|(k, v)| format!("{}=\"{}\"", k, v))
            .collect();
        write!(f, "{{{}}}", labels_str.join(","))
    }
}

/// A metric key with labels
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MetricKey {
    pub name: String,
    pub labels: Labels,
}

impl MetricKey {
    /// Create a new metric key
    pub fn new(name: impl Into<String>, labels: Labels) -> Self {
        Self {
            name: name.into(),
            labels,
        }
    }

    /// Generate a unique string identifier (for internal storage)
    pub fn to_storage_key(&self) -> String {
        if self.labels.is_empty() {
            self.name.clone()
        } else {
            format!("{}{}", self.name, self.labels)
        }
    }
}

impl fmt::Display for MetricKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.labels.is_empty() {
            write!(f, "{}", self.name)
        } else {
            write!(f, "{}{}", self.name, self.labels)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_labels_creation() {
        let labels = Labels::from_pairs(&[("method", "GET"), ("status", "200")]);
        assert_eq!(labels.len(), 2);
    }

    #[test]
    fn test_labels_ordering() {
        // Different order should produce the same result
        let labels1 = Labels::from_pairs(&[("b", "2"), ("a", "1")]);
        let labels2 = Labels::from_pairs(&[("a", "1"), ("b", "2")]);
        assert_eq!(labels1, labels2);
    }

    #[test]
    fn test_metric_key_storage() {
        let key = MetricKey::new("http_requests", Labels::from_pairs(&[("method", "GET")]));
        assert_eq!(key.to_storage_key(), "http_requests{method=\"GET\"}");
    }

    #[test]
    fn test_labels_display() {
        let labels = Labels::from_pairs(&[("method", "GET"), ("status", "200")]);
        let display = format!("{}", labels);
        assert!(display.contains("method=\"GET\""));
        assert!(display.contains("status=\"200\""));
    }

    #[test]
    fn test_labels_merge() {
        let labels1 = Labels::from_pairs(&[("a", "1")]);
        let labels2 = Labels::from_pairs(&[("b", "2")]);
        let merged = labels1.merge(labels2);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn test_labels_to_hashmap() {
        let labels = Labels::from_pairs(&[("key", "value")]);
        let map = labels.to_hashmap();
        assert_eq!(map.get("key"), Some(&"value"));
    }

    #[test]
    fn test_metric_key_without_labels() {
        let key = MetricKey::new("simple_metric", Labels::new());
        assert_eq!(key.to_storage_key(), "simple_metric");
        assert_eq!(format!("{}", key), "simple_metric");
    }

    #[test]
    fn test_labels_empty() {
        let labels = Labels::new();
        assert!(labels.is_empty());
        assert_eq!(labels.len(), 0);
    }

    #[test]
    fn test_labels_from_pairs_empty() {
        let labels = Labels::from_pairs(&[]);
        assert!(labels.is_empty());
    }

    #[test]
    fn test_labels_add_single() {
        let labels = Labels::new().add("key", "value");
        assert_eq!(labels.len(), 1);
        assert!(!labels.is_empty());
    }

    #[test]
    fn test_labels_merge_duplicate_keys() {
        let labels1 = Labels::from_pairs(&[("key", "value1")]);
        let labels2 = Labels::from_pairs(&[("key", "value2")]);
        let merged = labels1.merge(labels2);

        // Merge extends, so the second map's value should overwrite the first
        assert_eq!(merged.len(), 1);
        let hashmap = merged.to_hashmap();
        assert_eq!(hashmap.get("key"), Some(&"value2"));
    }

    #[test]
    fn test_labels_to_hashmap_empty() {
        let labels = Labels::new();
        let map = labels.to_hashmap();
        assert!(map.is_empty());
    }

    #[test]
    fn test_labels_display_empty() {
        let labels = Labels::new();
        let display = format!("{}", labels);
        assert_eq!(display, "{}");
    }

    #[test]
    fn test_labels_display_single() {
        let labels = Labels::from_pairs(&[("key", "value")]);
        let display = format!("{}", labels);
        assert!(display.contains("key=\"value\""));
    }

    #[test]
    fn test_metric_key_display_with_labels() {
        let key = MetricKey::new("test_metric", Labels::from_pairs(&[("label", "value")]));
        let display = format!("{}", key);
        assert!(display.contains("test_metric"));
        assert!(display.contains("label=\"value\""));
    }

    #[test]
    fn test_metric_key_storage_key_consistency() {
        let key1 = MetricKey::new("metric", Labels::from_pairs(&[("b", "2"), ("a", "1")]));
        let key2 = MetricKey::new("metric", Labels::from_pairs(&[("a", "1"), ("b", "2")]));

        // Different order should produce same storage key due to sorting
        assert_eq!(key1.to_storage_key(), key2.to_storage_key());
    }

    #[test]
    fn test_labels_special_characters() {
        let labels = Labels::from_pairs(&[
            ("key_with_underscore", "value-with-dash"),
            ("key.with.dots", "value/with/slashes"),
        ]);

        let display = format!("{}", labels);
        assert!(display.contains("key_with_underscore"));
        assert!(display.contains("value-with-dash"));
    }

    #[test]
    fn test_metric_key_special_characters() {
        let key = MetricKey::new(
            "metric.name_with.special:chars",
            Labels::from_pairs(&[("label", "value")]),
        );

        let storage_key = key.to_storage_key();
        assert!(storage_key.contains("metric.name_with.special:chars"));
    }
}
