//! Search handlers
//!
//! This module provides handlers for search operations including:
//! - Vector search
//! - BM25 search
//! - Hybrid search

use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use serde::Serialize;
use std::sync::Arc;

use crate::api::validation;
use cce_orchestrator::SearchResult as OrchestratorResultItem;
use cce_orchestrator::query::types::{ExcludableContentType, SearchSources};
use cce_orchestrator::query::{SubQuery, types::ResultFilterConfig, types::SearchConfig};
use cce_utils::text::is_blank;

use cce_api::models::{
    AggregatedSearchRequest, CallChainNode, ErrorResponse, SearchRequest, SearchResponse,
    SearchResultItem, error_codes,
};

/// Unified response enum for search handler
#[derive(Serialize)]
#[serde(untagged)]
pub enum SearchApiResponse {
    Error(ErrorResponse),
    Success(SearchResponse),
}

impl IntoResponse for SearchApiResponse {
    fn into_response(self) -> axum::response::Response {
        match self {
            SearchApiResponse::Error(err) => {
                let status = if err.error.code == error_codes::INVALID_REQUEST {
                    StatusCode::BAD_REQUEST
                } else {
                    StatusCode::INTERNAL_SERVER_ERROR
                };
                (status, Json(err)).into_response()
            }
            SearchApiResponse::Success(resp) => (StatusCode::OK, Json(resp)).into_response(),
        }
    }
}

/// Handle search request
#[axum::debug_handler]
pub async fn handle_search(
    State(state): State<crate::api::state::AppState>,
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

    // Load project config early to apply search-level defaults
    let project_entry = match state
        .engine
        .project_registry()
        .get_or_load(project_id)
        .await
    {
        Ok(entry) => entry,
        Err(e) => {
            return SearchApiResponse::Error(ErrorResponse::new(
                error_codes::INVALID_INPUT,
                format!("Failed to load project config: {}", e),
            ));
        }
    };
    let enabled_by_default = project_entry.config.search.relation.enabled_by_default;

    // Parse query type to SearchSources
    let mut search_sources = match request.query_type.to_lowercase().as_str() {
        "vector" => SearchSources::none().with_vector(),
        "bm25" => SearchSources::none().with_bm25(),
        "hybrid" => SearchSources::default(), // vector + bm25
        "hierarchical" => SearchSources::default(),
        "summary" => SearchSources::none().with_summary(),
        "semantic_with_relations" => SearchSources::default().with_relation(),
        _ => {
            return SearchApiResponse::Error(ErrorResponse::new(
                error_codes::INVALID_REQUEST,
                format!("Invalid query type: {}", request.query_type),
            ));
        }
    };
    // When `enabled_by_default` is true, hybrid/hierarchical automatically
    // include relation-based boosting without requiring an explicit query_type.
    if enabled_by_default {
        match request.query_type.to_lowercase().as_str() {
            "hybrid" | "hierarchical" => {
                search_sources = search_sources.with_relation();
            }
            _ => {}
        }
    }

    // Build query options with project_id
    let mut query_opts = cce_orchestrator::query::types::QueryConfigBuilder::new(project_id)
        .build(&request.query)
        .with_sources(search_sources)
        .with_limit(request.limit);

    // Merge the project's search and rerank configuration into the query options
    // so runtime parameters come from the config layer.
    query_opts.config.rerank = project_entry.config.rerank.clone();
    query_opts.config.relation = project_entry.config.search.relation.clone();
    query_opts.config.boost = project_entry.config.search.boost.clone();
    query_opts.config.result = project_entry.config.search.result.clone();

    // Per-request rerank overrides take precedence over the config.
    if let Some(enable_rerank) = request.enable_rerank {
        query_opts = query_opts.with_enable_rerank(enable_rerank);
    }
    if let Some(max_candidates) = request.rerank_max_candidates {
        query_opts.config.rerank.max_candidates = max_candidates;
    }

    if let Some(min_score) = request.min_score {
        query_opts.config.result.min_score = min_score;
    }

    // Directory prefix filter
    if let Some(prefix) = &request.directory_prefix {
        query_opts = query_opts.with_directory_prefix(prefix);
    }

    // Exclude content types (e.g., test files)
    if !request.exclude_content_types.is_empty() {
        for ct in &request.exclude_content_types {
            let exclude_type = match ct.to_lowercase().as_str() {
                "test" | "tests" => Some(ExcludableContentType::Test),
                _ => {
                    tracing::warn!("Unknown exclude content type: {}", ct);
                    None
                }
            };

            if let Some(exclude_type) = exclude_type {
                query_opts = query_opts.add_exclude_content_type(exclude_type);
            }
        }
    }

    // Exclude patterns
    if !request.exclude_patterns.is_empty() {
        query_opts = query_opts.with_exclude_patterns(request.exclude_patterns);
    }

    // Include patterns
    if !request.include_patterns.is_empty() {
        query_opts = query_opts.with_include_patterns(request.include_patterns);
    }

    // Category filters
    if !request.include_categories.is_empty() {
        let categories: Vec<cce_types::FileCategory> = request
            .include_categories
            .iter()
            .filter_map(|c| cce_types::FileCategory::from_name(c))
            .collect();
        query_opts = query_opts.with_include_categories(categories);
    }
    if !request.exclude_categories.is_empty() {
        let categories: Vec<cce_types::FileCategory> = request
            .exclude_categories
            .iter()
            .filter_map(|c| cce_types::FileCategory::from_name(c))
            .collect();
        query_opts = query_opts.with_exclude_categories(categories);
    }
    // Call chain configuration (optional)
    if let Some(depth) = request.call_chain_depth {
        query_opts.config.relation.depth = depth;
    }
    if request.include_call_chain {
        query_opts = query_opts.with_relations();
    }

    // Execute query using engine's search method with project_id
    let result = state.engine.search(project_id, &query_opts).await;

    match result {
        Ok(result) => {
            let items: Vec<SearchResultItem> = result
                .items
                .into_iter()
                .map(convert_orchestrator_result)
                .collect();

            let response = SearchResponse {
                success: true,
                total: result.total,
                items,
                elapsed_ms: start.elapsed().as_millis() as u64,
                sources_used: result.sources,
            };

            SearchApiResponse::Success(response)
        }
        Err(e) => {
            tracing::error!("Search failed: {}", e);
            SearchApiResponse::Error(ErrorResponse::new(error_codes::QUERY_ERROR, e.to_string()))
        }
    }
}

