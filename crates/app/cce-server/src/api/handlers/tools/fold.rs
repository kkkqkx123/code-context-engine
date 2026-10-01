//! Stateless file fold handler
//!
//! Exposes the orchestrator fold tool over HTTP. The handler borrows the
//! shared parse coordinator, maps caller language names, and always returns
//! the fold shape: degenerate inputs degrade instead of erroring.

use axum::Json;

use cce_api::models::{
    BatchFoldRequest, BatchFoldResponse, BatchFoldResultItem, BatchFoldStats, ErrorResponse,
    FoldRequest, FoldResponse, error_codes,
};
use cce_orchestrator::{
    FileFoldBatchItem, FileFoldBatchRequest, FileFoldMode, FileFoldRequest, FileFoldTool,
};
use cce_parser::parser::ParseCoordinator;
use cce_types::language::Language;

use crate::api::response::ApiResult;

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
pub async fn handle_fold(Json(request): Json<FoldRequest>) -> Json<FoldResponse> {
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

/// Handle stateless batch file folding
///
/// # Endpoint
///
/// `POST /api/tools/fold/batch`
#[utoipa::path(
    post, path = "/api/tools/fold/batch", tag = "Tools",
    request_body = BatchFoldRequest,
    responses(
        (status = 200, body = BatchFoldResponse, description = "Batch fold result, entry errors reported in-band"),
        (status = 400, body = ErrorResponse, description = "Invalid batch request")
    )
)]
pub async fn handle_fold_batch(
    Json(request): Json<BatchFoldRequest>,
) -> ApiResult<BatchFoldResponse> {
    let mut tool_request = FileFoldBatchRequest::new(
        request
            .items
            .into_iter()
            .map(|item| {
                let mut entry = FileFoldBatchItem::new(item.id, item.text);
                if let Some(name) = item.language.as_deref()
                    && let Some(language) = Language::from_name(name)
                {
                    entry = entry.with_language(language);
                }
                if let Some(file_name) = item.file_name {
                    entry = entry.with_file_name(file_name);
                }
                if let Some(max_tokens) = item.max_tokens {
                    entry = entry.with_max_tokens(max_tokens);
                }
                if let Some(name) = item.mode.as_deref() {
                    entry = entry.with_mode(FileFoldMode::parse(Some(name)));
                }
                entry
            })
            .collect(),
    );
    if let Some(name) = request.language.as_deref()
        && let Some(language) = Language::from_name(name)
    {
        tool_request = tool_request.with_language(language);
    }
    if let Some(max_tokens) = request.max_tokens {
        tool_request = tool_request.with_max_tokens(max_tokens);
    }
    if let Some(name) = request.mode.as_deref() {
        tool_request = tool_request.with_mode(FileFoldMode::parse(Some(name)));
    }
    if let Some(max_concurrency) = request.max_concurrency {
        tool_request = tool_request.with_max_concurrency(max_concurrency);
    }

    match FileFoldTool::fold_batch(tool_request) {
        Ok(folded) => ApiResult::Success(BatchFoldResponse {
            results: folded
                .results
                .into_iter()
                .map(|entry| BatchFoldResultItem {
                    id: entry.id,
                    folded_text: entry.folded_text,
                    language: entry.language,
                    structure_known: entry.structure_known,
                    original_tokens: entry.original_tokens,
                    folded_tokens: entry.folded_tokens,
                    kept_sections: entry.kept_sections,
                    dropped_sections: entry.dropped_sections,
                })
                .collect(),
            stats: BatchFoldStats {
                total_original_tokens: folded.stats.total_original_tokens,
                total_folded_tokens: folded.stats.total_folded_tokens,
                structure_known_count: folded.stats.structure_known_count,
            },
        }),
        Err(e) => ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            e.to_string(),
        )),
    }
}
