//! Watch (hot reload) handlers
//!
//! This module provides handlers for file watching including:
//! - Start/stop watching
//! - Watch status

use axum::{
    Json,
    extract::{Path, State},
};
use std::path::PathBuf;

use cce_api::models::{
    ErrorResponse, StartWatchRequest, StartWatchResponse, StopWatchResponse, WatchStatus,
    WatchStatusResponse, error_codes,
};

use crate::api::response::ApiResult;
use cce_orchestrator::hot_update::HotUpdateCoordinator;
use cce_orchestrator::hot_update::watcher::WatchStatusTracker;

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
    // Validate project_id
    if project_id <= 0 {
        return ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "Invalid project_id",
        ));
    }

    // Verify the watched path is within the project root directory
    let project_entry = match state
        .engine
        .project_registry()
        .get_or_load(project_id)
        .await
    {
        Ok(entry) => entry,
        Err(e) => {
            return ApiResult::Error(ErrorResponse::with_details(
                error_codes::ENTITY_NOT_FOUND,
                "Failed to load project",
                e.to_string(),
            ));
        }
    };
    let project_root = PathBuf::from(&project_entry.metadata.root_path);
    let watch_path = PathBuf::from(&request.path);
    // Canonicalize both sides so symlinked roots do not produce false
    // negatives for the containment check. A failed canonicalization means
    // the path is missing or unreadable, so fail closed instead of falling
    // back to the raw path and risking a wrong containment decision.
    let canonical_root = match project_root.canonicalize() {
        Ok(root) => root,
        Err(error) => {
            return ApiResult::Error(ErrorResponse::with_details(
                error_codes::INVALID_REQUEST,
                "Project root is not accessible",
                format!("Failed to canonicalize project root: {error}"),
            ));
        }
    };
    let canonical_watch = match watch_path.canonicalize() {
        Ok(path) => path,
        Err(error) => {
            return ApiResult::Error(ErrorResponse::with_details(
                error_codes::INVALID_REQUEST,
                "Watch path is not accessible",
                format!("Failed to canonicalize watch path: {error}"),
            ));
        }
    };
    if !canonical_watch.starts_with(&canonical_root) {
        return ApiResult::Error(ErrorResponse::with_details(
            error_codes::INVALID_REQUEST,
            "Watch path is not within project root",
            format!(
                "Watch path '{}' is not within project root '{}'",
                canonical_watch.display(),
                project_root.display()
            ),
        ));
    }

    // Get hot update coordinator for this project
    let hot_update = match state.engine.get_hot_update_coordinator(project_id).await {
        Ok(coord) => coord,
        Err(e) => {
            return ApiResult::Error(ErrorResponse::with_details(
                error_codes::INTERNAL_ERROR,
                "Failed to get hot update coordinator",
                e.to_string(),
            ));
        }
    };

    // Get lock on coordinator
    let mut coordinator = hot_update.lock().await;

    // Check if path exists
    if !canonical_watch.exists() {
        return ApiResult::Error(ErrorResponse::with_details(
            error_codes::INVALID_REQUEST,
            "Path does not exist",
            request.path.clone(),
        ));
    }

    // Start watching
    match coordinator.start_watch(&canonical_watch).await {
        Ok(()) => {
            // Start event processing loop
            match coordinator.start_event_loop().await {
                Ok(_handle) => {
                    // Start the background processor that consumes accumulated
                    // watch events and runs the stored processors. It must be
                    // spawned after releasing the coordinator lock (the
                    // background worker takes the notify handle under a
                    // blocking lock). Idempotent: repeated start-watch calls
                    // do not spawn competing workers.
                    drop(coordinator);
                    HotUpdateCoordinator::start_background_processor_from_arc(hot_update.clone())
                        .await;

                    // Update per-project watch status
                    let mut status_map = state.watch_status.write().await;
                    let tracker = status_map
                        .entry(project_id)
                        .or_insert_with(WatchStatusTracker::new);
                    tracker.start(&canonical_watch);
                    drop(status_map);

                    return ApiResult::Success(StartWatchResponse {
                        success: true,
                        message: "File watching started".to_string(),
                        project_id,
                        path: canonical_watch.to_string_lossy().into_owned(),
                        extensions: request.extensions,
                        debounce_ms: request.debounce_ms,
                    });
                }
                Err(e) => {
                    return ApiResult::Error(ErrorResponse::with_details(
                        error_codes::INTERNAL_ERROR,
                        "Failed to start event loop",
                        e.to_string(),
                    ));
                }
            }
        }
        Err(e) => ApiResult::Error(ErrorResponse::with_details(
            error_codes::INTERNAL_ERROR,
            "Failed to start watching",
            e.to_string(),
        )),
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
    // Validate project_id
    if project_id <= 0 {
        return ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "Invalid project_id",
        ));
    }

    // Get hot update coordinator for this project
    let hot_update = match state.engine.get_hot_update_coordinator(project_id).await {
        Ok(coord) => coord,
        Err(e) => {
            return ApiResult::Error(ErrorResponse::with_details(
                error_codes::INTERNAL_ERROR,
                "Failed to get hot update coordinator",
                e.to_string(),
            ));
        }
    };

    // Get lock on coordinator
    let mut coordinator = hot_update.lock().await;

    // Stop watching
    match coordinator.stop_watch().await {
        Ok(()) => {
            // Update per-project status
            let mut status_map = state.watch_status.write().await;
            if let Some(tracker) = status_map.get_mut(&project_id) {
                tracker.stop();
            }

            ApiResult::Success(StopWatchResponse {
                success: true,
                message: "File watching stopped".to_string(),
                project_id,
            })
        }
        Err(e) => ApiResult::Error(ErrorResponse::with_details(
            error_codes::INTERNAL_ERROR,
            "Failed to stop watching",
            e.to_string(),
        )),
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
    // Validate project_id
    if project_id <= 0 {
        return ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "Invalid project_id",
        ));
    }

    let status_map = state.watch_status.read().await;
    let tracker = status_map.get(&project_id).cloned().unwrap_or_default();
    drop(status_map);

    let events_processed =
        if let Ok(hot_update) = state.engine.get_hot_update_coordinator(project_id).await {
            let coordinator = hot_update.lock().await;
            coordinator.total_events()
        } else {
            tracker.events_processed as usize
        };

    let watch_status = WatchStatus {
        active: tracker.active,
        watched_dirs: tracker.watched_dirs.clone(),
        events_processed,
        started_at: tracker.started_at.map(|t| t.to_rfc3339()),
    };

    let response = WatchStatusResponse {
        success: true,
        status: watch_status,
    };

    ApiResult::Success(response)
}

