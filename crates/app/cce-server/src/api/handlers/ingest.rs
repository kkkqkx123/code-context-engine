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
    ErrorResponse, INGEST_CHUNK_BYTES, IngestBatchRequest, IngestBatchResponse,
    IngestCommitResponse, IngestEventKind, IngestEventRequest, IngestEventResponse,
    IngestManifestRequest, IngestManifestResponse, MAX_INGEST_BATCH_FILES, MAX_INGEST_FILE_BYTES,
    MissingChunk, error_codes, total_chunks_for_size,
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

/// Root directory holding received chunks for resume, outside the project
/// mirror so the scanner never indexes staging artifacts.
fn staging_root(project_id: i64, manifest_version: u64) -> PathBuf {
    std::env::temp_dir()
        .join("cce-ingest")
        .join(format!("project-{project_id}"))
        .join(format!("manifest-{manifest_version}"))
}

/// Stable directory name for one relative path inside the staging root.
fn staging_file_dir(root: &Path, relative: &str) -> PathBuf {
    let digest = cce_utils::hash::calculate_hash(relative.as_bytes());
    root.join(digest)
}

/// Indices already stored for one file in the given manifest version.
fn received_chunk_indices(project_id: i64, manifest_version: u64, relative: &str) -> Vec<u32> {
    if manifest_version == 0 {
        return Vec::new();
    }
    let dir = staging_file_dir(&staging_root(project_id, manifest_version), relative);
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(_) => return out,
    };
    for entry in entries.flatten() {
        if let Some(name) = entry.file_name().to_str()
            && let Some(index) = name.strip_prefix("chunk_")
            && let Ok(index) = index.parse::<u32>()
        {
            out.push(index);
        }
    }
    out.sort_unstable();
    out
}

/// Decode one chunk payload, decompressing when negotiated, and verify the
/// per-chunk hash of the uncompressed bytes. Corrupted pieces are rejected
/// without touching previously stored chunks.
fn decode_chunk_payload(
    relative: &str,
    encoded: &str,
    compressed: bool,
    expected_chunk_hash: Option<&str>,
) -> Result<Vec<u8>, String> {
    let raw = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|e| format!("{relative}: content is not valid base64: {e}"))?;
    let bytes = if compressed {
        zstd::decode_all(raw.as_slice())
            .map_err(|e| format!("{relative}: chunk decompression failed: {e}"))?
    } else {
        raw
    };
    if bytes.len() > INGEST_CHUNK_BYTES + 1024 {
        return Err(format!(
            "{relative}: chunk exceeds the {INGEST_CHUNK_BYTES} byte bound"
        ));
    }
    if let Some(expected) = expected_chunk_hash {
        let actual = cce_utils::hash::calculate_hash(&bytes);
        if actual != expected {
            return Err(format!(
                "{relative}: chunk hash mismatch; a fresh upload of the piece is required"
            ));
        }
    }
    Ok(bytes)
}

