//! Qdrant process lifecycle admin handlers
//!
//! Provides API endpoints for manual Qdrant process control:
//! - Query the current process status
//! - Start / Stop / Restart the managed Qdrant subprocess
//!
//! These endpoints are only available when Qdrant is configured with
//! `auto_start = true` and the `QdrantProcessHandle` is present in AppState.

use axum::{Json, extract::State, http::StatusCode};

use crate::api::state::AppState;
use cce_api::models::{QdrantActionResponse, QdrantProcessStatus, QdrantProcessStatusResponse};

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// GET /api/qdrant/process/status
///
/// Return the current lifecycle status of the managed Qdrant subprocess.
pub async fn handle_qdrant_process_status(
    State(state): State<AppState>,
) -> Result<Json<QdrantProcessStatusResponse>, (StatusCode, Json<serde_json::Value>)> {
    let handle = state
        .qdrant_control
        .as_ref()
        .ok_or_else(no_control_handle)?;

    let status = handle.current_status().await;

    Ok(Json(QdrantProcessStatusResponse {
        managed: handle.managed,
        status,
    }))
}

/// POST /api/qdrant/process/start
///
/// Start the Qdrant subprocess. This is idempotent — if the process
/// is already running the request is silently accepted.
pub async fn handle_qdrant_process_start(
    State(state): State<AppState>,
) -> Result<Json<QdrantActionResponse>, (StatusCode, Json<serde_json::Value>)> {
    let handle = state
        .qdrant_control
        .as_ref()
        .ok_or_else(no_control_handle)?;

    let current = handle.current_status().await;
    if current == QdrantProcessStatus::Running {
        return Ok(Json(QdrantActionResponse {
            success: true,
            message: "Qdrant is already running".into(),
            status: current,
        }));
    }

    handle.start();

    // Give the background task a moment to transition the status
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let status = handle.current_status().await;

    Ok(Json(QdrantActionResponse {
        success: true,
        message: "Qdrant start command dispatched".into(),
        status,
    }))
}

/// POST /api/qdrant/process/stop
///
/// Gracefully stop the Qdrant subprocess. Idempotent — if already
/// stopped the request is silently accepted.
pub async fn handle_qdrant_process_stop(
    State(state): State<AppState>,
) -> Result<Json<QdrantActionResponse>, (StatusCode, Json<serde_json::Value>)> {
    let handle = state
        .qdrant_control
        .as_ref()
        .ok_or_else(no_control_handle)?;

    let current = handle.current_status().await;
    if current != QdrantProcessStatus::Running && current != QdrantProcessStatus::Starting {
        return Ok(Json(QdrantActionResponse {
            success: true,
            message: "Qdrant is not running".into(),
            status: current,
        }));
    }

    handle.stop();

    // Give the background task a moment to transition the status
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let status = handle.current_status().await;

    Ok(Json(QdrantActionResponse {
        success: true,
        message: "Qdrant stop command dispatched".into(),
        status,
    }))
}

/// POST /api/qdrant/process/restart
///
/// Restart the Qdrant subprocess (stop + start). Safe to call regardless
/// of current state — if the process isn't running, this will start it.
pub async fn handle_qdrant_process_restart(
    State(state): State<AppState>,
) -> Result<Json<QdrantActionResponse>, (StatusCode, Json<serde_json::Value>)> {
    let handle = state
        .qdrant_control
        .as_ref()
        .ok_or_else(no_control_handle)?;

    handle.restart();

    // Give the background task a moment to transition the status
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let status = handle.current_status().await;

    Ok(Json(QdrantActionResponse {
        success: true,
        message: "Qdrant restart command dispatched".into(),
        status,
    }))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn no_control_handle() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(serde_json::json!({
            "error": "Qdrant subprocess management is not enabled. Set [database.qdrant] auto_start = true in config.toml"
        })),
    )
}
