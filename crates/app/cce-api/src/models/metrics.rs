//! Metrics models

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Aggregated historical metric record (mirrors cce-metrics storage rows)
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AggregatedMetric {
    /// Aggregation window end (RFC 3339)
    pub timestamp: String,
    pub metric_name: String,
    /// Metric kind ("counter", "gauge", "histogram")
    pub metric_type: String,
    #[serde(default)]
    pub labels_json: Option<String>,
    pub count: i64,
    #[serde(default)]
    pub avg: Option<f64>,
    #[serde(default)]
    pub median: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
    #[serde(default)]
    pub p90: Option<f64>,
    #[serde(default)]
    pub p99: Option<f64>,
    #[serde(default)]
    pub project_id: Option<i64>,
    #[serde(default)]
    pub operation_type: Option<String>,
}

/// Query parameters for metrics history
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct MetricsHistoryQuery {
    /// Start time (ISO 8601 format)
    pub from: String,
    /// End time (ISO 8601 format)
    pub to: String,
    /// Optional metric name filter
    pub metric: Option<String>,
    /// Optional project ID filter
    pub project_id: Option<i64>,
    /// Optional operation type filter (e.g., "index", "query", "embed")
    pub operation_type: Option<String>,
}

/// Query parameters for metrics cleanup
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct MetricsCleanupQuery {
    /// Delete all records if true
    #[serde(default)]
    pub all: bool,
    /// Delete records before this timestamp (ISO 8601 format)
    pub before: Option<String>,
}

/// Metrics cleanup response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct MetricsCleanupResponse {
    pub success: bool,
    pub deleted_count: usize,
}
