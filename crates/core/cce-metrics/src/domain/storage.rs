//! Storage-level metrics for data persistence operations

use std::sync::Arc;

use crate::{LabeledCounter, LabeledFloatGauge, LabeledGauge, LabeledHistogram, MetricsRegistry};

/// BM25 storage monitoring metrics
#[derive(Debug)]
pub struct Bm25Metrics {
    pub documents_indexed_total: LabeledCounter,
    pub index_latency_ms: LabeledHistogram,
    pub documents_deleted_total: LabeledCounter,
    pub delete_latency_ms: LabeledHistogram,
    pub errors_total: LabeledCounter,
    pub index_disk_bytes: LabeledFloatGauge,
}

impl Bm25Metrics {
    pub fn new(registry: &MetricsRegistry, project_id: Option<i64>) -> Arc<Self> {
        let pid_str = project_id.map(|id| id.to_string());
        let labels: Vec<(&str, &str)> = pid_str
            .as_ref()
            .map(|s| vec![("project_id", s.as_str())])
            .unwrap_or_default();
        Arc::new(Self {
            documents_indexed_total: registry.counter("bm25_documents_indexed_total", &labels),
            index_latency_ms: registry.histogram(
                "bm25_index_latency_ms",
                crate::LATENCY_BUCKETS.to_vec(),
                &labels,
            ),
            documents_deleted_total: registry.counter("bm25_documents_deleted_total", &labels),
            delete_latency_ms: registry.histogram(
                "bm25_delete_latency_ms",
                crate::LATENCY_BUCKETS.to_vec(),
                &labels,
            ),
            errors_total: registry.counter("bm25_errors_total", &labels),
            index_disk_bytes: registry.float_gauge("bm25_index_disk_bytes", &labels),
        })
    }

    pub fn record_index(&self, latency_ms: f64, document_count: usize, success: bool) {
        self.documents_indexed_total.add(document_count as u64);
        self.index_latency_ms.observe(latency_ms);

        if !success {
            self.errors_total.increment();
        }
    }

    pub fn record_delete(&self, latency_ms: f64, document_count: usize, success: bool) {
        self.documents_deleted_total.add(document_count as u64);
        self.delete_latency_ms.observe(latency_ms);

        if !success {
            self.errors_total.increment();
        }
    }

    pub fn record_disk_usage(&self, bytes: u64) {
        self.index_disk_bytes.set(bytes as f64);
    }
}

/// Qdrant vector database monitoring metrics
#[derive(Debug)]
pub struct QdrantMetrics {
    pub vectors_upserted_total: LabeledCounter,
    pub upsert_latency_ms: LabeledHistogram,
    pub search_queries_total: LabeledCounter,
    pub search_latency_ms: LabeledHistogram,
    pub vectors_deleted_total: LabeledCounter,
    pub delete_latency_ms: LabeledHistogram,
    pub errors_total: LabeledCounter,
    pub circuit_breaker_state: LabeledGauge,
    pub circuit_breaker_transitions_total: LabeledCounter,
    pub collection_size: LabeledGauge,
}

impl QdrantMetrics {
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
                crate::LATENCY_BUCKETS.to_vec(),
                &labels,
            ),
            search_queries_total: registry.counter("qdrant_search_queries_total", &labels),
            search_latency_ms: registry.histogram(
                "qdrant_search_latency_ms",
                crate::LATENCY_BUCKETS.to_vec(),
                &labels,
            ),
            vectors_deleted_total: registry.counter("qdrant_vectors_deleted_total", &labels),
            delete_latency_ms: registry.histogram(
                "qdrant_delete_latency_ms",
                crate::LATENCY_BUCKETS.to_vec(),
                &labels,
            ),
            errors_total: registry.counter("qdrant_errors_total", &labels),
            circuit_breaker_state: registry.gauge("qdrant_circuit_breaker_state", &labels),
            circuit_breaker_transitions_total: registry
                .counter("qdrant_circuit_breaker_transitions_total", &labels),
            collection_size: registry.gauge("qdrant_collection_size", &labels),
        })
    }

    pub fn record_upsert(&self, latency_ms: f64, vector_count: usize, success: bool) {
        self.vectors_upserted_total.add(vector_count as u64);
        self.upsert_latency_ms.observe(latency_ms);

        if !success {
            self.errors_total.increment();
        }
    }

    pub fn record_search(&self, latency_ms: f64, success: bool) {
        self.search_queries_total.increment();
        self.search_latency_ms.observe(latency_ms);

        if !success {
            self.errors_total.increment();
        }
    }

    pub fn record_delete(&self, latency_ms: f64, vector_count: usize, success: bool) {
        self.vectors_deleted_total.add(vector_count as u64);
        self.delete_latency_ms.observe(latency_ms);

        if !success {
            self.errors_total.increment();
        }
    }

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

    pub fn record_collection_size(&self, count: u64) {
        self.collection_size.set(count);
    }
}

