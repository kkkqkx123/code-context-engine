//! Project indexing handlers
//!
//! Handles indexing operations for projects.

use axum::extract::{Path, State};

use super::management::record_to_config;
use crate::api::response::ApiResult;
use cce_api::models::error_codes;
use cce_api::models::{
    DeadLetterActionResponse, DeadLetterAcknowledgeRequest, DeadLetterFileEntry,
    DeadLetterListResponse, DeadLetterModuleEntry, DeadLetterRetryRequest,
    DeadLetterRetryResponse, ErrorResponse, ProjectIndexResponse,
};
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

    // Build IndexOptions from the project record so indexing follows the
    // configuration registered for this project (root path, extensions,
    // excludes, ignore patterns, gitignore handling).
    let config = record_to_config(&record);

    let index_options = cce_orchestrator::IndexOptions::new(&config.root_path)
        .with_extensions(config.extensions)
        .with_exclude_dirs(config.exclude_dirs)
        .with_gitignore(config.respect_gitignore)
        .with_ignore_patterns(config.ignore_patterns);

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

/// Run a dead-letter truncate-retry pass restricted to the requested files
#[utoipa::path(
    post, path = "/api/project/{id}/dead-letters/retry", tag = "Project",
    params(("id" = i64, Path, description = "Project id")),
    request_body = DeadLetterRetryRequest,
    responses(
        (status = 200, body = DeadLetterActionResponse, description = "Success"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_dead_letter_files_retry(
    State(state): State<crate::api::state::AppState>,
    Path(id): Path<i64>,
    axum::Json(body): axum::Json<DeadLetterRetryRequest>,
) -> ApiResult<DeadLetterActionResponse> {
    match state.engine.retry_dead_letters_for_files(id, &body.files).await {
        Ok(report) => ApiResult::Success(DeadLetterActionResponse {
            success: true,
            affected: report.retried,
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

/// Acknowledge the dead letter(s) of one file so they leave the retry flow
#[utoipa::path(
    post, path = "/api/project/{id}/dead-letters/acknowledge", tag = "Project",
    params(("id" = i64, Path, description = "Project id")),
    request_body = DeadLetterAcknowledgeRequest,
    responses(
        (status = 200, body = DeadLetterActionResponse, description = "Success"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_dead_letter_acknowledge(
    State(state): State<crate::api::state::AppState>,
    Path(id): Path<i64>,
    axum::Json(body): axum::Json<DeadLetterAcknowledgeRequest>,
) -> ApiResult<DeadLetterActionResponse> {
    let module = match body.module.as_deref() {
        None => None,
        Some(name) => match cce_orchestrator::ModuleType::all()
            .into_iter()
            .find(|m| m.as_str() == name)
        {
            Some(m) => Some(m),
            None => {
                return ApiResult::Error(ErrorResponse::new(
                    error_codes::INVALID_REQUEST,
                    format!("Unknown module: {}", name),
                ));
            }
        },
    };

    match state
        .engine
        .acknowledge_dead_letter(id, &body.file_path, module)
        .await
    {
        Ok(updated) => ApiResult::Success(DeadLetterActionResponse {
            success: true,
            affected: updated,
            message: format!("acknowledged {} dead-letter module(s)", updated),
        }),
        Err(e) => ApiResult::Error(ErrorResponse::new(
            error_codes::INTERNAL_ERROR,
            format!("Dead-letter acknowledge failed: {}", e),
        )),
    }
}

/// List the dead-letter files of a project
#[utoipa::path(
    get, path = "/api/project/{id}/dead-letters", tag = "Project",
    params(("id" = i64, Path, description = "Project id")),
    responses(
        (status = 200, body = DeadLetterListResponse, description = "Success"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_dead_letter_list(
    State(state): State<crate::api::state::AppState>,
    Path(id): Path<i64>,
) -> ApiResult<DeadLetterListResponse> {
    match state.engine.dead_letters(id).await {
        Ok(states) => {
            let files: Vec<DeadLetterFileEntry> = states
                .iter()
                .map(|s| DeadLetterFileEntry {
                    file_path: s.file_path.clone(),
                    version: s.version,
                    modules: s
                        .module_states
                        .iter()
                        .filter(|(_, r)| {
                            matches!(r.state, cce_orchestrator::ModuleUpdateState::DeadLetter)
                        })
                        .map(|(m, r)| DeadLetterModuleEntry {
                            module: m.as_str().to_string(),
                            retry_count: r.retry_count,
                            error_code: r.error_code.clone(),
                            error_message: r.error_message.clone(),
                            truncated: r.truncated,
                            acknowledged: r.acknowledged,
                        })
                        .collect(),
                    updated_at: s.updated_at.to_rfc3339(),
                })
                .collect();
            ApiResult::Success(DeadLetterListResponse { project_id: id, files })
        }
        Err(e) => ApiResult::Error(ErrorResponse::new(
            error_codes::INTERNAL_ERROR,
            format!("Dead-letter list failed: {}", e),
        )),
    }
}
