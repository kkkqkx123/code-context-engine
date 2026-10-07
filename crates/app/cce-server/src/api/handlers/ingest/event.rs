//! Incremental event entry for gateway-observed changes.

use std::path::PathBuf;

use axum::extract::{Path as AxumPath, State};
use base64::Engine as _;
use cce_api::models::{
    ErrorResponse, IngestEventKind, IngestEventRequest, IngestEventResponse,
    MAX_INGEST_BATCH_FILES, error_codes,
};

use super::paths::{project_root, safe_join};
use super::staging::stage_bytes;
use crate::api::response::ApiResult;
use crate::api::state::AppState;

/// Apply gateway-observed changes through the hot-update pipeline.
///
/// Created and modified events carry content and are staged before the
/// coordinator runs; deletions remove the mirrored file and its index rows.
/// The remote host never starts its own filesystem watch in this shape:
/// every change arrives through this entry. Events travel whole and raw,
/// so a change that would exceed the entry-count or single-file ingest
/// bounds is not split here: the gateway falls back to a full sync batch
/// instead.
#[utoipa::path(
    post, path = "/api/project/{id}/ingest/event", tag = "Ingest",
    params(("id" = i64, Path, description = "Project id")),
    request_body = IngestEventRequest,
    responses(
        (status = 200, body = IngestEventResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_ingest_event(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<i64>,
    axum::Json(request): axum::Json<IngestEventRequest>,
) -> ApiResult<IngestEventResponse> {
    if request.events.len() > MAX_INGEST_BATCH_FILES {
        return ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            format!(
                "event holds {} entries, more than the {MAX_INGEST_BATCH_FILES} entry bound",
                request.events.len()
            ),
        ));
    }
    let root = match project_root(&state, id) {
        Ok(root) => root,
        Err(error) => return ApiResult::Error(error),
    };
    let mut changes: Vec<(PathBuf, bool)> = Vec::with_capacity(request.events.len());
    let mut errors = Vec::new();
    let mut removed = 0usize;
    for event in &request.events {
        match event.kind {
            IngestEventKind::Created | IngestEventKind::Modified => {
                let Some(encoded) = event.content_base64.as_deref() else {
                    errors.push(format!(
                        "{}: content is required for created and modified events",
                        event.relative_path
                    ));
                    continue;
                };
                let bytes = match base64::engine::general_purpose::STANDARD.decode(encoded) {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        errors.push(format!(
                            "{}: content is not valid base64: {e}",
                            event.relative_path
                        ));
                        continue;
                    }
                };
                match stage_bytes(
                    &root,
                    &event.relative_path,
                    &bytes,
                    event.content_hash.as_deref(),
                )
                .await
                {
                    Ok(()) => match safe_join(&root, &event.relative_path) {
                        Ok(dest) => changes.push((dest, false)),
                        Err(reason) => errors.push(reason),
                    },
                    Err(reason) => errors.push(reason),
                }
            }
            IngestEventKind::Deleted => match safe_join(&root, &event.relative_path) {
                Ok(dest) => {
                    match tokio::fs::remove_file(&dest).await {
                        Ok(()) => {}
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                        Err(e) => {
                            errors.push(format!(
                                "{}: failed to remove mirrored file: {e}",
                                event.relative_path
                            ));
                            continue;
                        }
                    }
                    removed += 1;
                    changes.push((dest, true));
                }
                Err(reason) => errors.push(reason),
            },
        }
    }
    let applied = changes.iter().filter(|(_, deleted)| !deleted).count();
    match state.engine.get_hot_update_coordinator(id).await {
        Ok(coordinator) => {
            let coordinator = coordinator.lock().await;
            if let Err(e) = coordinator.run_explicit_changes(changes).await {
                errors.push(e.to_string());
            }
        }
        Err(e) => errors.push(format!("failed to initialize hot update: {e}")),
    }
    ApiResult::Success(IngestEventResponse {
        success: errors.is_empty(),
        project_id: id,
        applied,
        removed,
        errors,
    })
}
