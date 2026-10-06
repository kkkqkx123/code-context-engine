//! Remote ingest entries for gateway-driven indexing.
//!
//! The gateway pushes manifests and raw file payloads; the server stages
//! them under the registered project root and then runs the existing full
//! and incremental pipelines. Encoding detection still runs inside those
//! pipelines, and project isolation filtering is untouched. These routes only
//! exist in admission-enabled builds and always sit behind the admission
//! layer. They carry no OpenAPI annotations yet because the push protocol
//! shape is still stabilizing.

use std::path::{Component, Path, PathBuf};

use axum::Router;
use axum::extract::{Extension, Path as AxumPath, State};
use axum::routing::{get, post};
use base64::Engine as _;
use cce_api::models::{
    ErrorResponse, IngestBatchRequest, IngestBatchResponse, IngestCommitResponse, IngestEventKind,
    IngestEventRequest, IngestEventResponse, IngestManifestRequest, IngestManifestResponse,
    MAX_INGEST_BATCH_FILES, MAX_INGEST_FILE_BYTES, error_codes,
};
use cce_storage_sqlite::{FileRepository, ProjectRepository, ProjectUpdateRecord};

use super::project::management::record_to_config;
use crate::api::response::ApiResult;
use crate::api::state::AppState;

/// Routes served only when the admission feature is enabled.
pub fn ingest_routes(metrics: std::sync::Arc<cce_admission::AdmissionMetrics>) -> Router<AppState> {
    Router::new()
        .route(
            "/api/project/{id}/ingest/manifest",
            post(handle_ingest_manifest),
        )
        .route("/api/project/{id}/ingest/batch", post(handle_ingest_batch))
        .route(
            "/api/project/{id}/ingest/commit",
            post(handle_ingest_commit),
        )
        .route("/api/project/{id}/ingest/event", post(handle_ingest_event))
        .route("/api/admission/stats", get(handle_admission_stats))
        .layer(axum::Extension(metrics))
}

/// Resolve the registered root directory of a project.
fn project_root(state: &AppState, project_id: i64) -> Result<PathBuf, ErrorResponse> {
    let store = state.engine.metadata_store().ok_or_else(|| {
        ErrorResponse::new(error_codes::STORAGE_ERROR, "Metadata store not initialized")
    })?;
    let record = store
        .as_ref()
        .with_transaction(|tx| ProjectRepository::get_by_id(tx, project_id))
        .map_err(|e| {
            ErrorResponse::new(
                error_codes::STORAGE_ERROR,
                format!("Failed to query project: {e}"),
            )
        })?
        .ok_or_else(|| {
            ErrorResponse::new(error_codes::ENTITY_NOT_FOUND, "Project does not exist")
        })?;
    Ok(PathBuf::from(record.root_path))
}

/// Join a gateway relative path onto the project root.
///
/// Absolute paths and parent components are rejected so a pushed manifest
/// can never escape the project mirror.
fn safe_join(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let normalized = relative.replace('\\', "/");
    if normalized.trim().is_empty() {
        return Err("relative path must not be empty".to_string());
    }
    let candidate = Path::new(&normalized);
    if candidate.is_absolute() {
        return Err(format!("absolute paths are not accepted: {relative}"));
    }
    for component in candidate.components() {
        if !matches!(component, Component::Normal(_)) {
            return Err(format!("path escapes the project root: {relative}"));
        }
    }
    let joined = root.join(candidate);
    if !joined.starts_with(root) {
        return Err(format!("path escapes the project root: {relative}"));
    }
    Ok(joined)
}

/// Canonical storage identity shared with the local scan pipeline.
fn storage_path(relative: &str) -> String {
    cce_types::path::normalize_project_path(&relative.replace('\\', "/"))
}

