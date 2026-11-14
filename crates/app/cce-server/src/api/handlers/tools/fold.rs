//! Stateless file fold handler
//!
//! Exposes the shared fold service over HTTP. The handler only maps the wire
//! payload onto the service spec and the result back onto the response shape.

use axum::Json;

use cce_api::models::{
    BatchFoldRequest, BatchFoldResponse, BatchFoldResultItem, BatchFoldStats, ErrorResponse,
    FoldRequest, FoldResponse, error_codes,
};
use cce_server_shared::services::tools::{self, FoldBatchItemSpec, FoldBatchSpec, FoldSpec};

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
    let folded = tools::fold(FoldSpec {
        text: request.text,
        language: request.language,
        file_name: request.file_name,
        max_tokens: request.max_tokens,
        mode: request.mode,
    });
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
    let spec = FoldBatchSpec {
        items: request
            .items
            .into_iter()
            .map(|item| FoldBatchItemSpec {
                id: item.id,
                text: item.text,
                language: item.language,
                file_name: item.file_name,
                max_tokens: item.max_tokens,
                mode: item.mode,
            })
            .collect(),
        language: request.language,
        max_tokens: request.max_tokens,
        mode: request.mode,
        max_concurrency: request.max_concurrency,
    };
    match tools::fold_batch(spec) {
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
        Err(error) => ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            error.to_string(),
        )),
    }
}
