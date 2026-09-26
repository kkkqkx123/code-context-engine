//! Project management handlers
//!
//! Handles create, update, and delete operations for projects.

use axum::{
    Json,
    extract::{Path, State},
};
use std::path::PathBuf;

use crate::api::response::ApiResult;
use crate::runtime::recovery::ProjectMeta;
use cce_api::models::error_codes;
use cce_api::models::{
    CreateProjectRequest, ErrorResponse, ProjectConfig, ProjectDeleteResponse,
    ProjectDetailResponse, UpdateProjectRequest,
};
use cce_storage_sqlite::{
    NewProjectRecord, ProjectRepository, ProjectUpdateRecord, generate_project_name,
};

/// Handle create project request
#[utoipa::path(
    post, path = "/api/project", tag = "Project",
    request_body = CreateProjectRequest,
    responses(
        (status = 200, body = ProjectDetailResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_create_project(
    State(state): State<crate::api::state::AppState>,
    Json(request): Json<CreateProjectRequest>,
) -> ApiResult<ProjectDetailResponse> {
    // Validate root_path is not empty
    if request.root_path.is_empty() {
        return ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "Root path cannot be empty",
        ));
    }

    // Validate root_path exists and is a directory
    let root_path = PathBuf::from(&request.root_path);
    if !root_path.exists() {
        return ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "Root path does not exist",
        ));
    }
    if !root_path.is_dir() {
        return ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "Root path is not a directory",
        ));
    }

    // Get canonical path
    let canonical_path = match root_path.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::INVALID_REQUEST,
                format!("Failed to resolve path: {}", e),
            ));
        }
    };
    let root_path_str = canonical_path.to_string_lossy().to_string();

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

    // Check if path already exists
    let client = metadata_store.as_ref();
    match client.with_transaction(|tx| ProjectRepository::path_exists(tx, &root_path_str)) {
        Ok(false) => {}
        Ok(true) => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::CONFLICT,
                "Project already exists at this path",
            ));
        }
        Err(e) => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::STORAGE_ERROR,
                format!("Failed to check path: {}", e),
            ));
        }
    }

    // Generate or use provided name
    let name = match &request.name {
        Some(n) if !n.is_empty() => n.clone(),
        _ => generate_project_name(&root_path_str),
    };

    // Create new project record with all fields
    let mut new_project = NewProjectRecord::new(name.clone(), root_path_str.clone());
    if !request.extensions.is_empty() {
        new_project.extensions =
            Some(serde_json::to_string(&request.extensions).unwrap_or_default());
    }
    if !request.exclude_dirs.is_empty() {
        new_project.exclude_dirs =
            Some(serde_json::to_string(&request.exclude_dirs).unwrap_or_default());
    }
    new_project.respect_gitignore = Some(request.respect_gitignore);
    if !request.ignore_patterns.is_empty() {
        new_project.ignore_patterns =
            Some(serde_json::to_string(&request.ignore_patterns).unwrap_or_default());
    }

    // Insert into database
    let client = metadata_store.as_ref();
    let project_id = match client.with_transaction(|tx| ProjectRepository::insert(tx, &new_project))
    {
        Ok(id) => id,
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("UNIQUE constraint failed") {
                return ApiResult::Error(ErrorResponse::new(
                    error_codes::CONFLICT,
                    "Project with this name or root path already exists",
                ));
            }
            return ApiResult::Error(ErrorResponse::new(
                error_codes::STORAGE_ERROR,
                format!("Failed to create project: {}", msg),
            ));
        }
    };

    // Initialize project metadata for version management
    if let Err(e) = ProjectMeta::init_for_project(client, project_id) {
        tracing::warn!(
            project_id = project_id,
            error = %e,
            "Failed to initialize project metadata (non-critical)"
        );
    }

    // Build response
    ApiResult::Success(ProjectDetailResponse {
        success: true,
        project: ProjectConfig {
            id: project_id.to_string(),
            name,
            root_path: root_path_str,
            extensions: request.extensions,
            exclude_dirs: request.exclude_dirs,
            respect_gitignore: request.respect_gitignore,
            ignore_patterns: request.ignore_patterns,
            created_at: chrono::Utc::now().to_rfc3339(),
            last_indexed: None,
        },
    })
}

