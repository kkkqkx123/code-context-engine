//! Function detail handler
//!
//! The wire parameter is a symbol seed; resolution and the entity view come
//! from the shared relation service.

use axum::extract::{Path, State};

use cce_api::models::{ErrorResponse, FunctionDetailResponse, FunctionInfo};

use crate::api::response::{ApiResult, relation_error};
use cce_server_shared::services::relation;

/// Unified response type for detail handler
pub type DetailApiResponse = ApiResult<FunctionDetailResponse>;

/// Handle function detail request
///
/// The `id` parameter is a stable symbol ID (string), consistent with
/// the calls/callers/call-chain/class endpoints.
#[utoipa::path(
    get, path = "/api/project/{project_id}/function/{id}", tag = "Entity",
    params(("project_id" = i64, Path, description = "Project id"), ("id" = String, Path, description = "Function id")),
    responses(
        (status = 200, body = FunctionDetailResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 503, body = ErrorResponse, description = "Index unavailable"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_function_detail(
    State(state): State<cce_server_shared::context::SharedContext>,
    Path((project_id, id)): Path<(i64, String)>,
) -> DetailApiResponse {
    match relation::function_detail(&state, project_id, &id).await {
        Ok(detail) => DetailApiResponse::Success(FunctionDetailResponse {
            success: true,
            function: FunctionInfo {
                id,
                name: detail.entity.name,
                signature: detail.signature,
                parameters: Vec::new(),
                return_type: None,
                file_path: detail.entity.file_path,
                start_line: detail.start_line,
                end_line: detail.end_line,
                doc_comment: detail.doc_comment,
            },
            relation_info: detail.stamp.stale,
        }),
        Err(error) => DetailApiResponse::Error(relation_error(error)),
    }
}
