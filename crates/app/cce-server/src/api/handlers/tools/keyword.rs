use axum::{Json, extract::State};
use serde::Serialize;

use cce_orchestrator::{KeywordSearchRequest, KeywordSearchResponse};

use crate::api::AppState;

#[derive(Debug, Serialize)]
pub struct KeywordSearchApiResponse {
    pub success: bool,
    pub data: Option<KeywordSearchResponse>,
    pub error: Option<String>,
}

pub async fn handle_keyword_search(
    State(state): State<AppState>,
    Json(request): Json<KeywordSearchRequest>,
) -> Json<KeywordSearchApiResponse> {
    let tool = match &state.keyword_search {
        Some(t) => t,
        None => {
            return Json(KeywordSearchApiResponse {
                success: false,
                data: None,
                error: Some("Keyword search tool not initialized".to_string()),
            });
        }
    };

    let mut request = request;
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
        Ok(response) => Json(KeywordSearchApiResponse {
            success: true,
            data: Some(response),
            error: None,
        }),
        Err(e) => Json(KeywordSearchApiResponse {
            success: false,
            data: None,
            error: Some(e.to_string()),
        }),
    }
}
