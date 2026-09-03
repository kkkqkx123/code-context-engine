//! HTTP request metrics
//!
//! Provides metrics for HTTP request handling with path normalization
//! to prevent cardinality explosion from dynamic route segments.

use std::sync::Arc;

use crate::{LabeledGauge, MetricsRegistry};

/// HTTP request monitoring metrics
///
/// Tracks HTTP request count, latency distribution, error rates,
/// active connections, and in-flight requests.
/// Paths are normalized to route templates (e.g., `/api/project/{id}`)
/// to avoid unbounded cardinality from dynamic segments.
#[derive(Debug, Clone)]
pub struct HttpMetrics {
    registry: Arc<MetricsRegistry>,
    active_connections: LabeledGauge,
    requests_in_flight: LabeledGauge,
}

impl HttpMetrics {
    /// Create new HTTP metrics with the given registry
    pub fn new(registry: &MetricsRegistry) -> Arc<Self> {
        Arc::new(Self {
            registry: Arc::new(registry.clone()),
            active_connections: registry.gauge("http_active_connections", &[]),
            requests_in_flight: registry.gauge("http_requests_in_flight", &[]),
        })
    }

    /// Increment the active connections gauge
    pub fn increment_connections(&self) {
        self.active_connections.increment();
    }

    /// Decrement the active connections gauge (saturating at 0)
    pub fn decrement_connections(&self) {
        self.active_connections.decrement();
    }

    /// Increment the in-flight requests gauge
    pub fn increment_in_flight(&self) {
        self.requests_in_flight.increment();
    }

    /// Decrement the in-flight requests gauge (saturating at 0)
    pub fn decrement_in_flight(&self) {
        self.requests_in_flight.decrement();
    }

    /// Record a completed HTTP request
    ///
    /// # Arguments
    ///
    /// * `method` - HTTP method (GET, POST, etc.)
    /// * `status` - HTTP status code
    /// * `path` - Raw request path (will be normalized)
    /// * `duration_ms` - Request duration in milliseconds
    /// * `body_size_bytes` - Request body size in bytes (0 if unknown)
    pub fn record_request(
        &self,
        method: &str,
        status: u16,
        path: &str,
        duration_ms: f64,
        body_size_bytes: u64,
    ) {
        let normalized = normalize_path(path);
        let status_str = status.to_string();
        let status_class = format!("{}xx", status / 100);

        self.registry
            .counter(
                "http_requests_total",
                &[
                    ("method", method),
                    ("status", &status_str),
                    ("status_class", &status_class),
                    ("path", normalized),
                ],
            )
            .increment();

        self.registry
            .histogram(
                "http_request_duration_ms",
                crate::LATENCY_BUCKETS.to_vec(),
                &[("method", method), ("path", normalized)],
            )
            .observe(duration_ms);

        if body_size_bytes > 0 {
            self.registry
                .histogram(
                    "http_request_body_size_bytes",
                    vec![
                        64.0, 256.0, 1024.0, 4096.0, 16384.0, 65536.0, 262144.0, 1048576.0,
                        4194304.0, 16777216.0,
                    ],
                    &[],
                )
                .observe(body_size_bytes as f64);
        }

        if status >= 500 {
            self.registry
                .counter(
                    "http_errors_total",
                    &[
                        ("method", method),
                        ("status", &status_str),
                        ("path", normalized),
                    ],
                )
                .increment();
        }
    }
}

