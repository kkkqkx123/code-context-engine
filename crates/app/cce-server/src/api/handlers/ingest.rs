//! Remote ingest entries for gateway-driven indexing.
//!
//! The gateway pushes manifests and raw file payloads; the server stages
//! them under the registered project root and then runs the existing full
//! and incremental pipelines. Encoding detection still runs inside those
//! pipelines, and project isolation filtering is untouched. These routes only
//! exist in admission-enabled builds and always sit behind the admission
//! layer. They carry no OpenAPI annotations yet because the push protocol
//! shape is still stabilizing.
//!
//! Chunk staging lives under the system temp directory, keyed by project
//! and manifest version. Every batch, whatever its manifest version, runs
//! the same decode, per-chunk verification, reassembly, and whole-file
//! check; version zero only skips the resume bookkeeping. Staged chunk
//! usage per version is admitted against explicit bounds, and orphan
//! version directories are reclaimed lazily: a sweep at server startup and
//! a sweep in the commit entry before indexing, both driven by directory
//! age with no timer or extra process.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, SystemTime};

use axum::Router;
use axum::extract::{Extension, Path as AxumPath, State};
use axum::routing::{get, post};
use base64::Engine as _;
use cce_api::models::{
    ErrorResponse, INGEST_CHUNK_BYTES, IngestBatchRequest, IngestBatchResponse,
    IngestCommitResponse, IngestEventKind, IngestEventRequest, IngestEventResponse,
    IngestManifestRequest, IngestManifestResponse, IngestedFile, MAX_INGEST_BATCH_FILES,
    MAX_INGEST_FILE_BYTES, MissingChunk, error_codes, total_chunks_for_size,
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

/// Retention for manifest-version chunk staging.
///
/// Days-scale so an interrupted push can resume across a long outage,
/// short enough that abandoned versions do not pile up on the remote host.
pub const INGEST_STAGING_RETENTION: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// Chunks staged for one manifest version before further batch requests
/// for that version are refused.
const MAX_STAGING_CHUNKS_PER_VERSION: u64 = 1_048_576;

/// Bytes staged for one manifest version before further batch requests
/// for that version are refused.
const MAX_STAGING_BYTES_PER_VERSION: u64 = 64 * 1024 * 1024 * 1024;

/// Base directory holding received chunks for resume, outside the project
/// mirror so the scanner never indexes staging artifacts.
fn ingest_staging_base() -> PathBuf {
    std::env::temp_dir().join("cce-ingest")
}

/// Version staging directory for one project under an explicit base.
fn staging_root_in(base: &Path, project_id: i64, manifest_version: u64) -> PathBuf {
    base.join(format!("project-{project_id}"))
        .join(format!("manifest-{manifest_version}"))
}

/// Stable directory name for one relative path inside the staging root.
fn staging_file_dir(root: &Path, relative: &str) -> PathBuf {
    let digest = cce_utils::hash::calculate_hash(relative.as_bytes());
    root.join(digest)
}

/// Chunk usage recorded for one manifest version.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct VersionStagingUsage {
    /// Chunk files on disk.
    chunks: u64,
    /// Raw bytes held by those chunk files.
    bytes: u64,
}

/// Process-local chunk usage per manifest version.
///
/// The version directory is walked the first time this process stages into
/// it; afterwards batch admission adjusts the counters in memory so the
/// check stays proportional to the request instead of the staged tree.
static STAGING_USAGE: LazyLock<Mutex<HashMap<(i64, u64), VersionStagingUsage>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Chunk usage counters for one version, walking staging on first touch.
fn staging_usage_snapshot(project_id: i64, manifest_version: u64) -> VersionStagingUsage {
    let mut registry = STAGING_USAGE.lock().unwrap_or_else(|p| p.into_inner());
    *registry
        .entry((project_id, manifest_version))
        .or_insert_with(|| {
            walk_version_staging_in(&ingest_staging_base(), project_id, manifest_version)
        })
}

