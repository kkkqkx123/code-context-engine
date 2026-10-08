//! Commit entry running the index pipeline over staged files.

use axum::extract::{Path as AxumPath, State};
use cce_api::models::{ErrorResponse, IngestCommitResponse, error_codes};
use cce_storage_metadb_sqlite::{ProjectRepository, ProjectUpdateRecord};

use super::staging::{INGEST_STAGING_RETENTION, clear_project_staging, sweep_project_staging};
use crate::api::handlers::project::management::record_to_config;
use crate::api::response::ApiResult;
use crate::api::state::AppState;

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
