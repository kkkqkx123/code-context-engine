//! Wiki context export handlers.
//!
//! Wire decoding and response mapping for the Wiki Context Package export
//! and the incremental invalidation diff; assembly runs in the shared wiki
//! service.

use axum::extract::{Json, Path, Query as QueryParams, State};

use cce_api::models::{
    ErrorResponse, WikiContextQuery, WikiContextResponse, WikiDiffRequest, WikiDiffResponse,
};
use cce_config::ExternalDepsMode;
use cce_server_shared::services::wiki;
use cce_types::error_codes;

use crate::api::response::{ApiResult, relation_error};

/// Handle Wiki Context Package export.
#[utoipa::path(
    get, path = "/api/project/{project_id}/wiki/context", tag = "Wiki",
    params(WikiContextQuery, ("project_id" = i64, Path, description = "Project id")),
    responses(
        (status = 200, body = WikiContextResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 503, body = ErrorResponse, description = "Index unavailable"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_wiki_context(
    State(state): State<cce_server_shared::context::SharedContext>,
    Path(project_id): Path<i64>,
    QueryParams(params): QueryParams<WikiContextQuery>,
) -> ApiResult<WikiContextResponse> {
    let external_deps_mode = match params.external_deps_mode.as_deref() {
        None => None,
        Some(raw) => match ExternalDepsMode::parse(raw) {
            Some(mode) => Some(mode),
            None => {
                return ApiResult::Error(ErrorResponse::new(
                    error_codes::INVALID_REQUEST,
                    "external_deps_mode must be one of symbol, package".to_string(),
                ));
            }
        },
    };
    match wiki::export_context(
        &state,
        project_id,
        external_deps_mode,
        params.include_snippets,
        params.include_mermaid,
    )
    .await
    {
        Ok(response) => ApiResult::Success(response),
        Err(error) => ApiResult::Error(relation_error(error)),
    }
}

/// Handle incremental invalidation diff against baseline hashes.
#[utoipa::path(
    post, path = "/api/project/{project_id}/wiki/diff", tag = "Wiki",
    params(("project_id" = i64, Path, description = "Project id")),
    request_body = WikiDiffRequest,
    responses(
        (status = 200, body = WikiDiffResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 503, body = ErrorResponse, description = "Index unavailable"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_wiki_diff(
    State(state): State<cce_server_shared::context::SharedContext>,
    Path(project_id): Path<i64>,
    Json(request): Json<WikiDiffRequest>,
) -> ApiResult<WikiDiffResponse> {
    match wiki::diff(&state, project_id, &request.baseline).await {
        Ok(response) => ApiResult::Success(response),
        Err(error) => ApiResult::Error(relation_error(error)),
    }
}
