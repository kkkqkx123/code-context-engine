//! AST diagnosis tool handler
//!
//! Provides syntax error detection based on tree-sitter parsing results.
//! Can locate code format issues such as unclosed brackets, unclosed strings,
//! missing semicolons, etc.

use axum::Json;

use cce_api::models::{DiagnoseApiResponse, DiagnoseRequest};
use cce_server_shared::services::tools::{self, DiagnosisSpec};

use super::diagnose_result_from_view;

/// Handle AST diagnosis
///
/// # Endpoint
///
/// `POST /api/tools/diagnose`
#[utoipa::path(
    post, path = "/api/tools/diagnose", tag = "Tools",
    request_body = DiagnoseRequest,
    responses(
        (status = 200, body = DiagnoseApiResponse, description = "Diagnosis result, errors reported in-band")
    )
)]
pub async fn handle_diagnose(Json(request): Json<DiagnoseRequest>) -> Json<DiagnoseApiResponse> {
    let spec = DiagnosisSpec {
        code: request.code,
        language: request.language,
        file_name: request.file_name,
        include_ast: request.include_ast,
    };
    match tools::diagnose(spec) {
        Ok(view) => Json(DiagnoseApiResponse {
            success: true,
            result: Some(diagnose_result_from_view(view)),
            error: None,
        }),
        Err(error) => Json(DiagnoseApiResponse {
            success: false,
            result: None,
            error: Some(error.to_string()),
        }),
    }
}
