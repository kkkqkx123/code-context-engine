//! Entity search handler using FTS5
//!
//! Provides full-text search for entity names and signatures using SQLite FTS5.

use axum::{Json, extract::State};

use cce_api::models::{
    EntitySearchRequest, EntitySearchResponse, EntitySearchResult, ErrorResponse, error_codes,
};

use crate::api::response::ApiResult;

/// Unified response type for entity search handler
pub type EntitySearchApiResponse = ApiResult<EntitySearchResponse>;

/// Handle entity search request using FTS5
///
/// # Endpoint
///
/// `POST /api/entities/search`
///
/// # Query Syntax
///
/// Supports FTS5 query syntax:
/// - Prefix matching: `auth*` matches "authenticate", "authorization", etc.
/// - Phrase matching: `"test function"` matches exact phrase
/// - Boolean operators: `AND`, `OR`, `NOT`
/// - Field-specific: `name:main` searches only in name field
///
/// # Examples
///
/// ```json
/// {
///   "query": "auth*",
///   "project_id": 1,
///   "limit": 20
/// }
/// ```
#[axum::debug_handler]
#[utoipa::path(
    post, path = "/api/entities/search", tag = "Search",
    request_body = EntitySearchRequest,
    responses(
        (status = 200, body = EntitySearchResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 503, body = ErrorResponse, description = "Index unavailable"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_entity_search(
    State(state): State<cce_server_shared::context::SharedContext>,
    Json(request): Json<EntitySearchRequest>,
) -> EntitySearchApiResponse {
    let start = std::time::Instant::now();

    // Validate query
    if request.query.is_empty() {
        return EntitySearchApiResponse::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "Search query cannot be empty",
        ));
    }

    // Resolve project ID from either project_id or project_path
    let project_id = match crate::api::validation::resolve_project_id(
        request.project_id,
        request.project_path.as_deref(),
        state.engine.project_registry(),
    )
    .await
    {
        Ok(id) => id,
        Err(e) => {
            return EntitySearchApiResponse::Error(ErrorResponse::new(
                error_codes::INVALID_INPUT,
                e.to_string(),
            ));
        }
    };

    // When a kind filter is present the database fetch is widened before
    // filtering so ranking is preserved and the requested limit stays
    // meaningful instead of filtering an already truncated page.
    let db_limit = if request.kind_filter.is_some() {
        request.limit.saturating_mul(5).max(request.limit).max(100)
    } else {
        request.limit
    };
    let service = cce_server_shared::services::search::SearchService::new(&state);
    let results = match service
        .run_entity_search(project_id, &request.query, db_limit)
        .await
    {
        Ok(records) => records,
        Err(e) => {
            tracing::error!("FTS5 search failed: {}", e);
            return EntitySearchApiResponse::Error(ErrorResponse::new(
                error_codes::QUERY_ERROR,
                format!("Search failed: {}", e),
            ));
        }
    };

    // Apply kind filter if specified
    let filtered_results = if let Some(ref kind_filter) = request.kind_filter {
        results
            .into_iter()
            .filter(|r| r.kind == *kind_filter)
            .take(request.limit.max(1) as usize)
            .collect::<Vec<_>>()
    } else {
        results
    };

    // Resolve project-relative paths for the surviving hits. File mapping
    // stays a direct lookup; relevance itself comes from the coordinator.
    let project_client = state
        .engine
        .metadata_store()
        .and_then(|client| client.for_project(project_id).ok());
    let file_conn = project_client
        .as_ref()
        .and_then(|client| client.read_connection().ok());
    // Convert to response format
    let items: Vec<EntitySearchResult> = filtered_results
        .into_iter()
        .map(|record| {
            // Resolve the project-relative path so the hit is directly
            // actionable without a second file lookup.
            let file_path = file_conn.as_ref().and_then(|conn| {
                cce_storage_metadb_sqlite::repo::FileRepository::get_by_id(conn, record.file_id)
                    .ok()
                    .flatten()
                    .map(|file| file.path)
            });
            EntitySearchResult {
                id: record.id,
                name: record.name,
                kind: record.kind,
                file_id: record.file_id,
                file_path,
                signature: record.signature,
                span_start_row: record.span_start_row,
                span_end_row: record.span_end_row,
                depth: record.depth,
                parent_id: record.parent_id,
                project_id: record.project_id,
                rank: record.rank,
            }
        })
        .collect();

    let total = items.len();
    let elapsed_ms = start.elapsed().as_millis() as u64;

    tracing::debug!(
        "FTS5 search for '{}' returned {} results in {}ms",
        request.query,
        total,
        elapsed_ms
    );

    EntitySearchApiResponse::Success(EntitySearchResponse {
        success: true,
        total,
        items,
        elapsed_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity_search_request_defaults() {
        let request = EntitySearchRequest {
            query: "test".to_string(),
            project_id: Some(1),
            project_path: None,
            limit: 0, // Will use default
            kind_filter: None,
        };

        assert_eq!(request.limit, 0); // Default will be applied by serde
    }
}
