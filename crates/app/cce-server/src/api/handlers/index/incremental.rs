//! HTTP incremental indexing handler.
//!
//! Explicit HTTP changes use the same hot-update operation as filesystem
//! events. This keeps candidate generation, relation publication and hash
//! commit under one lifecycle.

use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use std::path::PathBuf;

use cce_api::models::{ErrorResponse, IncrementalIndexRequest, IncrementalIndexResponse};
use cce_relation::index::entity_index::EntityIndexOps;

/// Handle an explicit incremental index request.
#[utoipa::path(
    post, path = "/api/index/incremental", tag = "Index",
    request_body = IncrementalIndexRequest,
    responses(
        (status = 200, body = IncrementalIndexResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_incremental(
    State(state): State<crate::api::state::AppState>,
    Json(request): Json<IncrementalIndexRequest>,
) -> impl IntoResponse {
    let start = std::time::Instant::now();
    let project_id = request.project_id;
    let files_indexed = request.files_to_index.len();
    let files_removed = request.files_to_remove.len();
    let changes: Vec<_> = request
        .files_to_remove
        .iter()
        .map(|path| (PathBuf::from(path), true))
        .chain(
            request
                .files_to_index
                .iter()
                .map(|path| (PathBuf::from(path), false)),
        )
        .collect();

    let mut errors = Vec::new();

    let mut total_entities = 0usize;
    let mut total_vectors = 0usize;

    match state.engine.get_hot_update_coordinator(project_id).await {
        Ok(coordinator) => {
            let coordinator = coordinator.lock().await;
            if let Err(error) = coordinator.run_explicit_changes(changes).await {
                errors.push(error.to_string());
            }
        }
        Err(error) => errors.push(format!("failed to initialize hot update: {error}")),
    }

    // Report authoritative post-run totals: entity count from the relation
    // index, vector count from the project-scoped Qdrant collection. The
    // operation result is the authoritative success/failure record; processors
    // may reparse dependent files as part of relation propagation, so
    // request-level counts would understate the actual index state.
    if errors.is_empty() {
        if let Ok(orchestrator) = state.engine.get_orchestrator(project_id).await {
            let orchestrator = orchestrator.lock().await;
            if let Some(builder) = orchestrator.get_relation_builder() {
                total_entities = builder.index().function_count();
            }
        }
        let group_id = crate::api::handlers::storage::resolve_group_id(&state, project_id).await;
        if let Some(gid) = group_id {
            total_vectors = state
                .engine
                .qdrant()
                .count_points_by_group(&gid)
                .await
                .unwrap_or(0);
        }
    }

    let response = IncrementalIndexResponse {
        success: errors.is_empty(),
        files_indexed,
        files_removed,
        total_entities,
        total_vectors,
        elapsed_ms: start.elapsed().as_millis() as u64,
        errors,
    };

    (StatusCode::OK, Json(response))
}
