//! Project configuration handlers
//!
//! Handles configuration update and reload operations.

use axum::{
    Json,
    extract::{Path, State},
};

use crate::api::response::ApiResult;
use cce_api::models::error_codes;
use cce_api::models::{
    ErrorResponse, ProjectConfigReloadResponse, ProjectConfigUpdateRequest,
    ProjectConfigUpdateResponse,
};
use cce_config::global::AppConfig;
use cce_config::project_registry::RegistryError;

/// Handle update project config request
#[utoipa::path(
    put, path = "/api/project/{id}/config", tag = "Project",
    params(("project_id" = i64, Path, description = "Project id")),
    request_body = ProjectConfigUpdateRequest,
    responses(
        (status = 200, body = ProjectConfigUpdateResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_update_project_config(
    State(state): State<crate::api::state::AppState>,
    Path(project_id): Path<i64>,
    Json(payload): Json<ProjectConfigUpdateRequest>,
) -> ApiResult<ProjectConfigUpdateResponse> {
    // Check if project registry is available
    let registry = match &state.project_registry {
        Some(registry) => registry,
        None => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::STORAGE_ERROR,
                "Project registry not initialized",
            ));
        }
    };

    // Validate the submitted config against the application schema
    let config = match serde_json::from_value::<AppConfig>(payload.config) {
        Ok(config) => config,
        Err(e) => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::INVALID_INPUT,
                format!("Invalid configuration format: {}", e),
            ));
        }
    };

    // 1. Update configuration
    if let Err(e) = registry.update_config(project_id, config).await {
        // Log the error with context
        tracing::error!(
            project_id,
            error = %e,
            "Failed to update project config"
        );

        let (code, message) = match &e {
            RegistryError::ProjectNotFound(id) => (
                error_codes::ENTITY_NOT_FOUND,
                format!("Project {} not found", id),
            ),
            RegistryError::PathNotFound(path) => (
                error_codes::INVALID_INPUT,
                format!("Path does not exist: {:?}", path),
            ),
            RegistryError::DuplicatePath(path) => (
                error_codes::CONFLICT,
                format!("Path already registered: {:?}", path),
            ),
            RegistryError::Validation(msg) => (
                error_codes::INVALID_INPUT,
                format!("Configuration validation failed: {}", msg),
            ),
            RegistryError::Configuration(msg) => (
                error_codes::INVALID_INPUT,
                format!("Configuration error: {}", msg),
            ),
            RegistryError::Serialization(msg) => (
                error_codes::INVALID_INPUT,
                format!("Invalid configuration format: {}", msg),
            ),
            RegistryError::Deserialization(msg) => (
                error_codes::INVALID_INPUT,
                format!("Invalid configuration format: {}", msg),
            ),
            RegistryError::Io(err) => (error_codes::STORAGE_ERROR, format!("IO error: {}", err)),
            RegistryError::Database(err) => (
                error_codes::STORAGE_ERROR,
                format!("Database error: {}", err),
            ),
        };

        return ApiResult::Error(ErrorResponse::new(code, message));
    }

    // 2. Notify HotUpdateCoordinator to reload (if exists)
    let mut hot_reload_success = true;
    let engine = &state.engine;
    match engine.get_hot_update_coordinator(project_id).await {
        Ok(coordinator) => {
            let mut coord = coordinator.lock().await;
            if let Err(e) = coord.reload_project_config().await {
                tracing::error!(
                    project_id,
                    error = %e,
                    "Failed to reload HotUpdateCoordinator config after update"
                );
                hot_reload_success = false;
                // Don't return error - main config was updated successfully
            }
        }
        Err(e) => {
            tracing::warn!(
                project_id,
                error = %e,
                "HotUpdateCoordinator not available, config updated but hot reload skipped"
            );
            hot_reload_success = false;
        }
    }

    // 3. Return success response with hot reload status
    ApiResult::Success(ProjectConfigUpdateResponse {
        success: true,
        hot_reload_applied: hot_reload_success,
        message: if hot_reload_success {
            "Configuration updated and hot reload triggered successfully".to_string()
        } else {
            "Configuration updated, but hot reload failed. Manual restart may be required for some components.".to_string()
        },
    })
}

/// Handle reload project config request (hot reload)
#[utoipa::path(
    post, path = "/api/project/{id}/reload", tag = "Project",
    params(("id" = i64, Path, description = "Project id")),
    responses(
        (status = 200, body = ProjectConfigReloadResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_reload_project_config(
    State(state): State<crate::api::state::AppState>,
    Path(id): Path<i64>,
) -> ApiResult<ProjectConfigReloadResponse> {
    // Check if project registry is available
    let project_registry = match &state.project_registry {
        Some(registry) => registry,
        None => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::STORAGE_ERROR,
                "Project registry not initialized",
            ));
        }
    };

    // Invalidate cache to force reload from file on next access
    let _ = project_registry.invalidate_cache(Some(id)).await;

    // Clear engine component caches to force recreation with new config
    if let Err(e) = state.engine.reload_project_config(id).await {
        tracing::warn!(
            project_id = id,
            error = %e,
            "Failed to reload engine components (non-critical)"
        );
    }

    // Verify project still exists by reloading
    match project_registry.get_or_load(id).await {
        Ok(entry) => ApiResult::Success(ProjectConfigReloadResponse {
            success: true,
            message: "Configuration cache invalidated. Will reload from file on next access."
                .to_string(),
            project_id: id,
            config_version: entry.version,
        }),
        Err(e) => {
            tracing::error!(
                project_id = id,
                error = %e,
                "Failed to reload project config"
            );

            let (code, message) = match e {
                RegistryError::ProjectNotFound(_) | RegistryError::PathNotFound(_) => (
                    error_codes::ENTITY_NOT_FOUND,
                    format!("Project {} not found", id),
                ),
                RegistryError::Validation(msg)
                | RegistryError::Configuration(msg)
                | RegistryError::Serialization(msg)
                | RegistryError::Deserialization(msg) => (error_codes::INVALID_INPUT, msg),
                RegistryError::Io(err) => (error_codes::STORAGE_ERROR, err.to_string()),
                RegistryError::Database(err) => (error_codes::STORAGE_ERROR, err.to_string()),
                RegistryError::DuplicatePath(path) => (
                    error_codes::CONFLICT,
                    format!("Path already registered: {:?}", path),
                ),
            };

            ApiResult::Error(ErrorResponse::new(code, message))
        }
    }
}
