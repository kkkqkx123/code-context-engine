//! AST diagnosis tool handler
//!
//! Provides syntax error detection based on tree-sitter parsing results.
//! Can locate code format issues such as unclosed brackets, unclosed strings,
//! missing semicolons, etc.

use axum::{Json, extract::State};

use cce_api::models::{DiagnoseApiResponse, DiagnoseRequest, DiagnoseResult};
use cce_orchestrator::DiagnosisRequest;
use cce_types::language::Language;

use super::to_api_model;
use crate::api::AppState;

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
pub async fn handle_diagnose(
    State(state): State<AppState>,
    Json(request): Json<DiagnoseRequest>,
) -> Json<DiagnoseApiResponse> {
    let mut diagnosis = state.ast_diagnosis.lock().await;

    let mut req = DiagnosisRequest::new(&request.code);

    if let Some(lang) = &request.language {
        if let Ok(l) = parse_language(lang) {
            req = req.with_language(l);
        }
    }

    if let Some(file_name) = &request.file_name {
        req = req.with_file_name(file_name);
    }

    req = req.with_ast(request.include_ast);

    match diagnosis.diagnose(req) {
        Ok(response) => match to_api_model::<_, DiagnoseResult>(response) {
            Ok(result) => Json(DiagnoseApiResponse {
                success: true,
                result: Some(result),
                error: None,
            }),
            Err(e) => Json(DiagnoseApiResponse {
                success: false,
                result: None,
                error: Some(format!("Failed to serialize response: {}", e)),
            }),
        },
        Err(e) => Json(DiagnoseApiResponse {
            success: false,
            result: None,
            error: Some(e.to_string()),
        }),
    }
}

/// Parse language string to Language enum
fn parse_language(s: &str) -> Result<Language, ()> {
    match s.to_lowercase().as_str() {
        "rust" => Ok(Language::Rust),
        "python" | "py" => Ok(Language::Python),
        "javascript" | "js" => Ok(Language::JavaScript),
        "typescript" | "ts" => Ok(Language::TypeScript),
        "c" => Ok(Language::C),
        "cpp" | "c++" => Ok(Language::Cpp),
        "csharp" | "c#" => Ok(Language::CSharp),
        "go" => Ok(Language::Go),
        "java" => Ok(Language::Java),
        "kotlin" | "kt" => Ok(Language::Kotlin),
        "ruby" | "rb" => Ok(Language::Ruby),
        "php" => Ok(Language::Php),
        "json" => Ok(Language::Json),
        "yaml" | "yml" => Ok(Language::Yaml),
        "toml" => Ok(Language::Toml),
        "xml" => Ok(Language::Xml),
        "html" => Ok(Language::Html),
        "css" => Ok(Language::Css),
        "scss" | "sass" => Ok(Language::Scss),
        "less" => Ok(Language::Less),
        "vue" => Ok(Language::Vue),
        "svelte" => Ok(Language::Svelte),
        "jsx" => Ok(Language::Jsx),
        "tsx" => Ok(Language::Tsx),
        _ => Err(()),
    }
}
