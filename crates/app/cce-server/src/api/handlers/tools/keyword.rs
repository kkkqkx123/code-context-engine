use axum::{Json, extract::State};

use cce_api::models::{KeywordSearchApiResponse, KeywordSearchRequest, KeywordTermOperator};
use cce_server_shared::services::tools::{self, KeywordOperator, KeywordSpec};

use super::keyword_result_from_orch;
use crate::api::SharedContext;

#[utoipa::path(
    post, path = "/api/tools/keyword-search", tag = "Tools",
    request_body = KeywordSearchRequest,
    responses(
        (status = 200, body = KeywordSearchApiResponse, description = "Keyword search result, errors reported in-band")
    )
)]
pub async fn handle_keyword_search(
    State(state): State<SharedContext>,
    Json(request): Json<KeywordSearchRequest>,
) -> Json<KeywordSearchApiResponse> {
    let operator = match request.term_operator {
        KeywordTermOperator::Or => KeywordOperator::Or,
        KeywordTermOperator::And => KeywordOperator::And,
    };
    let spec = KeywordSpec {
        query: request.query,
        top_n: request.top_n,
        project_id: request.project_id,
        epoch: request.epoch,
        offset: request.offset,
        operator,
    };
    match tools::keyword_search(&state, spec).await {
        Ok(response) => Json(KeywordSearchApiResponse {
            success: true,
            result: Some(keyword_result_from_orch(response)),
            error: None,
        }),
        Err(error) => Json(KeywordSearchApiResponse {
            success: false,
            result: None,
            error: Some(error.to_string()),
        }),
    }
}