/// Compare a gateway manifest against the stored file hashes.
///
/// Returns the relative paths whose content the server still needs.
pub async fn handle_ingest_manifest(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<i64>,
    axum::Json(request): axum::Json<IngestManifestRequest>,
) -> ApiResult<IngestManifestResponse> {
    let store = match state.engine.metadata_store() {
        Some(store) => store,
        None => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::STORAGE_ERROR,
                "Metadata store not initialized",
            ));
        }
    };
    let conn = match store.as_ref().read_connection() {
        Ok(conn) => conn,
        Err(e) => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::STORAGE_ERROR,
                format!("Failed to open metadata connection: {e}"),
            ));
        }
    };
    let mut upload = Vec::new();
    let mut unchanged = 0usize;
    for meta in &request.files {
        let path = storage_path(&meta.relative_path);
        let stored = match FileRepository::get_by_path_and_project(&conn, &path, id) {
            Ok(record) => record,
            Err(e) => {
                return ApiResult::Error(ErrorResponse::new(
                    error_codes::STORAGE_ERROR,
                    format!("Failed to compare manifest entry {path}: {e}"),
                ));
            }
        };
        let fresh = stored.as_ref().is_some_and(|record| {
            record.content_hash.as_deref() == meta.content_hash.as_deref()
                && meta.content_hash.is_some()
        });
        if fresh {
            unchanged += 1;
        } else {
            upload.push(path);
        }
    }
    upload.sort();
    ApiResult::Success(IngestManifestResponse {
        success: true,
        project_id: id,
        upload,
        unchanged,
    })
}

/// Stage one decoded payload under the project mirror.
async fn stage_bytes(
    root: &Path,
    relative: &str,
    bytes: &[u8],
    expected_hash: Option<&str>,
) -> Result<(), String> {
    if bytes.len() as u64 > MAX_INGEST_FILE_BYTES {
        return Err(format!(
            "{relative} exceeds the {MAX_INGEST_FILE_BYTES} byte ingest bound"
        ));
    }
    if let Some(expected) = expected_hash {
        let actual = cce_utils::hash::calculate_hash(bytes);
        if actual != expected {
            return Err(format!(
                "{relative} changed between manifest and upload; a fresh manifest is required"
            ));
        }
    }
    let dest = safe_join(root, relative)?;
    if let Some(parent) = dest.parent()
        && let Err(e) = tokio::fs::create_dir_all(parent).await
    {
        return Err(format!("failed to create parent of {relative}: {e}"));
    }
    tokio::fs::write(&dest, bytes)
        .await
        .map_err(|e| format!("failed to stage {relative}: {e}"))
}

/// Store pushed file contents under the project mirror without indexing.
///
/// Indexing happens explicitly through the commit entry or incrementally
/// through the event entry, so large initial syncs can stage in batches.
pub async fn handle_ingest_batch(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<i64>,
    axum::Json(request): axum::Json<IngestBatchRequest>,
) -> ApiResult<IngestBatchResponse> {
    if request.files.len() > MAX_INGEST_BATCH_FILES {
        return ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            format!(
                "batch holds {} files, more than the {MAX_INGEST_BATCH_FILES} file bound",
                request.files.len()
            ),
        ));
    }
    let root = match project_root(&state, id) {
        Ok(root) => root,
        Err(error) => return ApiResult::Error(error),
    };
    let mut staged = 0usize;
    let mut errors = Vec::new();
    for file in &request.files {
        let bytes = match base64::engine::general_purpose::STANDARD.decode(&file.content_base64) {
            Ok(bytes) => bytes,
            Err(e) => {
                errors.push(format!(
                    "{}: content is not valid base64: {e}",
                    file.relative_path
                ));
                continue;
            }
        };
        match stage_bytes(
            &root,
            &file.relative_path,
            &bytes,
            file.content_hash.as_deref(),
        )
        .await
        {
            Ok(()) => staged += 1,
            Err(reason) => errors.push(reason),
        }
    }
    ApiResult::Success(IngestBatchResponse {
        success: errors.is_empty(),
        project_id: id,
        staged,
        errors,
    })
}

