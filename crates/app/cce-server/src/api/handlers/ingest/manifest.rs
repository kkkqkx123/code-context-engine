//! Manifest comparison entry for gateway-driven ingest.

use axum::extract::{Path as AxumPath, State};
use cce_api::models::{
    ErrorResponse, IngestManifestRequest, IngestManifestResponse, MissingChunk,
    SUPPLY_MODE_GATEWAY, SUPPLY_MODE_KEY, error_codes, total_chunks_for_size,
};
use cce_storage_metadb_sqlite::{FileRepository, ProjectRepository};

use super::paths::storage_path;
use super::staging::received_chunk_indices;
use crate::api::response::ApiResult;
use crate::api::state::AppState;

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
