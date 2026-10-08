//! Qdrant-specific metrics definitions

use std::sync::Arc;

use cce_metrics::{
    LATENCY_BUCKETS, LabeledCounter, LabeledGauge, LabeledHistogram, MetricsRegistry,
};

/// Qdrant vector database monitoring metrics
///
/// Tracks performance and capacity metrics for Qdrant vector database operations.
#[derive(Debug)]
pub struct QdrantMetrics {
    /// Total number of vectors upserted
    pub vectors_upserted_total: LabeledCounter,
    /// Vector upsert latency distribution (in milliseconds)
    pub upsert_latency_ms: LabeledHistogram,
    /// Total number of search queries
    pub search_queries_total: LabeledCounter,
    /// Search query latency distribution (in milliseconds)
    pub search_latency_ms: LabeledHistogram,
    /// Total number of vectors deleted
    pub vectors_deleted_total: LabeledCounter,
    /// Deletion operation latency distribution (in milliseconds)
    pub delete_latency_ms: LabeledHistogram,
    /// Total number of operation errors
    pub errors_total: LabeledCounter,
    /// Circuit breaker state: 0=closed, 1=half-open, 2=open, 3=locked
    pub circuit_breaker_state: LabeledGauge,
    /// Total number of circuit breaker state transitions
    pub circuit_breaker_transitions_total: LabeledCounter,
    /// Current number of vectors in the Qdrant collection
    pub collection_size: LabeledGauge,
}

impl QdrantMetrics {
    /// Create new Qdrant metrics with the given registry and optional project ID
    pub fn new(registry: &MetricsRegistry, project_id: Option<i64>) -> Arc<Self> {
        let pid_str = project_id.map(|id| id.to_string());
        let labels: Vec<(&str, &str)> = pid_str
            .as_ref()
            .map(|s| vec![("project_id", s.as_str())])
            .unwrap_or_default();
        Arc::new(Self {
            vectors_upserted_total: registry.counter("qdrant_vectors_upserted_total", &labels),
            upsert_latency_ms: registry.histogram(
                "qdrant_upsert_latency_ms",
                LATENCY_BUCKETS.to_vec(),
                &labels,
            ),
            search_queries_total: registry.counter("qdrant_search_queries_total", &labels),
            search_latency_ms: registry.histogram(
                "qdrant_search_latency_ms",
                LATENCY_BUCKETS.to_vec(),
                &labels,
            ),
            vectors_deleted_total: registry.counter("qdrant_vectors_deleted_total", &labels),
            delete_latency_ms: registry.histogram(
                "qdrant_delete_latency_ms",
                LATENCY_BUCKETS.to_vec(),
                &labels,
            ),
            errors_total: registry.counter("qdrant_errors_total", &labels),
            circuit_breaker_state: registry.gauge("qdrant_circuit_breaker_state", &labels),
            circuit_breaker_transitions_total: registry
                .counter("qdrant_circuit_breaker_transitions_total", &labels),
            collection_size: registry.gauge("qdrant_collection_size", &labels),
        })
    }

    /// Record a completed vector upsert operation
    pub fn record_upsert(&self, latency_ms: f64, vector_count: usize, success: bool) {
        self.vectors_upserted_total.add(vector_count as u64);
        self.upsert_latency_ms.observe(latency_ms);

        if !success {
            self.errors_total.increment();
        }
    }

    /// Record a completed search operation
    pub fn record_search(&self, latency_ms: f64, success: bool) {
        self.search_queries_total.increment();
        self.search_latency_ms.observe(latency_ms);

        if !success {
            self.errors_total.increment();
        }
    }

    /// Record a completed deletion operation
    pub fn record_delete(&self, latency_ms: f64, vector_count: usize, success: bool) {
        self.vectors_deleted_total.add(vector_count as u64);
        self.delete_latency_ms.observe(latency_ms);

        if !success {
            self.errors_total.increment();
        }
    }

    /// Record a circuit breaker state transition.
    pub fn record_circuit_breaker_state(&self, state: &str) {
        let value: u64 = match state {
            "closed" => 0,
            "half-open" => 1,
            "open" => 2,
            _ => 3,
        };
        let prev = self.circuit_breaker_state.get();
        if prev != value {
            self.circuit_breaker_state.set(value);
            self.circuit_breaker_transitions_total.increment();
        }
    }

    /// Record current Qdrant collection size (vector count)
    pub fn record_collection_size(&self, count: u64) {
        self.collection_size.set(count);
    }
}
