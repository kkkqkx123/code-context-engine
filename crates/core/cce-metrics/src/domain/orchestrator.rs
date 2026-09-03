//! Orchestrator-level metrics for system coordination
//!
//! This module provides high-level metrics for system coordinators that manage
//! multiple subsystems. These metrics track overall system behavior and
//! coordination efficiency rather than individual operation performance.

use std::sync::Arc;

use crate::{LabeledCounter, LabeledFloatGauge, LabeledGauge, LabeledHistogram, MetricsRegistry};

/// Query engine monitoring metrics
///
/// Tracks performance and efficiency metrics for query execution operations.
#[derive(Debug)]
pub struct QueryMetrics {
    pub queries_total: LabeledCounter,
    pub query_latency_ms: LabeledHistogram,
    pub cache_hits_total: LabeledCounter,
    pub cache_misses_total: LabeledCounter,
    pub cache_hit_rate: LabeledFloatGauge,
    pub results_returned_total: LabeledCounter,
}

impl QueryMetrics {
    pub fn new(registry: &MetricsRegistry, project_id: i64) -> Arc<Self> {
        let proj_val = project_id.to_string();
        Arc::new(Self {
            queries_total: registry.counter("queries_executed_total", &[("project_id", &proj_val)]),
            query_latency_ms: registry
                .histogram_default("query_execution_latency_ms", &[("project_id", &proj_val)]),
            cache_hits_total: registry
                .counter("query_cache_hits_total", &[("project_id", &proj_val)]),
            cache_misses_total: registry
                .counter("query_cache_misses_total", &[("project_id", &proj_val)]),
            cache_hit_rate: registry
                .float_gauge("query_cache_hit_rate", &[("project_id", &proj_val)]),
            results_returned_total: registry
                .counter("query_results_returned_total", &[("project_id", &proj_val)]),
        })
    }

    pub fn record_query(&self, latency_ms: f64, cache_hit: bool, result_count: usize) {
        self.queries_total.increment();
        self.query_latency_ms.observe(latency_ms);
        self.results_returned_total.add(result_count as u64);

        if cache_hit {
            self.cache_hits_total.increment();
        } else {
            self.cache_misses_total.increment();
        }

        let hits = self.cache_hits_total.get() as f64;
        let misses = self.cache_misses_total.get() as f64;
        let total = hits + misses;

        if total > 0.0 {
            let rate = (hits / total) * 100.0;
            self.cache_hit_rate.set(rate);
        }
    }
}

/// Hot update coordinator monitoring metrics
#[derive(Debug)]
pub struct HotUpdateMetrics {
    pub update_cycles_total: LabeledCounter,
    pub update_latency_ms: LabeledHistogram,
    pub files_changed_total: LabeledCounter,
    pub files_processed_total: LabeledCounter,
    pub files_failed_total: LabeledCounter,
    pub entity_changes_total: LabeledCounter,
    pub module_retry_total: LabeledCounter,
    pub watch_overflow_total: LabeledCounter,
    pub full_rescan_fallback_total: LabeledCounter,
}

/// Hot-update storage-side metrics
#[derive(Debug)]
pub struct HotUpdateStorageMetrics {
    pub work_unit_committed_total: LabeledCounter,
    pub work_unit_uncommitted_total: LabeledCounter,
    pub work_unit_skip_committed_total: LabeledCounter,
    pub candidate_reuse_adopted_total: LabeledCounter,
    pub candidate_reuse_rejected_total: LabeledCounter,
    pub rechunk_rebuilt_total: LabeledCounter,
    pub rechunk_skipped_total: LabeledCounter,
}