/// Credit successfully staged chunks to a version's usage counters.
fn credit_staging_usage(project_id: i64, manifest_version: u64, pending: VersionStagingUsage) {
    if pending.chunks == 0 && pending.bytes == 0 {
        return;
    }
    let mut registry = STAGING_USAGE.lock().unwrap_or_else(|p| p.into_inner());
    let used = registry
        .entry((project_id, manifest_version))
        .or_insert_with(|| {
            walk_version_staging_in(&ingest_staging_base(), project_id, manifest_version)
        });
    used.chunks = used.chunks.saturating_add(pending.chunks);
    used.bytes = used.bytes.saturating_add(pending.bytes);
}

/// Drop the usage record for one version, e.g. after its directory went away.
fn forget_usage(project_id: i64, manifest_version: u64) {
    STAGING_USAGE
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .remove(&(project_id, manifest_version));
}

/// Drop every usage record belonging to one project.
fn forget_project_usage(project_id: i64) {
    STAGING_USAGE
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .retain(|(project, _), _| *project != project_id);
}

/// Count the chunk files already staged for one version under an explicit base.
fn walk_version_staging_in(
    base: &Path,
    project_id: i64,
    manifest_version: u64,
) -> VersionStagingUsage {
    let root = staging_root_in(base, project_id, manifest_version);
    let mut usage = VersionStagingUsage::default();
    let Ok(file_dirs) = std::fs::read_dir(&root) else {
        return usage;
    };
    for file_dir in file_dirs.flatten() {
        let Ok(chunks) = std::fs::read_dir(file_dir.path()) else {
            continue;
        };
        for chunk in chunks.flatten() {
            let Ok(metadata) = chunk.metadata() else {
                continue;
            };
            usage.chunks = usage.chunks.saturating_add(1);
            usage.bytes = usage.bytes.saturating_add(metadata.len());
        }
    }
    usage
}

/// Whether staged plus pending usage stays inside the per-version bounds.
fn within_staging_bounds(used: VersionStagingUsage, pending: VersionStagingUsage) -> bool {
    used.chunks.saturating_add(pending.chunks) <= MAX_STAGING_CHUNKS_PER_VERSION
        && used.bytes.saturating_add(pending.bytes) <= MAX_STAGING_BYTES_PER_VERSION
}

/// Drop manifest-version staging directories older than `retention`.
///
/// Directory modification time decides expiry. Removal failures are only
/// logged, so reclaiming orphans never blocks the ingest flow.
pub fn sweep_expired_staging(retention: Duration) {
    sweep_expired_staging_in(&ingest_staging_base(), retention);
}

/// Sweep every project's staging base for expired manifest versions.
fn sweep_expired_staging_in(base: &Path, retention: Duration) {
    let Ok(projects) = std::fs::read_dir(base) else {
        return;
    };
    for project in projects.flatten() {
        let file_name = project.file_name();
        let Some(name) = file_name.to_str().and_then(|n| n.strip_prefix("project-")) else {
            continue;
        };
        let Ok(project_id) = name.parse::<i64>() else {
            continue;
        };
        sweep_project_staging_in(base, project_id, retention);
    }
}

/// Sweep one project's expired manifest versions before a commit indexes.
fn sweep_project_staging(project_id: i64, retention: Duration) {
    sweep_project_staging_in(&ingest_staging_base(), project_id, retention);
}

/// Remove a project's manifest directories whose age reaches `retention`.
fn sweep_project_staging_in(base: &Path, project_id: i64, retention: Duration) {
    let root = base.join(format!("project-{project_id}"));
    let Ok(versions) = std::fs::read_dir(&root) else {
        return;
    };
    for version in versions.flatten() {
        let file_name = version.file_name();
        let Some(name) = file_name.to_str().and_then(|n| n.strip_prefix("manifest-")) else {
            continue;
        };
        let Ok(manifest_version) = name.parse::<u64>() else {
            continue;
        };
        let Ok(metadata) = version.metadata() else {
            continue;
        };
        let Ok(modified) = metadata.modified() else {
            continue;
        };
        let Ok(age) = SystemTime::now().duration_since(modified) else {
            continue;
        };
        if age < retention {
            continue;
        }
        let removed = match std::fs::remove_dir_all(version.path()) {
            Ok(()) => true,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => true,
            Err(e) => {
                tracing::warn!(
                    project_id,
                    manifest_version,
                    error = %e,
                    "Failed to remove expired ingest staging"
                );
                false
            }
        };
        if removed {
            forget_usage(project_id, manifest_version);
        }
    }
}

