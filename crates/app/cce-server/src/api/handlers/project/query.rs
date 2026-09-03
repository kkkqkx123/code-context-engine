//! Project query handlers
//!
//! Handles read-only operations for project information.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde_json::json;

use cce_api::models::ProjectConfig;
use cce_api::models::error_codes;
use cce_storage_sqlite::{ProjectRecord, ProjectRepository};

/// Handle list all projects request
pub async fn handle_list_projects(
    State(state): State<crate::api::state::AppState>,
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

    // Get all projects
    let records = match metadata_store
        .as_ref()
        .with_transaction(|tx| ProjectRepository::get_all(tx))
    {
        Ok(r) => r,
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

    // Convert to ProjectConfig
    let mut projects = Vec::new();
    for record in records {
        match record_to_config(&record) {
            Ok(config) => projects.push(config),
            Err(e) => {
                tracing::warn!("Failed to convert project record: {}", e);
            }
        }
    }

    let total = projects.len();

    (
        StatusCode::OK,
        Json(json!({
            "success": true,
            "projects": projects,
            "total": total
        })),
    )
}

/// Handle get single project request
pub async fn handle_get_project(
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

    // Convert to ProjectConfig
    let project = match record_to_config(&record) {
        Ok(p) => p,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(error_response(
                    error_codes::STORAGE_ERROR,
                    &format!("Failed to convert project data: {}", e),
                )),
            );
        }
    };

    (
        StatusCode::OK,
        Json(json!({
            "success": true,
            "project": project
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

/// Convert ProjectRecord to ProjectConfig
fn record_to_config(record: &ProjectRecord) -> Result<ProjectConfig, serde_json::Error> {
    Ok(ProjectConfig {
        id: record.id.to_string(),
        name: record.name.clone(),
        root_path: record.root_path.clone(),
        extensions: vec![],
        exclude_dirs: vec![],
        respect_gitignore: true,
        ignore_patterns: vec![],
        created_at: record.created_at.to_string(),
        last_indexed: record.last_indexed.clone(),
    })
}
