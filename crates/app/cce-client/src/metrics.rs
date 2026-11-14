//! Metrics export, history and retention cleanup.

use cce_api::models::{
    AggregatedMetric, MetricsCleanupQuery, MetricsCleanupResponse, MetricsHistoryQuery,
};
use cce_api::paths;

use crate::error::ClientError;
use crate::transport::{Client, INTERACTIVE_BUDGET};

impl Client {
    /// Metrics snapshot as Prometheus text.
    pub async fn metrics_prometheus(&self) -> Result<String, ClientError> {
        self.get_text(paths::METRICS.to_string(), INTERACTIVE_BUDGET)
            .await
    }

    /// Metrics snapshot as free-form JSON.
    ///
    /// The payload mirrors the registry and is not part of the typed contract,
    /// so it stays untyped instead of inventing a shape the server does not
    /// promise.
    pub async fn metrics_json(&self) -> Result<serde_json::Value, ClientError> {
        self.get_json(paths::METRICS_JSON.to_string(), INTERACTIVE_BUDGET)
            .await
    }

    /// Aggregated historical records for a time window.
    pub async fn metrics_history(
        &self,
        query: &MetricsHistoryQuery,
    ) -> Result<Vec<AggregatedMetric>, ClientError> {
        self.get_json_query(
            paths::METRICS_HISTORY.to_string(),
            query,
            INTERACTIVE_BUDGET,
        )
        .await
    }

    /// Delete historical records by age or wholesale.
    pub async fn cleanup_metrics(
        &self,
        query: &MetricsCleanupQuery,
    ) -> Result<MetricsCleanupResponse, ClientError> {
        self.delete_json_with_query(
            paths::METRICS_CLEANUP.to_string(),
            query,
            self.long_task_budget(),
        )
        .await
    }
}
