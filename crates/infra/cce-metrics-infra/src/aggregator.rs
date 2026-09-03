//! Metrics aggregation engine
//!
//! This module provides background aggregation of metrics data from memory
//! into SQLite for historical analysis.

use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::time::{self, Duration};
use tracing::{debug, error, info, warn};

use cce_metrics::{BackgroundTaskMetrics, MetricKey, MetricsRegistry};
use cce_storage_common::{AggregatedMetric, SqliteStore};

/// Configuration for the aggregation engine
#[derive(Debug, Clone)]
pub struct AggregationConfig {
    pub interval_secs: u64,
    pub enabled: bool,
    pub retention_seconds: u64,
    pub cleanup_interval_secs: u64,
    pub aggregate_counters: bool,
    pub aggregate_gauges: bool,
}

impl Default for AggregationConfig {
    fn default() -> Self {
        Self {
            interval_secs: 300,
            enabled: true,
            retention_seconds: 604800,
            cleanup_interval_secs: 3600,
            aggregate_counters: true,
            aggregate_gauges: true,
        }
    }
}

/// Per-histogram window baseline for delta computation
#[derive(Debug, Default, Clone)]
pub(crate) struct HistogramWindowState {
    pub(crate) last_count: u64,
    pub(crate) last_sum_us: u64,
    pub(crate) last_bucket_counts: Vec<u64>,
}

/// State maintained between aggregation cycles for delta computation
#[derive(Debug, Default)]
pub(crate) struct AggregationState {
    pub(crate) last_counter_values: HashMap<String, u64>,
    pub(crate) last_histogram_values: HashMap<String, HistogramWindowState>,
}

/// Metrics aggregation engine
pub struct MetricsAggregator<S: SqliteStore> {
    store: Arc<S>,
    metrics_registry: Arc<MetricsRegistry>,
    config: AggregationConfig,
    last_aggregation: Arc<std::sync::Mutex<Option<DateTime<Utc>>>>,
    aggregation_state: Arc<std::sync::Mutex<AggregationState>>,
    background_metrics: Option<Arc<BackgroundTaskMetrics>>,
}

impl<S: SqliteStore> MetricsAggregator<S> {
    pub fn new(
        store: Arc<S>,
        metrics_registry: Arc<MetricsRegistry>,
        config: AggregationConfig,
    ) -> Self {
        Self {
            store,
            metrics_registry,
            config,
            last_aggregation: Arc::new(std::sync::Mutex::new(None)),
            aggregation_state: Arc::new(std::sync::Mutex::new(AggregationState::default())),
            background_metrics: None,
        }
    }

    pub fn with_background_metrics(
        mut self,
        background_metrics: Arc<BackgroundTaskMetrics>,
    ) -> Self {
        self.background_metrics = Some(background_metrics);
        self
    }

    pub fn config(&self) -> &AggregationConfig {
        &self.config
    }

    pub fn metrics_registry(&self) -> &Arc<MetricsRegistry> {
        &self.metrics_registry
    }

    pub fn start(&self) -> tokio::task::JoinHandle<()> {
        let store = Arc::clone(&self.store);
        let metrics_registry = Arc::clone(&self.metrics_registry);
        let interval = Duration::from_secs(self.config.interval_secs);
        let last_agg = Arc::clone(&self.last_aggregation);
        let agg_state = Arc::clone(&self.aggregation_state);
        let background_metrics = self.background_metrics.clone();

        let aggregate_counters = self.config.aggregate_counters;
        let aggregate_gauges = self.config.aggregate_gauges;

        tokio::spawn(async move {
            info!(
                interval_secs = interval.as_secs(),
                "Starting metrics aggregation task"
            );

            Self::initialize_baseline(&metrics_registry, &agg_state);

            let mut interval_timer = time::interval(interval);

            loop {
                interval_timer.tick().await;

                let started = std::time::Instant::now();
                match Self::aggregate_and_store(
                    store.as_ref(),
                    &metrics_registry,
                    &last_agg,
                    &agg_state,
                    aggregate_counters,
                    aggregate_gauges,
                )
                .await
                {
                    Ok(count) => {
                        debug!(aggregated_count = count, "Metrics aggregation completed");
                        if let Some(bg) = &background_metrics {
                            bg.record_aggregation(
                                count,
                                started.elapsed().as_secs_f64() * 1000.0,
                                Utc::now().timestamp() as u64,
                            );
                        }
                    }
                    Err(e) => {
                        error!(error = %e, "Metrics aggregation failed");
                        if let Some(bg) = &background_metrics {
                            bg.record_aggregation_error();
                        }
                    }
                }
            }
        })
    }

