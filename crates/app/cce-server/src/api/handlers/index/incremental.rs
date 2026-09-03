//! HTTP incremental indexing handler.
//!
//! Explicit HTTP changes use the same hot-update operation as filesystem
//! events. This keeps candidate generation, relation publication and hash
//! commit under one lifecycle.

use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use std::path::PathBuf;

use cce_api::models::{IncrementalIndexRequest, IncrementalIndexResponse};

/// Handle an explicit incremental index request.
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
    let total_entities = 0usize;
    let total_vectors = 0usize;

    match state.engine.get_hot_update_coordinator(project_id).await {
        Ok(coordinator) => {
            let coordinator = coordinator.lock().await;
            if let Err(error) = coordinator.run_explicit_changes(changes).await {
                errors.push(error.to_string());
            }
        }
        Err(error) => errors.push(format!("failed to initialize hot update: {error}")),
    }

    // Entity/vector counts are intentionally reported as request-level counts
    // here. The operation result is the authoritative success/failure record;
    // processors may reparse dependent files as part of relation propagation.
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
