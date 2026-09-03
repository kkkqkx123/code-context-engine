//! Single-core metric rendering cache

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use cce_metrics::MetricsRegistry;

use crate::exporter::format_prometheus_snapshot;

/// A rendered set of metric formats sharing one snapshot moment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedRender {
    pub prometheus: String,
    pub json: String,
    pub rendered_at: chrono::DateTime<chrono::Utc>,
}

/// Cached metric renderer owned by a single rendering task
#[derive(Debug, Clone)]
pub struct RenderCache {
    registry: Arc<MetricsRegistry>,
    inner: Arc<tokio::sync::RwLock<CachedRender>>,
}

impl RenderCache {
    pub fn new(registry: Arc<MetricsRegistry>) -> Self {
        Self {
            registry,
            inner: Arc::new(tokio::sync::RwLock::new(CachedRender {
                prometheus: String::new(),
                json: String::new(),
                rendered_at: chrono::Utc::now(),
            })),
        }
    }

    pub async fn render(&self) {
        let snapshot = self.registry.export_all();
        let prometheus = format_prometheus_snapshot(&snapshot);
        let json = serde_json::to_string_pretty(&snapshot)
            .unwrap_or_else(|e| format!("{{\"error\": \"serialization failed: {}\"}}", e));
        let mut guard = self.inner.write().await;
        guard.prometheus = prometheus;
        guard.json = json;
        guard.rendered_at = chrono::Utc::now();
    }

    pub fn start(self: Arc<Self>, refresh_interval: Duration) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(refresh_interval);
            ticker.tick().await;
            loop {
                ticker.tick().await;
                self.render().await;
            }
        })
    }

    pub async fn prometheus(&self) -> String {
        if self.inner.read().await.prometheus.is_empty() {
            self.render().await;
        }
        self.inner.read().await.prometheus.clone()
    }

    pub async fn json(&self) -> String {
        if self.inner.read().await.json.is_empty() {
            self.render().await;
        }
        self.inner.read().await.json.clone()
    }

    pub async fn rendered_at(&self) -> chrono::DateTime<chrono::Utc> {
        self.inner.read().await.rendered_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_metrics::MetricsRegistry;

    #[tokio::test]
    async fn test_render_cache_serves_both_formats() {
        let registry = Arc::new(MetricsRegistry::new());
        registry.counter("render_test_counter", &[]).increment();
        registry.gauge("render_test_gauge", &[]).set(7);
        registry
            .histogram_default("render_test_hist", &[])
            .observe(12.0);

        let cache = RenderCache::new(registry);
        let prometheus = cache.prometheus().await;
        let json = cache.json().await;

        assert!(prometheus.contains("render_test_counter 1"));
        assert!(prometheus.contains("render_test_gauge 7"));
        assert!(json.contains("render_test_counter"));
        assert!(json.contains("total_counters"));
    }

    #[tokio::test]
    async fn test_render_cache_background_refresh() {
        let registry = Arc::new(MetricsRegistry::new());
        let cache = RenderCache::new(registry.clone());

        cache.render().await;
        let before = cache.rendered_at().await;

        registry.counter("refresh_test_counter", &[]).increment();

        let handle = Arc::new(cache.clone()).start(Duration::from_millis(50));
        tokio::time::sleep(Duration::from_millis(180)).await;
        handle.abort();

        let after = cache.rendered_at().await;
        assert!(after > before);
        assert!(cache.prometheus().await.contains("refresh_test_counter 1"));
    }

    #[tokio::test]
    async fn test_render_cache_empty_registry() {
        let cache = RenderCache::new(Arc::new(MetricsRegistry::new()));
        cache.render().await;
        let json = cache.json().await;
        assert!(json.contains("\"metrics\": []"));
        assert!(cache.prometheus().await.is_empty());
    }
}
