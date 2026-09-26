//! Project query handlers
//!
//! Handles read-only operations for project information.

use axum::extract::{Path, State};

use super::management::record_to_config;
use crate::api::response::ApiResult;
use cce_api::models::error_codes;
use cce_api::models::{ErrorResponse, ProjectDetailResponse, ProjectListResponse};
use cce_storage_sqlite::ProjectRepository;

/// Handle list all projects request
#[utoipa::path(
    get, path = "/api/project", tag = "Project",
    responses(
        (status = 200, body = ProjectListResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_list_projects(
    State(state): State<crate::api::state::AppState>,
) -> ApiResult<ProjectListResponse> {
    // Check if metadata store is available
    let metadata_store = match &state.metadata_store {
        Some(s) => s,
        None => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::STORAGE_ERROR,
                "Metadata store not initialized",
            ));
        }
    };

    // Get all projects
    let records = match metadata_store
        .as_ref()
        .with_transaction(|tx| ProjectRepository::get_all(tx))
    {
        Ok(r) => r,
        Err(e) => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::STORAGE_ERROR,
                format!("Failed to query project: {}", e),
            ));
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

    ApiResult::Success(ProjectListResponse {
        success: true,
        projects,
        total,
    })
}

/// Handle get single project request
#[utoipa::path(
    get, path = "/api/project/{id}", tag = "Project",
    params(("id" = i64, Path, description = "Project id")),
    responses(
        (status = 200, body = ProjectDetailResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_get_project(
    State(state): State<crate::api::state::AppState>,
    Path(id): Path<i64>,
) -> ApiResult<ProjectDetailResponse> {
    // Check if metadata store is available
    let metadata_store = match &state.metadata_store {
        Some(s) => s,
        None => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::STORAGE_ERROR,
                "Metadata store not initialized",
            ));
        }
    };

    let record = match metadata_store
        .as_ref()
        .with_transaction(|tx| ProjectRepository::get_by_id(tx, id))
    {
        Ok(Some(r)) => r,
        Ok(None) => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::ENTITY_NOT_FOUND,
                "Project does not exist",
            ));
        }
        Err(e) => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::STORAGE_ERROR,
                format!("Failed to query project: {}", e),
            ));
        }
    };

    // Convert to ProjectConfig
    let project = match record_to_config(&record) {
        Ok(p) => p,
        Err(e) => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::STORAGE_ERROR,
                format!("Failed to convert project data: {}", e),
            ));
        }
    };

    ApiResult::Success(ProjectDetailResponse {
        success: true,
        project,
    })
}