/// Normalize a request path to a route template
///
/// Converts dynamic path segments (IDs, arbitrary paths) into template
/// placeholders to prevent metric label cardinality explosion.
///
/// Known route patterns from the router are matched; unknown paths
/// fall back to a generic bucket.
fn normalize_path(path: &str) -> &'static str {
    let path = path.split('?').next().unwrap_or(path);

    match path {
        "/api/index" => "/api/index",
        "/api/index/incremental" => "/api/index/incremental",
        "/api/parse" => "/api/parse",
        "/api/summary" => "/api/summary",
        "/api/index/batch" => "/api/index/batch",
        "/api/index/stats" => "/api/index/stats",
        "/api/storage/status" => "/api/storage/status",
        "/api/project" => "/api/project",
        "/api/metrics" => "/api/metrics",
        "/api/metrics/json" => "/api/metrics/json",
        "/api/metrics/history" => "/api/metrics/history",
        "/api/metrics/cleanup" => "/api/metrics/cleanup",
        "/api/search" => "/api/search",
        "/api/search/aggregated" => "/api/search/aggregated",
        "/api/entities/search" => "/api/entities/search",
        "/api/config" => "/api/config",
        "/api/config/reload" => "/api/config/reload",
        "/api/config/validate" => "/api/config/validate",
        "/api/health" => "/api/health",
        "/api/health/qdrant" => "/api/health/qdrant",
        "/api/health/embedding" => "/api/health/embedding",
        "/api/health/bm25" => "/api/health/bm25",
        "/api/retry-queue" => "/api/retry-queue",
        "/api/retry-queue/process" => "/api/retry-queue/process",
        "/api/qdrant/process/status" => "/api/qdrant/process/status",
        "/api/qdrant/process/start" => "/api/qdrant/process/start",
        "/api/qdrant/process/stop" => "/api/qdrant/process/stop",
        "/api/qdrant/process/restart" => "/api/qdrant/process/restart",
        "/api/tools/compress" => "/api/tools/compress",
        "/api/tools/compress/batch" => "/api/tools/compress/batch",
        "/api/tools/diagnose" => "/api/tools/diagnose",
        "/api/tools/keyword-search" => "/api/tools/keyword-search",
        "/api/tools/symbols" => "/api/tools/symbols",
        "/api/tools/references" => "/api/tools/references",
        "/api/tools/definition" => "/api/tools/definition",
        _ => {
            if path.starts_with("/api/index/file/") {
                "/api/index/file/{path}"
            } else if path.starts_with("/api/index/entity/") {
                "/api/index/entity/{id}"
            } else if path.starts_with("/api/project/") {
                normalize_project_path(path)
            } else {
                "_unmatched"
            }
        }
    }
}

