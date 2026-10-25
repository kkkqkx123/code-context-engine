//! Watch (hot reload) handlers
//!
//! This module provides handlers for file watching including:
//! - Start/stop watching
//! - Watch status

use axum::{
    Json,
    extract::{Path, State},
};

use cce_api::models::{
    ErrorResponse, StartWatchRequest, StartWatchResponse, StopWatchResponse, WatchStatusResponse,
    error_codes,
};

use crate::api::response::ApiResult;

/// Handle start watch request
///
/// Starts file watching for the specified directory.
#[utoipa::path(
    post, path = "/api/project/{project_id}/watch/start", tag = "Watch",
    params(("project_id" = i64, Path, description = "Project id")),
    request_body = StartWatchRequest,
    responses(
        (status = 200, body = StartWatchResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_start_watch(
    State(state): State<crate::api::state::AppState>,
    Path(project_id): Path<i64>,
    Json(request): Json<StartWatchRequest>,
) -> ApiResult<StartWatchResponse> {
    if project_id <= 0 {
        return ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "Invalid project_id",
        ));
    }

    match cce_server_shared::watch_start(&state, project_id, &request).await {
        Ok(response) => ApiResult::Success(response),
        Err(err) => match err {
            cce_server_shared::WatchStartError::InvalidProjectId => {
                ApiResult::Error(ErrorResponse::new(error_codes::INVALID_REQUEST, "Invalid project_id"))
            }
            cce_server_shared::WatchStartError::GatewaySupplied => ApiResult::Error(
                ErrorResponse::with_details(
                    error_codes::INVALID_REQUEST,
                    "Project is gateway-supplied; local watching is disabled",
                    format!(
                        "Project {project_id} receives changes through POST /api/project/{project_id}/ingest/event; use the gateway watch loop instead of local watching"
                    ),
                ),
            ),
            cce_server_shared::WatchStartError::ProjectLoad(m) => ApiResult::Error(
                ErrorResponse::with_details(error_codes::ENTITY_NOT_FOUND, "Failed to load project", m),
            ),
            cce_server_shared::WatchStartError::RootNotAccessible(m) => ApiResult::Error(
                ErrorResponse::with_details(
                    error_codes::INVALID_REQUEST,
                    "Project root is not accessible",
                    format!("Failed to canonicalize project root: {m}"),
                ),
            ),
            cce_server_shared::WatchStartError::WatchNotAccessible(m) => ApiResult::Error(
                ErrorResponse::with_details(
                    error_codes::INVALID_REQUEST,
                    "Watch path is not accessible",
                    format!("Failed to canonicalize watch path: {m}"),
                ),
            ),
            cce_server_shared::WatchStartError::NotWithinRoot { watch, root } => {
                ApiResult::Error(ErrorResponse::with_details(
                    error_codes::INVALID_REQUEST,
                    "Watch path is not within project root",
                    format!("Watch path '{}' is not within project root '{}'", watch, root),
                ))
            }
            cce_server_shared::WatchStartError::PathNotFound(path) => ApiResult::Error(
                ErrorResponse::with_details(error_codes::INVALID_REQUEST, "Path does not exist", path),
            ),
            cce_server_shared::WatchStartError::HotCoordinator(m) => ApiResult::Error(
                ErrorResponse::with_details(
                    error_codes::INTERNAL_ERROR,
                    "Failed to get hot update coordinator",
                    m,
                ),
            ),
            cce_server_shared::WatchStartError::StartWatch(m) => ApiResult::Error(
                ErrorResponse::with_details(error_codes::INTERNAL_ERROR, "Failed to start watching", m),
            ),
            cce_server_shared::WatchStartError::EventLoop(m) => ApiResult::Error(
                ErrorResponse::with_details(error_codes::INTERNAL_ERROR, "Failed to start event loop", m),
            ),
        },
    }
}

/// Handle stop watch request
///
/// Stops file watching and cleans up resources.
#[utoipa::path(
    post, path = "/api/project/{project_id}/watch/stop", tag = "Watch",
    params(("project_id" = i64, Path, description = "Project id")),
    responses(
        (status = 200, body = StopWatchResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_stop_watch(
    State(state): State<crate::api::state::AppState>,
    Path(project_id): Path<i64>,
) -> ApiResult<StopWatchResponse> {
    if project_id <= 0 {
        return ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "Invalid project_id",
        ));
    }
    match cce_server_shared::watch_stop(&state, project_id).await {
        Ok(response) => ApiResult::Success(response),
        Err(err) => match err {
            cce_server_shared::WatchStopError::InvalidProjectId => {
                ApiResult::Error(ErrorResponse::new(error_codes::INVALID_REQUEST, "Invalid project_id"))
            }
            cce_server_shared::WatchStopError::HotCoordinator(m) => ApiResult::Error(
                ErrorResponse::with_details(
                    error_codes::INTERNAL_ERROR,
                    "Failed to get hot update coordinator",
                    m,
                ),
            ),
            cce_server_shared::WatchStopError::StopWatch(m) => ApiResult::Error(
                ErrorResponse::with_details(error_codes::INTERNAL_ERROR, "Failed to stop watching", m),
            ),
        },
    }
}

/// Handle watch status request
///
/// Returns the current status of file watching.
#[utoipa::path(
    get, path = "/api/project/{project_id}/watch/status", tag = "Watch",
    params(("project_id" = i64, Path, description = "Project id")),
    responses(
        (status = 200, body = WatchStatusResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_watch_status(
    State(state): State<crate::api::state::AppState>,
    Path(project_id): Path<i64>,
) -> ApiResult<WatchStatusResponse> {
    if project_id <= 0 {
        return ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "Invalid project_id",
        ));
    }
    match cce_server_shared::watch_status(&state, project_id).await {
        Ok(response) => ApiResult::Success(response),
        Err(cce_server_shared::WatchStatusError::InvalidProjectId) => {
            ApiResult::Error(ErrorResponse::new(error_codes::INVALID_REQUEST, "Invalid project_id"))
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_watch_path_outside_project_rejected() {
        let tmp = tempfile::tempdir().expect("Failed to create temp dir");
        let project_a_root = tmp.path().join("project_a");
        let project_b_root = tmp.path().join("project_b");
        std::fs::create_dir_all(&project_a_root).expect("create project_a");
        std::fs::create_dir_all(&project_b_root).expect("create project_b");

        let canonical_root = project_a_root.canonicalize().unwrap();
        let good_path = project_a_root.join("src");
        std::fs::create_dir_all(&good_path).unwrap();
        let canonical_good = good_path.canonicalize().unwrap();

        assert!(
            canonical_good.starts_with(&canonical_root),
            "Path inside project root should be accepted"
        );

        let canonical_bad = project_b_root.canonicalize().unwrap();

        assert!(
            !canonical_bad.starts_with(&canonical_root),
            "Path outside project root should be rejected"
        );
    }
}
