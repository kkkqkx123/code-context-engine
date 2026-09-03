//! HTTP metrics middleware
//!
//! This module provides Axum middleware for collecting HTTP request metrics,
//! including request count, latency distribution, and error rates.
//!
//! Uses the `HttpMetrics` domain wrapper for path normalization and
//! consistent metric naming across the system.

use axum::{extract::Request, middleware::Next, response::Response};
use cce_metrics_infra::HttpMetrics;
use std::sync::Arc;
use std::time::Instant;

/// HTTP metrics middleware layer
#[derive(Clone)]
pub struct MetricsMiddleware {
    metrics: Arc<HttpMetrics>,
}

impl MetricsMiddleware {
    /// Create new metrics middleware
    pub fn new(metrics: Arc<HttpMetrics>) -> Self {
        Self { metrics }
    }

    /// Middleware handler function
    pub async fn call(&self, request: Request, next: Next) -> Response {
        let start = Instant::now();
        let method = request.method().clone();
        let uri = request.uri().path().to_string();
        let body_size = request
            .headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);

        self.metrics.increment_connections();
        self.metrics.increment_in_flight();

        let response = next.run(request).await;

        let duration_ms = start.elapsed().as_secs_f64() * 1000.0;
        let status = response.status().as_u16();

        self.metrics
            .record_request(method.as_str(), status, &uri, duration_ms, body_size);

        self.metrics.decrement_in_flight();
        self.metrics.decrement_connections();

        response
    }
}

/// Convenience function to create the middleware
pub fn metrics_middleware(
    metrics: Arc<HttpMetrics>,
) -> impl Fn(Request, Next) -> std::pin::Pin<Box<dyn std::future::Future<Output = Response> + Send>>
+ Clone {
    let middleware = MetricsMiddleware::new(metrics);

    move |request: Request, next: Next| {
        let mw = middleware.clone();
        Box::pin(async move { mw.call(request, next).await })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_middleware_creation() {
        let registry = cce_metrics_infra::MetricsRegistry::new();
        let metrics = HttpMetrics::new(&registry);
        let _middleware = MetricsMiddleware::new(metrics);
    }
}
