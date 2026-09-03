//! Watch (hot reload) handlers
//!
//! This module provides handlers for file watching including:
//! - Start/stop watching
//! - Watch status

use axum::{Json, extract::Path, extract::State, http::StatusCode, response::IntoResponse};
use serde_json::json;
use std::path::PathBuf;

use cce_api::models::{
    ErrorResponse, StartWatchRequest, WatchStatus, WatchStatusResponse, error_codes,
};
use cce_orchestrator::hot_update::HotUpdateCoordinator;
use cce_orchestrator::hot_update::watcher::WatchStatusTracker;

/// Handle start watch request
///
/// Starts file watching for the specified directory.
pub async fn handle_start_watch(
    State(state): State<crate::api::state::AppState>,
    Path(project_id): Path<i64>,
    Json(request): Json<StartWatchRequest>,
) -> impl IntoResponse {
    // Validate project_id
    if project_id <= 0 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!(ErrorResponse::new(
                error_codes::INVALID_REQUEST,
                "Invalid project_id"
            ))),
        );
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
            return (
                StatusCode::BAD_REQUEST,
                Json(json!(ErrorResponse::with_details(
                    error_codes::ENTITY_NOT_FOUND,
                    "Failed to load project",
                    e.to_string(),
                ))),
            );
        }
    };
    let project_root = PathBuf::from(&project_entry.metadata.root_path);
    let watch_path = PathBuf::from(&request.path);
    // Canonicalize both sides so symlinked roots do not produce false
    // negatives for the containment check.
    let canonical_root = project_root.canonicalize().unwrap_or(project_root.clone());
    let canonical_watch = watch_path.canonicalize().unwrap_or(watch_path.clone());
    if !canonical_watch.starts_with(&canonical_root) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!(ErrorResponse::with_details(
                error_codes::INVALID_REQUEST,
                "Watch path is not within project root",
                format!(
                    "Watch path '{}' is not within project root '{}'",
                    canonical_watch.display(),
                    project_root.display()
                ),
            ))),
        );
    }

    // Get hot update coordinator for this project
    let hot_update = match state.engine.get_hot_update_coordinator(project_id).await {
        Ok(coord) => coord,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!(ErrorResponse::with_details(
                    error_codes::INTERNAL_ERROR,
                    "Failed to get hot update coordinator",
                    e.to_string(),
                ))),
            );
        }
    };

    // Get lock on coordinator
    let mut coordinator = hot_update.lock().await;

    // Check if path exists
    if !canonical_watch.exists() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!(ErrorResponse::with_details(
                error_codes::INVALID_REQUEST,
                "Path does not exist",
                request.path.clone(),
            ))),
        );
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

                    (
                        StatusCode::OK,
                        Json(json!({
                            "success": true,
                            "message": "File watching started",
                            "project_id": project_id,
                            "path": canonical_watch.to_string_lossy(),
                            "extensions": request.extensions,
                            "debounce_ms": request.debounce_ms
                        })),
                    )
                }
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!(ErrorResponse::with_details(
                        error_codes::INTERNAL_ERROR,
                        "Failed to start event loop",
                        e.to_string(),
                    ))),
                ),
            }
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!(ErrorResponse::with_details(
                error_codes::INTERNAL_ERROR,
                "Failed to start watching",
                e.to_string(),
            ))),
        ),
    }
}

/// Handle stop watch request
///
/// Stops file watching and cleans up resources.
pub async fn handle_stop_watch(
    State(state): State<crate::api::state::AppState>,
    Path(project_id): Path<i64>,
) -> impl IntoResponse {
    // Validate project_id
    if project_id <= 0 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!(ErrorResponse::new(
                error_codes::INVALID_REQUEST,
                "Invalid project_id"
            ))),
        );
    }

    // Get hot update coordinator for this project
    let hot_update = match state.engine.get_hot_update_coordinator(project_id).await {
        Ok(coord) => coord,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!(ErrorResponse::with_details(
                    error_codes::INTERNAL_ERROR,
                    "Failed to get hot update coordinator",
                    e.to_string(),
                ))),
            );
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

            (
                StatusCode::OK,
                Json(json!({
                    "success": true,
                    "message": "File watching stopped",
                    "project_id": project_id
                })),
            )
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!(ErrorResponse::with_details(
                error_codes::INTERNAL_ERROR,
                "Failed to stop watching",
                e.to_string(),
            ))),
        ),
    }
}

/// Handle watch status request
///
/// Returns the current status of file watching.
pub async fn handle_watch_status(
    State(state): State<crate::api::state::AppState>,
    Path(project_id): Path<i64>,
) -> impl IntoResponse {
    // Validate project_id
    if project_id <= 0 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!(ErrorResponse::new(
                error_codes::INVALID_REQUEST,
                "Invalid project_id"
            ))),
        );
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

    (StatusCode::OK, Json(json!(response)))
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
