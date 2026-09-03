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
pub async fn handle_entity_search(
    State(state): State<crate::api::state::AppState>,
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

    // Get SQLite connection from state
    let sqlite_client = match &state.metadata_store {
        Some(client) => match client.for_project(project_id) {
            Ok(project) => project,
            Err(e) => {
                return EntitySearchApiResponse::Error(ErrorResponse::new(
                    error_codes::INDEX_NOT_INITIALIZED,
                    format!("Failed to open project database: {e}"),
                ));
            }
        },
        None => {
            return EntitySearchApiResponse::Error(ErrorResponse::new(
                error_codes::INDEX_NOT_INITIALIZED,
                "SQLite database not initialized",
            ));
        }
    };

    // Execute FTS5 search
    let conn = match sqlite_client.read_connection() {
        Ok(c) => c,
        Err(e) => {
            return EntitySearchApiResponse::Error(ErrorResponse::new(
                error_codes::INDEX_NOT_INITIALIZED,
                format!("Failed to get database connection: {}", e).as_str(),
            ));
        }
    };

    let active_epoch =
        cce_storage_sqlite::ProjectIndexManifestRepository::get_active(&conn, project_id)
            .ok()
            .flatten()
            .map(|manifest| manifest.data_epoch)
            .or_else(|| {
                conn.query_row(
                    "SELECT value FROM project_meta WHERE project_id = ?1 AND key = 'active_epoch'",
                    rusqlite::params![project_id],
                    |row| {
                        let value: String = row.get(0)?;
                        value.parse().map_err(|_| rusqlite::Error::InvalidQuery)
                    },
                )
                .ok()
            })
            .unwrap_or(0);

    let results = match cce_storage_sqlite::repo::EntityRepository::search_fts_at_epoch(
        &conn,
        &request.query,
        project_id,
        request.limit,
        active_epoch,
    ) {
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
            .collect::<Vec<_>>()
    } else {
        results
    };

    // Convert to response format
    let items: Vec<EntitySearchResult> = filtered_results
        .into_iter()
        .map(|record| EntitySearchResult {
            id: record.id,
            name: record.name,
            kind: record.kind,
            file_id: record.file_id,
            signature: record.signature,
            span_start_row: record.span_start_row,
            span_end_row: record.span_end_row,
            depth: record.depth,
            parent_id: record.parent_id,
            project_id: record.project_id,
            rank: 1.0, // FTS5 rank is handled by ORDER BY in SQL, set to 1.0 for now
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
