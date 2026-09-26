use axum::{Json, extract::State};

use cce_api::models::{
    KeywordSearchApiResponse, KeywordSearchRequest, KeywordSearchResult, KeywordTermOperator,
};
use cce_orchestrator::KeywordSearchRequest as OrchKeywordSearchRequest;
use cce_storage_bm25::TermOperator;

use super::to_api_model;
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
    let Some(tool) = state.keyword_search.as_ref() else {
        return Json(KeywordSearchApiResponse {
            success: false,
            result: None,
            error: Some("Keyword search tool not initialized".to_string()),
        });
    };

    let mut request = OrchKeywordSearchRequest {
        query: request.query,
        top_n: request.top_n,
        project_id: request.project_id,
        epoch: request.epoch,
        term_operator: match request.term_operator {
            KeywordTermOperator::Or => TermOperator::Or,
            KeywordTermOperator::And => TermOperator::And,
        },
    };
    if request.epoch.is_none()
        && let Some(sqlite) = &state.metadata_store
        && let Ok(project) = sqlite.for_project(request.project_id)
        && let Ok(conn) = project.read_connection()
    {
        request.epoch = cce_storage_sqlite::ProjectIndexManifestRepository::get_active(
            &conn,
            request.project_id,
        )
        .ok()
        .flatten()
        .map(|manifest| manifest.data_epoch)
        .or_else(|| {
            conn.query_row(
                "SELECT value FROM project_meta WHERE project_id = ?1 AND key = 'active_epoch'",
                rusqlite::params![request.project_id],
                |row| {
                    let value: String = row.get(0)?;
                    value.parse().map_err(|_| rusqlite::Error::InvalidQuery)
                },
            )
            .ok()
        });
    }

    match tool.search(request).await {
        Ok(response) => match to_api_model::<_, KeywordSearchResult>(response) {
            Ok(result) => Json(KeywordSearchApiResponse {
                success: true,
                result: Some(result),
                error: None,
            }),
            Err(e) => Json(KeywordSearchApiResponse {
                success: false,
                result: None,
                error: Some(format!("Failed to serialize response: {}", e)),
            }),
        },
        Err(e) => Json(KeywordSearchApiResponse {
            success: false,
            result: None,
            error: Some(e.to_string()),
        }),
    }
}
