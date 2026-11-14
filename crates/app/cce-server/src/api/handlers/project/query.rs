//! Project query handlers
//!
//! Handles read-only operations for project information.

use axum::extract::{Path, State};

use crate::api::response::ApiResult;
use cce_api::models::error_codes;
use cce_api::models::{ErrorResponse, ProjectDetailResponse, ProjectListResponse};
use cce_server_shared::services::project::{ProjectServiceError, get_project, list_projects};

/// Map a project service failure onto the HTTP error shape.
fn project_error(error: ProjectServiceError) -> ErrorResponse {
    match error {
        ProjectServiceError::MetadataUnavailable => {
            ErrorResponse::new(error_codes::STORAGE_ERROR, "Metadata store not initialized")
        }
        ProjectServiceError::NotFound(_) => {
            ErrorResponse::new(error_codes::ENTITY_NOT_FOUND, "Project does not exist")
        }
        ProjectServiceError::Invalid(message) => {
            ErrorResponse::new(error_codes::INVALID_INPUT, message)
        }
        ProjectServiceError::Conflict(message) => {
            ErrorResponse::new(error_codes::CONFLICT, message)
        }
        ProjectServiceError::Storage(message) => {
            ErrorResponse::new(error_codes::STORAGE_ERROR, message)
        }
    }
}

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
    State(state): State<cce_server_shared::context::SharedContext>,
) -> ApiResult<ProjectListResponse> {
    let projects = match list_projects(&state).await {
        Ok(projects) => projects,
        Err(error) => return ApiResult::Error(project_error(error)),
    };

    let total = projects.len();

    ApiResult::Success(ProjectListResponse {
        success: true,
        projects,
        total,
    })
}

/// Handle get single project request
#[utoipa::path(
    get, path = "/api/project/{project_id}", tag = "Project",
    params(("project_id" = i64, Path, description = "Project id")),
    responses(
        (status = 200, body = ProjectDetailResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_get_project(
    State(state): State<cce_server_shared::context::SharedContext>,
    Path(id): Path<i64>,
) -> ApiResult<ProjectDetailResponse> {
    let project = match get_project(&state, id).await {
        Ok(project) => project,
        Err(error) => return ApiResult::Error(project_error(error)),
    };

    ApiResult::Success(ProjectDetailResponse {
        success: true,
        project,
    })
}
