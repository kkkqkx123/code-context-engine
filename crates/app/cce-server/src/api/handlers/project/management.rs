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
use cce_storage_metadb_sqlite::{
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
    // Root path semantics: registry stores the logical identifier used for
    // display and de-duplication. Local supply requires a server-side
    // physical directory; gateway supply allows a logical-only path since
    // content arrives via ingest and the server never touches the filesystem.
    let supply_is_gateway = request
        .supply_mode
        .as_deref()
        .map(|mode| mode.trim().to_lowercase() == cce_api::models::SUPPLY_MODE_GATEWAY)
        .unwrap_or(false);
    if request.root_path.is_empty() && !supply_is_gateway {
        return ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "Root path cannot be empty",
        ));
    }
    let root_path_str = if supply_is_gateway {
        cce_supply::normalize_logical_path(&request.root_path)
    } else {
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
        let canonical_path = match root_path.canonicalize() {
            Ok(p) => p,
            Err(e) => {
                return ApiResult::Error(ErrorResponse::new(
                    error_codes::INVALID_REQUEST,
                    format!("Failed to resolve path: {}", e),
                ));
            }
        };
        canonical_path.to_string_lossy().to_string()
    };

    // Check if metadata store is available
    let metadata_store = match state.engine.metadata_store() {
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

    // Create new project record. The profile document is the single source
    // of truth for scan configuration; supply filter arguments become a
    // one-shot profile patch through the shared supply kernel.
    let mut new_project = NewProjectRecord::new(name.clone(), root_path_str.clone());
    let initial_profile = cce_supply::profile_patch_from_supply_filter(
        request.extensions.clone(),
        request.exclude_dirs.clone(),
        Some(request.respect_gitignore),
        request.ignore_patterns.clone(),
    );
    new_project.profile_json = Some(initial_profile.to_canonical_json());

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
    if let Some(supply_mode) = request.supply_mode.as_deref() {
        let normalized = supply_mode.trim().to_lowercase();
        if normalized == cce_api::models::SUPPLY_MODE_GATEWAY
            || normalized == cce_api::models::SUPPLY_MODE_LOCAL
        {
            if let Err(e) = client.with_transaction(|tx| {
                ProjectRepository::meta_set_string(
                    tx,
                    project_id,
                    cce_api::models::SUPPLY_MODE_KEY,
                    &normalized,
                )
            }) {
                tracing::warn!(
                    project_id = project_id,
                    error = %e,
                    "Failed to record supply mode (non-critical)"
                );
            }
        }
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
    put, path = "/api/project/{project_id}", tag = "Project",
    params(("project_id" = String, Path, description = "Project id or path")),
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
    let metadata_store = match state.engine.metadata_store() {
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

    // Build update record. Scan-list changes patch the stored profile
    // document so the profile stays the single source of truth.
    let mut updates = ProjectUpdateRecord::default();
    if let Some(name) = request.name {
        updates = updates.with_name(name);
    }
    if request.extensions.is_some()
        || request.exclude_dirs.is_some()
        || request.respect_gitignore.is_some()
        || request.ignore_patterns.is_some()
    {
        let client_for_profile = metadata_store.as_ref();
        let patched = client_for_profile.with_transaction(|tx| {
            let record = ProjectRepository::get_by_id(tx, id)?.ok_or_else(|| {
                cce_types::StorageError::not_found(format!("Project {id} not found"))
            })?;
            let mut profile = cce_config::ProjectAppConfig::from_json_str(&record.profile_json)
                .map_err(|e| {
                    cce_types::StorageError::query(format!("Corrupt project profile: {e}"))
                })?;
            if let Some(extensions) = request.extensions.clone() {
                let mut orchestrator = profile.orchestrator.take().unwrap_or_default();
                let mut indexer = orchestrator.indexer.take().unwrap_or_default();
                indexer.extensions = Some(extensions);
                orchestrator.indexer = Some(indexer);
                profile.orchestrator = Some(orchestrator);
            }
            if let Some(exclude_dirs) = request.exclude_dirs.clone() {
                let mut orchestrator = profile.orchestrator.take().unwrap_or_default();
                let mut indexer = orchestrator.indexer.take().unwrap_or_default();
                indexer.exclude_dirs = Some(exclude_dirs);
                orchestrator.indexer = Some(indexer);
                profile.orchestrator = Some(orchestrator);
            }
            if request.respect_gitignore.is_some() || request.ignore_patterns.is_some() {
                let mut scanner = profile.scanner.take().unwrap_or_default();
                if let Some(respect) = request.respect_gitignore {
                    scanner.respect_gitignore = Some(respect);
                }
                if let Some(patterns) = request.ignore_patterns.clone() {
                    scanner.gitignore_patterns = Some(patterns);
                }
                profile.scanner = Some(scanner);
            }
            Ok::<String, cce_types::StorageError>(profile.to_canonical_json())
        });
        match patched {
            Ok(canonical) => {
                updates = updates.with_profile_json(canonical);
            }
            Err(error) => {
                let message = error.to_string();
                if message.contains("not found") {
                    return ApiResult::Error(ErrorResponse::new(
                        error_codes::ENTITY_NOT_FOUND,
                        "Project does not exist",
                    ));
                }
                return ApiResult::Error(ErrorResponse::new(
                    error_codes::STORAGE_ERROR,
                    format!("Failed to patch project profile: {}", message),
                ));
            }
        }
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
    let _ = state.engine.clear_project_component_caches(id).await;
    let _ = state
        .engine
        .project_registry()
        .invalidate_cache(Some(id))
        .await;
    if let Some(supply_mode) = request.supply_mode.as_deref() {
        let normalized = supply_mode.trim().to_lowercase();
        if normalized == cce_api::models::SUPPLY_MODE_GATEWAY
            || normalized == cce_api::models::SUPPLY_MODE_LOCAL
        {
            if let Err(e) = client.with_transaction(|tx| {
                ProjectRepository::meta_set_string(
                    tx,
                    id,
                    cce_api::models::SUPPLY_MODE_KEY,
                    &normalized,
                )
            }) {
                tracing::warn!(
                    project_id = id,
                    error = %e,
                    "Failed to record supply mode (non-critical)"
                );
            }
        }
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
    let project = record_to_config(&record);

    ApiResult::Success(ProjectDetailResponse {
        success: true,
        project,
    })
}

/// Handle delete project request
#[utoipa::path(
    delete, path = "/api/project/{project_id}", tag = "Project",
    params(("project_id" = i64, Path, description = "Project id")),
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
        Some(state.engine.vector_clone()),
        Some(state.engine.fulltext_clone()),
        state.engine.metadata_store_clone(),
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

    // Drop the per-project shared caches so a recreated project with the
    // same ID never inherits entries from the deleted one.
    state.invalidate_relation_searcher(id).await;
    state.engine.invalidate_query_cache(id).await;

    ApiResult::Success(ProjectDeleteResponse {
        success: true,
        message: "Project deleted successfully".to_string(),
        project_id: id,
    })
}

/// Convert ProjectRecord to ProjectConfig
///
/// This is a display and request-prefill view only. The profile document is
/// the single source of truth for scan lists. A section absent from the
/// profile is reported as empty so callers can tell that no explicit value
/// was registered. Effective indexing values always come from the merged
/// registry configuration, not from this view.
pub(crate) fn record_to_config(
    record: &cce_storage_metadb_sqlite::ProjectRecord,
) -> cce_api::models::ProjectConfig {
    use cce_api::models::ProjectConfig;

    let profile = cce_config::ProjectAppConfig::from_json_str(&record.profile_json).ok();
    let (extensions, exclude_dirs, respect_gitignore, ignore_patterns) = match profile {
        Some(profile) => (
            profile
                .orchestrator
                .as_ref()
                .and_then(|orchestrator| orchestrator.indexer.as_ref())
                .and_then(|indexer| indexer.extensions.clone())
                .unwrap_or_default(),
            profile
                .orchestrator
                .as_ref()
                .and_then(|orchestrator| orchestrator.indexer.as_ref())
                .and_then(|indexer| indexer.exclude_dirs.clone())
                .unwrap_or_default(),
            profile
                .scanner
                .as_ref()
                .and_then(|scanner| scanner.respect_gitignore)
                .unwrap_or(true),
            profile
                .scanner
                .as_ref()
                .and_then(|scanner| scanner.gitignore_patterns.clone())
                .unwrap_or_default(),
        ),
        None => (Vec::new(), Vec::new(), true, Vec::new()),
    };

    ProjectConfig {
        id: record.id.to_string(),
        name: record.name.clone(),
        root_path: record.root_path.clone(),
        extensions,
        exclude_dirs,
        respect_gitignore,
        ignore_patterns,
        created_at: record.created_at.to_string(),
        last_indexed: record.last_indexed.clone(),
    }
}
