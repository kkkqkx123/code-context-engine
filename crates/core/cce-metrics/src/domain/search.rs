//! Search engine metrics
//!
//! Tracks retrieval execution metrics for the searcher. Index storage is
//! measured by the storage backends (`bm25_*`, `qdrant_*`).

use std::sync::Arc;

use dashmap::DashMap;

use crate::{LabeledCounter, LabeledHistogram, MetricsRegistry, SearchType};

/// Search engine monitoring metrics
#[derive(Debug)]
pub struct SearchMetrics {
    pub queries_total: LabeledCounter,
    pub query_latency_ms: LabeledHistogram,
    pub queries_by_type: Arc<DashMap<String, LabeledCounter>>,
    /// Share of vector keys that also exist on the BM25 path.
    pub hybrid_alignment_match_ratio: LabeledHistogram,
    /// Share of candidates that carry no cross-path alignment key (either no
    /// key at all or the intentional chunk-id fallback).
    pub hybrid_alignment_unalignable_ratio: LabeledHistogram,
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
            queries_by_type: Arc::new(DashMap::new()),
            hybrid_alignment_match_ratio: registry.histogram_default(
                "search_hybrid_alignment_match_ratio",
                &[("project_id", &proj_val)],
            ),
            hybrid_alignment_unalignable_ratio: registry.histogram_default(
                "search_hybrid_alignment_unalignable_ratio",
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

    /// Record cross-path alignment coverage for one hybrid query.
    ///
    /// `match_ratio` uses the vector key count as the denominator (the vector
    /// path is the primary semantic recall). `unalignable_ratio` uses every
    /// observed entry as the denominator, so it reports how much of the
    /// candidate set could never align rather than how much failed to align.
    pub fn record_hybrid_alignment(
        &self,
        vector_keys: usize,
        bm25_keys: usize,
        matched: usize,
        unalignable: usize,
    ) {
        let ratio = if vector_keys == 0 {
            0.0
        } else {
            matched as f64 / vector_keys as f64
        };
        self.hybrid_alignment_match_ratio.observe(ratio);

        let total = vector_keys + bm25_keys + unalignable;
        let unalignable_ratio = if total == 0 {
            0.0
        } else {
            unalignable as f64 / total as f64
        };
        self.hybrid_alignment_unalignable_ratio
            .observe(unalignable_ratio);
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
        assert_eq!(metrics.query_latency_ms.get_count(), 0);
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
    }
}