/// Compare a gateway manifest against the stored file hashes.
///
/// Fresh files are skipped. For stale files the comparison descends to
/// chunk granularity when the request carries a manifest version: chunks
/// already stored under the staging root are not requested again, so an
/// interrupted push resumes without retransmission.
#[utoipa::path(
    post, path = "/api/project/{id}/ingest/manifest", tag = "Ingest",
    params(("id" = i64, Path, description = "Project id")),
    request_body = IngestManifestRequest,
    responses(
        (status = 200, body = IngestManifestResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
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
    let mut missing_chunks = Vec::new();
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
            continue;
        }
        if request.manifest_version == 0 {
            upload.push(path);
            continue;
        }
        let total = total_chunks_for_size(meta.size.max(1));
        let received = received_chunk_indices(id, request.manifest_version, &path);
        if received.len() as u32 >= total {
            continue;
        }
        if received.is_empty() {
            upload.push(path);
            continue;
        }
        let received_set: std::collections::HashSet<u32> = received.into_iter().collect();
        for chunk_index in 0..total {
            if !received_set.contains(&chunk_index) {
                missing_chunks.push(MissingChunk {
                    relative_path: path.clone(),
                    chunk_index,
                    total_chunks: total,
                });
            }
        }
    }
    upload.sort();
    missing_chunks.sort_by(|left, right| {
        left.relative_path
            .cmp(&right.relative_path)
            .then(left.chunk_index.cmp(&right.chunk_index))
    });
    if let Some(store) = state.engine.metadata_store() {
        let _ = store.as_ref().with_transaction(|tx| {
            ProjectRepository::meta_set_string(
                tx,
                id,
                cce_api::models::SUPPLY_MODE_KEY,
                cce_api::models::SUPPLY_MODE_GATEWAY,
            )
        });
    }
    ApiResult::Success(IngestManifestResponse {
        success: true,
        project_id: id,
        manifest_version: request.manifest_version,
        upload,
        missing_chunks,
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
/// Batches exceeding the token byte quota are rejected as a whole with
/// usage details; no partial staging occurs in that case.
#[utoipa::path(
    post, path = "/api/project/{id}/ingest/batch", tag = "Ingest",
    params(("id" = i64, Path, description = "Project id")),
    request_body = IngestBatchRequest,
    responses(
        (status = 200, body = IngestBatchResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_ingest_batch(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<i64>,
    context: Option<Extension<cce_admission::AdmissionContext>>,
    metrics: Extension<std::sync::Arc<cce_admission::AdmissionMetrics>>,
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
    if let Some(Extension(context)) = context.as_ref().map(|Extension(c)| Extension(c.clone()))
        && let Some(quota) = context.quota_bytes
    {
        let mut batch_bytes: u64 = 0;
        for file in &request.files {
            let raw_len = file.content_base64.len() as u64 * 3 / 4;
            batch_bytes = batch_bytes.saturating_add(raw_len);
        }
        if let Some(store) = state.engine.metadata_store()
            && let Ok(used) = store.as_ref().with_transaction(|tx| {
                cce_storage_sqlite::AdmissionAuditRepository::get(tx, &context.fingerprint)
                    .map(|record| record.map_or(0, |r| r.bytes_used.max(0) as u64))
            })
            && used.saturating_add(batch_bytes) > quota
        {
            metrics.record_quota_rejection();
            if let Some(store) = state.engine.metadata_store() {
                let _ = store.as_ref().with_transaction(|tx| {
                    cce_storage_sqlite::AdmissionAuditRepository::record_rejection(
                        tx,
                        &context.fingerprint,
                        &context.projects,
                        context.quota_bytes,
                        "quota",
                    )
                });
            }
            return ApiResult::Error(ErrorResponse::new(
                error_codes::INVALID_REQUEST,
                format!(
                    "batch exceeds the token quota: used {used} bytes, batch ~{batch_bytes} bytes, quota {quota} bytes"
                ),
            ));
        }
    }
    let mut staged = 0usize;
    let mut staged_chunks = 0usize;
    let mut errors = Vec::new();
    for file in &request.files {
        if request.manifest_version == 0 && file.total_chunks <= 1 {
            let bytes = match base64::engine::general_purpose::STANDARD.decode(&file.content_base64)
            {
                Ok(bytes) => bytes,
                Err(e) => {
                    errors.push(format!(
                        "{}: content is not valid base64: {e}",
                        file.relative_path
                    ));
                    continue;
                }
            };
            let bytes = if file.compressed {
                match zstd::decode_all(bytes.as_slice()) {
                    Ok(decoded) => decoded,
                    Err(e) => {
                        errors.push(format!(
                            "{}: chunk decompression failed: {e}",
                            file.relative_path
                        ));
                        continue;
                    }
                }
            } else {
                bytes
            };
            if let Some(expected) = file.chunk_hash.as_deref() {
                let actual = cce_utils::hash::calculate_hash(&bytes);
                if actual != expected {
                    errors.push(format!(
                        "{}: chunk hash mismatch; a fresh upload of the piece is required",
                        file.relative_path
                    ));
                    continue;
                }
            }
            match stage_bytes(
                &root,
                &file.relative_path,
                &bytes,
                file.content_hash.as_deref(),
            )
            .await
            {
                Ok(()) => {
                    staged += 1;
                    staged_chunks += 1;
                }
                Err(reason) => errors.push(reason),
            }
            continue;
        }
        match stage_chunk(ChunkStageParams {
            root: &root,
            project_id: id,
            manifest_version: request.manifest_version,
            relative: file.relative_path.as_str(),
            chunk_index: file.chunk_index,
            total_chunks: file.total_chunks,
            encoded: file.content_base64.as_str(),
            compressed: file.compressed,
            chunk_hash: file.chunk_hash.as_deref(),
            content_hash: file.content_hash.as_deref(),
        })
        .await
        {
            Ok(completed) => {
                staged_chunks += 1;
                if completed {
                    staged += 1;
                }
            }
            Err(reason) => errors.push(reason),
        }
    }
    if cce_orchestrator::supply::IngestSupplyMode::from_env().is_direct() {
        for file in &request.files {
            let path = storage_path(&file.relative_path);
            let mirror_path = root.join(Path::new(&path));
            let Ok(mirror_bytes) = std::fs::read(&mirror_path) else {
                continue;
            };
            let payloads = cce_orchestrator::supply::build_ready_payloads(vec![(
                path.clone(),
                mirror_bytes,
                file.content_hash.clone(),
            )]);
            if let Err(reason) =
                cce_orchestrator::supply::verify_direct_batch(&payloads, &root).await
            {
                errors.push(reason);
            }
        }
    }
    if errors.is_empty()
        && let Some(context) = context.map(|Extension(c)| c)
        && let Some(store) = state.engine.metadata_store()
    {
        let batch_bytes: u64 = request
            .files
            .iter()
            .map(|f| f.content_base64.len() as u64 * 3 / 4)
            .sum();
        let _ = store.as_ref().with_transaction(|tx| {
            cce_storage_sqlite::AdmissionAuditRepository::record_admitted(
                tx,
                &context.fingerprint,
                &context.projects,
                context.quota_bytes,
                batch_bytes,
            )
        });
    }
    ApiResult::Success(IngestBatchResponse {
        success: errors.is_empty(),
        project_id: id,
        manifest_version: request.manifest_version,
        staged,
        staged_chunks,
        errors,
    })
}

/// Parameters for staging one file chunk.
struct ChunkStageParams<'a> {
    root: &'a Path,
    project_id: i64,
    manifest_version: u64,
    relative: &'a str,
    chunk_index: u32,
    total_chunks: u32,
    encoded: &'a str,
    compressed: bool,
    chunk_hash: Option<&'a str>,
    content_hash: Option<&'a str>,
}

/// Store one chunk and reassemble the file once every piece arrived.
///
/// Chunk bytes are verified individually before storage; the reassembled
/// file is verified against the whole-file hash before it reaches the
/// mirror, so a corrupted piece can never poison a completed file.
async fn stage_chunk(params: ChunkStageParams<'_>) -> Result<bool, String> {
    let ChunkStageParams {
        root,
        project_id,
        manifest_version,
        relative,
        chunk_index,
        total_chunks,
        encoded,
        compressed,
        chunk_hash,
        content_hash,
    } = params;
    if total_chunks == 0 {
        return Err(format!("{relative}: total chunks must be positive"));
    }
    if chunk_index >= total_chunks {
        return Err(format!(
            "{relative}: chunk {chunk_index} exceeds {total_chunks}"
        ));
    }
    let bytes = decode_chunk_payload(relative, encoded, compressed, chunk_hash)?;
    if manifest_version == 0 {
        stage_bytes(root, relative, &bytes, content_hash).await?;
        return Ok(true);
    }
    let path = storage_path(relative);
    let version_root = staging_root(project_id, manifest_version);
    let file_dir = staging_file_dir(&version_root, &path);
    std::fs::create_dir_all(&file_dir)
        .map_err(|e| format!("{relative}: failed to stage chunk: {e}"))?;
    let chunk_path = file_dir.join(format!("chunk_{chunk_index}"));
    std::fs::write(&chunk_path, &bytes)
        .map_err(|e| format!("{relative}: failed to stage chunk: {e}"))?;
    let mut present = Vec::new();
    for index in 0..total_chunks {
        let candidate = file_dir.join(format!("chunk_{index}"));
        if candidate.exists() {
            present.push(index);
        }
    }
    if present.len() as u32 != total_chunks {
        return Ok(false);
    }
    let mut assembled = Vec::new();
    for index in 0..total_chunks {
        let candidate = file_dir.join(format!("chunk_{index}"));
        let piece = std::fs::read(&candidate)
            .map_err(|e| format!("{relative}: failed to reassemble: {e}"))?;
        assembled.extend_from_slice(&piece);
    }
    stage_bytes(root, &path, &assembled, content_hash).await?;
    Ok(true)
}

/// Run the existing full index pipeline over the staged project mirror.
#[utoipa::path(
    post, path = "/api/project/{id}/ingest/commit", tag = "Ingest",
    params(("id" = i64, Path, description = "Project id")),
    responses(
        (status = 200, body = IngestCommitResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
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
    clear_project_staging(id);
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

/// Remove chunk staging for a project after a successful commit.
fn clear_project_staging(project_id: i64) {
    let root = std::env::temp_dir()
        .join("cce-ingest")
        .join(format!("project-{project_id}"));
    if let Err(e) = std::fs::remove_dir_all(&root)
        && e.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!("Failed to clear ingest staging for project {project_id}: {e}");
    }
}

/// Apply gateway-observed changes through the hot-update pipeline.
///
/// Created and modified events carry content and are staged before the
/// coordinator runs; deletions remove the mirrored file and its index rows.
/// The remote host never starts its own filesystem watch in this shape:
/// every change arrives through this entry.
#[utoipa::path(
    post, path = "/api/project/{id}/ingest/event", tag = "Ingest",
    params(("id" = i64, Path, description = "Project id")),
    request_body = IngestEventRequest,
    responses(
        (status = 200, body = IngestEventResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
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

/// Per-token audit entry for persistent admission observability.
#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct TokenAuditEntry {
    /// Log-safe token fingerprint.
    pub fingerprint: String,
    /// Bound projects as comma separated ids.
    pub projects: String,
    /// Configured byte quota, if any.
    pub quota_bytes: Option<i64>,
    /// Stored bytes accepted.
    pub bytes_used: i64,
    /// Admitted batches.
    pub admitted: i64,
    /// Rejections by cause.
    pub auth_rejections: i64,
    /// Rejections by cause.
    pub scope_rejections: i64,
    /// Rejections by cause.
    pub rate_rejections: i64,
    /// Rejections by cause.
    pub body_rejections: i64,
    /// Rejections for exceeding the byte quota.
    pub quota_rejections: i64,
    /// Last use as seconds since the unix epoch, if any.
    pub last_used: Option<i64>,
    /// Last rejection reason, if any.
    pub last_reject_reason: Option<String>,
}

/// Admission counters with per-token persistent audit.
#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct AdmissionStatsResponse {
    /// Global in-memory counters.
    #[serde(flatten)]
    pub global: cce_admission::AdmissionStats,
    /// Per-token durable audit rows.
    #[serde(default)]
    pub tokens: Vec<TokenAuditEntry>,
}

/// Render admission counters with per-token durable audit for rejection
/// analysis. Usage and last-use time survive restarts through the metadata
/// store; in-memory counters cover the current process.
#[utoipa::path(
    get, path = "/api/admission/stats", tag = "Admission",
    responses(
        (status = 200, body = AdmissionStatsResponse, description = "Success")
    )
)]
pub async fn handle_admission_stats(
    State(state): State<AppState>,
    Extension(metrics): Extension<std::sync::Arc<cce_admission::AdmissionMetrics>>,
) -> axum::Json<AdmissionStatsResponse> {
    let global = metrics.snapshot();
    let mut tokens = Vec::new();
    if let Some(store) = state.engine.metadata_store()
        && let Ok(records) = store
            .as_ref()
            .with_transaction(|tx| cce_storage_sqlite::AdmissionAuditRepository::list_all(tx))
    {
        for record in records {
            tokens.push(TokenAuditEntry {
                fingerprint: record.token_fingerprint,
                projects: record.projects,
                quota_bytes: record.quota_bytes,
                bytes_used: record.bytes_used,
                admitted: record.admitted,
                auth_rejections: record.auth_rejections,
                scope_rejections: record.scope_rejections,
                rate_rejections: record.rate_rejections,
                body_rejections: record.body_rejections,
                quota_rejections: record.quota_rejections,
                last_used: record.last_used,
                last_reject_reason: record.last_reject_reason,
            });
        }
    }
    axum::Json(AdmissionStatsResponse { global, tokens })
}
