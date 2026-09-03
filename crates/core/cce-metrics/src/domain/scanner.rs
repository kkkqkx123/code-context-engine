use std::sync::Arc;

use crate::{LabeledCounter, LabeledHistogram, MetricsRegistry};

#[derive(Debug)]
pub struct ScannerMetrics {
    pub files_scanned_total: LabeledCounter,
    pub files_filtered_total: LabeledCounter,
    pub files_skipped_total: LabeledCounter,
    pub files_hash_reused_total: LabeledCounter,
    pub scan_latency_ms: LabeledHistogram,
    pub languages_detected_total: LabeledCounter,
}

impl ScannerMetrics {
    pub fn new(registry: &MetricsRegistry, project_id: i64) -> Arc<Self> {
        let proj = project_id.to_string();
        Arc::new(Self {
            files_scanned_total: registry
                .counter("scanner_files_scanned_total", &[("project_id", &proj)]),
            files_filtered_total: registry
                .counter("scanner_files_filtered_total", &[("project_id", &proj)]),
            files_skipped_total: registry
                .counter("scanner_files_skipped_total", &[("project_id", &proj)]),
            files_hash_reused_total: registry
                .counter("scanner_files_hash_reused_total", &[("project_id", &proj)]),
            scan_latency_ms: registry
                .histogram_default("scanner_scan_latency_ms", &[("project_id", &proj)]),
            languages_detected_total: registry
                .counter("scanner_languages_detected_total", &[("project_id", &proj)]),
        })
    }

    pub fn record_scan(&self, latency_ms: f64, filtered: bool, skipped: bool) {
        self.files_scanned_total.increment();
        self.scan_latency_ms.observe(latency_ms);
        if filtered {
            self.files_filtered_total.increment();
        }
        if skipped {
            self.files_skipped_total.increment();
        }
    }

    /// Record that a file's content hash was reused from a previous scan
    /// because its (size, mtime) fingerprint was unchanged.
    pub fn record_hash_reuse(&self) {
        self.files_hash_reused_total.increment();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MetricsRegistry;

    #[test]
    fn test_scanner_metrics_creation() {
        let registry = MetricsRegistry::new();
        let metrics = ScannerMetrics::new(&registry, 1);
        assert_eq!(metrics.files_scanned_total.get(), 0);
    }

    #[test]
    fn test_scanner_metrics_record() {
        let registry = MetricsRegistry::new();
        let metrics = ScannerMetrics::new(&registry, 1);
        metrics.record_scan(10.5, false, false);
        assert_eq!(metrics.files_scanned_total.get(), 1);
        assert_eq!(metrics.scan_latency_ms.get_count(), 1);
    }

    #[test]
    fn test_scanner_metrics_filtered() {
        let registry = MetricsRegistry::new();
        let metrics = ScannerMetrics::new(&registry, 1);
        metrics.record_scan(5.0, true, false);
        assert_eq!(metrics.files_scanned_total.get(), 1);
        assert_eq!(metrics.files_filtered_total.get(), 1);
        assert_eq!(metrics.files_skipped_total.get(), 0);
    }

    #[test]
    fn test_scanner_metrics_skipped() {
        let registry = MetricsRegistry::new();
        let metrics = ScannerMetrics::new(&registry, 1);
        metrics.record_scan(5.0, false, true);
        assert_eq!(metrics.files_scanned_total.get(), 1);
        assert_eq!(metrics.files_filtered_total.get(), 0);
        assert_eq!(metrics.files_skipped_total.get(), 1);
    }
}
