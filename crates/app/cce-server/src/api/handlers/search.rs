//! Search handlers
//!
//! This module provides handlers for search operations including:
//! - Vector search
//! - BM25 search
//! - Hybrid search
//!
//! Handlers only validate wire arguments and render responses. Query
//! assembly, execution, and conversion live in the shared query service
//! (`cce_server_shared::services::search`) together with the MCP surface.

use axum::{Json, extract::State};

use crate::api::validation;
use cce_server_shared::services::search::{
    AggregatedQuerySpec, SearchService, SearchServiceError, SharedQueryFields, SingleQuerySpec,
    SubQuerySpec,
};
use cce_utils::text::is_blank;

use cce_api::models::{
    AggregatedSearchRequest, ErrorResponse, SearchRequest, SearchResponse, error_codes,
};

use crate::api::response::ApiResult;

/// Unified response type for search handlers
pub type SearchApiResponse = ApiResult<SearchResponse>;

/// Map a service failure to the transport error shape.
fn service_error(error: SearchServiceError) -> ErrorResponse {
    match error {
        SearchServiceError::InvalidInput(message) | SearchServiceError::Project(message) => {
            ErrorResponse::new(error_codes::INVALID_INPUT, message)
        }
        SearchServiceError::Execution(message) => {
            ErrorResponse::new(error_codes::QUERY_ERROR, message)
        }
    }
}

/// Handle search request
#[axum::debug_handler]
#[utoipa::path(
    post, path = "/api/search", tag = "Search",
    request_body = SearchRequest,
    responses(
        (status = 200, body = SearchResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 503, body = ErrorResponse, description = "Index unavailable"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_search(
    State(state): State<cce_server_shared::context::SharedContext>,
    Json(request): Json<SearchRequest>,
) -> SearchApiResponse {
    let start = std::time::Instant::now();

    // Resolve project ID from either project_id or project_path
    let project_id = match validation::resolve_project_id(
        request.project_id,
        request.project_path.as_deref(),
        state.engine.project_registry(),
    )
    .await
    {
        Ok(id) => id,
        Err(e) => {
            return SearchApiResponse::Error(ErrorResponse::new(
                error_codes::INVALID_INPUT,
                e.to_string(),
            ));
        }
    };

    // Validate limit
    const MAX_LIMIT: usize = 100;
    if let Err(e) = validation::validate_limit(request.limit, MAX_LIMIT) {
        return SearchApiResponse::Error(ErrorResponse::new(
            error_codes::INVALID_INPUT,
            e.to_string(),
        ));
    }

    // Validate query
    if is_blank(&request.query) {
        return SearchApiResponse::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "The query text cannot be empty",
        ));
    }

    // Validate glob patterns
    if let Err(e) = validation::validate_glob_patterns(&request.exclude_patterns) {
        return SearchApiResponse::Error(ErrorResponse::new(
            error_codes::INVALID_INPUT,
            format!("Invalid exclude pattern: {}", e),
        ));
    }

    if let Err(e) = validation::validate_glob_patterns(&request.include_patterns) {
        return SearchApiResponse::Error(ErrorResponse::new(
            error_codes::INVALID_INPUT,
            format!("Invalid include pattern: {}", e),
        ));
    }

    let service = SearchService::new(&state);
    let sources = match SearchService::parse_search_sources(&request.query_type, "query type") {
        Ok(sources) => sources,
        Err(error) => return SearchApiResponse::Error(service_error(error)),
    };
    // Exclude content types: reject unknown values instead of ignoring them.
    let mut exclude_content_types = Vec::with_capacity(request.exclude_content_types.len());
    for raw in &request.exclude_content_types {
        match SearchService::parse_exclude_content_type(raw) {
            Ok(content_type) => exclude_content_types.push(content_type),
            Err(error) => return SearchApiResponse::Error(service_error(error)),
        }
    }
    // Category filters: unknown names are rejected so typos stay visible.
    let mut include_categories = Vec::with_capacity(request.include_categories.len());
    for raw in &request.include_categories {
        match SearchService::parse_file_category(raw, "include") {
            Ok(category) => include_categories.push(category),
            Err(error) => return SearchApiResponse::Error(service_error(error)),
        }
    }
    let mut exclude_categories = Vec::with_capacity(request.exclude_categories.len());
    for raw in &request.exclude_categories {
        match SearchService::parse_file_category(raw, "exclude") {
            Ok(category) => exclude_categories.push(category),
            Err(error) => return SearchApiResponse::Error(service_error(error)),
        }
    }
    let query_intent = match SearchService::parse_query_intent(request.query_intent.as_deref()) {
        Ok(intent) => intent,
        Err(error) => return SearchApiResponse::Error(service_error(error)),
    };

    let spec = SingleQuerySpec {
        query: request.query,
        sources,
        fields: SharedQueryFields {
            limit: Some(request.limit),
            min_score: request.min_score,
            enable_rerank: request.enable_rerank,
            rerank_max_candidates: request.rerank_max_candidates,
            directory_prefix: request.directory_prefix,
            exclude_content_types,
            include_categories,
            exclude_categories,
            exclude_patterns: request.exclude_patterns,
            include_patterns: request.include_patterns,
            query_intent,
            expand: request.expand.unwrap_or(false),
        },
    };
    match service.run_single(project_id, spec).await {
        Ok(outcome) => SearchApiResponse::Success(SearchResponse {
            success: true,
            total: outcome.total,
            items: outcome.items,
            elapsed_ms: start.elapsed().as_millis() as u64,
            sources_used: outcome.sources,
            failed_sub_queries: outcome.failed_sub_queries,
            relation_epoch: outcome.relation_epoch,
            relation_stale: outcome.relation_stale,
        }),
        Err(error) => {
            tracing::error!("Search failed: {}", error);
            SearchApiResponse::Error(service_error(error))
        }
    }
}

