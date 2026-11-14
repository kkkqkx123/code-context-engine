//! File summary handler
//!
//! Provides temporary file summary generation without storage.
//! Supports single files, multiple files, and directory scanning. The request
//! is executed by the shared summary service; this handler maps the result.

use axum::Json;

use cce_api::models::{
    ErrorResponse, FileSummaryItem, SummaryRequest, SummaryResponse, error_codes,
};
use cce_server_shared::services::tools::{self, SummarySpec};

use crate::api::response::ApiResult;

/// Unified response type for summary handler
pub type SummaryApiResponse = ApiResult<SummaryResponse>;

/// Handle summary generation request
///
/// Generates temporary file summaries without storing them.
/// Supports single files, multiple files, and directory scanning.
#[utoipa::path(
    post, path = "/api/summary", tag = "Summary",
    request_body = SummaryRequest,
    responses(
        (status = 200, body = SummaryResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_summary(Json(request): Json<SummaryRequest>) -> SummaryApiResponse {
    let spec = SummarySpec {
        file_paths: request.file_paths,
        directory_paths: request.directory_paths,
        extensions: request.extensions,
        exclude_dirs: request.exclude_dirs,
        respect_gitignore: request.respect_gitignore,
        ignore_patterns: request.ignore_patterns,
        max_files: request.max_files,
    };
    let outcome = match tools::summarize(spec).await {
        Ok(outcome) => outcome,
        Err(error) => {
            return SummaryApiResponse::Error(ErrorResponse::new(
                error_codes::INVALID_REQUEST,
                error.to_string(),
            ));
        }
    };
    let success = outcome.succeeded();
    SummaryApiResponse::Success(SummaryResponse {
        success,
        total_files: outcome.items.len(),
        success_count: outcome.success_count,
        failed_count: outcome.failed_count,
        summaries: outcome
            .items
            .into_iter()
            .map(|item| FileSummaryItem {
                file_path: item.file_path,
                language: item.language,
                summary: item.summary,
                main_entities: item.main_entities,
                imports: item.imports,
                exports: item.exports,
                entity_count: item.entity_count,
                line_count: item.line_count,
                tags: item.tags,
                importance_level: item.importance_level,
                success: item.success,
                error: item.error,
            })
            .collect(),
        elapsed_ms: outcome.elapsed_ms,
        warnings: outcome.warnings,
    })
}
