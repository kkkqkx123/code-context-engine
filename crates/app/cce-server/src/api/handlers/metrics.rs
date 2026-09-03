//! Metrics handlers
//!
//! Provides endpoints for monitoring system performance and health.
//! This module aggregates metrics from various subsystems (Project Registry, Query Cache, etc.).

use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use cce_metrics_infra::AggregatedMetric;
use chrono::DateTime;
use serde::Deserialize;

/// Handle get metrics request
///
/// This endpoint exports all registered metrics in Prometheus exposition format.
/// Returns plain text format compatible with Prometheus scraper.
///
/// Rendering is served from the single-core render cache (started by the
/// engine) so concurrent scrapes never trigger parallel registry traversals.
///
/// For JSON format, use `/api/metrics/json` instead.
pub async fn handle_get_metrics(
    State(state): State<crate::api::state::AppState>,
) -> impl IntoResponse {
    // Serve from the single-core render cache when available
    if let Some(cache) = state.engine.render_cache().await {
        let prometheus_text = cache.prometheus().await;
        return (
            StatusCode::OK,
            [("Content-Type", "text/plain; version=0.0.4; charset=utf-8")],
            prometheus_text,
        );
    }

    // Fallback: render on demand (cache not started)
    let registry = state.engine.metrics_registry();

    // Export to Prometheus format using ExporterManager
    let exporter_manager = cce_metrics_infra::ExporterManager::new();

    match exporter_manager.export("prometheus", registry).await {
        Ok(prometheus_text) => (
            StatusCode::OK,
            [("Content-Type", "text/plain; version=0.0.4; charset=utf-8")],
            prometheus_text,
        ),
        Err(e) => {
            tracing::error!(error = %e, "Failed to export Prometheus metrics");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                [("Content-Type", "text/plain")],
                format!("# Error exporting metrics: {}\n", e),
            )
        }
    }
}

/// Handle get metrics in JSON format request
///
/// This endpoint exports all registered metrics (counters, gauges, histograms)
/// in a structured JSON format suitable for external monitoring systems.
pub async fn handle_get_metrics_json(
    State(state): State<crate::api::state::AppState>,
) -> impl IntoResponse {
    // Serve from the single-core render cache when available
    if let Some(cache) = state.engine.render_cache().await {
        let json_text = cache.json().await;
        let snapshot_value: serde_json::Value = match serde_json::from_str(&json_text) {
            Ok(value) => value,
            Err(e) => {
                tracing::error!(error = %e, "Failed to parse cached metrics snapshot");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({
                        "error": format!("Failed to parse cached metrics snapshot: {}", e)
                    })),
                );
            }
        };
        return (StatusCode::OK, Json(snapshot_value));
    }

    // Fallback: export on demand (cache not started)
    let registry = state.engine.metrics_registry();

    // Export all metrics as snapshot
    let snapshot = registry.export_all();

    let snapshot_value = match serde_json::to_value(snapshot) {
        Ok(value) => value,
        Err(e) => {
            tracing::error!(error = %e, "Failed to serialize metrics snapshot");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("Failed to serialize metrics snapshot: {}", e)
                })),
            );
        }
    };

    (StatusCode::OK, Json(snapshot_value))
}

/// Query parameters for metrics history
#[derive(Debug, Deserialize)]
pub struct HistoryQueryParams {
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

/// Handle get metrics history request
///
/// This endpoint queries aggregated historical metrics from SQLite.
/// Returns time-series data with statistics (count, avg, median, max, p90, p99).
pub async fn handle_get_metrics_history(
    State(state): State<crate::api::state::AppState>,
    axum::extract::Query(params): axum::extract::Query<HistoryQueryParams>,
) -> Result<Json<Vec<AggregatedMetric>>, (StatusCode, Json<serde_json::Value>)> {
    // Parse timestamps
    let from = DateTime::parse_from_rfc3339(&params.from)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("Invalid 'from' timestamp: {}", e)
                })),
            )
        })?;

    let to = DateTime::parse_from_rfc3339(&params.to)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("Invalid 'to' timestamp: {}", e)
                })),
            )
        })?;

    // Get aggregator from engine
    let aggregator = state.engine.metrics_aggregator().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        Json(serde_json::json!({
            "error": "Metrics aggregation is not enabled"
        })),
    ))?;

    // Query history
    let records = aggregator
        .query_history(
            from,
            to,
            params.metric.as_deref(),
            params.project_id,
            params.operation_type.as_deref(),
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("Failed to query metrics history: {}", e)
                })),
            )
        })?;

    Ok(Json(records))
}

/// Query parameters for metrics cleanup
#[derive(Debug, Deserialize)]
pub struct CleanupQueryParams {
    /// Delete all records if true
    #[serde(default)]
    pub all: bool,
    /// Delete records before this timestamp (ISO 8601 format)
    pub before: Option<String>,
}

/// Handle metrics cleanup request
///
/// This endpoint deletes historical aggregated metrics from SQLite.
/// Supports two modes:
/// - Full cleanup: `?all=true`
/// - Time-based cleanup: `?before=2024-01-01T00:00:00Z`
pub async fn handle_cleanup_metrics(
    State(state): State<crate::api::state::AppState>,
    axum::extract::Query(params): axum::extract::Query<CleanupQueryParams>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    // Get aggregator from engine
    let aggregator = state.engine.metrics_aggregator().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        Json(serde_json::json!({
            "error": "Metrics aggregation is not enabled"
        })),
    ))?;

    // Parse 'before' timestamp if provided
    let before = if let Some(before_str) = &params.before {
        Some(
            DateTime::parse_from_rfc3339(before_str)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .map_err(|e| {
                    (
                        StatusCode::BAD_REQUEST,
                        Json(serde_json::json!({
                            "error": format!("Invalid 'before' timestamp: {}", e)
                        })),
                    )
                })?,
        )
    } else {
        None
    };

    // Validate parameters
    if !params.all && before.is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "Either 'all=true' or 'before=<timestamp>' must be specified"
            })),
        ));
    }

    // Execute cleanup
    let deleted_count = aggregator.cleanup(before, params.all).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("Failed to cleanup metrics: {}", e)
            })),
        )
    })?;

    Ok(Json(serde_json::json!({
        "success": true,
        "deleted_count": deleted_count
    })))
}
