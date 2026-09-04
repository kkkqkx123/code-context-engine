//! Metric export in different formats
//!
//! Provides Prometheus exposition format and JSON serialization.

use cce_metrics::serialization::MetricData;
use cce_metrics::{
    HistogramStats, MetricValue, MetricsRegistry, MetricsSnapshot, MetricsSystemMetrics,
    metric_description,
};

/// Error type for metric export operations
#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Export failed: {0}")]
    Other(String),
}

/// Supported export formats
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Prometheus,
    Json,
}

/// Export metrics in the specified format
pub fn export(registry: &MetricsRegistry, format: ExportFormat) -> String {
    export_with_metrics(registry, format, None)
}

/// Export metrics while recording latency and volume self-monitoring.
pub fn export_with_metrics(
    registry: &MetricsRegistry,
    format: ExportFormat,
    system_metrics: Option<&MetricsSystemMetrics>,
) -> String {
    let started = std::time::Instant::now();
    let output = match format {
        ExportFormat::Prometheus => format_prometheus(registry),
        ExportFormat::Json => {
            let snapshot = cce_metrics::serialization::MetricsSnapshot::from_registry(registry);
            serde_json::to_string_pretty(&snapshot)
                .unwrap_or_else(|e| format!("{{\"error\": \"serialization failed: {}\"}}", e))
        }
    };
    if let Some(sys) = system_metrics {
        let format_name = match format {
            ExportFormat::Prometheus => "prometheus",
            ExportFormat::Json => "json",
        };
        sys.record_export(
            format_name,
            started.elapsed().as_secs_f64() * 1000.0,
            output.len(),
        );
    }
    output
}

/// Format metrics in Prometheus exposition format
fn format_prometheus(registry: &MetricsRegistry) -> String {
    format_prometheus_snapshot(&registry.export_all())
}

/// Render Prometheus exposition text from a single unified snapshot
pub(crate) fn format_prometheus_snapshot(snapshot: &MetricsSnapshot) -> String {
    let mut output = String::new();
    for metric in &snapshot.metrics {
        match &metric.value {
            MetricData::Counter(value) => {
                output.push_str(&format_counter(metric, *value));
            }
            MetricData::Gauge(value) => {
                output.push_str(&format_gauge(metric, *value));
            }
            MetricData::FloatGauge(value) => {
                output.push_str(&format_float_gauge(metric, *value));
            }
            MetricData::Histogram(stats) => {
                output.push_str(&format_histogram(metric, stats));
            }
        }
    }
    output
}

fn format_counter(metric: &MetricValue, value: u64) -> String {
    let labels = format_labels_snapshot(&metric.labels);
    format!(
        "# HELP {} {}\n# TYPE {} counter\n{}{} {}\n",
        metric.name,
        metric_help(&metric.name),
        metric.name,
        metric.name,
        labels,
        value
    )
}

fn format_gauge(metric: &MetricValue, value: u64) -> String {
    let labels = format_labels_snapshot(&metric.labels);
    format!(
        "# HELP {} {}\n# TYPE {} gauge\n{}{} {}\n",
        metric.name,
        metric_help(&metric.name),
        metric.name,
        metric.name,
        labels,
        value
    )
}

fn format_float_gauge(metric: &MetricValue, value: f64) -> String {
    let labels = format_labels_snapshot(&metric.labels);
    format!(
        "# HELP {} {}\n# TYPE {} gauge\n{}{} {}\n",
        metric.name,
        metric_help(&metric.name),
        metric.name,
        metric.name,
        labels,
        value
    )
}

fn format_histogram(metric: &MetricValue, stats: &HistogramStats) -> String {
    let mut output = String::new();
    let base_labels = format_labels_snapshot(&metric.labels);

    output.push_str(&format!(
        "# HELP {} {}\n# TYPE {} histogram\n",
        metric.name,
        metric_help(&metric.name),
        metric.name
    ));

    let mut cumulative = 0u64;
    for (i, &bucket) in stats.buckets.iter().enumerate() {
        cumulative += stats.bucket_counts[i];
        let bucket_labels = if base_labels.is_empty() {
            format!("{{le=\"{}\"}}", bucket)
        } else {
            format!(
                "{},le=\"{}\"}}",
                &base_labels[..base_labels.len() - 1],
                bucket
            )
        };
        output.push_str(&format!(
            "{}_bucket{} {}\n",
            metric.name, bucket_labels, cumulative
        ));
    }

    let inf_labels = if base_labels.is_empty() {
        "{le=\"+Inf\"}".to_string()
    } else {
        format!("{},le=\"+Inf\"}}", &base_labels[..base_labels.len() - 1])
    };
    output.push_str(&format!(
        "{}_bucket{} {}\n",
        metric.name, inf_labels, stats.count
    ));

    let sum_value = stats.sum_microseconds as f64 / 1000.0;
    output.push_str(&format!(
        "{}_sum{} {}\n",
        metric.name, base_labels, sum_value
    ));
    output.push_str(&format!(
        "{}_count{} {}\n",
        metric.name, base_labels, stats.count
    ));

    output
}

