//! Project configuration handlers
//!
//! Handles configuration update and reload operations.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde_json::json;

use cce_api::models::error_codes;
use cce_config::global::AppConfig;
use cce_config::project_registry::RegistryError;

/// Request body for updating project config
#[derive(serde::Deserialize)]
pub struct UpdateConfigRequest {
    /// Project-level configuration (partial config)
    pub config: AppConfig,
}

/// Handle update project config request
pub async fn handle_update_project_config(
    State(state): State<crate::api::state::AppState>,
    Path(project_id): Path<i64>,
    Json(payload): Json<UpdateConfigRequest>,
) -> impl IntoResponse {
    // Check if project registry is available
    let registry = match &state.project_registry {
        Some(registry) => registry,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(error_response(
                    error_codes::STORAGE_ERROR,
                    "Project registry not initialized",
                )),
            );
        }
    };

    // 1. Update configuration
    if let Err(e) = registry.update_config(project_id, payload.config).await {
        // Log the error with context
        tracing::error!(
            project_id,
            error = %e,
            "Failed to update project config"
        );

        let (status, code, message) = match &e {
            // Client errors (4xx)
            RegistryError::ProjectNotFound(id) => (
                StatusCode::NOT_FOUND,
                error_codes::ENTITY_NOT_FOUND,
                format!("Project {} not found", id),
            ),
            RegistryError::PathNotFound(path) => (
                StatusCode::BAD_REQUEST,
                error_codes::INVALID_INPUT,
                format!("Path does not exist: {:?}", path),
            ),
            RegistryError::DuplicatePath(path) => (
                StatusCode::CONFLICT,
                error_codes::INVALID_INPUT,
                format!("Path already registered: {:?}", path),
            ),
            RegistryError::Validation(msg) => (
                StatusCode::BAD_REQUEST,
                error_codes::INVALID_INPUT,
                format!("Configuration validation failed: {}", msg),
            ),
            RegistryError::Configuration(msg) => (
                StatusCode::BAD_REQUEST,
                error_codes::INVALID_INPUT,
                format!("Configuration error: {}", msg),
            ),
            RegistryError::Serialization(msg) => (
                StatusCode::BAD_REQUEST,
                error_codes::INVALID_INPUT,
                format!("Invalid configuration format: {}", msg),
            ),
            RegistryError::Deserialization(msg) => (
                StatusCode::BAD_REQUEST,
                error_codes::INVALID_INPUT,
                format!("Invalid configuration format: {}", msg),
            ),

            // Server errors (5xx)
            RegistryError::Io(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                error_codes::STORAGE_ERROR,
                format!("IO error: {}", err),
            ),
            RegistryError::Database(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                error_codes::STORAGE_ERROR,
                format!("Database error: {}", err),
            ),
        };

        return (status, Json(error_response(code, &message)));
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
    (
        StatusCode::OK,
        Json(json!({
            "success": true,
            "hot_reload_applied": hot_reload_success,
            "message": if hot_reload_success {
                "Configuration updated and hot reload triggered successfully"
            } else {
                "Configuration updated, but hot reload failed. Manual restart may be required for some components."
            }
        })),
    )
}

/// Handle reload project config request (hot reload)
pub async fn handle_reload_project_config(
    State(state): State<crate::api::state::AppState>,
    Path(id_str): Path<String>,
) -> impl IntoResponse {
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

    // Check if project registry is available
    let project_registry = match &state.project_registry {
        Some(registry) => registry,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(error_response(
                    error_codes::STORAGE_ERROR,
                    "Project registry not initialized",
                )),
            );
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
        Ok(entry) => (
            StatusCode::OK,
            Json(json!({
                "success": true,
                "message": "Configuration cache invalidated. Will reload from file on next access.",
                "project_id": id_str,
                "config_version": entry.version
            })),
        ),
        Err(e) => {
            tracing::error!(
                project_id = id,
                error = %e,
                "Failed to reload project config"
            );

            let (status, code, message) = match e {
                RegistryError::ProjectNotFound(_) | RegistryError::PathNotFound(_) => (
                    StatusCode::NOT_FOUND,
                    error_codes::ENTITY_NOT_FOUND,
                    format!("Project {} not found", id),
                ),
                RegistryError::Validation(msg)
                | RegistryError::Configuration(msg)
                | RegistryError::Serialization(msg)
                | RegistryError::Deserialization(msg) => {
                    (StatusCode::BAD_REQUEST, error_codes::INVALID_INPUT, msg)
                }
                RegistryError::Io(err) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    error_codes::STORAGE_ERROR,
                    err.to_string(),
                ),
                RegistryError::Database(err) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    error_codes::STORAGE_ERROR,
                    err.to_string(),
                ),
                RegistryError::DuplicatePath(path) => {
                    (StatusCode::CONFLICT, error_codes::INVALID_INPUT, path)
                }
            };

            (status, Json(error_response(code, &message)))
        }
    }
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
