//! Core metric primitive types
//!
//! This module provides atomic-backed metric primitives (Counter, Gauge, Histogram)
//! and static label enums to prevent cardinality explosion.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::labels::Labels;

/// Error type classification for embedding operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EmbeddingErrorType {
    Timeout,
    RateLimited,
    Authentication,
    ServiceUnavailable,
    InvalidRequest,
    Unknown,
}

impl std::fmt::Display for EmbeddingErrorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Timeout => write!(f, "timeout"),
            Self::RateLimited => write!(f, "rate_limited"),
            Self::Authentication => write!(f, "authentication"),
            Self::ServiceUnavailable => write!(f, "service_unavailable"),
            Self::InvalidRequest => write!(f, "invalid_request"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

/// Pipeline stage classification for intermediate processing steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PipelineStage {
    Grouper,
    Converter,
    Chunker,
}

impl std::fmt::Display for PipelineStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Grouper => write!(f, "grouper"),
            Self::Converter => write!(f, "converter"),
            Self::Chunker => write!(f, "chunker"),
        }
    }
}

/// Static label for search query types.
///
/// Used to classify search queries by their execution strategy,
/// preventing cardinality explosion on the `search_type` label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SearchType {
    DenseRecall,
    SparseRecall,
    HybridRecall,
    Keyword,
    BM25,
}

impl std::fmt::Display for SearchType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DenseRecall => write!(f, "dense_recall"),
            Self::SparseRecall => write!(f, "sparse_recall"),
            Self::HybridRecall => write!(f, "hybrid_recall"),
            Self::Keyword => write!(f, "keyword"),
            Self::BM25 => write!(f, "bm25"),
        }
    }
}

impl SearchType {
    /// Try to parse a search type from a string label.
    ///
    /// Used for converting dynamic strategy labels to typed metrics labels.
    /// Returns `None` if the label doesn't match any known type.
    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "dense_recall" => Some(Self::DenseRecall),
            "hybrid_recall" => Some(Self::HybridRecall),
            "bm25_recall" | "sparse_recall" => Some(Self::BM25),
            "keyword" => Some(Self::Keyword),
            _ => None,
        }
    }
}

/// Progress phase for indexing categorization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProgressPhase {
    /// File scanning phase
    Scanning,
    /// File parsing phase
    Parsing,
    /// Embedding/vector storage phase
    Embedding,
    /// Relation building phase
    RelationBuilding,
    /// Summary generation phase
    SummaryGeneration,
}

impl std::fmt::Display for ProgressPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProgressPhase::Scanning => write!(f, "scanning"),
            ProgressPhase::Parsing => write!(f, "parsing"),
            ProgressPhase::Embedding => write!(f, "embedding"),
            ProgressPhase::RelationBuilding => write!(f, "relation_building"),
            ProgressPhase::SummaryGeneration => write!(f, "summary_generation"),
        }
    }
}

/// A labeled counter metric
#[derive(Debug)]
pub struct LabeledCounter {
    value: Arc<AtomicU64>,
    labels: Labels,
}

impl Clone for LabeledCounter {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            labels: self.labels.clone(),
        }
    }
}

impl LabeledCounter {
    /// Create a new labeled counter
    pub fn new(labels: Labels) -> Self {
        Self {
            value: Arc::new(AtomicU64::new(0)),
            labels,
        }
    }

    /// Increment the counter by 1
    pub fn increment(&self) {
        self.value.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment the counter by a specific amount
    pub fn increment_by(&self, amount: u64) {
        self.value.fetch_add(amount, Ordering::Relaxed);
    }

    /// Add a value to the counter (alias for increment_by)
    pub fn add(&self, value: u64) {
        self.value.fetch_add(value, Ordering::Relaxed);
    }

    /// Get the current value
    pub fn get(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }

    /// Get the labels
    pub fn labels(&self) -> &Labels {
        &self.labels
    }
}

/// A labeled gauge metric for integer values
#[derive(Debug)]
pub struct LabeledGauge {
    value: Arc<AtomicU64>,
    labels: Labels,
}

impl Clone for LabeledGauge {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            labels: self.labels.clone(),
        }
    }
}

impl LabeledGauge {
    /// Create a new labeled gauge
    pub fn new(labels: Labels) -> Self {
        Self {
            value: Arc::new(AtomicU64::new(0)),
            labels,
        }
    }

    /// Set the gauge value
    pub fn set(&self, val: u64) {
        self.value.store(val, Ordering::Relaxed);
    }

    /// Increment the gauge by 1
    pub fn increment(&self) {
        self.value.fetch_add(1, Ordering::Relaxed);
    }

    /// Decrement the gauge by 1, saturating at 0
    pub fn decrement(&self) {
        self.value
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
                Some(v.saturating_sub(1))
            })
            .ok();
    }

    /// Increment the gauge by a specific amount
    pub fn increment_by(&self, amount: u64) {
        self.value.fetch_add(amount, Ordering::Relaxed);
    }

    /// Get the current value
    pub fn get(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }

    /// Get the labels
    pub fn labels(&self) -> &Labels {
        &self.labels
    }
}

