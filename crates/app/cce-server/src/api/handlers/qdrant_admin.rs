//! Qdrant process lifecycle admin handlers
//!
//! Provides API endpoints for manual Qdrant process control:
//! - Query the current process status
//! - Start / Stop / Restart the managed Qdrant subprocess
//!
//! These endpoints are only available when Qdrant is configured with
//! `auto_start = true` and the `QdrantProcessHandle` is present in AppState.

use axum::extract::State;

use crate::api::response::ApiResult;
use crate::api::state::AppState;
use cce_api::models::{
    ErrorResponse, QdrantActionResponse, QdrantProcessStatus, QdrantProcessStatusResponse,
    error_codes,
};

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// GET /api/qdrant/process/status
///
/// Return the current lifecycle status of the managed Qdrant subprocess.
#[utoipa::path(
    get, path = "/api/qdrant/process/status", tag = "Qdrant",
    responses(
        (status = 200, body = QdrantProcessStatusResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_qdrant_process_status(
    State(state): State<AppState>,
) -> ApiResult<QdrantProcessStatusResponse> {
    let Some(handle) = state.qdrant_control.as_ref() else {
        return ApiResult::Error(no_control_handle());
    };

    let status = handle.current_status().await;

    ApiResult::Success(QdrantProcessStatusResponse {
        managed: handle.managed,
        status,
    })
}

/// POST /api/qdrant/process/start
///
/// Start the Qdrant subprocess. This is idempotent — if the process
/// is already running the request is silently accepted.
#[utoipa::path(
    post, path = "/api/qdrant/process/start", tag = "Qdrant",
    responses(
        (status = 200, body = QdrantActionResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_qdrant_process_start(
    State(state): State<AppState>,
) -> ApiResult<QdrantActionResponse> {
    let Some(handle) = state.qdrant_control.as_ref() else {
        return ApiResult::Error(no_control_handle());
    };

    let current = handle.current_status().await;
    if current == QdrantProcessStatus::Running {
        return ApiResult::Success(QdrantActionResponse {
            success: true,
            message: "Qdrant is already running".into(),
            status: current,
        });
    }

    handle.start();

    // Give the background task a moment to transition the status
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let status = handle.current_status().await;

    ApiResult::Success(QdrantActionResponse {
        success: true,
        message: "Qdrant start command dispatched".into(),
        status,
    })
}

/// POST /api/qdrant/process/stop
///
/// Gracefully stop the Qdrant subprocess. Idempotent — if already
/// stopped the request is silently accepted.
#[utoipa::path(
    post, path = "/api/qdrant/process/stop", tag = "Qdrant",
    responses(
        (status = 200, body = QdrantActionResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_qdrant_process_stop(
    State(state): State<AppState>,
) -> ApiResult<QdrantActionResponse> {
    let Some(handle) = state.qdrant_control.as_ref() else {
        return ApiResult::Error(no_control_handle());
    };

    let current = handle.current_status().await;
    if current != QdrantProcessStatus::Running && current != QdrantProcessStatus::Starting {
        return ApiResult::Success(QdrantActionResponse {
            success: true,
            message: "Qdrant is not running".into(),
            status: current,
        });
    }

    handle.stop();

    // Give the background task a moment to transition the status
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let status = handle.current_status().await;

    ApiResult::Success(QdrantActionResponse {
        success: true,
        message: "Qdrant stop command dispatched".into(),
        status,
    })
}

/// POST /api/qdrant/process/restart
///
/// Restart the Qdrant subprocess (stop + start). Safe to call regardless
/// of current state — if the process isn't running, this will start it.
#[utoipa::path(
    post, path = "/api/qdrant/process/restart", tag = "Qdrant",
    responses(
        (status = 200, body = QdrantActionResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_qdrant_process_restart(
    State(state): State<AppState>,
) -> ApiResult<QdrantActionResponse> {
    let Some(handle) = state.qdrant_control.as_ref() else {
        return ApiResult::Error(no_control_handle());
    };

    handle.restart();

    // Give the background task a moment to transition the status
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let status = handle.current_status().await;

    ApiResult::Success(QdrantActionResponse {
        success: true,
        message: "Qdrant restart command dispatched".into(),
        status,
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn no_control_handle() -> ErrorResponse {
    ErrorResponse::new(
        error_codes::NOT_IMPLEMENTED,
        "Qdrant subprocess management is not enabled. Set [database.qdrant] auto_start = true in config.toml",
    )
}
