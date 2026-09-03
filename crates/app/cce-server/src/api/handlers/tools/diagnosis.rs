//! AST diagnosis tool handler
//!
//! Provides syntax error detection based on tree-sitter parsing results.
//! Can locate code format issues such as unclosed brackets, unclosed strings,
//! missing semicolons, etc.

use axum::{Json, extract::State};
use serde::Serialize;

use cce_api::models::DiagnoseRequest;
use cce_orchestrator::{DiagnosisRequest, DiagnosisResponse};
use cce_types::language::Language;

use crate::api::AppState;

/// AST diagnosis response
#[derive(Debug, Serialize)]
pub struct DiagnoseApiResponse {
    /// Whether the operation succeeded
    pub success: bool,
    /// Diagnosis result (if successful)
    pub result: Option<DiagnosisResponse>,
    /// Error message (if failed)
    pub error: Option<String>,
}

/// Handle AST diagnosis
///
/// # Endpoint
///
/// `POST /api/tools/diagnose`
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
        Ok(response) => Json(DiagnoseApiResponse {
            success: true,
            result: Some(response),
            error: None,
        }),
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
