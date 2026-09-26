//! Metrics handlers
//!
//! Provides endpoints for monitoring system performance and health.
//! This module aggregates metrics from various subsystems (Project Registry, Query Cache, etc.).

use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use cce_api::models::{
    AggregatedMetric as ApiAggregatedMetric, ErrorResponse, MetricsCleanupQuery,
    MetricsCleanupResponse, MetricsHistoryQuery, error_codes,
};
use chrono::DateTime;

use crate::api::response::ApiResult;

/// Handle get metrics request
///
/// This endpoint exports all registered metrics in Prometheus exposition format.
/// Returns plain text format compatible with Prometheus scraper.
///
/// Rendering is served from the single-core render cache (started by the
/// engine) so concurrent scrapes never trigger parallel registry traversals.
///
/// For JSON format, use `/api/metrics/json` instead.
#[utoipa::path(
    get, path = "/api/metrics", tag = "Metrics",
    responses(
        (status = 200, content_type = "text/plain", body = String, description = "Prometheus exposition format")
    )
)]
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
    let exporter_manager = cce_metrics::ExporterManager::new();

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
#[utoipa::path(
    get, path = "/api/metrics/json", tag = "Metrics",
    responses(
        (status = 200, description = "Metrics snapshot as untyped JSON"), (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
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

/// Handle get metrics history request
///
/// This endpoint queries aggregated historical metrics from SQLite.
/// Returns time-series data with statistics (count, avg, median, max, p90, p99).
#[utoipa::path(
    get, path = "/api/metrics/history", tag = "Metrics",
    params(MetricsHistoryQuery),
    responses(
        (status = 200, body = Vec<ApiAggregatedMetric>, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_get_metrics_history(
    State(state): State<crate::api::state::AppState>,
    axum::extract::Query(params): axum::extract::Query<MetricsHistoryQuery>,
) -> ApiResult<Vec<ApiAggregatedMetric>> {
    // Parse timestamps
    let from = match DateTime::parse_from_rfc3339(&params.from) {
        Ok(dt) => dt.with_timezone(&chrono::Utc),
        Err(e) => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::INVALID_INPUT,
                format!("Invalid 'from' timestamp: {}", e),
            ));
        }
    };

    let to = match DateTime::parse_from_rfc3339(&params.to) {
        Ok(dt) => dt.with_timezone(&chrono::Utc),
        Err(e) => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::INVALID_INPUT,
                format!("Invalid 'to' timestamp: {}", e),
            ));
        }
    };

    // Get aggregator from engine
    let aggregator = match state.engine.metrics_aggregator() {
        Some(aggregator) => aggregator,
        None => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::SERVICE_UNAVAILABLE,
                "Metrics aggregation is not enabled",
            ));
        }
    };

    // Query history
    let records = match aggregator
        .query_history(
            from,
            to,
            params.metric.as_deref(),
            params.project_id,
            params.operation_type.as_deref(),
        )
        .await
    {
        Ok(records) => records,
        Err(e) => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                format!("Failed to query metrics history: {}", e),
            ));
        }
    };

    let metrics = records
        .into_iter()
        .map(|record| ApiAggregatedMetric {
            timestamp: record.timestamp.to_rfc3339(),
            metric_name: record.metric_name,
            metric_type: record.metric_type,
            labels_json: record.labels_json,
            count: record.count,
            avg: record.avg,
            median: record.median,
            max: record.max,
            p90: record.p90,
            p99: record.p99,
            project_id: record.project_id,
            operation_type: record.operation_type,
        })
        .collect();

    ApiResult::Success(metrics)
}

/// Handle metrics cleanup request
///
/// This endpoint deletes historical aggregated metrics from SQLite.
/// Supports two modes:
/// - Full cleanup: `?all=true`
/// - Time-based cleanup: `?before=2024-01-01T00:00:00Z`
#[utoipa::path(
    delete, path = "/api/metrics/cleanup", tag = "Metrics",
    params(MetricsCleanupQuery),
    responses(
        (status = 200, body = MetricsCleanupResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_cleanup_metrics(
    State(state): State<crate::api::state::AppState>,
    axum::extract::Query(params): axum::extract::Query<MetricsCleanupQuery>,
) -> ApiResult<MetricsCleanupResponse> {
    // Get aggregator from engine
    let aggregator = match state.engine.metrics_aggregator() {
        Some(aggregator) => aggregator,
        None => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::SERVICE_UNAVAILABLE,
                "Metrics aggregation is not enabled",
            ));
        }
    };

    // Parse 'before' timestamp if provided
    let before = if let Some(before_str) = &params.before {
        match DateTime::parse_from_rfc3339(before_str) {
            Ok(dt) => Some(dt.with_timezone(&chrono::Utc)),
            Err(e) => {
                return ApiResult::Error(ErrorResponse::new(
                    error_codes::INVALID_INPUT,
                    format!("Invalid 'before' timestamp: {}", e),
                ));
            }
        }
    } else {
        None
    };

    // Validate parameters
    if !params.all && before.is_none() {
        return ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "Either 'all=true' or 'before=<timestamp>' must be specified",
        ));
    }

    // Execute cleanup
    let deleted_count = match aggregator.cleanup(before, params.all).await {
        Ok(count) => count,
        Err(e) => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                format!("Failed to cleanup metrics: {}", e),
            ));
        }
    };

    ApiResult::Success(MetricsCleanupResponse {
        success: true,
        deleted_count,
    })
}