    fn initialize_baseline(
        metrics_registry: &MetricsRegistry,
        agg_state: &Arc<std::sync::Mutex<AggregationState>>,
    ) {
        let counters = metrics_registry.get_all_counters_with_keys();
        if counters.is_empty() {
            return;
        }
        let mut state = agg_state.lock().expect("aggregation_state lock poisoned");
        for (metric_key, value) in counters {
            if metric_key.name.starts_with("tokio_") {
                continue;
            }
            state
                .last_counter_values
                .insert(metric_key.to_storage_key(), value);
        }
        info!(
            baseline_counters = state.last_counter_values.len(),
            "Metrics aggregation baseline initialized"
        );
    }

    pub fn start_with_cleanup(&self) -> Vec<tokio::task::JoinHandle<()>> {
        let agg_handle = self.start();

        let store = Arc::clone(&self.store);
        let cleanup_interval = Duration::from_secs(self.config.cleanup_interval_secs);
        let retention = self.config.retention_seconds;
        let background_metrics = self.background_metrics.clone();

        let cleanup_handle = tokio::spawn(async move {
            info!(
                cleanup_interval_secs = cleanup_interval.as_secs(),
                retention_secs = retention,
                "Starting metrics cleanup task"
            );

            let mut interval_timer = time::interval(cleanup_interval);
            interval_timer.tick().await;

            loop {
                interval_timer.tick().await;

                let cutoff = Utc::now() - chrono::Duration::seconds(retention as i64);
                let cutoff_str = cutoff.to_rfc3339();
                match store.execute_write(
                    "DELETE FROM metrics_aggregated WHERE timestamp < ?1",
                    &[&cutoff_str as &dyn rusqlite::ToSql],
                ) {
                    Ok(deleted) => {
                        debug!(deleted_count = deleted, "Metrics cleanup completed");
                        if let Some(bg) = &background_metrics {
                            bg.record_cleanup(Utc::now().timestamp() as u64);
                        }
                    }
                    Err(e) => {
                        error!(error = %e, "Metrics cleanup failed");
                    }
                }
            }
        });

        vec![agg_handle, cleanup_handle]
    }

    pub(crate) async fn aggregate_and_store(
        store: &S,
        metrics_registry: &MetricsRegistry,
        last_agg: &Arc<std::sync::Mutex<Option<DateTime<Utc>>>>,
        agg_state: &Arc<std::sync::Mutex<AggregationState>>,
        aggregate_counters: bool,
        aggregate_gauges: bool,
    ) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
        let now = Utc::now();
        let mut records = Vec::new();

        Self::collect_histogram_records(metrics_registry, agg_state, now, &mut records);

        if aggregate_counters {
            Self::collect_counter_delta_records(metrics_registry, agg_state, now, &mut records);
        }

        if aggregate_gauges {
            Self::collect_gauge_snapshot_records(metrics_registry, now, &mut records);
        }

        let aggregated_count = if records.is_empty() {
            debug!("No metrics to aggregate");
            0
        } else {
            match Self::store_records(store, &records).await {
                Ok(count) => count,
                Err(e) => {
                    error!(error = %e, "Failed to store aggregated metrics batch");
                    0
                }
            }
        };

        {
            let mut last = last_agg.lock().expect("last_aggregation lock poisoned");
            *last = Some(now);
        }