/// A labeled gauge metric for floating-point values
///
/// This type stores f64 values using atomic operations by converting
/// the float to its bit representation as u64. This allows for
/// thread-safe storage of percentages, ratios, and averages without
/// precision loss.
#[derive(Debug)]
pub struct LabeledFloatGauge {
    value: Arc<AtomicU64>,
    labels: Labels,
}

impl Clone for LabeledFloatGauge {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            labels: self.labels.clone(),
        }
    }
}

impl LabeledFloatGauge {
    /// Create a new labeled float gauge initialized to 0.0
    pub fn new(labels: Labels) -> Self {
        Self {
            value: Arc::new(AtomicU64::new(0.0f64.to_bits())),
            labels,
        }
    }

    /// Set the gauge value
    ///
    /// # Arguments
    ///
    /// * `val` - The floating-point value to set
    pub fn set(&self, val: f64) {
        self.value.store(val.to_bits(), Ordering::Relaxed);
    }

    /// Get the current value
    pub fn get(&self) -> f64 {
        f64::from_bits(self.value.load(Ordering::Relaxed))
    }

    /// Get the labels
    pub fn labels(&self) -> &Labels {
        &self.labels
    }
}

/// A labeled histogram metric
#[derive(Debug)]
pub struct LabeledHistogram {
    buckets: Vec<f64>,
    counts: Vec<Arc<AtomicU64>>,
    sum: Arc<AtomicU64>,
    count: Arc<AtomicU64>,
    /// Count of observations that exceed all finite buckets (for +Inf in Prometheus)
    overflow: Arc<AtomicU64>,
    /// Cumulative maximum observed value in microseconds
    max: Arc<AtomicU64>,
    /// Maximum observed value since the last window read (microseconds)
    window_max: Arc<AtomicU64>,
    labels: Labels,
}

impl Clone for LabeledHistogram {
    fn clone(&self) -> Self {
        Self {
            buckets: self.buckets.clone(),
            counts: self.counts.clone(),
            sum: self.sum.clone(),
            count: self.count.clone(),
            overflow: self.overflow.clone(),
            max: self.max.clone(),
            window_max: self.window_max.clone(),
            labels: self.labels.clone(),
        }
    }
}

impl LabeledHistogram {
    /// Create a new labeled histogram
    pub fn new(buckets: Vec<f64>, labels: Labels) -> Self {
        let counts = buckets
            .iter()
            .map(|_| Arc::new(AtomicU64::new(0)))
            .collect();
        Self {
            buckets,
            counts,
            sum: Arc::new(AtomicU64::new(0)),
            count: Arc::new(AtomicU64::new(0)),
            overflow: Arc::new(AtomicU64::new(0)),
            max: Arc::new(AtomicU64::new(0)),
            window_max: Arc::new(AtomicU64::new(0)),
            labels,
        }
    }

    /// Observe a value (in milliseconds)
    pub fn observe(&self, value_ms: f64) {
        let value_ms = value_ms.max(0.0);
        let value_us = (value_ms * 1000.0) as u64;
        self.sum.fetch_add(value_us, Ordering::Relaxed);
        self.count.fetch_add(1, Ordering::Relaxed);
        self.max.fetch_max(value_us, Ordering::Relaxed);
        self.window_max.fetch_max(value_us, Ordering::Relaxed);

        // Find the appropriate bucket
        for (i, &bucket) in self.buckets.iter().enumerate() {
            if value_ms <= bucket {
                self.counts[i].fetch_add(1, Ordering::Relaxed);
                return;
            }
        }

        // Value exceeds all finite buckets — track as overflow for +Inf
        self.overflow.fetch_add(1, Ordering::Relaxed);
    }

    /// Get the total count of observations
    pub fn get_count(&self) -> u64 {
        self.count.load(Ordering::Relaxed)
    }

    /// Get the sum of all observed values (in microseconds)
    pub fn get_sum(&self) -> u64 {
        self.sum.load(Ordering::Relaxed)
    }

    /// Get the cumulative maximum observed value (in milliseconds)
    pub fn get_max_ms(&self) -> f64 {
        self.max.load(Ordering::Relaxed) as f64 / 1000.0
    }

    /// Read and reset the window maximum (in milliseconds)
    ///
    /// The reset is best-effort: if a larger value is observed between the
    /// load and the compare-and-swap, the exchange fails and the new value is
    /// kept for the next window. A smaller racing observation may be lost,
    /// which is acceptable since it cannot dominate the window maximum.
    pub fn take_window_max_ms(&self) -> f64 {
        let current = self.window_max.load(Ordering::Relaxed);
        if current == 0 {
            return 0.0;
        }
        if self
            .window_max
            .compare_exchange(current, 0, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
            current as f64 / 1000.0
        } else {
            self.window_max.load(Ordering::Relaxed) as f64 / 1000.0
        }
    }

    /// Calculate the average value (in milliseconds)
    pub fn get_average(&self) -> f64 {
        let count = self.count.load(Ordering::Relaxed);
        if count == 0 {
            return 0.0;
        }
        let sum_us = self.sum.load(Ordering::Relaxed);
        (sum_us as f64 / count as f64) / 1000.0
    }