impl HotUpdateStorageMetrics {
    pub fn new(registry: &MetricsRegistry, project_id: i64) -> Arc<Self> {
        let proj_val = project_id.to_string();
        Arc::new(Self {
            work_unit_committed_total: registry
                .counter("work_unit_committed_total", &[("project_id", &proj_val)]),
            work_unit_uncommitted_total: registry
                .counter("work_unit_uncommitted_total", &[("project_id", &proj_val)]),
            work_unit_skip_committed_total: registry.counter(
                "work_unit_skip_committed_total",
                &[("project_id", &proj_val)],
            ),
            candidate_reuse_adopted_total: registry.counter(
                "candidate_reuse_adopted_total",
                &[("project_id", &proj_val)],
            ),
            candidate_reuse_rejected_total: registry.counter(
                "candidate_reuse_rejected_total",
                &[("project_id", &proj_val)],
            ),
            rechunk_rebuilt_total: registry
                .counter("rechunk_rebuilt_total", &[("project_id", &proj_val)]),
            rechunk_skipped_total: registry
                .counter("rechunk_skipped_total", &[("project_id", &proj_val)]),
        })
    }

    pub fn record_chunking_drift_sweep(&self, rebuilt: usize, skipped: usize) {
        self.rechunk_rebuilt_total.add(rebuilt as u64);
        if skipped > 0 {
            self.rechunk_skipped_total.add(skipped as u64);
        }
    }
}

/// File watch monitoring metrics
#[derive(Debug)]
pub struct WatchMetrics {
    pub events_total: LabeledCounter,
    pub file_events_total: LabeledCounter,
    pub dir_events_total: LabeledCounter,
    pub config_events_total: LabeledCounter,
    pub filtered_events_total: LabeledCounter,
    pub forwarded_events_total: LabeledCounter,
    pub failed_events_total: LabeledCounter,
    pub active_gauge: LabeledGauge,
    pub status_code: LabeledGauge,
    pub watched_paths: LabeledGauge,
    pub watch_overflow_total: LabeledCounter,
    pub full_rescan_fallback_total: LabeledCounter,
}

impl WatchMetrics {
    pub fn new(registry: &MetricsRegistry, project_id: i64) -> Arc<Self> {
        let proj_val = project_id.to_string();
        Arc::new(Self {
            events_total: registry.counter("watch_events_total", &[("project_id", &proj_val)]),
            file_events_total: registry
                .counter("watch_file_events_total", &[("project_id", &proj_val)]),
            dir_events_total: registry
                .counter("watch_dir_events_total", &[("project_id", &proj_val)]),
            config_events_total: registry
                .counter("watch_config_events_total", &[("project_id", &proj_val)]),
            filtered_events_total: registry
                .counter("watch_filtered_events_total", &[("project_id", &proj_val)]),
            forwarded_events_total: registry
                .counter("watch_forwarded_events_total", &[("project_id", &proj_val)]),
            failed_events_total: registry
                .counter("watch_failed_events_total", &[("project_id", &proj_val)]),
            active_gauge: registry.gauge("watch_active", &[("project_id", &proj_val)]),
            status_code: registry.gauge("watch_status_code", &[("project_id", &proj_val)]),
            watched_paths: registry.gauge("watch_watched_paths", &[("project_id", &proj_val)]),
            watch_overflow_total: registry
                .counter("watch_overflow_total", &[("project_id", &proj_val)]),
            full_rescan_fallback_total: registry
                .counter("full_rescan_fallback_total", &[("project_id", &proj_val)]),
        })
    }

    pub fn record_event(&self) {
        self.events_total.increment();
    }

    pub fn record_file_event(&self) {
        self.file_events_total.increment();
    }

    pub fn record_dir_event(&self) {
        self.dir_events_total.increment();
    }

    pub fn record_config_event(&self) {
        self.config_events_total.increment();
    }

    pub fn record_filtered_event(&self) {
        self.filtered_events_total.increment();
    }

    pub fn record_forwarded_event(&self) {
        self.forwarded_events_total.increment();
    }

    pub fn record_failed_event(&self) {
        self.failed_events_total.increment();
    }

    pub fn set_active(&self, active: bool) {
        self.active_gauge.set(if active { 1 } else { 0 });
    }

    pub fn set_status_code(&self, status_code: u64) {
        self.status_code.set(status_code);
    }

    pub fn set_watched_paths(&self, count: usize) {
        self.watched_paths.set(count as u64);
    }

    pub fn record_overflow(&self) {
        self.watch_overflow_total.increment();
    }

    pub fn record_full_rescan_fallback(&self) {
        self.full_rescan_fallback_total.increment();
    }
}

