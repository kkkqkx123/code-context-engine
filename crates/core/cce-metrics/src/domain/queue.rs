//! Queue backpressure metrics for monitoring internal queue depth.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;

use crate::{LabeledCounter, LabeledGauge, MetricsRegistry};

pub struct QueueMetrics {
    registry: Arc<MetricsRegistry>,
    pub retry_queue_processed_total: LabeledCounter,
    pub retry_queue_failed_total: LabeledCounter,
    pub files_permanently_failed_total: LabeledCounter,
    operation_depth_cache: Arc<Mutex<HashMap<i64, LabeledGauge>>>,
    pending_changes_cache: Arc<Mutex<HashMap<i64, LabeledGauge>>>,
    retry_depth_cache: Arc<Mutex<HashMap<i64, LabeledGauge>>>,
}

impl QueueMetrics {
    pub fn new(registry: &MetricsRegistry) -> Self {
        let retry_queue_processed_total = registry.counter("retry_queue_processed_total", &[]);
        let retry_queue_failed_total = registry.counter("retry_queue_failed_total", &[]);
        let files_permanently_failed_total =
            registry.counter("files_permanently_failed_total", &[]);
        Self {
            registry: Arc::new(registry.clone()),
            retry_queue_processed_total,
            retry_queue_failed_total,
            files_permanently_failed_total,
            operation_depth_cache: Arc::new(Mutex::new(HashMap::new())),
            pending_changes_cache: Arc::new(Mutex::new(HashMap::new())),
            retry_depth_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn set_operation_depth(&self, project_id: i64, depth: u64) {
        let gauge = self.get_or_create_cached(
            &self.operation_depth_cache,
            project_id,
            "operation_queue_depth",
        );
        gauge.set(depth);
    }

    pub fn set_pending_changes_depth(&self, project_id: i64, depth: u64) {
        let gauge = self.get_or_create_cached(
            &self.pending_changes_cache,
            project_id,
            "pending_watch_changes",
        );
        gauge.set(depth);
    }

    pub fn set_retry_depth(&self, project_id: i64, depth: u64) {
        let gauge =
            self.get_or_create_cached(&self.retry_depth_cache, project_id, "retry_queue_depth");
        gauge.set(depth);
    }

    pub fn record_retry_processed(&self) {
        self.retry_queue_processed_total.increment();
    }

    pub fn record_retry_failed(&self) {
        self.retry_queue_failed_total.increment();
    }

    pub fn record_file_permanently_failed(&self) {
        self.files_permanently_failed_total.increment();
    }

    fn get_or_create_cached(
        &self,
        cache: &Arc<Mutex<HashMap<i64, LabeledGauge>>>,
        project_id: i64,
        name: &str,
    ) -> LabeledGauge {
        {
            let guard = cache.lock().expect("queue metrics lock poisoned");
            if let Some(gauge) = guard.get(&project_id) {
                return gauge.clone();
            }
        }
        let gauge = self
            .registry
            .gauge(name, &[("project_id", &project_id.to_string())]);
        let mut guard = cache.lock().expect("queue metrics lock poisoned");
        guard.insert(project_id, gauge.clone());
        gauge
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_operation_depth() {
        let registry = MetricsRegistry::new();
        let metrics = QueueMetrics::new(&registry);
        metrics.set_operation_depth(42, 3);
        metrics.set_operation_depth(42, 5);

        let gauge = registry.gauge("operation_queue_depth", &[("project_id", "42")]);
        assert_eq!(gauge.get(), 5);
    }

    #[test]
    fn test_set_pending_changes_depth() {
        let registry = MetricsRegistry::new();
        let metrics = QueueMetrics::new(&registry);
        metrics.set_pending_changes_depth(1, 10);

        let gauge = registry.gauge("pending_watch_changes", &[("project_id", "1")]);
        assert_eq!(gauge.get(), 10);
    }

    #[test]
    fn test_set_retry_depth() {
        let registry = MetricsRegistry::new();
        let metrics = QueueMetrics::new(&registry);
        metrics.set_retry_depth(99, 2);

        let gauge = registry.gauge("retry_queue_depth", &[("project_id", "99")]);
        assert_eq!(gauge.get(), 2);
    }

    #[test]
    fn test_multiple_projects() {
        let registry = MetricsRegistry::new();
        let metrics = QueueMetrics::new(&registry);
        metrics.set_operation_depth(1, 10);
        metrics.set_operation_depth(2, 20);

        let g1 = registry.gauge("operation_queue_depth", &[("project_id", "1")]);
        let g2 = registry.gauge("operation_queue_depth", &[("project_id", "2")]);
        assert_eq!(g1.get(), 10);
        assert_eq!(g2.get(), 20);
    }

    #[test]
    fn test_cached_gauge_reuse() {
        let registry = MetricsRegistry::new();
        let metrics = QueueMetrics::new(&registry);
        metrics.set_operation_depth(1, 5);
        metrics.set_operation_depth(1, 10);
        metrics.set_operation_depth(1, 15);

        let gauge = registry.gauge("operation_queue_depth", &[("project_id", "1")]);
        assert_eq!(gauge.get(), 15);
    }

    #[test]
    fn test_retry_queue_processing_rate() {
        let registry = MetricsRegistry::new();
        let metrics = QueueMetrics::new(&registry);

        metrics.record_retry_processed();
        metrics.record_retry_processed();
        metrics.record_retry_processed();
        assert_eq!(metrics.retry_queue_processed_total.get(), 3);

        metrics.record_retry_failed();
        assert_eq!(metrics.retry_queue_failed_total.get(), 1);
    }

    #[test]
    fn test_files_permanently_failed() {
        let registry = MetricsRegistry::new();
        let metrics = QueueMetrics::new(&registry);

        metrics.record_file_permanently_failed();
        metrics.record_file_permanently_failed();
        metrics.record_file_permanently_failed();

        assert_eq!(metrics.files_permanently_failed_total.get(), 3);
    }
}