/// Return the HELP text for a known metric name, or a default description.
fn metric_help(name: &str) -> &'static str {
    metric_description(name).unwrap_or("No description available")
}

/// Format snapshot labels into Prometheus label text
fn format_labels_snapshot(labels: &Option<std::collections::HashMap<String, String>>) -> String {
    let Some(labels) = labels else {
        return String::new();
    };
    if labels.is_empty() {
        return String::new();
    }

    let mut pairs: Vec<(&String, &String)> = labels.iter().collect();
    pairs.sort_by(|a, b| a.0.cmp(b.0));

    let label_pairs: Vec<String> = pairs
        .iter()
        .map(|(k, v)| format!("{}=\"{}\"", k, v))
        .collect();

    format!("{{{}}}", label_pairs.join(","))
}

/// Multi-format exporter manager
pub struct ExporterManager {
    system_metrics: Option<std::sync::Arc<MetricsSystemMetrics>>,
}

impl Default for ExporterManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ExporterManager {
    pub fn new() -> Self {
        Self {
            system_metrics: None,
        }
    }

    pub fn with_system_metrics(
        mut self,
        system_metrics: std::sync::Arc<MetricsSystemMetrics>,
    ) -> Self {
        self.system_metrics = Some(system_metrics);
        self
    }

    pub async fn export(
        &self,
        format: &str,
        registry: &MetricsRegistry,
    ) -> Result<String, ExportError> {
        let format = match format {
            "prometheus" => ExportFormat::Prometheus,
            "json" => ExportFormat::Json,
            _ => return Err(ExportError::Other(format!("Unknown format: {}", format))),
        };
        Ok(export_with_metrics(
            registry,
            format,
            self.system_metrics.as_deref(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_metrics::MetricsRegistry;

    #[test]
    fn test_export_json() {
        let registry = MetricsRegistry::new();
        registry
            .counter("test_counter", &[("label", "value")])
            .increment();

        let result = export(&registry, ExportFormat::Json);
        assert!(result.contains("test_counter"));
        assert!(result.contains("value"));
    }

    #[test]
    fn test_export_prometheus() {
        let registry = MetricsRegistry::new();
        registry
            .counter("test_counter", &[("label", "value")])
            .increment();

        let result = export(&registry, ExportFormat::Prometheus);
        assert!(result.contains("test_counter"));
        assert!(result.contains("label=\"value\""));
        assert!(result.contains("# HELP"));
    }

    #[tokio::test]
    async fn test_exporter_manager() {
        let registry = MetricsRegistry::new();
        registry.counter("test", &[]).increment();

        let manager = ExporterManager::new();

        let json = manager.export("json", &registry).await.unwrap();
        assert!(json.contains("test"));

        let prom = manager.export("prometheus", &registry).await.unwrap();
        assert!(prom.contains("test"));
    }

    #[tokio::test]
    async fn test_unknown_format() {
        let registry = MetricsRegistry::new();
        let manager = ExporterManager::new();

        let result = manager.export("unknown", &registry).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_export_with_metrics_records_self_monitoring() {
        let registry = MetricsRegistry::new();
        registry.counter("test", &[]).increment();
        let system_metrics = MetricsSystemMetrics::new(&registry);

        let output =
            export_with_metrics(&registry, ExportFormat::Prometheus, Some(&system_metrics));
        assert!(output.contains("test"));
        assert_eq!(system_metrics.export_latency_ms.get_count(), 1);
        system_metrics.update_registry_size(registry.registry_size());
        assert!(system_metrics.registry_size.get() > 0);
    }

    #[tokio::test]
    async fn test_exporter_manager_with_system_metrics() {
        let registry = MetricsRegistry::new();
        registry.counter("test", &[]).increment();
        let system_metrics = MetricsSystemMetrics::new(&registry);
        let manager = ExporterManager::new().with_system_metrics(system_metrics.clone());

        let output = manager.export("json", &registry).await.unwrap();
        assert!(output.contains("test"));
        assert_eq!(system_metrics.export_latency_ms.get_count(), 1);
    }

    #[test]
    fn test_export_json_empty_registry() {
        let registry = MetricsRegistry::new();
        let result = export(&registry, ExportFormat::Json);

        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(parsed.is_object());
    }

    #[test]
    fn test_export_prometheus_histogram() {
        let registry = MetricsRegistry::new();
        let hist = registry.histogram_default("test_latency", &[]);
        hist.observe(10.0);
        hist.observe(20.0);
        hist.observe(30.0);

        let result = export(&registry, ExportFormat::Prometheus);

        assert!(result.contains("test_latency_bucket"));
        assert!(result.contains("test_latency_sum"));
        assert!(result.contains("test_latency_count"));
    }

    #[test]
    fn test_export_prometheus_with_labels() {
        let registry = MetricsRegistry::new();
        registry
            .counter("test_counter", &[("method", "GET"), ("status", "200")])
            .increment();

        let result = export(&registry, ExportFormat::Prometheus);

        assert!(result.contains("test_counter"));
        assert!(result.contains("method=\"GET\""));
        assert!(result.contains("status=\"200\""));
    }

    #[test]
    fn test_prometheus_histogram_bucket_format() {
        let registry = MetricsRegistry::new();
        let hist = registry.histogram("test_req_ms", vec![10.0, 50.0, 100.0], &[]);
        hist.observe(5.0);
        hist.observe(15.0);
        hist.observe(55.0);
        hist.observe(200.0);

        let result = export(&registry, ExportFormat::Prometheus);

        assert!(result.contains("# HELP test_req_ms"));
        assert!(result.contains("# TYPE test_req_ms histogram"));

        let le_10 = result.find("le=\"10\"").expect("le=10 bucket missing");
        let le_50 = result.find("le=\"50\"").expect("le=50 bucket missing");
        let le_100 = result.find("le=\"100\"").expect("le=100 bucket missing");
        let le_inf = result.find("le=\"+Inf\"").expect("le=+Inf bucket missing");
        assert!(le_10 < le_50 && le_50 < le_100 && le_100 < le_inf);

        assert!(result.contains("test_req_ms_bucket{le=\"10\"} 1"));
        assert!(result.contains("test_req_ms_bucket{le=\"50\"} 2"));
        assert!(result.contains("test_req_ms_bucket{le=\"100\"} 3"));
        assert!(result.contains("test_req_ms_bucket{le=\"+Inf\"} 4"));

        assert!(result.contains("test_req_ms_sum"));
        assert!(result.contains("test_req_ms_count 4"));
    }

    #[test]
    fn test_prometheus_counter_type() {
        let registry = MetricsRegistry::new();
        registry.counter("api_requests_total", &[]).add(42);

        let result = export(&registry, ExportFormat::Prometheus);
        assert!(result.contains("# HELP api_requests_total"));
        assert!(result.contains("# TYPE api_requests_total counter"));
        assert!(result.contains("api_requests_total 42"));
    }

    #[test]
    fn test_prometheus_gauge_type() {
        let registry = MetricsRegistry::new();
        registry.gauge("queue_depth", &[]).set(7);

        let result = export(&registry, ExportFormat::Prometheus);
        assert!(result.contains("# HELP queue_depth"));
        assert!(result.contains("# TYPE queue_depth gauge"));
        assert!(result.contains("queue_depth 7"));
    }

    #[test]
    fn test_metric_help_for_known_metrics() {
        assert_eq!(
            metric_help("http_requests_total"),
            "Total number of HTTP requests"
        );
        assert_eq!(
            metric_help("embedding_latency_ms"),
            "Embedding request latency in milliseconds"
        );
        assert_eq!(metric_help("unknown_metric"), "No description available");
    }

    #[test]
    fn test_metric_help_covers_all_domain_metrics() {
        use std::sync::Arc;

        use cce_metrics::{
            Bm25Metrics, EmbeddingErrorType, EmbeddingMetrics, FileProcessingMetrics,
            HotUpdateMetrics, HotUpdateStorageMetrics, HttpMetrics, ParserMetrics, PipelineStage,
            PipelineStageMetrics, PluginMetrics, QdrantMetrics, QueryMetrics, QueueMetrics,
            RelationMetrics, RerankMetrics, ScannerMetrics, SearchMetrics, SearchType,
            SqliteMetrics, SummaryMetrics, SystemMetrics, WatchMetrics,
        };

        let registry = Arc::new(MetricsRegistry::new());

        let embedding = EmbeddingMetrics::new(&registry, "test-provider");
        embedding.record_request(12.5, 128, true);
        embedding.record_request_with_batch(10.0, 2, 256, true);
        embedding.record_error(EmbeddingErrorType::RateLimited);
        embedding.record_retry();

        let parser = ParserMetrics::new(&registry, 1);
        parser.record_parse(5.0, true);
        parser.record_parse(8.0, false);

        let relation = RelationMetrics::new(&registry, 1);
        relation.record_build(30.0, 10, 5);

        let summary = SummaryMetrics::new(&registry, 1);
        summary.record_generation(50.0, 120);
        summary.record_model_enhancement(200.0, true);

        let stage = PipelineStageMetrics::new(&registry, PipelineStage::Grouper, 1);
        stage.record_chunk_size(1024);

        let files = FileProcessingMetrics::new(&registry, 1);
        files.record_file(7.0, true);

        let plugins = PluginMetrics::new(&registry);
        plugins.record_load();
        plugins.record_load_failure();
        plugins.record_execution(3.0, true);
        plugins.record_capability_execution("transform", 4.0, true);
        plugins.record_execution_error("my-plugin");
        plugins.record_unload();

        let rerank = RerankMetrics::new(&registry, "test-model");
        rerank.record_request(90.0, 50, true);
        rerank.record_retry();

        let http = HttpMetrics::new(&registry);
        http.increment_connections();
        http.increment_in_flight();
        http.record_request("GET", 200, "/api/metrics", 5.0, 1024);

        let search = SearchMetrics::new(&registry, 1);
        search.record_search(15.0, Some(SearchType::HybridRecall));
        search.record_index(3);
        search.record_hybrid_alignment(5, 5, 4);

        let query = QueryMetrics::new(&registry, 1);
        query.record_query(20.0, true, 10);

        let hot_update = HotUpdateMetrics::new(&registry, 1);
        hot_update.record_update(100.0, 5, 2, 1, 4);
        hot_update.module_retry_total.increment();

        let hot_update_storage = HotUpdateStorageMetrics::new(&registry, 1);
        hot_update_storage.work_unit_committed_total.increment();
        hot_update_storage.work_unit_uncommitted_total.increment();
        hot_update_storage
            .work_unit_skip_committed_total
            .increment();
        hot_update_storage.candidate_reuse_adopted_total.increment();
        hot_update_storage
            .candidate_reuse_rejected_total
            .increment();

        let watch = WatchMetrics::new(&registry, 1);
        watch.record_event();
        watch.record_file_event();
        watch.record_dir_event();
        watch.record_config_event();
        watch.record_filtered_event();
        watch.record_forwarded_event();
        watch.record_failed_event();
        watch.set_active(true);
        watch.set_status_code(0);
        watch.set_watched_paths(3);

        let bm25 = Bm25Metrics::new(&registry, Some(1));
        bm25.record_index(2.0, 2, true);
        bm25.record_delete(1.0, 1, true);
        bm25.record_disk_usage(4096);

        let qdrant = QdrantMetrics::new(&registry, Some(1));
        qdrant.record_upsert(20.0, 2, true);
        qdrant.record_search(10.0, true);
        qdrant.record_delete(5.0, 1, true);
        qdrant.record_circuit_breaker_state("closed");
        qdrant.record_collection_size(100);

        let sqlite = SqliteMetrics::new(&registry, Some(1));
        sqlite.record_transaction(1.0, true, true);
        sqlite.record_transaction(1.0, false, false);

        let queue = QueueMetrics::new(&registry);
        queue.set_operation_depth(1, 3);
        queue.set_pending_changes_depth(1, 2);
        queue.set_retry_depth(1, 1);
        queue.record_retry_processed();
        queue.record_retry_failed();
        queue.record_file_permanently_failed();

        let scanner = ScannerMetrics::new(&registry, 1);
        scanner.record_scan(30.0, false, false);

        let system = SystemMetrics::new(&registry);
        system.collect();

        let output = export(&registry, ExportFormat::Prometheus);
        let help_lines: Vec<&str> = output.lines().filter(|l| l.starts_with("# HELP")).collect();
        assert!(
            !help_lines.is_empty(),
            "no HELP lines rendered from domain metrics"
        );
        for line in help_lines {
            let name = line.split_whitespace().nth(2).expect("metric name");
            assert!(
                !line.contains("No description available"),
                "metric '{}' has no HELP description",
                name
            );
        }
    }
}
