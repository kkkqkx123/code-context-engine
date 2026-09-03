//! Tokio Runtime metrics
//!
//! This module provides metrics for monitoring the Tokio async runtime,
//! including worker thread status, task scheduling, and resource utilization.

use crate::{LabeledGauge, MetricsRegistry};
use std::sync::Arc;
use tokio::runtime::Handle;
use tracing::debug;

/// Tokio runtime metrics collector
pub struct RuntimeMetrics {
    worker_count: LabeledGauge,
    active_tasks: LabeledGauge,
    global_queue_depth: LabeledGauge,
    worker_busy_durations: Vec<LabeledGauge>,
}

impl RuntimeMetrics {
    pub fn new(registry: &Arc<MetricsRegistry>) -> Self {
        let handle = Handle::current();
        let metrics = handle.metrics();

        let worker_count = registry.gauge("tokio_workers_total", &[]);
        let active_tasks = registry.gauge("tokio_active_tasks", &[]);
        let global_queue_depth = registry.gauge("tokio_global_queue_depth", &[]);

        worker_count.set(metrics.num_workers() as u64);

        let mut worker_busy_durations = Vec::new();

        for i in 0..metrics.num_workers() {
            let busy_duration_gauge = registry.gauge(
                "tokio_worker_busy_duration_ms",
                &[("worker_id", &i.to_string())],
            );
            worker_busy_durations.push(busy_duration_gauge);
        }

        Self {
            worker_count,
            active_tasks,
            global_queue_depth,
            worker_busy_durations,
        }
    }

    pub fn collect(&self) {
        let handle = Handle::current();
        let metrics = handle.metrics();

        let total_workers = metrics.num_workers();

        self.worker_count.set(total_workers as u64);
        self.active_tasks.set(metrics.num_alive_tasks() as u64);
        self.global_queue_depth
            .set(metrics.global_queue_depth() as u64);

        for worker_id in 0..total_workers {
            if let Some(busy_gauge) = self.worker_busy_durations.get(worker_id) {
                let busy_ms = metrics.worker_total_busy_duration(worker_id).as_secs_f64() * 1000.0;
                busy_gauge.set(busy_ms as u64);
            }
        }

        debug!(
            workers = total_workers,
            "Successfully collected Tokio runtime metrics"
        );
    }

    pub fn get_health_summary(&self) -> RuntimeHealthSummary {
        let handle = Handle::current();
        let metrics = handle.metrics();

        RuntimeHealthSummary {
            num_workers: metrics.num_workers(),
            active_tasks: metrics.num_alive_tasks(),
            injection_queue_depth: metrics.global_queue_depth(),
        }
    }
}

/// Runtime health summary for API responses
#[derive(Debug, Clone, serde::Serialize)]
pub struct RuntimeHealthSummary {
    pub num_workers: usize,
    pub active_tasks: usize,
    pub injection_queue_depth: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MetricsRegistry;

    #[tokio::test]
    async fn test_runtime_metrics_creation() {
        let registry = Arc::new(MetricsRegistry::new());
        let _metrics = RuntimeMetrics::new(&registry);
    }

    #[tokio::test]
    async fn test_runtime_metrics_collection() {
        let registry = Arc::new(MetricsRegistry::new());
        let metrics = RuntimeMetrics::new(&registry);

        metrics.collect();

        let snapshot = registry.export_all();
        let has_gauge = snapshot
            .metrics
            .iter()
            .any(|m| matches!(m.value, crate::MetricData::Gauge(_)));
        assert!(has_gauge);
    }

    #[tokio::test]
    async fn test_health_summary() {
        let registry = Arc::new(MetricsRegistry::new());
        let metrics = RuntimeMetrics::new(&registry);

        let summary = metrics.get_health_summary();
        assert!(summary.num_workers > 0);
    }
}
