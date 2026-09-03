//! Metrics models

use serde::{Deserialize, Serialize};

/// Metrics history response
#[derive(Debug, Serialize, Deserialize)]
pub struct MetricsHistoryResponse {
    pub timestamp: String,
    pub metric_name: String,
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