    /// Calculate a specific percentile value (in milliseconds)
    pub fn percentile(&self, p: f64) -> f64 {
        let total = self.count.load(Ordering::Relaxed);
        if total == 0 {
            return 0.0;
        }

        let target = (total as f64 * p / 100.0) as u64;
        let mut cumulative = 0u64;

        for (i, count) in self.counts.iter().enumerate() {
            cumulative += count.load(Ordering::Relaxed);
            if cumulative >= target {
                return self.buckets[i];
            }
        }

        // If we haven't found it, return the last bucket
        self.buckets.last().copied().unwrap_or(0.0)
    }

    /// Get P50 (median) value in milliseconds
    pub fn p50(&self) -> f64 {
        self.percentile(50.0)
    }

    /// Get P90 value in milliseconds
    pub fn p90(&self) -> f64 {
        self.percentile(90.0)
    }

    /// Get P95 value in milliseconds
    pub fn p95(&self) -> f64 {
        self.percentile(95.0)
    }

    /// Get P99 value in milliseconds
    pub fn p99(&self) -> f64 {
        self.percentile(99.0)
    }

    /// Get bucket counts as a vector
    ///
    /// Each position contains the count of observations where value <= bucket boundary.
    /// Unlike Prometheus cumulative buckets, these are per-bucket counts (not cumulative).
    pub fn get_bucket_counts(&self) -> Vec<u64> {
        self.counts
            .iter()
            .map(|c| c.load(Ordering::Relaxed))
            .collect()
    }

    /// Get the count of observations that exceed all finite buckets
    pub fn get_overflow_count(&self) -> u64 {
        self.overflow.load(Ordering::Relaxed)
    }

    /// Get bucket boundaries
    pub fn get_buckets(&self) -> &[f64] {
        &self.buckets
    }

    /// Get the labels
    pub fn labels(&self) -> &Labels {
        &self.labels
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== LabeledCounter Tests ====================

    #[test]
    fn test_labeled_counter_increment() {
        let labels = Labels::from_pairs(&[("test", "label")]);
        let counter = LabeledCounter::new(labels);

        assert_eq!(counter.get(), 0);

        counter.increment();
        assert_eq!(counter.get(), 1);

        counter.add(5);
        assert_eq!(counter.get(), 6);
    }

    #[test]
    fn test_labeled_counter_labels_access() {
        let labels = Labels::from_pairs(&[("key", "value")]);
        let counter = LabeledCounter::new(labels.clone());

        assert_eq!(counter.labels(), &labels);
    }

    // ==================== LabeledGauge Tests ====================

    #[test]
    fn test_labeled_gauge_set_get() {
        let labels = Labels::from_pairs(&[("test", "label")]);
        let gauge = LabeledGauge::new(labels);

        gauge.set(42);
        assert_eq!(gauge.get(), 42);

        gauge.set(100);
        assert_eq!(gauge.get(), 100);
    }

    #[test]
    fn test_labeled_gauge_labels_access() {
        let labels = Labels::from_pairs(&[("key", "value")]);
        let gauge = LabeledGauge::new(labels.clone());

        assert_eq!(gauge.labels(), &labels);
    }

    // ==================== LabeledHistogram Tests ====================

    #[test]
    fn test_labeled_histogram_observe() {
        let labels = Labels::from_pairs(&[("test", "label")]);
        let hist = LabeledHistogram::new(vec![10.0, 50.0, 100.0], labels);

        hist.observe(25.0);
        assert_eq!(hist.get_count(), 1);
        assert!(hist.get_average() > 0.0);
    }

    #[test]
    fn test_labeled_histogram_labels_access() {
        let labels = Labels::from_pairs(&[("key", "value")]);
        let hist = LabeledHistogram::new(vec![10.0, 50.0], labels.clone());

        assert_eq!(hist.labels(), &labels);
    }

    #[test]
    fn test_labeled_histogram_percentiles() {
        let labels = Labels::new();
        let hist = LabeledHistogram::new(vec![10.0, 50.0, 100.0], labels);

        for i in 1..=100 {
            hist.observe(i as f64);
        }

        let p50 = hist.p50();
        let p90 = hist.p90();
        let p99 = hist.p99();

        assert!(p50 <= p90);
        assert!(p90 <= p99);
    }

    #[test]
    fn test_labeled_histogram_with_empty_buckets() {
        let labels = Labels::new();
        let hist = LabeledHistogram::new(vec![], labels);
        hist.observe(10.0);

        assert_eq!(hist.get_count(), 1);
        assert_eq!(hist.get_bucket_counts().len(), 0);
        assert_eq!(hist.get_overflow_count(), 1);
    }

    #[test]
    fn test_labeled_histogram_overflow() {
        let labels = Labels::new();
        let hist = LabeledHistogram::new(vec![10.0, 50.0, 100.0], labels);

        hist.observe(5.0);
        hist.observe(200.0);
        hist.observe(1000.0);

        assert_eq!(hist.get_count(), 3);
        assert_eq!(hist.get_bucket_counts(), vec![1, 0, 0]);
        assert_eq!(hist.get_overflow_count(), 2);
    }
}
