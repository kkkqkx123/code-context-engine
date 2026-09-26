//! Storage management handlers
//!
//! This module provides handlers for storage management including:
//! - Clear index
//! - Delete file/entity
//! - Index statistics
//! - Storage status

use axum::{
    Json,
    extract::{Path, Query, State},
};

use crate::api::response::ApiResult;
use crate::api::validation::validate_project_id;
use crate::maintenance::ProjectIndexMaintenanceService;
use cce_api::models::{
    BackendResultInfo, BatchDeleteRequest, BatchDeleteResponse, ClearIndexRequest,
    ClearIndexResponse, DeleteEntityResponse, DeleteFileResponse, ErrorResponse, IndexStatistics,
    IndexStatsResponse, QdrantProcessInfo, QdrantProcessStatus, StorageComponentStatus,
    StorageQuery, StorageStatus, StorageStatusResponse, error_codes,
};
use cce_relation::index::entity_index::EntityIndexOps;
use cce_relation::index::file_index::FileLevelOps;
use cce_relation::index::relation_query::RelationQueryOps;

// ============================================================================
// Clear Index
// ============================================================================

/// Handle clear index request
///
/// Uses ProjectIndexMaintenanceService to coordinate Qdrant, BM25, SQLite,
/// relations, and cache cleanup. Returns per-backend results so partial
/// failures are observable. Always idempotent.
#[utoipa::path(
    delete, path = "/api/index", tag = "Storage",
    request_body = ClearIndexRequest,
    responses(
        (status = 200, body = ClearIndexResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_clear_index(
    State(state): State<crate::api::state::AppState>,
    Json(request): Json<ClearIndexRequest>,
) -> ApiResult<ClearIndexResponse> {
    let start = std::time::Instant::now();

    if request.project_id <= 0 {
        return ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "project_id must be a positive integer",
        ));
    }

    let maintenance = ProjectIndexMaintenanceService::new(
        state.engine.clone(),
        state.qdrant.clone(),
        state.bm25.clone(),
        state.metadata_store.clone(),
    );

    let m_result = maintenance.clear_project_index(request.project_id).await;

    let response = ClearIndexResponse {
        success: m_result.success,
        project_id: request.project_id,
        backends: m_result
            .backends
            .into_iter()
            .map(|b| BackendResultInfo {
                backend: b.backend.to_string(),
                ok: b.ok,
                detail: b.detail,
            })
            .collect(),
        elapsed_ms: start.elapsed().as_millis() as u64,
        message: if m_result.success {
            "Index clearance completed.".to_string()
        } else {
            "Index clearance completed with errors.".to_string()
        },
    };

    ApiResult::Success(response)
}

// ============================================================================
// Delete Operations
// ============================================================================