impl HotUpdateMetrics {
    pub fn new(registry: &MetricsRegistry, project_id: i64) -> Arc<Self> {
        let proj_val = project_id.to_string();
        Arc::new(Self {
            update_cycles_total: registry
                .counter("hot_update_cycles_total", &[("project_id", &proj_val)]),
            update_latency_ms: registry
                .histogram_default("hot_update_latency_ms", &[("project_id", &proj_val)]),
            files_changed_total: registry
                .counter("files_changed_total", &[("project_id", &proj_val)]),
            files_processed_total: registry.counter(
                "files_processed_in_hot_update_total",
                &[("project_id", &proj_val)],
            ),
            files_failed_total: registry.counter(
                "files_failed_in_hot_update_total",
                &[("project_id", &proj_val)],
            ),
            entity_changes_total: registry
                .counter("entity_changes_total", &[("project_id", &proj_val)]),
            module_retry_total: registry.counter(
                "hot_update_module_retry_total",
                &[("project_id", &proj_val)],
            ),
            watch_overflow_total: registry.counter(
                "hot_update_watch_overflow_total",
                &[("project_id", &proj_val)],
            ),
            full_rescan_fallback_total: registry.counter(
                "hot_update_full_rescan_fallback_total",
                &[("project_id", &proj_val)],
            ),
        })
    }

    pub fn record_overflow(&self) {
        self.watch_overflow_total.increment();
    }

    pub fn record_full_rescan_fallback(&self) {
        self.full_rescan_fallback_total.increment();
    }