/// Normalize project-scoped paths with their specific sub-routes
fn normalize_project_path(path: &str) -> &'static str {
    const PREFIX: &str = "/api/project/";
    let after = &path[PREFIX.len()..];

    if !after.contains('/') {
        return "/api/project/{id}";
    }

    let slash_idx = after.find('/').unwrap_or(after.len());
    let project_id = &after[..slash_idx];
    let sub = &after[slash_idx..];

    match sub {
        "/index" => "/api/project/{id}/index",
        "/reload" => "/api/project/{id}/reload",
        "/config" => "/api/project/{id}/config",
        "/watch/start" => "/api/project/{id}/watch/start",
        "/watch/stop" => "/api/project/{id}/watch/stop",
        "/watch/status" => "/api/project/{id}/watch/status",
        "/call-path" => "/api/project/{project_id}/call-path",
        _ => {
            if let Some(func_suffix) = sub.strip_prefix("/function/") {
                if func_suffix.ends_with("/calls") {
                    "/api/project/{project_id}/function/{id}/calls"
                } else if func_suffix.ends_with("/callers") {
                    "/api/project/{project_id}/function/{id}/callers"
                } else {
                    "/api/project/{project_id}/function/{id}"
                }
            } else if let Some(class_suffix) = sub.strip_prefix("/class/") {
                if class_suffix.ends_with("/inheritance") {
                    "/api/project/{project_id}/class/{id}/inheritance"
                } else if class_suffix.ends_with("/implementations") {
                    "/api/project/{project_id}/class/{id}/implementations"
                } else {
                    "_unmatched"
                }
            } else if sub.starts_with("/call-chain/") {
                "/api/project/{project_id}/call-chain/{id}"
            } else {
                let _ = project_id;
                "_unmatched"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_static_paths() {
        assert_eq!(normalize_path("/api/index"), "/api/index");
        assert_eq!(normalize_path("/api/search"), "/api/search");
        assert_eq!(normalize_path("/api/health"), "/api/health");
        assert_eq!(normalize_path("/api/health/qdrant"), "/api/health/qdrant");
    }

    #[test]
    fn test_normalize_dynamic_paths() {
        assert_eq!(normalize_path("/api/project/123"), "/api/project/{id}");
        assert_eq!(
            normalize_path("/api/project/abc/index"),
            "/api/project/{id}/index"
        );
        assert_eq!(
            normalize_path("/api/index/file/src/main.rs"),
            "/api/index/file/{path}"
        );
        assert_eq!(
            normalize_path("/api/index/entity/42"),
            "/api/index/entity/{id}"
        );
    }

    #[test]
    fn test_normalize_function_routes() {
        assert_eq!(
            normalize_path("/api/project/1/function/42"),
            "/api/project/{project_id}/function/{id}"
        );
        assert_eq!(
            normalize_path("/api/project/1/function/42/calls"),
            "/api/project/{project_id}/function/{id}/calls"
        );
        assert_eq!(
            normalize_path("/api/project/1/function/42/callers"),
            "/api/project/{project_id}/function/{id}/callers"
        );
    }

    #[test]
    fn test_normalize_class_routes() {
        assert_eq!(
            normalize_path("/api/project/1/class/42/inheritance"),
            "/api/project/{project_id}/class/{id}/inheritance"
        );
        assert_eq!(
            normalize_path("/api/project/1/class/42/implementations"),
            "/api/project/{project_id}/class/{id}/implementations"
        );
    }

    #[test]
    fn test_normalize_call_chain_routes() {
        assert_eq!(
            normalize_path("/api/project/1/call-chain/42"),
            "/api/project/{project_id}/call-chain/{id}"
        );
        assert_eq!(
            normalize_path("/api/project/1/call-path"),
            "/api/project/{project_id}/call-path"
        );
    }

    #[test]
    fn test_normalize_with_query_string() {
        assert_eq!(
            normalize_path("/api/metrics/history?from=100&to=200"),
            "/api/metrics/history"
        );
        assert_eq!(
            normalize_path("/api/project/123?foo=bar"),
            "/api/project/{id}"
        );
    }

    #[test]
    fn test_normalize_unmatched() {
        assert_eq!(normalize_path("/unknown/path"), "_unmatched");
    }

    #[test]
    fn test_path_cardinality_bounded() {
        let paths = [
            normalize_path("/api/project/1"),
            normalize_path("/api/project/2"),
            normalize_path("/api/project/999"),
            normalize_path("/api/project/abc"),
            normalize_path("/api/project/very-long-id-string"),
        ];
        for p in &paths {
            assert_eq!(p, &"/api/project/{id}");
        }
    }

    #[test]
    fn test_http_metrics_creation() {
        let registry = MetricsRegistry::new();
        let metrics = HttpMetrics::new(&registry);
        assert_eq!(
            metrics.registry.counter("http_requests_total", &[]).get(),
            0
        );
    }

    #[test]
    fn test_http_metrics_record_request() {
        let registry = MetricsRegistry::new();
        let metrics = HttpMetrics::new(&registry);

        metrics.record_request("GET", 200, "/api/index", 5.0, 0);
        metrics.record_request("POST", 200, "/api/search", 15.0, 100);
        metrics.record_request("GET", 500, "/api/project/42", 100.0, 0);

        let req_total = registry.counter(
            "http_requests_total",
            &[
                ("method", "GET"),
                ("status", "200"),
                ("status_class", "2xx"),
                ("path", "/api/index"),
            ],
        );
        assert_eq!(req_total.get(), 1);

        let err_total = registry.counter(
            "http_errors_total",
            &[
                ("method", "GET"),
                ("status", "500"),
                ("path", "/api/project/{id}"),
            ],
        );
        assert_eq!(err_total.get(), 1);

        let latency = registry.histogram(
            "http_request_duration_ms",
            crate::LATENCY_BUCKETS.to_vec(),
            &[("method", "GET"), ("path", "/api/index")],
        );
        assert_eq!(latency.get_count(), 1);
    }

    #[test]
    fn test_http_active_connections() {
        let registry = MetricsRegistry::new();
        let metrics = HttpMetrics::new(&registry);

        metrics.increment_connections();
        metrics.increment_connections();
        metrics.increment_connections();

        let active = registry.gauge("http_active_connections", &[]);
        assert_eq!(active.get(), 3);

        metrics.decrement_connections();
        assert_eq!(active.get(), 2);

        metrics.decrement_connections();
        metrics.decrement_connections();
        metrics.decrement_connections();
        assert_eq!(active.get(), 0);
    }

    #[test]
    fn test_http_requests_in_flight() {
        let registry = MetricsRegistry::new();
        let metrics = HttpMetrics::new(&registry);

        metrics.increment_in_flight();
        metrics.increment_in_flight();

        let in_flight = registry.gauge("http_requests_in_flight", &[]);
        assert_eq!(in_flight.get(), 2);

        metrics.decrement_in_flight();
        assert_eq!(in_flight.get(), 1);

        metrics.decrement_in_flight();
        metrics.decrement_in_flight();
        assert_eq!(in_flight.get(), 0);
    }
}
