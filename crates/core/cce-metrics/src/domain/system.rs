//! System resource metrics
//!
//! This module provides metrics for monitoring system resources,
//! including CPU usage, memory consumption, swap utilization,
//! per-process disk I/O (Linux), and system-wide network I/O.

use crate::{LabeledFloatGauge, LabeledGauge, MetricsRegistry};
use std::sync::Arc;
use sysinfo::{Networks, System};
use tracing::debug;

/// System resource metrics collector
pub struct SystemMetrics {
    registry: Arc<MetricsRegistry>,
    system: Arc<parking_lot::Mutex<System>>,
    networks: Arc<parking_lot::Mutex<Networks>>,

    cpu_usage_percent: LabeledFloatGauge,

    memory_used_bytes: LabeledGauge,
    memory_total_bytes: LabeledGauge,
    memory_usage_percent: LabeledFloatGauge,

    swap_used_bytes: LabeledGauge,
    swap_total_bytes: LabeledGauge,

    disk_read_bytes: LabeledGauge,
    disk_write_bytes: LabeledGauge,

    net_recv_bytes: LabeledGauge,
    net_sent_bytes: LabeledGauge,
}

impl SystemMetrics {
    pub fn new(registry: &Arc<MetricsRegistry>) -> Self {
        let mut system = System::new_all();
        system.refresh_all();

        Self {
            registry: registry.clone(),
            system: Arc::new(parking_lot::Mutex::new(system)),
            networks: Arc::new(parking_lot::Mutex::new(Networks::new_with_refreshed_list())),

            cpu_usage_percent: registry.float_gauge("system_cpu_usage_percent", &[]),

            memory_used_bytes: registry.gauge("system_memory_used_bytes", &[]),
            memory_total_bytes: registry.gauge("system_memory_total_bytes", &[]),
            memory_usage_percent: registry.float_gauge("system_memory_usage_percent", &[]),

            swap_used_bytes: registry.gauge("system_swap_used_bytes", &[]),
            swap_total_bytes: registry.gauge("system_swap_total_bytes", &[]),

            disk_read_bytes: registry.gauge("system_disk_read_bytes", &[]),
            disk_write_bytes: registry.gauge("system_disk_write_bytes", &[]),

            net_recv_bytes: registry.gauge("system_net_recv_bytes", &[]),
            net_sent_bytes: registry.gauge("system_net_sent_bytes", &[]),
        }
    }

    pub fn collect(&self) {
        self.collect_cpu_memory_swap();
        self.collect_network();
        #[cfg(target_os = "linux")]
        self.collect_disk_io();
    }

    fn collect_cpu_memory_swap(&self) {
        let mut system = self.system.lock();

        system.refresh_cpu_usage();
        system.refresh_memory();

        let cpu_usage = system.global_cpu_usage();
        self.cpu_usage_percent.set(cpu_usage as f64);

        let memory_used = system.used_memory();
        let memory_total = system.total_memory();
        let memory_percent = if memory_total > 0 {
            (memory_used as f64 / memory_total as f64) * 100.0
        } else {
            0.0
        };

        self.memory_used_bytes.set(memory_used);
        self.memory_total_bytes.set(memory_total);
        self.memory_usage_percent.set(memory_percent);

        let swap_used = system.used_swap();
        let swap_total = system.total_swap();
        self.swap_used_bytes.set(swap_used);
        self.swap_total_bytes.set(swap_total);

        debug!(
            cpu_usage = cpu_usage,
            memory_percent = memory_percent,
            "Collected system metrics"
        );
    }

    fn collect_network(&self) {
        let mut networks = self.networks.lock();

        networks.refresh(true);

        let (total_recv, total_sent) =
            networks
                .iter()
                .fold((0u64, 0u64), |(acc_recv, acc_sent), (_name, data)| {
                    (
                        acc_recv.saturating_add(data.total_received()),
                        acc_sent.saturating_add(data.total_transmitted()),
                    )
                });

        self.net_recv_bytes.set(total_recv);
        self.net_sent_bytes.set(total_sent);

        debug!(
            recv_bytes = total_recv,
            sent_bytes = total_sent,
            "Collected network I/O metrics"
        );
    }