    pub fn record_update(
        &self,
        latency_ms: f64,
        files_changed: usize,
        files_processed: usize,
        files_failed: usize,
        entity_changes: usize,
    ) {
        self.update_cycles_total.increment();
        self.update_latency_ms.observe(latency_ms);
        self.files_changed_total.add(files_changed as u64);
        self.files_processed_total.add(files_processed as u64);
        self.files_failed_total.add(files_failed as u64);
        self.entity_changes_total.add(entity_changes as u64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_metrics_creation() {
        let registry = MetricsRegistry::new();
        let metrics = QueryMetrics::new(&registry, 1);

        assert_eq!(metrics.queries_total.get(), 0);
        assert_eq!(metrics.cache_hits_total.get(), 0);
        assert_eq!(metrics.cache_misses_total.get(), 0);
        assert!((metrics.cache_hit_rate.get() - 0.0).abs() < f64::EPSILON);
        assert_eq!(metrics.results_returned_total.get(), 0);
    }

    #[test]
    fn test_query_metrics_record() {
        let registry = MetricsRegistry::new();
        let metrics = QueryMetrics::new(&registry, 1);

        metrics.record_query(50.0, false, 10);
        assert_eq!(metrics.queries_total.get(), 1);
        assert_eq!(metrics.cache_misses_total.get(), 1);
        assert_eq!(metrics.cache_hits_total.get(), 0);
        assert_eq!(metrics.results_returned_total.get(), 10);
        assert_eq!(metrics.query_latency_ms.get_count(), 1);
        assert!((metrics.cache_hit_rate.get() - 0.0).abs() < f64::EPSILON);

        metrics.record_query(5.0, true, 8);
        assert_eq!(metrics.queries_total.get(), 2);
        assert_eq!(metrics.cache_hits_total.get(), 1);
        assert_eq!(metrics.cache_misses_total.get(), 1);
        assert_eq!(metrics.results_returned_total.get(), 18);
        assert!((metrics.cache_hit_rate.get() - 50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_hot_update_metrics_creation() {
        let registry = MetricsRegistry::new();
        let metrics = HotUpdateMetrics::new(&registry, 1);

        assert_eq!(metrics.update_cycles_total.get(), 0);
        assert_eq!(metrics.files_changed_total.get(), 0);
        assert_eq!(metrics.files_processed_total.get(), 0);
        assert_eq!(metrics.files_failed_total.get(), 0);
        assert_eq!(metrics.entity_changes_total.get(), 0);
    }

    #[test]
    fn test_hot_update_metrics_record() {
        let registry = MetricsRegistry::new();
        let metrics = HotUpdateMetrics::new(&registry, 1);

        metrics.record_update(100.0, 5, 4, 1, 10);

        assert_eq!(metrics.update_cycles_total.get(), 1);
        assert_eq!(metrics.files_changed_total.get(), 5);
        assert_eq!(metrics.files_processed_total.get(), 4);
        assert_eq!(metrics.files_failed_total.get(), 1);
        assert_eq!(metrics.entity_changes_total.get(), 10);
        assert_eq!(metrics.update_latency_ms.get_count(), 1);
    }

    #[test]
    fn test_hot_update_metrics_incremental_updates() {
        let registry = MetricsRegistry::new();
        let metrics = HotUpdateMetrics::new(&registry, 1);

        metrics.record_update(100.0, 5, 4, 1, 10);
        assert_eq!(metrics.update_cycles_total.get(), 1);

        metrics.record_update(150.0, 3, 3, 0, 5);
        assert_eq!(metrics.update_cycles_total.get(), 2);
        assert_eq!(metrics.files_changed_total.get(), 8);
        assert_eq!(metrics.files_processed_total.get(), 7);
        assert_eq!(metrics.files_failed_total.get(), 1);
        assert_eq!(metrics.entity_changes_total.get(), 15);
    }

    #[test]
    fn test_watch_metrics_creation() {
        let registry = MetricsRegistry::new();
        let metrics = WatchMetrics::new(&registry, 1);

        assert_eq!(metrics.events_total.get(), 0);
        assert_eq!(metrics.file_events_total.get(), 0);
        assert_eq!(metrics.dir_events_total.get(), 0);
        assert_eq!(metrics.config_events_total.get(), 0);
        assert_eq!(metrics.filtered_events_total.get(), 0);
        assert_eq!(metrics.forwarded_events_total.get(), 0);
        assert_eq!(metrics.failed_events_total.get(), 0);
        assert_eq!(metrics.active_gauge.get(), 0);
        assert_eq!(metrics.status_code.get(), 0);
        assert_eq!(metrics.watched_paths.get(), 0);
    }

    #[test]
    fn test_watch_metrics_recording() {
        let registry = MetricsRegistry::new();
        let metrics = WatchMetrics::new(&registry, 1);

        metrics.record_event();
        metrics.record_file_event();
        metrics.record_dir_event();
        metrics.record_config_event();
        metrics.record_filtered_event();
        metrics.record_forwarded_event();
        metrics.record_failed_event();
        metrics.set_active(true);
        metrics.set_status_code(1);
        metrics.set_watched_paths(3);

        assert_eq!(metrics.events_total.get(), 1);
        assert_eq!(metrics.file_events_total.get(), 1);
        assert_eq!(metrics.dir_events_total.get(), 1);
        assert_eq!(metrics.config_events_total.get(), 1);
        assert_eq!(metrics.filtered_events_total.get(), 1);
        assert_eq!(metrics.forwarded_events_total.get(), 1);
        assert_eq!(metrics.failed_events_total.get(), 1);
        assert_eq!(metrics.active_gauge.get(), 1);
        assert_eq!(metrics.status_code.get(), 1);
        assert_eq!(metrics.watched_paths.get(), 3);
    }

    #[test]
    fn test_query_metrics_precision_with_float_gauge() {
        let registry = MetricsRegistry::new();
        let metrics = QueryMetrics::new(&registry, 1);

        metrics.record_query(10.0, true, 5);
        metrics.record_query(10.0, true, 5);
        metrics.record_query(10.0, false, 5);

        let rate = metrics.cache_hit_rate.get();
        assert!(
            (rate - 66.67).abs() < 0.1,
            "Expected ~66.67%, got {}%",
            rate
        );
    }

    #[test]
    fn test_boundary_values() {
        let registry = MetricsRegistry::new();

        let query_metrics = QueryMetrics::new(&registry, 1);
        query_metrics.record_query(999999.99, true, usize::MAX);
        assert_eq!(query_metrics.queries_total.get(), 1);
        assert_eq!(
            query_metrics.results_returned_total.get(),
            usize::MAX as u64
        );
    }
}