/// Handle delete file request
///
/// Removes all data associated with a file across all storage backends,
/// scoped to the specified project.
#[utoipa::path(
    delete, path = "/api/index/file/{path}", tag = "Storage",
    params(StorageQuery, ("path" = String, Path, description = "File path")),
    responses(
        (status = 200, body = DeleteFileResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_delete_file(
    State(state): State<crate::api::state::AppState>,
    Path(file_path): Path<String>,
    Query(query): Query<StorageQuery>,
) -> ApiResult<DeleteFileResponse> {
    let start = std::time::Instant::now();
    let project_id = query.project_id;

    if let Err(e) = validate_project_id(project_id) {
        tracing::warn!(%project_id, "Invalid project_id in delete file request");
        return ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            format!("Invalid project_id: {}", e),
        ));
    }

    let group_id = resolve_group_id(&state, project_id).await;

    // Step 1: Remove from Qdrant
    let vectors_deleted = 0;
    if let Some(ref qdrant) = state.qdrant {
        if let Some(ref gid) = group_id {
            let result = qdrant
                .delete_by_file_path_scoped(&file_path, gid, None)
                .await;
            match result {
                Ok(()) => {
                    tracing::info!(file = %file_path, %project_id, "Deleted vectors from Qdrant");
                }
                Err(e) => {
                    tracing::error!(file = %file_path, error = %e, "Failed to delete vectors from Qdrant");
                    return ApiResult::Error(ErrorResponse::with_details(
                        error_codes::STORAGE_ERROR,
                        format!("Failed to delete vectors: {}", e),
                        file_path,
                    ));
                }
            }
        }
    }

    // Step 2: Remove from BM25
    let mut bm25_deleted = 0;
    if let Some(ref bm25) = state.bm25 {
        let mut client = bm25.lock().await;
        let result = client
            .delete_by_file_path_scoped("default", &file_path, project_id)
            .await
            .map(|_| ());
        match result {
            Ok(()) => {
                bm25_deleted = 1;
                tracing::info!(file = %file_path, %project_id, "Deleted documents from BM25");
            }
            Err(e) => {
                tracing::error!(file = %file_path, error = %e, "Failed to delete from BM25");
                return ApiResult::Error(ErrorResponse::with_details(
                    error_codes::STORAGE_ERROR,
                    format!("Failed to delete from BM25: {}", e),
                    file_path,
                ));
            }
        }
    }

    // Step 3: Remove relations from relation index
    let relations_deleted;
    {
        if let Ok(orchestrator) = state.engine.get_orchestrator(project_id).await {
            let mut orchestrator = orchestrator.lock().await;
            relations_deleted = orchestrator.clear_relations_for_file(&file_path);
            tracing::info!(file = %file_path, relations = relations_deleted, "Deleted relations");
        } else {
            relations_deleted = 0;
        }
        if let Err(e) = state
            .engine
            .publish_relation_snapshot_from_orchestrator(project_id)
            .await
        {
            tracing::warn!(
                project_id,
                error = %e,
                "Failed to publish relation snapshot after file deletion"
            );
        }
    }

    // Step 4: Remove entity detail mappings and file summary mappings from SQLite
    if let Some(client) = state.metadata_store.as_deref()
        && let Ok(project) = client.for_project(project_id)
    {
        let file_id_opt = match project.with_transaction(|tx| {
            use rusqlite::{OptionalExtension, params};
            tx.query_row(
                "SELECT id FROM files WHERE path = ?1 AND project_id = ?2",
                params![&file_path, project_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|e| cce_types::StorageError::Sqlite(e.to_string()))
        }) {
            Ok(id_opt) => id_opt,
            Err(e) => {
                tracing::error!(file = %file_path, error = %e, "Failed to query file_id");
                return ApiResult::Error(ErrorResponse::with_details(
                    error_codes::STORAGE_ERROR,
                    format!("Failed to query file: {}", e),
                    file_path,
                ));
            }
        };

        if let Some(file_id_num) = file_id_opt {
            match client.with_transaction(|tx| {
                use cce_storage_sqlite::EntityDetailMappingRepository;
                EntityDetailMappingRepository::delete_by_file_id(tx, file_id_num)?;
                Ok(())
            }) {
                Ok(_) => {
                    tracing::debug!(file = %file_path, "Deleted entity mappings");
                }
                Err(e) => {
                    tracing::error!(file = %file_path, error = %e, "Failed to delete entity mappings");
                }
            }
        }
    }

    let response = DeleteFileResponse {
        success: true,
        message: format!("File deleted successfully: {}", file_path),
        file_path,
        vectors_deleted,
        bm25_documents_deleted: bm25_deleted,
        relations_deleted,
        elapsed_ms: start.elapsed().as_millis() as u64,
    };

    ApiResult::Success(response)
}

/// Handle delete entity request
///
/// Removes a specific entity and all its associated data
/// across all storage backends, scoped to the specified project.
#[utoipa::path(
    delete, path = "/api/index/entity/{id}", tag = "Storage",
    params(StorageQuery, ("id" = u64, Path, description = "Entity id")),
    responses(
        (status = 200, body = DeleteEntityResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_delete_entity(
    State(state): State<crate::api::state::AppState>,
    Path(entity_id): Path<u64>,
    Query(query): Query<StorageQuery>,
) -> ApiResult<DeleteEntityResponse> {
    let start = std::time::Instant::now();
    let project_id = query.project_id;

    if let Err(e) = validate_project_id(project_id) {
        tracing::warn!(%project_id, "Invalid project_id in delete entity request");
        return ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            format!("Invalid project_id: {}", e),
        ));
    }

    let group_id = resolve_group_id(&state, project_id).await;

    // Step 1: Get entity info from relation index to find file path
    let entity_file_path = {
        if let Ok(orchestrator) = state.engine.get_orchestrator(project_id).await {
            let orchestrator = orchestrator.lock().await;
            if let Some(builder) = orchestrator.get_relation_builder() {
                builder
                    .index()
                    .get_file_path_by_entity(cce_types::EntityId(entity_id))
            } else {
                None
            }
        } else {
            None
        }
    };

    // Step 2: Remove from Qdrant
    let mut vectors_deleted = 0;
    if let Some(ref qdrant) = state.qdrant {
        if let Some(ref file_path) = entity_file_path {
            if let Some(ref gid) = group_id {
                let result = qdrant
                    .delete_by_file_path_scoped(file_path, gid, None)
                    .await;
                if result.is_ok() {
                    vectors_deleted = 1;
                }
            }
        }
    }

    // Step 3: Remove from BM25
    let mut bm25_deleted = 0;
    if let Some(ref bm25) = state.bm25 {
        if let Some(ref file_path) = entity_file_path {
            let mut client = bm25.lock().await;
            let result = client
                .delete_by_file_path_scoped("default", file_path, project_id)
                .await
                .map(|_| ());
            if result.is_ok() {
                bm25_deleted = 1;
            }
        }
    }

    // Step 4: Remove relations involving this entity
    let mut relations_deleted = 0;
    {
        if let Ok(orchestrator) = state.engine.get_orchestrator(project_id).await {
            let mut orchestrator = orchestrator.lock().await;
            if let Some(ref builder) = orchestrator.get_relation_builder_mut() {
                let index = builder.index();

                if let Some(ref file_path) = entity_file_path {
                    index.remove_file(file_path);
                    relations_deleted = 1;
                } else {
                    tracing::warn!(
                        entity_id = entity_id,
                        "Cannot delete entity without file path"
                    );
                }
            }
        }
        if let Err(e) = state
            .engine
            .publish_relation_snapshot_from_orchestrator(project_id)
            .await
        {
            tracing::warn!(
                project_id,
                error = %e,
                "Failed to publish relation snapshot after entity deletion"
            );
        }
    }

    // Step 5: Remove entity detail mapping from SQLite
    if let Some(client) = state.metadata_store.as_deref()
        && let Ok(project) = client.for_project(project_id)
    {
        let _ = project.with_transaction(|tx| {
            use cce_storage_sqlite::EntityDetailMappingRepository;
            EntityDetailMappingRepository::delete_by_entity_id(tx, entity_id as i64, project_id)
        });
    }

    let response = DeleteEntityResponse {
        success: true,
        message: format!("Entity deleted successfully: {}", entity_id),
        entity_id,
        vectors_deleted,
        bm25_documents_deleted: bm25_deleted,
        relations_deleted,
        elapsed_ms: start.elapsed().as_millis() as u64,
    };

    ApiResult::Success(response)
}

/// Handle batch delete request
///
/// Batch deletes files and entities from all storage backends,
/// scoped to the specified project.
#[utoipa::path(
    delete, path = "/api/index/batch", tag = "Storage",
    params(StorageQuery),
    request_body = BatchDeleteRequest,
    responses(
        (status = 200, body = BatchDeleteResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_batch_delete(
    State(state): State<crate::api::state::AppState>,
    Query(query): Query<StorageQuery>,
    Json(request): Json<BatchDeleteRequest>,
) -> ApiResult<BatchDeleteResponse> {
    let start = std::time::Instant::now();
    let project_id = query.project_id;

    if let Err(e) = validate_project_id(project_id) {
        tracing::warn!(%project_id, "Invalid project_id in batch delete request");
        return ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            format!("Invalid project_id: {}", e),
        ));
    }

    let mut errors: Vec<String> = Vec::new();
    let mut files_deleted = 0;
    let mut entities_deleted = 0;

    let group_id = resolve_group_id(&state, project_id).await;

    // Delete files
    for file_path in &request.file_paths {
        // Delete from Qdrant
        if let Some(ref qdrant) = state.qdrant {
            if let Some(ref gid) = group_id {
                let result = qdrant
                    .delete_by_file_path_scoped(file_path, gid, None)
                    .await;
                if let Err(e) = result {
                    errors.push(format!(
                        "Failed to delete file {} from Qdrant: {}",
                        file_path, e
                    ));
                    continue;
                }
            }
        }

        // Delete from BM25
        if let Some(ref bm25) = state.bm25 {
            let mut client = bm25.lock().await;
            let result = client
                .delete_by_file_path_scoped("default", file_path, project_id)
                .await
                .map(|_| ());
            if let Err(e) = result {
                errors.push(format!(
                    "Failed to delete file {} from BM25: {}",
                    file_path, e
                ));
                continue;
            }
        }

        {
            if let Ok(orchestrator) = state.engine.get_orchestrator(project_id).await {
                let mut orchestrator = orchestrator.lock().await;
                let _ = orchestrator.clear_relations_for_file(file_path);
            }
        }

        files_deleted += 1;
    }

    // Delete entities
    for entity_id in &request.entity_ids {
        let eid = cce_types::EntityId(*entity_id);

        let file_path = {
            if let Ok(orchestrator) = state.engine.get_orchestrator(project_id).await {
                let orchestrator = orchestrator.lock().await;
                if let Some(builder) = orchestrator.get_relation_builder() {
                    builder.index().get_file_path_by_entity(eid)
                } else {
                    None
                }
            } else {
                None
            }
        };

        if let Some(ref fp) = file_path {
            if let Some(ref qdrant) = state.qdrant {
                if let Some(ref gid) = group_id {
                    let result = qdrant.delete_by_file_path_scoped(fp, gid, None).await;
                    if let Err(e) = result {
                        errors.push(format!(
                            "Failed to delete entity {} from Qdrant: {}",
                            entity_id, e
                        ));
                        continue;
                    }
                }
            }

            if let Some(ref bm25) = state.bm25 {
                let mut client = bm25.lock().await;
                let result = client
                    .delete_by_file_path_scoped("default", fp, project_id)
                    .await
                    .map(|_| ());
                if let Err(e) = result {
                    errors.push(format!(
                        "Failed to delete entity {} from BM25: {}",
                        entity_id, e
                    ));
                    continue;
                }
            }
        }

        {
            if let Ok(orchestrator) = state.engine.get_orchestrator(project_id).await {
                let mut orchestrator = orchestrator.lock().await;
                if let Some(ref builder) = orchestrator.get_relation_builder_mut() {
                    let index = builder.index();
                    if let Some(file_path) = index.get_file_path_by_entity(eid) {
                        index.remove_file(&file_path);
                    } else {
                        tracing::warn!(
                            entity_id = entity_id,
                            "Cannot delete entity without file path in batch"
                        );
                    }
                }
            }
        }

        entities_deleted += 1;
    }

    // Publish relation snapshot after all deletions
    if !request.file_paths.is_empty() || !request.entity_ids.is_empty() {
        if let Err(e) = state
            .engine
            .publish_relation_snapshot_from_orchestrator(project_id)
            .await
        {
            errors.push(format!("Failed to publish relation snapshot: {}", e));
        }
    }

    let response = BatchDeleteResponse {
        success: errors.is_empty(),
        files_deleted,
        entities_deleted,
        errors,
        elapsed_ms: start.elapsed().as_millis() as u64,
    };

    ApiResult::Success(response)
}

// ============================================================================
// Statistics & Status
// ============================================================================

/// Handle index stats request
#[utoipa::path(
    get, path = "/api/index/stats", tag = "Storage",
    params(StorageQuery),
    responses(
        (status = 200, body = IndexStatsResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_index_stats(
    State(state): State<crate::api::state::AppState>,
    Query(query): Query<StorageQuery>,
) -> ApiResult<IndexStatsResponse> {
    let start = std::time::Instant::now();
    let project_id = query.project_id;

    // Get statistics from relation index
    let (total_entities, total_relations) = {
        if let Ok(orchestrator) = state.engine.get_orchestrator(project_id).await {
            let orchestrator = orchestrator.lock().await;
            if let Some(builder) = orchestrator.get_relation_builder() {
                let index = builder.index();
                (index.function_count(), index.resolved_relation_count())
            } else {
                (0, 0)
            }
        } else {
            (0, 0)
        }
    };

    // Resolve Qdrant group ID
    let group_id = resolve_group_id(&state, project_id).await;

    // Get Qdrant stats (project-scoped via group filter)
    let vector_count = if let (Some(qdrant), Some(gid)) = (&state.qdrant, &group_id) {
        qdrant.count_points_by_group(gid).await.unwrap_or(0)
    } else {
        0
    };

    // Get BM25 stats (project-scoped)
    let bm25_doc_count = if let Some(ref bm25) = state.bm25 {
        let client = bm25.lock().await;
        client
            .document_count_by_project(project_id)
            .await
            .unwrap_or(0)
    } else {
        0
    };

    // Get file count from metadata store (project-scoped)
    let file_count = if let Some(client) = state.metadata_store.as_deref()
        && let Ok(project) = client.for_project(project_id)
    {
        use cce_storage_sqlite::FileRepository;
        match project.with_transaction(|tx| FileRepository::count_by_project(tx, project_id)) {
            Ok(count) => count as usize,
            Err(_) => 0,
        }
    } else {
        0
    };

    let response = IndexStatsResponse {
        success: true,
        statistics: IndexStatistics {
            total_entities,
            total_relations,
            total_vectors: vector_count,
            total_bm25_documents: bm25_doc_count,
            total_files: file_count,
        },
        elapsed_ms: start.elapsed().as_millis() as u64,
    };

    ApiResult::Success(response)
}

/// Handle storage status request
///
/// Checks all storage components and returns their status.
/// Uses Qdrant's diagnose() method for comprehensive health assessment.
#[utoipa::path(
    get, path = "/api/storage/status", tag = "Storage",
    responses(
        (status = 200, body = StorageStatusResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_storage_status(
    State(state): State<crate::api::state::AppState>,
) -> ApiResult<StorageStatusResponse> {
    let mut vector_storage = StorageComponentStatus {
        connected: false,
        item_count: 0,
        disk_usage_mb: 0.0,
        version: None,
        last_error: None,
    };

    let mut bm25_storage = StorageComponentStatus {
        connected: false,
        item_count: 0,
        disk_usage_mb: 0.0,
        version: None,
        last_error: None,
    };

    // Check Qdrant with comprehensive diagnostics
    if let Some(ref qdrant) = state.qdrant {
        match qdrant.diagnose().await {
            Ok(diag) => {
                vector_storage = StorageComponentStatus {
                    connected: diag.reachable,
                    item_count: diag.points_count as usize,
                    disk_usage_mb: 0.0, // Qdrant does not expose disk usage via REST API
                    version: diag.version,
                    last_error: diag.error,
                };
            }
            Err(e) => {
                vector_storage = StorageComponentStatus {
                    connected: false,
                    item_count: 0,
                    disk_usage_mb: 0.0,
                    version: None,
                    last_error: Some(format!("Diagnostic failed: {}", e)),
                };
            }
        }
    }

    // Check BM25
    if let Some(ref bm25) = state.bm25 {
        let client = bm25.lock().await;
        let item_count = client.document_count().await.unwrap_or(0);
        bm25_storage = StorageComponentStatus {
            connected: client.is_connected(),
            item_count,
            disk_usage_mb: 0.0,
            version: None,
            last_error: None,
        };
        drop(client);
    }

    // Get relation storage stats (cached from all loaded runtimes)
    let relation_item_count = 0;

    let relation_storage = StorageComponentStatus {
        connected: true,
        item_count: relation_item_count,
        disk_usage_mb: 0.0,
        version: None,
        last_error: None,
    };

    // Get Qdrant process info if subprocess management is available
    let process_status = if let Some(handle) = state.qdrant_control.as_ref() {
        let status = handle.current_status().await;
        Some(QdrantProcessInfo {
            managed: handle.managed,
            running: matches!(status, QdrantProcessStatus::Running),
            status,
        })
    } else {
        state.qdrant.as_ref().map(|qdrant| {
            let config = qdrant.config();
            QdrantProcessInfo {
                managed: config.auto_start,
                status: if vector_storage.connected {
                    QdrantProcessStatus::Running
                } else {
                    QdrantProcessStatus::Stopped
                },
                running: vector_storage.connected,
            }
        })
    };

    // Calculate total disk usage (approximate)
    let total_disk_usage_mb =
        vector_storage.disk_usage_mb + bm25_storage.disk_usage_mb + relation_storage.disk_usage_mb;

    let status = StorageStatus {
        vector_storage,
        bm25_storage,
        relation_storage,
        total_disk_usage_mb,
        process_status,
    };

    let response = StorageStatusResponse {
        success: true,
        status,
    };

    ApiResult::Success(response)
}

/// Resolve the Qdrant group_id from a project_id using the project registry.
///
/// Returns `None` if the project is not found or has no root_path configured.
async fn resolve_group_id(state: &crate::api::state::AppState, project_id: i64) -> Option<String> {
    let registry = state.project_registry.as_ref()?;
    match registry.get_or_load(project_id).await {
        Ok(entry) => {
            let gid = cce_storage_qdrant::generate_project_group_id(
                project_id,
                &entry.metadata.root_path,
            );
            Some(gid)
        }
        Err(e) => {
            tracing::warn!(
                project_id,
                error = %e,
                "Failed to resolve project for scoped deletion, falling back to unscoped"
            );
            None
        }
    }
}