#[cfg(test)]
mod tests {
    /// Watch path outside project root is rejected
    ///
    /// Verifies that the path validation logic used in handle_start_watch
    /// correctly rejects paths that are not within the project root directory.
    #[test]
    fn test_watch_path_outside_project_rejected() {
        let tmp = tempfile::tempdir().expect("Failed to create temp dir");
        let project_a_root = tmp.path().join("project_a");
        let project_b_root = tmp.path().join("project_b");
        std::fs::create_dir_all(&project_a_root).expect("create project_a");
        std::fs::create_dir_all(&project_b_root).expect("create project_b");

        // Canonicalize project root and a valid subdirectory
        let canonical_root = project_a_root.canonicalize().unwrap();
        let good_path = project_a_root.join("src");
        std::fs::create_dir_all(&good_path).unwrap();
        let canonical_good = good_path.canonicalize().unwrap();

        // Path within project_a should pass the starts_with check
        assert!(
            canonical_good.starts_with(&canonical_root),
            "Path inside project root should be accepted"
        );

        // Canonicalize project_b (outside project_a's root)
        let canonical_bad = project_b_root.canonicalize().unwrap();

        // Path in project_b should NOT pass the starts_with check for project_a
        assert!(
            !canonical_bad.starts_with(&canonical_root),
            "Path outside project root should be rejected"
        );
    }
}