/// Handle aggregated search request (multi-query with parallel retrieval)
#[utoipa::path(
    post, path = "/api/search/aggregated", tag = "Search",
    request_body = AggregatedSearchRequest,
    responses(
        (status = 200, body = SearchResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 503, body = ErrorResponse, description = "Index unavailable"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_aggregated_search(
    State(state): State<cce_server_shared::context::SharedContext>,
    Json(request): Json<AggregatedSearchRequest>,
) -> SearchApiResponse {
    let start = std::time::Instant::now();

    // Resolve project ID from either project_id or project_path
    let project_id = match validation::resolve_project_id(
        request.project_id,
        request.project_path.as_deref(),
        state.engine.project_registry(),
    )
    .await
    {
        Ok(id) => id,
        Err(e) => {
            return SearchApiResponse::Error(ErrorResponse::new(
                error_codes::INVALID_INPUT,
                e.to_string(),
            ));
        }
    };

    // Validate sub-queries count
    const MAX_SUB_QUERIES: usize = 10;
    if let Err(e) =
        validation::validate_sub_queries_count(request.sub_queries.len(), MAX_SUB_QUERIES)
    {
        return SearchApiResponse::Error(ErrorResponse::new(
            error_codes::INVALID_INPUT,
            e.to_string(),
        ));
    }

    // Validate limit
    const MAX_LIMIT: usize = 100;
    if let Err(e) = validation::validate_limit(request.limit, MAX_LIMIT) {
        return SearchApiResponse::Error(ErrorResponse::new(
            error_codes::INVALID_INPUT,
            e.to_string(),
        ));
    }

    // Validate glob patterns
    if let Err(e) = validation::validate_glob_patterns(&request.exclude_patterns) {
        return SearchApiResponse::Error(ErrorResponse::new(
            error_codes::INVALID_INPUT,
            format!("Invalid exclude pattern: {}", e),
        ));
    }

    if let Err(e) = validation::validate_glob_patterns(&request.include_patterns) {
        return SearchApiResponse::Error(ErrorResponse::new(
            error_codes::INVALID_INPUT,
            format!("Invalid include pattern: {}", e),
        ));
    }

    let service = SearchService::new(&state);
    let mut sub_queries = Vec::with_capacity(request.sub_queries.len());
    for sq in &request.sub_queries {
        let sources =
            match SearchService::parse_search_sources(&sq.query_type, "query type in sub-query") {
                Ok(sources) => sources,
                Err(error) => return SearchApiResponse::Error(service_error(error)),
            };
        // The weight scales this sub-query's candidate scores before the
        // cross-query merge; only finite non-negative weights are meaningful.
        if !sq.weight.is_finite() || sq.weight < 0.0 {
            return SearchApiResponse::Error(ErrorResponse::new(
                error_codes::INVALID_INPUT,
                format!(
                    "Invalid weight in sub-query '{}': must be finite and >= 0",
                    sq.text
                ),
            ));
        }
        sub_queries.push(SubQuerySpec {
            text: sq.text.clone(),
            sources,
            weight: sq.weight,
        });
    }

    let mut exclude_content_types = Vec::with_capacity(request.exclude_content_types.len());
    for raw in &request.exclude_content_types {
        match SearchService::parse_exclude_content_type(raw) {
            Ok(content_type) => exclude_content_types.push(content_type),
            Err(error) => return SearchApiResponse::Error(service_error(error)),
        }
    }
    let mut include_categories = Vec::with_capacity(request.include_categories.len());
    for raw in &request.include_categories {
        match SearchService::parse_file_category(raw, "include") {
            Ok(category) => include_categories.push(category),
            Err(error) => return SearchApiResponse::Error(service_error(error)),
        }
    }
    let mut exclude_categories = Vec::with_capacity(request.exclude_categories.len());
    for raw in &request.exclude_categories {
        match SearchService::parse_file_category(raw, "exclude") {
            Ok(category) => exclude_categories.push(category),
            Err(error) => return SearchApiResponse::Error(service_error(error)),
        }
    }
    let query_intent = match SearchService::parse_query_intent(request.query_intent.as_deref()) {
        Ok(intent) => intent,
        Err(error) => return SearchApiResponse::Error(service_error(error)),
    };

    let spec = AggregatedQuerySpec {
        sub_queries,
        fields: SharedQueryFields {
            limit: Some(request.limit),
            min_score: request.min_score,
            enable_rerank: request.enable_rerank,
            rerank_max_candidates: request.rerank_max_candidates,
            directory_prefix: request.directory_prefix,
            exclude_content_types,
            include_categories,
            exclude_categories,
            exclude_patterns: request.exclude_patterns,
            include_patterns: request.include_patterns,
            query_intent,
            expand: request.expand.unwrap_or(false),
        },
    };
    match service.run_aggregated(project_id, spec).await {
        Ok(outcome) => SearchApiResponse::Success(SearchResponse {
            success: true,
            total: outcome.total,
            items: outcome.items,
            elapsed_ms: start.elapsed().as_millis() as u64,
            sources_used: outcome.sources,
            failed_sub_queries: outcome.failed_sub_queries,
            relation_epoch: outcome.relation_epoch,
            relation_stale: outcome.relation_stale,
        }),
        Err(error) => {
            tracing::error!("Aggregated search failed: {}", error);
            SearchApiResponse::Error(service_error(error))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_request_defaults() {
        let request = SearchRequest {
            project_id: Some(1),
            project_path: None,
            query: "test query".to_string(),
            query_type: "".to_string(),
            limit: 0,
            min_score: None,
            directory_prefix: None,
            exclude_patterns: vec![],
            include_patterns: vec![],
            exclude_content_types: vec![],
            include_categories: vec![],
            exclude_categories: vec![],
            enable_rerank: None,
            rerank_max_candidates: None,
            query_intent: None,
            expand: None,
        };

        assert_eq!(request.query_type, ""); // will use default
        assert_eq!(request.limit, 0); // will use default
    }

    #[test]
    fn test_error_response_creation() {
        let error = ErrorResponse::new(error_codes::INVALID_REQUEST, "Invalid input");
        assert!(!error.success);
        assert_eq!(error.error.code, error_codes::INVALID_REQUEST);
        assert_eq!(error.error.message, "Invalid input");
    }
}