/// Handle aggregated search request (multi-query with parallel retrieval)
pub async fn handle_aggregated_search(
    State(state): State<crate::api::state::AppState>,
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

    // Load project config for default-relation handling and global search config
    let project_entry = match state
        .engine
        .project_registry()
        .get_or_load(project_id)
        .await
    {
        Ok(entry) => entry,
        Err(e) => {
            return SearchApiResponse::Error(ErrorResponse::new(
                error_codes::INVALID_INPUT,
                format!("Failed to load project config: {}", e),
            ));
        }
    };
    let enabled_by_default = project_entry.config.search.relation.enabled_by_default;

    // Build aggregated query options
    let mut sub_queries = Vec::with_capacity(request.sub_queries.len());
    for sq in &request.sub_queries {
        // Parse query type to SearchSources
        let mut search_sources = match sq.query_type.to_lowercase().as_str() {
            "vector" => SearchSources::none().with_vector(),
            "bm25" => SearchSources::none().with_bm25(),
            "hybrid" => SearchSources::default(),
            "summary" => SearchSources::none().with_summary(),
            _ => {
                return SearchApiResponse::Error(ErrorResponse::new(
                    error_codes::INVALID_INPUT,
                    format!("Invalid query type in sub-query: {}", sq.query_type),
                ));
            }
        };
        if enabled_by_default && sq.query_type.to_lowercase() == "hybrid" {
            search_sources = search_sources.with_relation();
        }

        sub_queries.push(SubQuery {
            text: sq.text.clone(),
            sources: search_sources,
            weight: sq.weight,
        });
    }

    // Build global config
    let mut global_config = SearchConfig {
        result: ResultFilterConfig {
            limit: request.limit,
            ..Default::default()
        },
        ..Default::default()
    };
    global_config.relation = project_entry.config.search.relation.clone();
    global_config.boost = project_entry.config.search.boost.clone();
    global_config.rerank = project_entry.config.rerank.clone();
    global_config.result = {
        let mut r = project_entry.config.search.result.clone();
        r.limit = request.limit;
        r
    };

    if let Some(min_score) = request.min_score {
        global_config.result.min_score = min_score;
    }

    // Per-request rerank overrides take precedence over the config.
    if let Some(max_candidates) = request.rerank_max_candidates {
        global_config.rerank.max_candidates = max_candidates;
    }

    // Build filters
    let filters = if request.directory_prefix.is_some() || !request.exclude_content_types.is_empty()
    {
        let exclude_types: Vec<ExcludableContentType> = request
            .exclude_content_types
            .iter()
            .filter_map(|ct| match ct.to_lowercase().as_str() {
                "test" | "tests" => Some(ExcludableContentType::Test),
                _ => {
                    tracing::warn!("Unknown exclude content type: {}", ct);
                    None
                }
            })
            .collect();

        Some(cce_orchestrator::query::FilterOptions {
            directory_prefix: request.directory_prefix,
            exclude_content_types: exclude_types,
            include_categories: request
                .include_categories
                .iter()
                .filter_map(|c| cce_types::FileCategory::from_name(c))
                .collect(),
            exclude_categories: request
                .exclude_categories
                .iter()
                .filter_map(|c| cce_types::FileCategory::from_name(c))
                .collect(),
        })
    } else {
        None
    };

    let agg_options = cce_orchestrator::query::AggregatedQueryOptions {
        original_query: String::new(), // Not used anymore, kept for compatibility
        project_id,
        sub_queries,
        global_config,
        filters,
        exclude_patterns: request.exclude_patterns,
        include_patterns: request.include_patterns,
        enable_rerank: request.enable_rerank,
    };

    // Build QueryCoordinator per-request from engine components
    let searcher = match state.engine.get_searcher(project_id).await {
        Ok(s) => s,
        Err(e) => {
            return SearchApiResponse::Error(ErrorResponse::new(
                error_codes::QUERY_ERROR,
                format!("Failed to get searcher: {}", e),
            ));
        }
    };

    // Reuse cached relation searcher (LRU) instead of per-request construction
    let relation_searcher = match state.get_relation_searcher(project_id).await {
        Ok(s) => s,
        Err(_) => Arc::new(cce_orchestrator::query::RelationSearcher::new(Arc::new(
            cce_relation::CallChainQuery::new(),
        ))),
    };

    let searcher = searcher.lock().await.clone();
    // expose the relation propagation configuration as a capability so
    // query-side consumers can detect when call chains may lack cross-file
    // edges (`track_cross_file_deps=false` or limited propagation depth).
    let capabilities = match state
        .engine
        .project_registry()
        .get_or_load(project_id)
        .await
    {
        Ok(entry) => {
            let mut caps = cce_orchestrator::query::IndexCapabilities::from(
                &entry.config.orchestrator.indexer,
            );
            caps = caps.merge(&cce_orchestrator::query::IndexCapabilities::from(
                &entry.config.relation,
            ));
            // The relation config is the authoritative source for the
            // propagation depth; the indexer-derived caps report unlimited
            // (0) and must not override a finite configured depth on merge.
            caps =
                caps.with_relation_propagation_depth(entry.config.relation.max_propagation_depth);
            caps
        }
        Err(e) => {
            return SearchApiResponse::Error(ErrorResponse::new(
                error_codes::INVALID_INPUT,
                format!("Failed to load project config: {}", e),
            ));
        }
    };
    let query_coordinator = cce_orchestrator::query::QueryCoordinator::with_capabilities(
        Arc::new(searcher),
        relation_searcher,
        capabilities,
        project_id,
    )
    .with_metrics(cce_metrics_infra::QueryMetrics::new(
        state.engine.metrics_registry(),
        project_id,
    ));

    // Execute aggregated search
    match query_coordinator.search_aggregated(&agg_options).await {
        Ok(result) => {
            let items: Vec<SearchResultItem> = result
                .items
                .into_iter()
                .map(convert_orchestrator_result)
                .collect();

            SearchApiResponse::Success(SearchResponse {
                success: true,
                total: result.total,
                items,
                elapsed_ms: start.elapsed().as_millis() as u64,
                sources_used: result.sources,
            })
        }
        Err(e) => {
            tracing::error!("Aggregated search failed: {}", e);
            SearchApiResponse::Error(ErrorResponse::new(error_codes::QUERY_ERROR, e.to_string()))
        }
    }
}

