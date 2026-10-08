//! Batch staging entry for gateway-driven ingest.

use std::collections::BTreeMap;
use std::path::Path;

use axum::extract::{Extension, Path as AxumPath, State};
use cce_api::models::{
    ErrorResponse, IngestBatchRequest, IngestBatchResponse, IngestedFile, MAX_INGEST_BATCH_FILES,
    error_codes,
};

use super::paths::{project_root, storage_path};
use super::staging::{
    DecodedFile, FileStageParams, MAX_STAGING_BYTES_PER_VERSION, MAX_STAGING_CHUNKS_PER_VERSION,
    VersionStagingUsage, credit_staging_usage, decode_group, ingest_staging_base,
    pending_pieces_in, stage_file, staging_usage_snapshot, within_staging_bounds,
};
use crate::api::response::ApiResult;
use crate::api::state::AppState;

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
                cce_storage_relation_sqlite::AdmissionAuditRepository::get(tx, &context.fingerprint)
                    .map(|record| record.map_or(0, |r| r.bytes_used.max(0) as u64))
            })
            && used.saturating_add(batch_bytes) > quota
        {
            metrics.record_quota_rejection();
            if let Some(store) = state.engine.metadata_store() {
                let _ = store.as_ref().with_transaction(|tx| {
                    cce_storage_relation_sqlite::AdmissionAuditRepository::record_rejection(
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
            cce_storage_relation_sqlite::AdmissionAuditRepository::record_admitted(
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
