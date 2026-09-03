//! Search engine metrics
//!
//! Tracks performance metrics for search and indexing operations.

use std::sync::Arc;

use dashmap::DashMap;

use crate::{LabeledCounter, LabeledGauge, LabeledHistogram, MetricsRegistry, SearchType};

/// Search engine monitoring metrics
#[derive(Debug)]
pub struct SearchMetrics {
    pub queries_total: LabeledCounter,
    pub query_latency_ms: LabeledHistogram,
    pub index_size: LabeledGauge,
    pub index_operations_total: LabeledCounter,
    pub documents_indexed_total: LabeledCounter,
    pub queries_by_type: Arc<DashMap<String, LabeledCounter>>,
    pub hybrid_alignment_match_ratio: LabeledHistogram,
    project_id_label: String,
    registry: MetricsRegistry,
}

impl SearchMetrics {
    pub fn new(registry: &MetricsRegistry, project_id: i64) -> Arc<Self> {
        let proj_val = project_id.to_string();
        Arc::new(Self {
            queries_total: registry.counter("search_queries_total", &[("project_id", &proj_val)]),
            query_latency_ms: registry
                .histogram_default("search_query_latency_ms", &[("project_id", &proj_val)]),
            index_size: registry.gauge("search_index_size", &[("project_id", &proj_val)]),
            index_operations_total: registry.counter(
                "search_index_operations_total",
                &[("project_id", &proj_val)],
            ),
            documents_indexed_total: registry.counter(
                "search_documents_indexed_total",
                &[("project_id", &proj_val)],
            ),
            queries_by_type: Arc::new(DashMap::new()),
            hybrid_alignment_match_ratio: registry.histogram_default(
                "search_hybrid_alignment_match_ratio",
                &[("project_id", &proj_val)],
            ),
            project_id_label: proj_val,
            registry: registry.clone(),
        })
    }

    pub fn record_search(&self, latency_ms: f64, query_type: Option<SearchType>) {
        self.queries_total.increment();
        self.query_latency_ms.observe(latency_ms);

        if let Some(qtype) = query_type {
            let qtype_str = qtype.to_string();
            let registry = self.registry.clone();
            let pid = self.project_id_label.clone();
            let counter = self
                .queries_by_type
                .entry(qtype_str.clone())
                .or_insert_with(|| {
                    registry.counter(
                        "search_queries_total",
                        &[("project_id", &pid), ("search_type", &qtype_str)],
                    )
                });
            counter.increment();
        }
    }

    pub fn record_index(&self, document_count: usize) {
        self.index_operations_total.increment();
        self.documents_indexed_total.add(document_count as u64);
    }

    pub fn record_hybrid_alignment(&self, vector_keys: usize, _bm25_keys: usize, matched: usize) {
        let ratio = if vector_keys == 0 {
            0.0
        } else {
            matched as f64 / vector_keys as f64
        };
        self.hybrid_alignment_match_ratio.observe(ratio);
    }

    pub fn update_index_size(&self, size: usize) {
        self.index_size.set(size as u64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SearchType;

    #[test]
    fn test_search_metrics_creation() {
        let registry = MetricsRegistry::new();
        let metrics = SearchMetrics::new(&registry, 1);

        assert_eq!(metrics.queries_total.get(), 0);
        assert_eq!(metrics.index_size.get(), 0);
    }

    #[test]
    fn test_search_metrics_record() {
        let registry = MetricsRegistry::new();
        let metrics = SearchMetrics::new(&registry, 1);

        metrics.record_search(15.5, None);
        assert_eq!(metrics.queries_total.get(), 1);
        assert_eq!(metrics.query_latency_ms.get_count(), 1);

        metrics.record_search(10.0, Some(SearchType::DenseRecall));
        assert_eq!(metrics.queries_total.get(), 2);
        assert_eq!(metrics.query_latency_ms.get_count(), 2);
        let dense_counter = metrics.queries_by_type.get("dense_recall");
        assert!(dense_counter.is_some());
        assert_eq!(dense_counter.unwrap().get(), 1);

        metrics.record_index(100);
        assert_eq!(metrics.index_operations_total.get(), 1);

        metrics.update_index_size(150);
        assert_eq!(metrics.index_size.get(), 150);
    }
}
