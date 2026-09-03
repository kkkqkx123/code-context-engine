//! Project indexing handlers
//!
//! Handles indexing operations for projects.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde_json::json;

use cce_api::models::error_codes;
use cce_storage_sqlite::{ProjectRepository, ProjectUpdateRecord};

/// Handle project indexing request
pub async fn handle_project_index(
    State(state): State<crate::api::state::AppState>,
    Path(id_str): Path<String>,
) -> impl IntoResponse {
    // Check if metadata store is available
    let metadata_store = match &state.metadata_store {
        Some(s) => s,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(error_response(
                    error_codes::STORAGE_ERROR,
                    "Metadata store not initialized",
                )),
            );
        }
    };

    // Parse project ID
    let id: i64 = match id_str.parse() {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(error_response(
                    error_codes::INVALID_INPUT,
                    "Invalid project ID: must be a number",
                )),
            );
        }
    };

    // Get project record
    let record = match metadata_store
        .as_ref()
        .with_transaction(|tx| ProjectRepository::get_by_id(tx, id))
    {
        Ok(Some(r)) => r,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(error_response(
                    error_codes::ENTITY_NOT_FOUND,
                    "Project does not exist",
                )),
            );
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(error_response(
                    error_codes::STORAGE_ERROR,
                    &format!("Failed to query project: {}", e),
                )),
            );
        }
    };

    // Build IndexOptions from project config
    let extensions: Vec<String> = vec![]; // Default extensions
    let exclude_dirs: Vec<String> = vec![]; // Default exclude dirs
    let ignore_patterns: Vec<String> = vec![]; // Default ignore patterns

    let index_options = cce_orchestrator::IndexOptions::new(&record.root_path)
        .with_extensions(extensions)
        .with_exclude_dirs(exclude_dirs)
        .with_gitignore(true)
        .with_ignore_patterns(ignore_patterns);

    // Execute indexing using engine's index method with project_id
    let result = match state.engine.index(id, index_options).await {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(error_response(
                    error_codes::INTERNAL_ERROR,
                    &format!("Index execution failed: {}", e),
                )),
            );
        }
    };

    // Update last_indexed timestamp
    let now = chrono::Utc::now().to_rfc3339();
    // Use update method instead of delete-insert
    let client = metadata_store.as_ref();
    let updates = ProjectUpdateRecord::default().with_last_indexed(now);
    if let Err(e) = client.with_transaction(|tx| ProjectRepository::update(tx, id, &updates)) {
        tracing::warn!("Failed to update last_indexed: {}", e);
    }

    (
        StatusCode::OK,
        Json(json!({
            "success": true,
            "project_id": id_str,
            "project_name": record.name,
            "indexed_files": result.indexed_files,
            "total_entities": result.total_entities,
            "total_vectors": result.total_vectors,
            "elapsed_ms": result.elapsed_ms
        })),
    )
}

/// Create error response JSON
fn error_response(code: &str, message: &str) -> serde_json::Value {
    json!({
        "success": false,
        "error": {
            "code": code,
            "message": message
        }
    })
}