    #[cfg(target_os = "linux")]
    fn collect_disk_io(&self) {
        use procfs::process::Process;

        match Process::myself().and_then(|p| p.io()) {
            Ok(io) => {
                self.disk_read_bytes.set(io.read_bytes);
                self.disk_write_bytes.set(io.write_bytes);
                debug!(
                    read_bytes = io.read_bytes,
                    write_bytes = io.write_bytes,
                    "Collected per-process disk I/O"
                );
            }
            Err(e) => {
                debug!(error = %e, "Failed to collect per-process disk I/O");
            }
        }
    }

    pub fn get_health_summary(&self) -> SystemHealthSummary {
        let system = self.system.lock();

        let memory_used = system.used_memory();
        let memory_total = system.total_memory();
        let memory_percent = if memory_total > 0 {
            (memory_used as f64 / memory_total as f64) * 100.0
        } else {
            0.0
        };

        SystemHealthSummary {
            cpu_usage_percent: system.global_cpu_usage() as f64,
            memory_used_bytes: memory_used,
            memory_total_bytes: memory_total,
            memory_usage_percent: memory_percent,
            swap_used_bytes: system.used_swap(),
            swap_total_bytes: system.total_swap(),
            disk_read_bytes: self.disk_read_bytes.get(),
            disk_write_bytes: self.disk_write_bytes.get(),
            net_recv_bytes: self.net_recv_bytes.get(),
            net_sent_bytes: self.net_sent_bytes.get(),
        }
    }

    pub fn registry(&self) -> &Arc<MetricsRegistry> {
        &self.registry
    }
}

/// System health summary for API responses
#[derive(Debug, Clone, serde::Serialize)]
pub struct SystemHealthSummary {
    pub cpu_usage_percent: f64,
    pub memory_used_bytes: u64,
    pub memory_total_bytes: u64,
    pub memory_usage_percent: f64,
    pub swap_used_bytes: u64,
    pub swap_total_bytes: u64,
    pub disk_read_bytes: u64,
    pub disk_write_bytes: u64,
    pub net_recv_bytes: u64,
    pub net_sent_bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MetricsRegistry;

    #[tokio::test]
    async fn test_system_metrics_creation() {
        let registry = Arc::new(MetricsRegistry::new());
        let _metrics = SystemMetrics::new(&registry);
    }

    #[tokio::test]
    async fn test_system_metrics_collection() {
        let registry = Arc::new(MetricsRegistry::new());
        let metrics = SystemMetrics::new(&registry);

        metrics.collect();

        let snapshot = registry.export_all();
        let has_float_gauge = snapshot
            .metrics
            .iter()
            .any(|m| matches!(m.value, crate::MetricData::FloatGauge(_)));
        let has_gauge = snapshot
            .metrics
            .iter()
            .any(|m| matches!(m.value, crate::MetricData::Gauge(_)));
        assert!(has_float_gauge, "Should have float gauges");
        assert!(has_gauge, "Should have gauges");
    }

    #[tokio::test]
    async fn test_health_summary() {
        let registry = Arc::new(MetricsRegistry::new());
        let metrics = SystemMetrics::new(&registry);

        let summary = metrics.get_health_summary();
        assert!(summary.cpu_usage_percent >= 0.0);
        assert!(summary.cpu_usage_percent <= 100.0);
        assert!(summary.memory_total_bytes > 0);
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn test_disk_io_collection() {
        let registry = Arc::new(MetricsRegistry::new());
        let metrics = SystemMetrics::new(&registry);

        metrics.collect();

        let disk_read = registry.gauge("system_disk_read_bytes", &[]);
        let disk_write = registry.gauge("system_disk_write_bytes", &[]);

        let _ = disk_read.get();
        let _ = disk_write.get();

        metrics.collect();
    }

    #[tokio::test]
    async fn test_new_gauges_initialized() {
        let registry = Arc::new(MetricsRegistry::new());
        let _metrics = SystemMetrics::new(&registry);

        let disk_read = registry.gauge("system_disk_read_bytes", &[]);
        let disk_write = registry.gauge("system_disk_write_bytes", &[]);
        let net_recv = registry.gauge("system_net_recv_bytes", &[]);
        let net_sent = registry.gauge("system_net_sent_bytes", &[]);

        assert_eq!(disk_read.get(), 0);
        assert_eq!(disk_write.get(), 0);
        assert_eq!(net_recv.get(), 0);
        assert_eq!(net_sent.get(), 0);
    }
}