/// SQLite metadata storage monitoring metrics
#[derive(Debug)]
pub struct SqliteMetrics {
    pub read_transactions_total: LabeledCounter,
    pub write_transactions_total: LabeledCounter,
    pub transaction_latency_ms: LabeledHistogram,
    pub errors_total: LabeledCounter,
}

impl SqliteMetrics {
    pub fn new(registry: &MetricsRegistry, project_id: Option<i64>) -> Arc<Self> {
        let pid_str = project_id.map(|id| id.to_string());
        let labels: Vec<(&str, &str)> = pid_str
            .as_ref()
            .map(|s| vec![("project_id", s.as_str())])
            .unwrap_or_default();
        Arc::new(Self {
            read_transactions_total: registry.counter("sqlite_read_transactions_total", &labels),
            write_transactions_total: registry.counter("sqlite_write_transactions_total", &labels),
            transaction_latency_ms: registry.histogram(
                "sqlite_transaction_latency_ms",
                crate::LATENCY_BUCKETS.to_vec(),
                &labels,
            ),
            errors_total: registry.counter("sqlite_errors_total", &labels),
        })
    }

    pub fn record_transaction(&self, latency_ms: f64, is_write: bool, success: bool) {
        if is_write {
            self.write_transactions_total.increment();
        } else {
            self.read_transactions_total.increment();
        }
        self.transaction_latency_ms.observe(latency_ms);

        if !success {
            self.errors_total.increment();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bm25_metrics_creation() {
        let registry = MetricsRegistry::new();
        let metrics = Bm25Metrics::new(&registry, None);

        assert_eq!(metrics.documents_indexed_total.get(), 0);
        assert_eq!(metrics.documents_deleted_total.get(), 0);
        assert_eq!(metrics.errors_total.get(), 0);
    }

    #[test]
    fn test_bm25_metrics_record_index() {
        let registry = MetricsRegistry::new();
        let metrics = Bm25Metrics::new(&registry, None);

        metrics.record_index(50.0, 10, true);
        assert_eq!(metrics.documents_indexed_total.get(), 10);
        assert_eq!(metrics.index_latency_ms.get_count(), 1);
        assert_eq!(metrics.errors_total.get(), 0);

        metrics.record_index(30.0, 5, false);
        assert_eq!(metrics.documents_indexed_total.get(), 15);
        assert_eq!(metrics.errors_total.get(), 1);
    }

    #[test]
    fn test_bm25_metrics_disk_usage() {
        let registry = MetricsRegistry::new();
        let metrics = Bm25Metrics::new(&registry, Some(1));

        metrics.record_disk_usage(102400);
        assert!((metrics.index_disk_bytes.get() - 102400.0).abs() < f64::EPSILON);

        metrics.record_disk_usage(204800);
        assert!((metrics.index_disk_bytes.get() - 204800.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_qdrant_metrics_creation() {
        let registry = MetricsRegistry::new();
        let metrics = QdrantMetrics::new(&registry, None);

        assert_eq!(metrics.vectors_upserted_total.get(), 0);
        assert_eq!(metrics.search_queries_total.get(), 0);
        assert_eq!(metrics.errors_total.get(), 0);
    }

    #[test]
    fn test_qdrant_metrics_record_upsert() {
        let registry = MetricsRegistry::new();
        let metrics = QdrantMetrics::new(&registry, None);

        metrics.record_upsert(100.0, 50, true);
        assert_eq!(metrics.vectors_upserted_total.get(), 50);
        assert_eq!(metrics.upsert_latency_ms.get_count(), 1);
        assert_eq!(metrics.errors_total.get(), 0);
    }

    #[test]
    fn test_qdrant_metrics_circuit_breaker_transitions() {
        let registry = MetricsRegistry::new();
        let metrics = QdrantMetrics::new(&registry, None);

        metrics.record_circuit_breaker_state("closed");
        assert_eq!(metrics.circuit_breaker_transitions_total.get(), 0);

        metrics.record_circuit_breaker_state("open");
        assert_eq!(metrics.circuit_breaker_transitions_total.get(), 1);

        metrics.record_circuit_breaker_state("open");
        assert_eq!(metrics.circuit_breaker_transitions_total.get(), 1);

        metrics.record_circuit_breaker_state("half-open");
        assert_eq!(metrics.circuit_breaker_transitions_total.get(), 2);

        metrics.record_circuit_breaker_state("closed");
        assert_eq!(metrics.circuit_breaker_transitions_total.get(), 3);
    }

    #[test]
    fn test_qdrant_metrics_collection_size() {
        let registry = MetricsRegistry::new();
        let metrics = QdrantMetrics::new(&registry, Some(1));

        metrics.record_collection_size(500);
        assert_eq!(metrics.collection_size.get(), 500);

        metrics.record_collection_size(1000);
        assert_eq!(metrics.collection_size.get(), 1000);
    }

    #[test]
    fn test_sqlite_metrics_creation() {
        let registry = MetricsRegistry::new();
        let metrics = SqliteMetrics::new(&registry, None);

        assert_eq!(metrics.read_transactions_total.get(), 0);
        assert_eq!(metrics.write_transactions_total.get(), 0);
        assert_eq!(metrics.errors_total.get(), 0);
    }

    #[test]
    fn test_sqlite_metrics_record_transaction() {
        let registry = MetricsRegistry::new();
        let metrics = SqliteMetrics::new(&registry, None);

        metrics.record_transaction(15.0, true, true);
        assert_eq!(metrics.write_transactions_total.get(), 1);
        assert_eq!(metrics.read_transactions_total.get(), 0);
        assert_eq!(metrics.transaction_latency_ms.get_count(), 1);
        assert_eq!(metrics.errors_total.get(), 0);

        metrics.record_transaction(5.0, false, true);
        assert_eq!(metrics.read_transactions_total.get(), 1);
        assert_eq!(metrics.write_transactions_total.get(), 1);
    }

    #[test]
    fn test_storage_metrics_arc_sharing() {
        let registry = MetricsRegistry::new();
        let bm25_metrics = Bm25Metrics::new(&registry, None);

        let metrics_clone = Arc::clone(&bm25_metrics);

        metrics_clone.record_index(50.0, 10, true);

        assert_eq!(bm25_metrics.documents_indexed_total.get(), 10);
    }

    #[test]
    fn test_concurrent_metric_updates() {
        use std::thread;

        let registry = Arc::new(MetricsRegistry::new());
        let sqlite_metrics = SqliteMetrics::new(&registry, None);

        let mut handles = vec![];

        for _ in 0..10 {
            let m = Arc::clone(&sqlite_metrics);
            let handle = thread::spawn(move || {
                for i in 0..100 {
                    m.record_transaction(10.0 + (i as f64), i % 2 == 0, i % 10 != 0);
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(sqlite_metrics.read_transactions_total.get(), 500);
        assert_eq!(sqlite_metrics.write_transactions_total.get(), 500);
        assert_eq!(sqlite_metrics.errors_total.get(), 100);
    }

    #[test]
    fn test_boundary_values() {
        let registry = MetricsRegistry::new();

        let bm25_metrics = Bm25Metrics::new(&registry, None);
        bm25_metrics.record_index(0.0, 0, true);
        assert_eq!(bm25_metrics.documents_indexed_total.get(), 0);
    }
}
