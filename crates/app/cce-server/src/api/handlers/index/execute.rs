//! Execute index handler
//!
//! This module provides handlers for full index execution.

use axum::{Json, extract::State};
use std::path::PathBuf;

use cce_api::models::{ErrorResponse, IndexRequest, IndexResponse, error_codes};
use cce_orchestrator::{IndexOptions, IndexResult};

use crate::api::response::ApiResult;

fn index_response_from_result(result: IndexResult) -> IndexResponse {
    let has_errors = !result.errors().is_empty();
    IndexResponse {
        success: result.is_success(),
        files_scanned: result.total_files,
        files_indexed: result.indexed_files,
        failed_files: result.failed_files,
        degraded_files: result.degraded_files,
        skipped_permanent: result.skipped_permanent,
        circuit_open: result.circuit_open,
        total_entities: result.total_entities,
        total_relations: result.total_relations,
        total_vectors: result.total_vectors,
        elapsed_ms: result.elapsed_ms,
        message: if !has_errors {
            format!(
                "Indexing completed, a total of {} files were processed, {} entities were extracted, and {} relationships were identified.",
                result.indexed_files, result.total_entities, result.total_relations
            )
        } else {
            format!(
                "The indexing process is complete. {} files were successful, and {} files failed.",
                result.indexed_files, result.failed_files
            )
        },
        errors: result.errors().to_vec(),
    }
}

/// Handle index request
#[axum::debug_handler]
#[utoipa::path(
    post, path = "/api/index", tag = "Index",
    request_body = IndexRequest,
    responses(
        (status = 200, body = IndexResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_index(
    State(state): State<crate::api::state::AppState>,
    Json(query): Json<IndexRequest>,
) -> ApiResult<IndexResponse> {
    // Validate project_id
    if let Err(e) = crate::api::validation::validate_project_id(query.project_id) {
        return ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            format!("Invalid project_id: {}", e),
        ));
    }

    // Validate root directory
    let root_dir = PathBuf::from(&query.path);
    if !root_dir.exists() {
        return ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            format!("Directory does not exist: {}", query.path),
        ));
    }

    if !root_dir.is_dir() {
        return ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            format!("The path is not a directory: {}", query.path),
        ));
    }

    // Build index options
    let mut options = IndexOptions::new(&root_dir)
        .with_extensions(if query.extensions.is_empty() {
            vec![
                "rs".to_string(),
                "py".to_string(),
                "js".to_string(),
                "ts".to_string(),
                "c".to_string(),
                "cpp".to_string(),
                "java".to_string(),
            ]
        } else {
            query.extensions
        })
        .with_exclude_dirs(if query.exclude_dirs.is_empty() {
            vec![
                "node_modules".to_string(),
                "target".to_string(),
                ".git".to_string(),
                "vendor".to_string(),
            ]
        } else {
            query.exclude_dirs
        })
        .with_gitignore(query.respect_gitignore)
        .with_ignore_patterns(query.ignore_patterns);

    if let Some(custom_gitignore) = query.custom_gitignore {
        options = options.with_custom_gitignore(custom_gitignore);
    }

    // Execute indexing with project_id using engine's index method
    let result = match state.engine.index(query.project_id, options).await {
        Ok(r) => r,
        Err(e) => {
            return ApiResult::Error(ErrorResponse::with_details(
                error_codes::INTERNAL_ERROR,
                "Index execution failed",
                e.to_string(),
            ));
        }
    };

    ApiResult::Success(index_response_from_result(result))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_response_from_result() {
        let result = IndexResult {
            total_files: 10,
            indexed_files: 9,
            failed_files: 1,
            total_entities: 100,
            total_relations: 50,
            total_vectors: 150,
            elapsed_ms: 1000,
            ..Default::default()
        };

        let response = index_response_from_result(result);
        assert!(response.success);
        assert_eq!(response.files_scanned, 10);
        assert_eq!(response.files_indexed, 9);
        assert_eq!(response.failed_files, 1);
        assert_eq!(response.degraded_files, 0);
        assert_eq!(response.skipped_permanent, 0);
        assert!(!response.circuit_open);
    }

    #[test]
    fn test_index_response_carries_degradation_signals() {
        let result = IndexResult {
            total_files: 10,
            indexed_files: 8,
            failed_files: 1,
            degraded_files: 3,
            skipped_permanent: 2,
            circuit_open: true,
            ..Default::default()
        };

        let response = index_response_from_result(result);
        assert_eq!(response.degraded_files, 3);
        assert_eq!(response.skipped_permanent, 2);
        assert!(response.circuit_open);
    }
}