/// Convert orchestrator result item to API result item
fn convert_orchestrator_result(item: OrchestratorResultItem) -> SearchResultItem {
    // Convert relations to call chain
    let call_chain = item.relations.and_then(|relations| {
        // Combine callers and callees into a single call chain
        let mut chain = Vec::new();
        for caller in relations.callers {
            chain.push(CallChainNode {
                function_id: format!("snapshot-local:{}", caller.id.0),
                function_name: caller.name,
                file_path: caller.file,
                depth: 0, // Will be set by caller
                relation_type: "caller".to_string(),
                call_line: caller.line.map(|l| l as usize),
            });
        }
        for callee in relations.callees {
            chain.push(CallChainNode {
                function_id: format!("snapshot-local:{}", callee.id.0),
                function_name: callee.name,
                file_path: callee.file,
                depth: 0, // Will be set by caller
                relation_type: "callee".to_string(),
                call_line: callee.line.map(|l| l as usize),
            });
        }
        if chain.is_empty() { None } else { Some(chain) }
    });

    // Get source from sources list (first one) or default
    let source = item.sources.first().cloned().unwrap_or_default();
    // Get entity type from kind
    let entity_type = if item.kind.is_empty() {
        None
    } else {
        Some(item.kind)
    };

    SearchResultItem {
        score: item.score,
        file_path: item.file_path,
        code_chunk: item.content, // Use content as code_chunk
        start_line: item.start_line,
        end_line: item.end_line,
        entity_type,
        source,
        call_chain,
        entity_ids: item.entity_ids.iter().map(|eid| eid.0).collect(),
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
            file_extensions: vec![],
            entity_types: vec![],
            languages: vec![],
            include_categories: vec![],
            exclude_categories: vec![],
            call_chain_depth: None,
            include_call_chain: false,
            enable_rerank: None,
            rerank_max_candidates: None,
        };

        assert_eq!(request.query_type, ""); // will use default
        assert_eq!(request.limit, 0); // will use default
        assert_eq!(request.call_chain_depth, None); // optional
    }

    #[test]
    fn test_error_response_creation() {
        let error = ErrorResponse::new(error_codes::INVALID_REQUEST, "Invalid input");
        assert!(!error.success);
        assert_eq!(error.error.code, error_codes::INVALID_REQUEST);
        assert_eq!(error.error.message, "Invalid input");
    }
}