/// Chunks and bytes one decoded request would add to a version's staging.
///
/// Pieces already on disk are resumed rather than counted again, so
/// re-pushing a version stays idempotent for admission as well as storage.
fn pending_pieces_in(
    base: &Path,
    project_id: i64,
    manifest_version: u64,
    file: &DecodedFile<'_>,
) -> VersionStagingUsage {
    let path = storage_path(file.relative);
    let received: HashSet<u32> =
        received_chunk_indices_in(base, project_id, manifest_version, &path)
            .into_iter()
            .collect();
    let mut pending = VersionStagingUsage::default();
    for (index, bytes) in &file.pieces {
        if !received.contains(index) {
            pending.chunks = pending.chunks.saturating_add(1);
            pending.bytes = pending.bytes.saturating_add(bytes.len() as u64);
        }
    }
    pending
}

/// Indices already stored for one file in the given manifest version under
/// an explicit staging base.
fn received_chunk_indices_in(
    base: &Path,
    project_id: i64,
    manifest_version: u64,
    relative: &str,
) -> Vec<u32> {
    if manifest_version == 0 {
        return Vec::new();
    }
    let dir = staging_file_dir(
        &staging_root_in(base, project_id, manifest_version),
        relative,
    );
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

/// Indices already stored for one file in the given manifest version.
fn received_chunk_indices(project_id: i64, manifest_version: u64, relative: &str) -> Vec<u32> {
    received_chunk_indices_in(
        &ingest_staging_base(),
        project_id,
        manifest_version,
        relative,
    )
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
    // Group the batch by file: decode, per-chunk verification, reassembly
    // and the whole-file check share one path for every manifest version.
    let mut groups: BTreeMap<&str, Vec<&IngestedFile>> = BTreeMap::new();
    for file in &request.files {
        groups
            .entry(file.relative_path.as_str())
            .or_default()
            .push(file);
    }
    let mut decoded: Vec<DecodedFile<'_>> = Vec::new();
    let mut errors = Vec::new();
    for (relative, entries) in &groups {
        match decode_group(relative, entries) {
            Ok(file) => decoded.push(file),
            Err(reason) => errors.push(reason),
        }
    }

    // Versioned staging is admitted against per-version chunk bounds so a
    // runaway push is refused as a whole request instead of being silently
    // truncated. Version zero keeps no staged chunks and needs no accounting.
    let staging_base = ingest_staging_base();
    let versioned = request.manifest_version != 0;
    if versioned {
        let mut pending = VersionStagingUsage::default();
        for file in &mut decoded {
            let piece_pending =
                pending_pieces_in(&staging_base, id, request.manifest_version, file);
            file.pending_chunks = piece_pending.chunks;
            file.pending_bytes = piece_pending.bytes;
            pending.chunks = pending.chunks.saturating_add(piece_pending.chunks);
            pending.bytes = pending.bytes.saturating_add(piece_pending.bytes);
        }
        let used = staging_usage_snapshot(id, request.manifest_version);
        if !within_staging_bounds(used, pending) {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::INVALID_REQUEST,
                format!(
                    "manifest {} staging bound exceeded: {} chunks / {} bytes staged, request adds {} chunks / {} bytes; limits are {MAX_STAGING_CHUNKS_PER_VERSION} chunks and {MAX_STAGING_BYTES_PER_VERSION} bytes",
                    request.manifest_version,
                    used.chunks,
                    used.bytes,
                    pending.chunks,
                    pending.bytes
                ),
            ));
        }
    }

    let mut staged = 0usize;
    let mut staged_chunks = 0usize;
    for file in &decoded {
        match stage_file(FileStageParams {
            root: &root,
            staging_base: &staging_base,
            project_id: id,
            manifest_version: request.manifest_version,
            file,
        })
        .await
        {
            Ok(completed) => {
                staged_chunks += file.pieces.len();
                if completed {
                    staged += 1;
                }
                if versioned {
                    credit_staging_usage(
                        id,
                        request.manifest_version,
                        VersionStagingUsage {
                            chunks: file.pending_chunks,
                            bytes: file.pending_bytes,
                        },
                    );
                }
            }
            Err(reason) => errors.push(reason),
        }
    }
    if cce_orchestrator::supply::IngestSupplyMode::from_env().is_direct() {
        for (relative, entries) in &groups {
            let path = storage_path(relative);
            let mirror_path = root.join(Path::new(&path));
            let Ok(mirror_bytes) = std::fs::read(&mirror_path) else {
                continue;
            };
            let content_hash = entries.first().and_then(|entry| entry.content_hash.clone());
            let payloads = cce_orchestrator::supply::build_ready_payloads(vec![(
                path.clone(),
                mirror_bytes,
                content_hash,
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

/// Upper bound on chunks for one file, derived from the 1 MiB file cap.
fn max_file_chunks() -> u32 {
    total_chunks_for_size(MAX_INGEST_FILE_BYTES)
}

/// One file's chunks decoded and verified, ready to be staged.
#[derive(Debug)]
struct DecodedFile<'a> {
    /// Gateway-relative path of the file.
    relative: &'a str,
    /// Declared chunk count of the file.
    total: u32,
    /// Whole-file hash the reassembled bytes must match.
    content_hash: Option<&'a str>,
    /// Decoded pieces keyed by chunk index.
    pieces: BTreeMap<u32, Vec<u8>>,
    /// Chunks this request would add to version staging.
    pending_chunks: u64,
    /// Bytes this request would add to version staging.
    pending_bytes: u64,
}

/// Decode and verify every chunk of one file inside a batch.
///
/// Group validation is identical for every manifest version: the declared
/// total must be positive and within the file bound, all pieces of a file
/// must agree on the total and whole-file hash, indices must stay in range
/// and unique, and each payload must pass per-chunk verification.
fn decode_group<'a>(
    relative: &'a str,
    entries: &[&'a IngestedFile],
) -> Result<DecodedFile<'a>, String> {
    let Some(first) = entries.first() else {
        return Err(format!("{relative}: chunk group is empty"));
    };
    let total = first.total_chunks;
    if total == 0 {
        return Err(format!("{relative}: total chunks must be positive"));
    }
    if total > max_file_chunks() {
        return Err(format!(
            "{relative}: declares {total} chunks, more than the {} bound",
            max_file_chunks()
        ));
    }
    let content_hash = first.content_hash.as_deref();
    let mut pieces: BTreeMap<u32, Vec<u8>> = BTreeMap::new();
    for entry in entries {
        if entry.total_chunks != total {
            return Err(format!(
                "{relative}: chunk {} declares {} total chunks while the group started with {total}",
                entry.chunk_index, entry.total_chunks
            ));
        }
        if entry.content_hash.as_deref() != content_hash {
            return Err(format!(
                "{relative}: chunk {} carries a different whole-file hash",
                entry.chunk_index
            ));
        }
        if entry.chunk_index >= total {
            return Err(format!(
                "{relative}: chunk {} exceeds {total}",
                entry.chunk_index
            ));
        }
        if pieces.contains_key(&entry.chunk_index) {
            return Err(format!(
                "{relative}: chunk {} appears more than once in the batch",
                entry.chunk_index
            ));
        }
        let bytes = decode_chunk_payload(
            relative,
            &entry.content_base64,
            entry.compressed,
            entry.chunk_hash.as_deref(),
        )?;
        pieces.insert(entry.chunk_index, bytes);
    }
    Ok(DecodedFile {
        relative,
        total,
        content_hash,
        pieces,
        pending_chunks: 0,
        pending_bytes: 0,
    })
}

/// Parameters for staging one decoded file.
struct FileStageParams<'a> {
    /// Project mirror root.
    root: &'a Path,
    /// Staging base directory holding manifest versions.
    staging_base: &'a Path,
    /// Project the file belongs to.
    project_id: i64,
    /// Manifest version of the batch.
    manifest_version: u64,
    /// Decoded file to stage.
    file: &'a DecodedFile<'a>,
}

/// Store one decoded file into the mirror for its manifest version.
///
/// Chunk bytes are already individually verified by the group decoder;
/// the reassembled file is verified against the whole-file hash by
/// `stage_bytes` before it reaches the mirror, so a corrupted piece can
/// never poison a completed file. Version zero keeps no resume bookkeeping
/// and therefore requires every piece inside this batch; versioned files
/// persist their pieces first and reassemble from staging, tolerating
/// pieces from earlier batches. Returns whether the whole file reached the
/// mirror.
async fn stage_file(params: FileStageParams<'_>) -> Result<bool, String> {
    let FileStageParams {
        root,
        staging_base,
        project_id,
        manifest_version,
        file,
    } = params;
    let relative = file.relative;
    let path = storage_path(relative);
    if manifest_version == 0 {
        if file.pieces.len() as u32 != file.total {
            return Err(format!(
                "{relative}: manifest version zero carries no resume bookkeeping; the whole file must arrive in one batch (received {} of {} chunks)",
                file.pieces.len(),
                file.total
            ));
        }
        let assembled: Vec<u8> = file
            .pieces
            .values()
            .flat_map(|piece| piece.iter().copied())
            .collect();
        stage_bytes(root, &path, &assembled, file.content_hash).await?;
        return Ok(true);
    }
    let file_dir = staging_file_dir(
        &staging_root_in(staging_base, project_id, manifest_version),
        &path,
    );
    std::fs::create_dir_all(&file_dir)
        .map_err(|e| format!("{relative}: failed to stage chunk: {e}"))?;
    for (index, bytes) in &file.pieces {
        let chunk_path = file_dir.join(format!("chunk_{index}"));
        std::fs::write(&chunk_path, bytes)
            .map_err(|e| format!("{relative}: failed to stage chunk: {e}"))?;
    }
    let Some(assembled) = read_assembled_pieces(&file_dir, file.total, relative)? else {
        return Ok(false);
    };
    stage_bytes(root, &path, &assembled, file.content_hash).await?;
    Ok(true)
}

/// Read every staged chunk of one file back in index order.
///
/// Returns `None` while any piece is still missing, so the caller reports
/// a partial file instead of a failure.
fn read_assembled_pieces(
    file_dir: &Path,
    total_chunks: u32,
    relative: &str,
) -> Result<Option<Vec<u8>>, String> {
    let mut assembled = Vec::new();
    for index in 0..total_chunks {
        let candidate = file_dir.join(format!("chunk_{index}"));
        match std::fs::read(&candidate) {
            Ok(piece) => assembled.extend_from_slice(&piece),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(format!("{relative}: failed to reassemble: {e}")),
        }
    }
    Ok(Some(assembled))
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
    // Reclaim expired version staging before indexing so abandoned chunks
    // from interrupted pushes do not survive until the next full push.
    sweep_project_staging(id, INGEST_STAGING_RETENTION);
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
    clear_project_staging_in(&ingest_staging_base(), project_id);
}

/// Drop a project's whole staging tree under an explicit base.
fn clear_project_staging_in(base: &Path, project_id: i64) {
    let root = base.join(format!("project-{project_id}"));
    if let Err(e) = std::fs::remove_dir_all(&root)
        && e.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!("Failed to clear ingest staging for project {project_id}: {e}");
    }
    forget_project_usage(project_id);
}

/// Apply gateway-observed changes through the hot-update pipeline.
///
/// Created and modified events carry content and are staged before the
/// coordinator runs; deletions remove the mirrored file and its index rows.
/// The remote host never starts its own filesystem watch in this shape:
/// every change arrives through this entry. Events travel whole and raw,
/// so a change that would exceed the entry-count or single-file ingest
/// bounds is not split here: the gateway falls back to a full sync batch
/// instead.
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Split `bytes` into two pieces for multi-chunk cases.
    fn split_two(bytes: &[u8]) -> (Vec<u8>, Vec<u8>) {
        let mid = bytes.len() / 2;
        (bytes[..mid].to_vec(), bytes[mid..].to_vec())
    }

    /// Build one ingested chunk carrying valid per-chunk hashes.
    fn entry(
        relative: &str,
        index: u32,
        total: u32,
        chunk: &[u8],
        content_hash: &str,
    ) -> IngestedFile {
        IngestedFile {
            relative_path: relative.to_string(),
            content_hash: Some(content_hash.to_string()),
            content_base64: base64::engine::general_purpose::STANDARD.encode(chunk),
            chunk_index: index,
            total_chunks: total,
            chunk_hash: Some(cce_utils::hash::calculate_hash(chunk)),
            compressed: false,
        }
    }

    #[tokio::test]
    async fn zero_version_single_chunk_stages_without_bookkeeping() {
        let mirror = tempfile::tempdir().expect("mirror");
        let staging = tempfile::tempdir().expect("staging");
        let full = b"fn main() {}";
        let content_hash = cce_utils::hash::calculate_hash(full);
        let single = entry("src/main.rs", 0, 1, full, &content_hash);
        let file = decode_group("src/main.rs", &[&single]).expect("decode");
        let staged = stage_file(FileStageParams {
            root: mirror.path(),
            staging_base: staging.path(),
            project_id: 9101,
            manifest_version: 0,
            file: &file,
        })
        .await
        .expect("stage");
        assert!(staged);
        let dest = mirror.path().join(storage_path("src/main.rs"));
        assert_eq!(std::fs::read(&dest).expect("staged file"), full);
        assert_eq!(
            walk_version_staging_in(staging.path(), 9101, 0),
            VersionStagingUsage::default(),
            "version zero stages no chunks for resume"
        );
    }

    #[tokio::test]
    async fn zero_version_requires_whole_file_in_one_batch() {
        let mirror = tempfile::tempdir().expect("mirror");
        let staging = tempfile::tempdir().expect("staging");
        let full = b"only the first half arrives in this batch";
        let content_hash = cce_utils::hash::calculate_hash(full);
        let (first, _second) = split_two(full);
        let partial = entry("lib.rs", 0, 2, &first, &content_hash);
        let file = decode_group("lib.rs", &[&partial]).expect("decode");
        let error = stage_file(FileStageParams {
            root: mirror.path(),
            staging_base: staging.path(),
            project_id: 9102,
            manifest_version: 0,
            file: &file,
        })
        .await
        .expect_err("incomplete version zero batch must fail");
        assert!(error.contains("version zero carries no resume bookkeeping"));
        assert!(!mirror.path().join(storage_path("lib.rs")).exists());
    }

    #[tokio::test]
    async fn versioned_batch_reassembles_and_verifies_whole_file() {
        let mirror = tempfile::tempdir().expect("mirror");
        let staging = tempfile::tempdir().expect("staging");
        let full = b"the quick brown fox jumps over the lazy dog";
        let content_hash = cce_utils::hash::calculate_hash(full);
        let (first, second) = split_two(full);
        let left = entry("lib.rs", 0, 2, &first, &content_hash);
        let right = entry("lib.rs", 1, 2, &second, &content_hash);
        let file = decode_group("lib.rs", &[&left, &right]).expect("decode");
        let staged = stage_file(FileStageParams {
            root: mirror.path(),
            staging_base: staging.path(),
            project_id: 9103,
            manifest_version: 7,
            file: &file,
        })
        .await
        .expect("stage");
        assert!(staged);
        assert_eq!(
            std::fs::read(mirror.path().join(storage_path("lib.rs"))).expect("staged file"),
            full
        );
        assert_eq!(
            walk_version_staging_in(staging.path(), 9103, 7),
            VersionStagingUsage {
                chunks: 2,
                bytes: full.len() as u64
            }
        );
        assert_eq!(
            pending_pieces_in(staging.path(), 9103, 7, &file),
            VersionStagingUsage::default(),
            "re-sending the same pieces costs no extra quota"
        );
    }

    #[tokio::test]
    async fn interrupted_push_resumes_from_staged_chunks() {
        let mirror = tempfile::tempdir().expect("mirror");
        let staging = tempfile::tempdir().expect("staging");
        let full = b"resume across two separate batches";
        let content_hash = cce_utils::hash::calculate_hash(full);
        let (first, second) = split_two(full);
        let left = entry("lib.rs", 0, 2, &first, &content_hash);
        let right = entry("lib.rs", 1, 2, &second, &content_hash);

        let opening = decode_group("lib.rs", &[&left]).expect("decode first batch");
        let staged = stage_file(FileStageParams {
            root: mirror.path(),
            staging_base: staging.path(),
            project_id: 9104,
            manifest_version: 9,
            file: &opening,
        })
        .await
        .expect("stage first batch");
        assert!(!staged, "a partial batch reports an incomplete file");
        assert!(!mirror.path().join(storage_path("lib.rs")).exists());
        assert_eq!(
            received_chunk_indices_in(staging.path(), 9104, 9, &storage_path("lib.rs")),
            vec![0]
        );
        assert_eq!(
            pending_pieces_in(staging.path(), 9104, 9, &opening),
            VersionStagingUsage::default(),
            "the stored chunk is not charged again"
        );

        let closing = decode_group("lib.rs", &[&right]).expect("decode second batch");
        let pending = pending_pieces_in(staging.path(), 9104, 9, &closing);
        assert_eq!(pending.chunks, 1, "only the missing chunk is new");
        let staged = stage_file(FileStageParams {
            root: mirror.path(),
            staging_base: staging.path(),
            project_id: 9104,
            manifest_version: 9,
            file: &closing,
        })
        .await
        .expect("stage second batch");
        assert!(staged, "the resumed file completes");
        assert_eq!(
            std::fs::read(mirror.path().join(storage_path("lib.rs"))).expect("staged file"),
            full
        );
    }

    #[test]
    fn chunk_hash_mismatch_is_rejected_before_staging() {
        let full = b"content";
        let content_hash = cce_utils::hash::calculate_hash(full);
        let mut corrupted = entry("a.rs", 0, 1, full, &content_hash);
        corrupted.chunk_hash = Some("00".repeat(32));
        let error =
            decode_group("a.rs", &[&corrupted]).expect_err("a corrupted piece must be refused");
        assert!(error.contains("chunk hash mismatch"));
    }

    #[tokio::test]
    async fn whole_file_hash_mismatch_is_rejected_after_reassembly() {
        let mirror = tempfile::tempdir().expect("mirror");
        let staging = tempfile::tempdir().expect("staging");
        let full = b"both pieces decode but the manifest hash is stale";
        let stale_hash = cce_utils::hash::calculate_hash(b"a different file");
        let (first, second) = split_two(full);
        let left = entry("a.rs", 0, 2, &first, &stale_hash);
        let right = entry("a.rs", 1, 2, &second, &stale_hash);
        let file = decode_group("a.rs", &[&left, &right]).expect("decode");
        let error = stage_file(FileStageParams {
            root: mirror.path(),
            staging_base: staging.path(),
            project_id: 9105,
            manifest_version: 3,
            file: &file,
        })
        .await
        .expect_err("a stale whole-file hash must be refused");
        assert!(error.contains("changed between manifest and upload"));
        assert!(!mirror.path().join(storage_path("a.rs")).exists());
    }

    #[test]
    fn group_validation_rejects_inconsistent_pieces() {
        let payload = b"payload";
        let content_hash = cce_utils::hash::calculate_hash(payload);

        let zero_total = entry("a.rs", 0, 0, payload, &content_hash);
        assert!(
            decode_group("a.rs", &[&zero_total])
                .expect_err("zero total must be refused")
                .contains("total chunks must be positive")
        );

        let over_bound = entry("a.rs", 0, max_file_chunks() + 1, payload, &content_hash);
        assert!(
            decode_group("a.rs", &[&over_bound])
                .expect_err("an oversized split must be refused")
                .contains("more than the")
        );

        let first = entry("a.rs", 0, 1, payload, &content_hash);
        let mismatched = entry("a.rs", 1, 2, payload, &content_hash);
        assert!(
            decode_group("a.rs", &[&first, &mismatched])
                .expect_err("totals must agree")
                .contains("while the group started with")
        );

        let out_of_range = entry("a.rs", 2, 2, payload, &content_hash);
        assert!(
            decode_group("a.rs", &[&out_of_range])
                .expect_err("the index must stay in range")
                .contains("exceeds")
        );

        let duplicate_one = entry("a.rs", 0, 1, payload, &content_hash);
        let duplicate_two = entry("a.rs", 0, 1, payload, &content_hash);
        assert!(
            decode_group("a.rs", &[&duplicate_one, &duplicate_two])
                .expect_err("duplicate indices must be refused")
                .contains("more than once")
        );
    }

    #[test]
    fn pending_usage_stays_within_version_bounds() {
        let used = VersionStagingUsage::default();
        let pending = VersionStagingUsage {
            chunks: 1,
            bytes: 7,
        };
        assert!(within_staging_bounds(used, pending));
        assert!(within_staging_bounds(
            VersionStagingUsage {
                chunks: MAX_STAGING_CHUNKS_PER_VERSION - 1,
                bytes: MAX_STAGING_BYTES_PER_VERSION - 7,
            },
            pending
        ));
        assert!(!within_staging_bounds(
            VersionStagingUsage {
                chunks: MAX_STAGING_CHUNKS_PER_VERSION,
                ..VersionStagingUsage::default()
            },
            pending
        ));
        assert!(!within_staging_bounds(
            VersionStagingUsage {
                bytes: MAX_STAGING_BYTES_PER_VERSION,
                ..VersionStagingUsage::default()
            },
            pending
        ));
        assert!(!within_staging_bounds(
            used,
            VersionStagingUsage {
                chunks: MAX_STAGING_CHUNKS_PER_VERSION + 1,
                ..VersionStagingUsage::default()
            }
        ));
    }

    #[test]
    fn sweep_reclaims_expired_versions_and_keeps_fresh_ones() {
        let base = tempfile::tempdir().expect("staging base");
        let project = base.path().join("project-9106");
        let expired = project.join("manifest-1");
        let fresh = project.join("manifest-2");
        for version in [&expired, &fresh] {
            let chunk = version.join("file-1").join("chunk_0");
            std::fs::create_dir_all(chunk.parent().expect("file dir")).expect("version dir");
            std::fs::write(chunk, b"stale").expect("chunk");
        }
        let age = SystemTime::now()
            .checked_sub(INGEST_STAGING_RETENTION + Duration::from_secs(60))
            .expect("clock is far enough from the epoch");
        std::fs::File::open(&expired)
            .expect("expired dir handle")
            .set_modified(age)
            .expect("age the expired version");

        sweep_project_staging_in(base.path(), 9106, INGEST_STAGING_RETENTION);
        assert!(!expired.exists(), "expired version is reclaimed");
        assert!(fresh.exists(), "a fresh version survives the sweep");

        sweep_expired_staging_in(base.path(), INGEST_STAGING_RETENTION);
        assert!(
            fresh.exists(),
            "the startup sweep only removes expired versions"
        );
    }

    #[tokio::test]
    async fn commit_cleanup_removes_every_version_of_one_project() {
        let base = tempfile::tempdir().expect("staging base");
        let mirror = tempfile::tempdir().expect("mirror");
        let full = b"cleanup removes both versions";
        let content_hash = cce_utils::hash::calculate_hash(full);
        let single = entry("a.rs", 0, 1, full, &content_hash);
        for manifest_version in [1u64, 2u64] {
            let file = decode_group("a.rs", &[&single]).expect("decode");
            stage_file(FileStageParams {
                root: mirror.path(),
                staging_base: base.path(),
                project_id: 9107,
                manifest_version,
                file: &file,
            })
            .await
            .expect("stage");
        }
        let sibling = entry("b.rs", 0, 1, full, &content_hash);
        let file = decode_group("b.rs", &[&sibling]).expect("decode");
        stage_file(FileStageParams {
            root: mirror.path(),
            staging_base: base.path(),
            project_id: 9108,
            manifest_version: 1,
            file: &file,
        })
        .await
        .expect("stage sibling");

        clear_project_staging_in(base.path(), 9107);
        assert!(
            !base.path().join("project-9107").exists(),
            "commit cleanup removes every version of the committed project"
        );
        assert!(
            base.path().join("project-9108").exists(),
            "other projects are untouched"
        );
    }
}