/// Run the existing full index pipeline over the staged project mirror.
pub async fn handle_ingest_commit(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<i64>,
) -> ApiResult<IngestCommitResponse> {
    let store = match state.engine.metadata_store() {
        Some(store) => store,
        None => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::STORAGE_ERROR,
                "Metadata store not initialized",
            ));
        }
    };
    let record = match store
        .as_ref()
        .with_transaction(|tx| ProjectRepository::get_by_id(tx, id))
    {
        Ok(Some(record)) => record,
        Ok(None) => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::ENTITY_NOT_FOUND,
                "Project does not exist",
            ));
        }
        Err(e) => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::STORAGE_ERROR,
                format!("Failed to query project: {e}"),
            ));
        }
    };
    let config = record_to_config(&record);
    let index_options = cce_orchestrator::IndexOptions::new(&config.root_path)
        .with_extensions(config.extensions)
        .with_exclude_dirs(config.exclude_dirs)
        .with_gitignore(config.respect_gitignore)
        .with_ignore_patterns(config.ignore_patterns);
    let result = match state.engine.index(id, index_options).await {
        Ok(result) => result,
        Err(e) => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                format!("Index execution failed: {e}"),
            ));
        }
    };
    let now = chrono::Utc::now().to_rfc3339();
    let updates = ProjectUpdateRecord::default().with_last_indexed(now);
    if let Err(e) = store
        .as_ref()
        .with_transaction(|tx| ProjectRepository::update(tx, id, &updates))
    {
        tracing::warn!("Failed to update last_indexed: {e}");
    }
    ApiResult::Success(IngestCommitResponse {
        success: result.is_success(),
        project_id: id,
        project_name: record.name,
        indexed_files: result.indexed_files,
        total_entities: result.total_entities,
        total_vectors: result.total_vectors,
        elapsed_ms: result.elapsed_ms,
    })
}

/// Apply gateway-observed changes through the hot-update pipeline.
///
/// Created and modified events carry content and are staged before the
/// coordinator runs; deletions remove the mirrored file and its index rows.
/// The remote host never starts its own filesystem watch in this shape:
/// every change arrives through this entry.
pub async fn handle_ingest_event(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<i64>,
    axum::Json(request): axum::Json<IngestEventRequest>,
) -> ApiResult<IngestEventResponse> {
    if request.events.len() > MAX_INGEST_BATCH_FILES {
        return ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            format!(
                "event holds {} entries, more than the {MAX_INGEST_BATCH_FILES} entry bound",
                request.events.len()
            ),
        ));
    }
    let root = match project_root(&state, id) {
        Ok(root) => root,
        Err(error) => return ApiResult::Error(error),
    };
    let mut changes: Vec<(PathBuf, bool)> = Vec::with_capacity(request.events.len());
    let mut errors = Vec::new();
    let mut removed = 0usize;
    for event in &request.events {
        match event.kind {
            IngestEventKind::Created | IngestEventKind::Modified => {
                let Some(encoded) = event.content_base64.as_deref() else {
                    errors.push(format!(
                        "{}: content is required for created and modified events",
                        event.relative_path
                    ));
                    continue;
                };
                let bytes = match base64::engine::general_purpose::STANDARD.decode(encoded) {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        errors.push(format!(
                            "{}: content is not valid base64: {e}",
                            event.relative_path
                        ));
                        continue;
                    }
                };
                match stage_bytes(
                    &root,
                    &event.relative_path,
                    &bytes,
                    event.content_hash.as_deref(),
                )
                .await
                {
                    Ok(()) => match safe_join(&root, &event.relative_path) {
                        Ok(dest) => changes.push((dest, false)),
                        Err(reason) => errors.push(reason),
                    },
                    Err(reason) => errors.push(reason),
                }
            }
            IngestEventKind::Deleted => match safe_join(&root, &event.relative_path) {
                Ok(dest) => {
                    match tokio::fs::remove_file(&dest).await {
                        Ok(()) => {}
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                        Err(e) => {
                            errors.push(format!(
                                "{}: failed to remove mirrored file: {e}",
                                event.relative_path
                            ));
                            continue;
                        }
                    }
                    removed += 1;
                    changes.push((dest, true));
                }
                Err(reason) => errors.push(reason),
            },
        }
    }
    let applied = changes.iter().filter(|(_, deleted)| !deleted).count();
    match state.engine.get_hot_update_coordinator(id).await {
        Ok(coordinator) => {
            let coordinator = coordinator.lock().await;
            if let Err(e) = coordinator.run_explicit_changes(changes).await {
                errors.push(e.to_string());
            }
        }
        Err(e) => errors.push(format!("failed to initialize hot update: {e}")),
    }
    ApiResult::Success(IngestEventResponse {
        success: errors.is_empty(),
        project_id: id,
        applied,
        removed,
        errors,
    })
}

/// Render admission counters for rejection auditing.
pub async fn handle_admission_stats(
    Extension(metrics): Extension<std::sync::Arc<cce_admission::AdmissionMetrics>>,
) -> axum::Json<cce_admission::AdmissionStats> {
    axum::Json(metrics.snapshot())
}
