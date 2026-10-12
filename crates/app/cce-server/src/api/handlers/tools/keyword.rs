use axum::{Json, extract::State};

use cce_api::models::{KeywordSearchApiResponse, KeywordSearchRequest, KeywordTermOperator};
use cce_orchestrator::KeywordSearchRequest as OrchKeywordSearchRequest;
use cce_orchestrator::KeywordSearchTool;
use cce_storage_bm25::TermOperator;

use super::keyword_result_from_orch;
use crate::api::AppState;

#[utoipa::path(
    post, path = "/api/tools/keyword-search", tag = "Tools",
    request_body = KeywordSearchRequest,
    responses(
        (status = 200, body = KeywordSearchApiResponse, description = "Keyword search result, errors reported in-band")
    )
)]
pub async fn handle_keyword_search(
    State(state): State<AppState>,
    Json(request): Json<KeywordSearchRequest>,
) -> Json<KeywordSearchApiResponse> {
    let tool = KeywordSearchTool::new(state.engine.fulltext_clone());

    let request = OrchKeywordSearchRequest {
        query: request.query,
        top_n: request.top_n,
        project_id: request.project_id,
        epoch: request.epoch,
        offset: request.offset,
        term_operator: match request.term_operator {
            KeywordTermOperator::Or => TermOperator::Or,
            KeywordTermOperator::And => TermOperator::And,
        },
    };

    match tool.search(request).await {
        Ok(response) => Json(KeywordSearchApiResponse {
            success: true,
            result: Some(keyword_result_from_orch(response)),
            error: None,
        }),
        Err(e) => Json(KeywordSearchApiResponse {
            success: false,
            result: None,
            error: Some(e.to_string()),
        }),
    }
}