/// Handle update project metadata request
#[utoipa::path(
    put, path = "/api/project/{id}", tag = "Project",
    params(("id" = String, Path, description = "Project id or path")),
    request_body = UpdateProjectRequest,
    responses(
        (status = 200, body = ProjectDetailResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_update_project(
    State(state): State<crate::api::state::AppState>,
    Path(id_str): Path<String>,
    Json(request): Json<UpdateProjectRequest>,
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

    // Check if project exists
    let id: i64 = match id_str.parse() {
        Ok(id) => id,
        Err(_) => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::INVALID_INPUT,
                "Invalid project ID: must be a number",
            ));
        }
    };

    match metadata_store
        .as_ref()
        .with_transaction(|tx| ProjectRepository::get_by_id(tx, id))
    {
        Ok(Some(_)) => {}
        Ok(None) => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::ENTITY_NOT_FOUND,
                "Project does not exist",
            ));
        }
        Err(e) => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::STORAGE_ERROR,
                format!("Failed to check project: {}", e),
            ));
        }
    };

    // Build update record with all provided fields
    let mut updates = ProjectUpdateRecord::default();
    if let Some(name) = request.name {
        updates = updates.with_name(name);
    }
    if let Some(extensions) = request.extensions {
        updates = updates.with_extensions(serde_json::to_string(&extensions).unwrap_or_default());
    }
    if let Some(exclude_dirs) = request.exclude_dirs {
        updates =
            updates.with_exclude_dirs(serde_json::to_string(&exclude_dirs).unwrap_or_default());
    }
    if let Some(respect_gitignore) = request.respect_gitignore {
        updates = updates.with_respect_gitignore(respect_gitignore);
    }
    if let Some(ignore_patterns) = request.ignore_patterns {
        updates = updates
            .with_ignore_patterns(serde_json::to_string(&ignore_patterns).unwrap_or_default());
    }
    // Note: root_path is not allowed to be updated
    // If needed, add root_path field to UpdateProjectRequest

    // Use update method instead of delete-insert
    let client = metadata_store.as_ref();
    if let Err(e) = client.with_transaction(|tx| ProjectRepository::update(tx, id, &updates)) {
        return ApiResult::Error(ErrorResponse::new(
            error_codes::STORAGE_ERROR,
            format!("Failed to update project: {}", e),
        ));
    }

    // Get updated project
    let record = match client.with_transaction(|tx| ProjectRepository::get_by_id(tx, id)) {
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
                format!("Failed to query updated project: {}", e),
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

/// Handle delete project request
#[utoipa::path(
    delete, path = "/api/project/{id}", tag = "Project",
    params(("id" = i64, Path, description = "Project id")),
    responses(
        (status = 200, body = ProjectDeleteResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_delete_project(
    State(state): State<crate::api::state::AppState>,
    Path(id): Path<i64>,
) -> ApiResult<ProjectDeleteResponse> {
    // 1. Stop watch if running (handler-specific: maintenance service doesn't manage watchers)
    if let Some(tracker) = state.watch_status.write().await.remove(&id) {
        if tracker.active {
            tracing::info!(project_id = id, "Watch stopped during project deletion");
        }
    }

    // 2. Use unified maintenance service for all storage-layer cleanup
    let maintenance = crate::maintenance::ProjectIndexMaintenanceService::new(
        state.engine.clone(),
        state.qdrant.clone(),
        state.bm25.clone(),
        state.metadata_store.clone(),
    );

    let m_result = maintenance.delete_project(id).await;

    if !m_result.success {
        let error_detail = m_result
            .backends
            .iter()
            .filter(|b| !b.ok)
            .map(|b| format!("{}: {}", b.backend, b.detail))
            .collect::<Vec<_>>()
            .join("; ");
        return ApiResult::Error(ErrorResponse::new(
            error_codes::STORAGE_ERROR,
            format!("Project deletion failed: {}", error_detail),
        ));
    }

    // Evict per-project metrics (gauges/counters/histograms carrying the
    // project_id label) from the registry so deleted projects never leak
    // stale series into snapshots/Prometheus output.
    let evicted = state
        .engine
        .metrics_registry()
        .remove_by_label_value("project_id", &id.to_string());
    if evicted > 0 {
        tracing::info!(
            project_id = id,
            evicted = evicted,
            "Evicted metrics for deleted project"
        );
    }

    ApiResult::Success(ProjectDeleteResponse {
        success: true,
        message: "Project deleted successfully".to_string(),
        project_id: id,
    })
}

/// Convert ProjectRecord to ProjectConfig
pub(crate) fn record_to_config(
    record: &cce_storage_sqlite::ProjectRecord,
) -> Result<cce_api::models::ProjectConfig, serde_json::Error> {
    use cce_api::models::ProjectConfig;

    let parse_json_list = |val: &Option<String>| -> Vec<String> {
        val.as_ref().map_or_else(Vec::new, |v| {
            serde_json::from_str(v).unwrap_or_else(|_| Vec::new())
        })
    };

    Ok(ProjectConfig {
        id: record.id.to_string(),
        name: record.name.clone(),
        root_path: record.root_path.clone(),
        extensions: parse_json_list(&record.extensions),
        exclude_dirs: parse_json_list(&record.exclude_dirs),
        respect_gitignore: record.respect_gitignore.unwrap_or(true),
        ignore_patterns: parse_json_list(&record.ignore_patterns),
        created_at: record.created_at.to_string(),
        last_indexed: record.last_indexed.clone(),
    })
}