        Ok(aggregated_count)
    }

    fn collect_histogram_records(
        metrics_registry: &MetricsRegistry,
        agg_state: &Arc<std::sync::Mutex<AggregationState>>,
        now: DateTime<Utc>,
        records: &mut Vec<AggregatedMetric>,
    ) {
        let histograms = metrics_registry.get_all_histograms_with_handles();

        for (metric_key, histogram) in histograms {
            if metric_key.name.starts_with("tokio_") {
                continue;
            }

            let storage_key = metric_key.to_storage_key();
            let current_count = histogram.get_count();
            let current_sum_us = histogram.get_sum();
            let current_bucket_counts = histogram.get_bucket_counts();
            let buckets = histogram.get_buckets().to_vec();

            let (window_count, window_sum_us, window_bucket_counts) = {
                let mut state = agg_state.lock().expect("aggregation_state lock poisoned");
                let last = state.last_histogram_values.entry(storage_key).or_default();
                let count = current_count.saturating_sub(last.last_count);
                let sum_us = current_sum_us.saturating_sub(last.last_sum_us);
                let bucket_counts = current_bucket_counts
                    .iter()
                    .zip(last.last_bucket_counts.iter())
                    .map(|(cur, prev)| cur.saturating_sub(*prev))
                    .collect::<Vec<_>>();
                last.last_count = current_count;
                last.last_sum_us = current_sum_us;
                last.last_bucket_counts = current_bucket_counts;
                (count, sum_us, bucket_counts)
            };

            if window_count == 0 {
                continue;
            }

            let labels_json = if !metric_key.labels.is_empty() {
                match serde_json::to_string(&metric_key.labels) {
                    Ok(json) => Some(json),
                    Err(e) => {
                        warn!(
                            metric_name = %metric_key.name,
                            error = %e,
                            "Failed to serialize labels"
                        );
                        continue;
                    }
                }
            } else {
                None
            };

            let project_id = metric_key
                .labels
                .iter()
                .find(|label| label.key == "project_id")
                .and_then(|label| label.value.parse::<i64>().ok());

            let operation_type = metric_key
                .labels
                .iter()
                .find(|label| label.key == "operation")
                .map(|label| label.value.clone());

            records.push(AggregatedMetric {
                timestamp: now,
                metric_name: metric_key.name.clone(),
                metric_type: "histogram".to_string(),
                labels_json,
                count: window_count as i64,
                avg: Some(window_sum_us as f64 / window_count as f64 / 1000.0),
                median: Some(Self::window_percentile(
                    &buckets,
                    &window_bucket_counts,
                    window_count,
                    50.0,
                )),
                max: Some(histogram.take_window_max_ms()),
                p90: Some(Self::window_percentile(
                    &buckets,
                    &window_bucket_counts,
                    window_count,
                    90.0,
                )),
                p99: Some(Self::window_percentile(
                    &buckets,
                    &window_bucket_counts,
                    window_count,
                    99.0,
                )),
                project_id,
                operation_type,
            });
        }
    }

    fn window_percentile(
        buckets: &[f64],
        bucket_counts: &[u64],
        total: u64,
        percentile: f64,
    ) -> f64 {
        if total == 0 {
            return 0.0;
        }
        let target = (total as f64 * percentile / 100.0) as u64;
        let mut cumulative = 0u64;
        for (i, count) in bucket_counts.iter().enumerate() {
            cumulative += count;
            if cumulative >= target {
                return buckets.get(i).copied().unwrap_or(0.0);
            }
        }
        buckets.last().copied().unwrap_or(0.0)
    }

    fn collect_counter_delta_records(
        metrics_registry: &MetricsRegistry,
        agg_state: &Arc<std::sync::Mutex<AggregationState>>,
        now: DateTime<Utc>,
        records: &mut Vec<AggregatedMetric>,
    ) {
        let counters = metrics_registry.get_all_counters_with_keys();

        for (metric_key, current_value) in counters {
            if metric_key.name.starts_with("tokio_") {
                continue;
            }

            let storage_key = metric_key.to_storage_key();

            let delta = {
                let mut state = agg_state.lock().expect("aggregation_state lock poisoned");
                let last = state
                    .last_counter_values
                    .get(&storage_key)
                    .copied()
                    .unwrap_or(0);
                state
                    .last_counter_values
                    .insert(storage_key.clone(), current_value);
                current_value.saturating_sub(last)
            };

            if delta == 0 {
                continue;
            }

            let labels_json = if !metric_key.labels.is_empty() {
                match serde_json::to_string(&metric_key.labels) {
                    Ok(json) => Some(json),
                    Err(e) => {
                        warn!(
                            metric_name = %metric_key.name,
                            error = %e,
                            "Failed to serialize counter labels"
                        );
                        continue;
                    }
                }
            } else {
                None
            };

            let project_id = metric_key
                .labels
                .iter()
                .find(|label| label.key == "project_id")
                .and_then(|label| label.value.parse::<i64>().ok());

            let operation_type = metric_key
                .labels
                .iter()
                .find(|label| label.key == "operation")
                .map(|label| label.value.clone());

            records.push(AggregatedMetric {
                timestamp: now,
                metric_name: metric_key.name.clone(),
                metric_type: "counter".to_string(),
                labels_json,
                count: delta as i64,
                avg: None,
                median: None,
                max: None,
                p90: None,
                p99: None,
                project_id,
                operation_type,
            });
        }
    }

    fn collect_gauge_snapshot_records(
        metrics_registry: &MetricsRegistry,
        now: DateTime<Utc>,
        records: &mut Vec<AggregatedMetric>,
    ) {
        let gauges = metrics_registry.get_all_gauges_with_keys();
        let float_gauges = metrics_registry.get_all_float_gauges_with_keys();
        let all_gauges: Vec<(MetricKey, f64)> = gauges
            .into_iter()
            .map(|(k, v)| (k, v as f64))
            .chain(float_gauges)
            .collect();

        for (metric_key, value) in all_gauges {
            if metric_key.name.starts_with("tokio_") {
                continue;
            }

            let labels_json = if !metric_key.labels.is_empty() {
                match serde_json::to_string(&metric_key.labels) {
                    Ok(json) => Some(json),
                    Err(e) => {
                        warn!(
                            metric_name = %metric_key.name,
                            error = %e,
                            "Failed to serialize gauge labels"
                        );
                        continue;
                    }
                }
            } else {
                None
            };

            let project_id = metric_key
                .labels
                .iter()
                .find(|label| label.key == "project_id")
                .and_then(|label| label.value.parse::<i64>().ok());

            let operation_type = metric_key
                .labels
                .iter()
                .find(|label| label.key == "operation")
                .map(|label| label.value.clone());

            records.push(AggregatedMetric {
                timestamp: now,
                metric_name: metric_key.name.clone(),
                metric_type: "gauge".to_string(),
                labels_json,
                count: 1,
                avg: Some(value),
                median: None,
                max: Some(value),
                p90: None,
                p99: None,
                project_id,
                operation_type,
            });
        }
    }

    async fn store_records(
        store: &S,
        records: &[AggregatedMetric],
    ) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
        let mut inserted = 0;
        for record in records {
            let params: Vec<Box<dyn rusqlite::ToSql>> = vec![
                Box::new(record.timestamp.to_rfc3339()),
                Box::new(record.metric_name.clone()),
                Box::new(record.metric_type.clone()),
                Box::new(record.labels_json.clone()),
                Box::new(record.count),
                Box::new(record.avg),
                Box::new(record.median),
                Box::new(record.max),
                Box::new(record.p90),
                Box::new(record.p99),
                Box::new(record.project_id),
                Box::new(record.operation_type.clone()),
            ];
            let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
            if store.execute_write(
                "INSERT INTO metrics_aggregated
                 (timestamp, metric_name, metric_type, labels_json, count, avg, median, max, p90, p99, project_id, operation_type)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                &param_refs,
            ).is_ok() {
                inserted += 1;
            }
        }
        Ok(inserted)
    }

    pub async fn query_history(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        metric_name: Option<&str>,
        project_id: Option<i64>,
        operation_type: Option<&str>,
    ) -> Result<Vec<AggregatedMetric>, Box<dyn std::error::Error + Send + Sync>> {
        let mut sql = "SELECT timestamp, metric_name, metric_type, labels_json, count, avg, median, max, p90, p99, project_id, operation_type
                       FROM metrics_aggregated
                       WHERE timestamp BETWEEN ?1 AND ?2".to_string();

        let mut params: Vec<Box<dyn rusqlite::ToSql>> =
            vec![Box::new(from.to_rfc3339()), Box::new(to.to_rfc3339())];

        if let Some(name) = metric_name {
            sql.push_str(" AND metric_name = ?3");
            params.push(Box::new(name.to_string()));
        }

        if let Some(pid) = project_id {
            let param_idx = params.len() + 1;
            sql.push_str(&format!(" AND project_id = ?{}", param_idx));
            params.push(Box::new(pid));
        }

        if let Some(op_type) = operation_type {
            let param_idx = params.len() + 1;
            sql.push_str(&format!(" AND operation_type = ?{}", param_idx));
            params.push(Box::new(op_type.to_string()));
        }

        sql.push_str(" ORDER BY timestamp ASC");

        let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();

        self.store
            .query_rows(&sql, &param_refs, &mut |row| {
                Ok(AggregatedMetric {
                    timestamp: row.get::<_, String>(0)?.parse().map_err(
                        |e: chrono::ParseError| {
                            rusqlite::Error::FromSqlConversionFailure(
                                0,
                                rusqlite::types::Type::Text,
                                Box::new(e),
                            )
                        },
                    )?,
                    metric_name: row.get(1)?,
                    metric_type: row.get(2)?,
                    labels_json: row.get(3)?,
                    count: row.get(4)?,
                    avg: row.get(5)?,
                    median: row.get(6)?,
                    max: row.get(7)?,
                    p90: row.get(8)?,
                    p99: row.get(9)?,
                    project_id: row.get(10)?,
                    operation_type: row.get(11)?,
                })
            })
            .map_err(|e| e.into())
    }

    pub async fn cleanup(
        &self,
        before: Option<DateTime<Utc>>,
        all: bool,
    ) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
        let deleted = if all {
            self.store
                .execute_write("DELETE FROM metrics_aggregated", &[])
                .map_err(|e| format!("Failed to cleanup all metrics: {}", e))?
        } else if let Some(cutoff) = before {
            let cutoff_str = cutoff.to_rfc3339();
            self.store
                .execute_write(
                    "DELETE FROM metrics_aggregated WHERE timestamp < ?1",
                    &[&cutoff_str as &dyn rusqlite::ToSql],
                )
                .map_err(|e| format!("Failed to cleanup metrics before {}: {}", cutoff, e))?
        } else {
            return Err("Either 'before' or 'all' must be specified".into());
        };

        info!(deleted_count = deleted, "Metrics cleanup completed");
        Ok(deleted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aggregation_config_default() {
        let config = AggregationConfig::default();
        assert_eq!(config.interval_secs, 300);
        assert!(config.enabled);
        assert_eq!(config.retention_seconds, 604800);
        assert_eq!(config.cleanup_interval_secs, 3600);
    }

    #[test]
    fn test_aggregation_config_custom() {
        let config = AggregationConfig {
            interval_secs: 60,
            enabled: false,
            retention_seconds: 86400,
            cleanup_interval_secs: 1800,
            aggregate_counters: false,
            aggregate_gauges: false,
        };
        assert_eq!(config.interval_secs, 60);
        assert!(!config.enabled);
        assert_eq!(config.retention_seconds, 86400);
        assert_eq!(config.cleanup_interval_secs, 1800);
    }

    #[test]
    fn test_window_percentile_empty() {
        assert_eq!(
            MetricsAggregator::<DummyStore>::window_percentile(&[], &[], 0, 50.0),
            0.0
        );
    }

    #[test]
    fn test_window_percentile_distribution() {
        let buckets = vec![10.0, 50.0, 100.0];
        let counts = vec![2, 5, 3];
        assert_eq!(
            MetricsAggregator::<DummyStore>::window_percentile(&buckets, &counts, 10, 50.0),
            50.0
        );
        assert_eq!(
            MetricsAggregator::<DummyStore>::window_percentile(&buckets, &counts, 10, 90.0),
            100.0
        );
        assert_eq!(
            MetricsAggregator::<DummyStore>::window_percentile(&buckets, &counts, 10, 99.0),
            100.0
        );
    }

    #[test]
    fn test_window_percentile_single_bucket() {
        let buckets = vec![100.0];
        let counts = vec![7];
        assert_eq!(
            MetricsAggregator::<DummyStore>::window_percentile(&buckets, &counts, 7, 50.0),
            100.0
        );
    }

    struct DummyStore;

    #[derive(Debug)]
    struct DummyError(String);
    impl std::fmt::Display for DummyError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.0)
        }
    }
    impl std::error::Error for DummyError {}

    impl SqliteStore for DummyStore {
        type Error = DummyError;

        fn execute_write(
            &self,
            _sql: &str,
            _params: &[&dyn rusqlite::ToSql],
        ) -> Result<usize, DummyError> {
            Ok(0)
        }

        fn query_rows(
            &self,
            _sql: &str,
            _params: &[&dyn rusqlite::ToSql],
            _f: &mut dyn FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<AggregatedMetric>,
        ) -> Result<Vec<AggregatedMetric>, DummyError> {
            Ok(vec![])
        }
    }
}
