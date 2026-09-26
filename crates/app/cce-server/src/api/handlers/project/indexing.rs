//! Project indexing handlers
//!
//! Handles indexing operations for projects.

use axum::extract::{Path, State};

use crate::api::response::ApiResult;
use cce_api::models::error_codes;
use cce_api::models::{DeadLetterRetryResponse, ErrorResponse, ProjectIndexResponse};
use cce_storage_sqlite::{ProjectRepository, ProjectUpdateRecord};

/// Handle project indexing request
#[utoipa::path(
    post, path = "/api/project/{id}/index", tag = "Project",
    params(("id" = i64, Path, description = "Project id")),
    responses(
        (status = 200, body = ProjectIndexResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_project_index(
    State(state): State<crate::api::state::AppState>,
    Path(id): Path<i64>,
) -> ApiResult<ProjectIndexResponse> {
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

    // Get project record
    let record = match metadata_store
        .as_ref()
        .with_transaction(|tx| ProjectRepository::get_by_id(tx, id))
    {
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
                format!("Failed to query project: {}", e),
            ));
        }
    };

    // Build IndexOptions from project config
    let extensions: Vec<String> = vec![]; // Default extensions
    let exclude_dirs: Vec<String> = vec![]; // Default exclude dirs
    let ignore_patterns: Vec<String> = vec![]; // Default ignore patterns

    let index_options = cce_orchestrator::IndexOptions::new(&record.root_path)
        .with_extensions(extensions)
        .with_exclude_dirs(exclude_dirs)
        .with_gitignore(true)
        .with_ignore_patterns(ignore_patterns);

    // Execute indexing using engine's index method with project_id
    let result = match state.engine.index(id, index_options).await {
        Ok(r) => r,
        Err(e) => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                format!("Index execution failed: {}", e),
            ));
        }
    };

    // Update last_indexed timestamp
    let now = chrono::Utc::now().to_rfc3339();
    // Use update method instead of delete-insert
    let client = metadata_store.as_ref();
    let updates = ProjectUpdateRecord::default().with_last_indexed(now);
    if let Err(e) = client.with_transaction(|tx| ProjectRepository::update(tx, id, &updates)) {
        tracing::warn!("Failed to update last_indexed: {}", e);
    }

    ApiResult::Success(ProjectIndexResponse {
        success: result.is_success(),
        project_id: id,
        project_name: record.name,
        indexed_files: result.indexed_files,
        total_entities: result.total_entities,
        total_vectors: result.total_vectors,
        elapsed_ms: result.elapsed_ms,
    })
}

/// Handle a manual dead-letter truncate-retry request
#[utoipa::path(
    post, path = "/api/project/{id}/dead-letter/retry", tag = "Project",
    params(("id" = i64, Path, description = "Project id")),
    responses(
        (status = 200, body = DeadLetterRetryResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_dead_letter_retry(
    State(state): State<crate::api::state::AppState>,
    Path(id): Path<i64>,
) -> ApiResult<DeadLetterRetryResponse> {
    match state.engine.retry_dead_letters(id).await {
        Ok(report) => ApiResult::Success(DeadLetterRetryResponse {
            success: true,
            retried: report.retried,
            succeeded: report.succeeded,
            still_failed: report.still_failed,
            truncated_chunks: report.truncated_chunks,
            message: format!(
                "dead-letter retry: {} retried, {} succeeded, {} still failed, {} chunks truncated",
                report.retried, report.succeeded, report.still_failed, report.truncated_chunks
            ),
        }),
        Err(e) => ApiResult::Error(ErrorResponse::new(
            error_codes::INTERNAL_ERROR,
            format!("Dead-letter retry failed: {}", e),
        )),
    }
}
