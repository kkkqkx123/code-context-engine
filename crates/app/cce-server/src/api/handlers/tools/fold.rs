//! Stateless file fold handler
//!
//! Exposes the orchestrator fold tool over HTTP. The handler borrows the
//! shared parse coordinator, maps caller language names, and always returns
//! the fold shape: degenerate inputs degrade instead of erroring.

use axum::Json;

use cce_api::models::{FoldRequest, FoldResponse};
use cce_orchestrator::{FileFoldMode, FileFoldRequest, FileFoldTool};
use cce_parser::parser::ParseCoordinator;
use cce_types::language::Language;

/// Handle stateless file folding
///
/// # Endpoint
///
/// `POST /api/tools/fold`
#[utoipa::path(
    post, path = "/api/tools/fold", tag = "Tools",
    request_body = FoldRequest,
    responses(
        (status = 200, body = FoldResponse, description = "Fold result, errors reported in-band")
    )
)]
pub async fn handle_fold(
    Json(request): Json<FoldRequest>,
) -> Json<FoldResponse> {
    let language = request.language.as_deref().and_then(Language::from_name);
    let mode = request
        .mode
        .as_deref()
        .map(|name| FileFoldMode::parse(Some(name)));

    let mut tool_request = FileFoldRequest::new(request.text);
    if let Some(resolved) = language {
        tool_request = tool_request.with_language(resolved);
    }
    if let Some(file_name) = request.file_name {
        tool_request = tool_request.with_file_name(file_name);
    }
    if let Some(max_tokens) = request.max_tokens {
        tool_request = tool_request.with_max_tokens(max_tokens);
    }
    if let Some(parsed_mode) = mode {
        tool_request = tool_request.with_mode(parsed_mode);
    }

    let mut coordinator = ParseCoordinator::new();
    let folded = FileFoldTool::fold(&mut coordinator, tool_request);

    Json(FoldResponse {
        success: true,
        folded_text: folded.folded_text,
        language: folded.language,
        structure_known: folded.structure_known,
        original_tokens: folded.original_tokens,
        folded_tokens: folded.folded_tokens,
        kept_sections: folded.kept_sections,
        dropped_sections: folded.dropped_sections,
    })
}
