//! Search handlers
//!
//! This module provides handlers for search operations including:
//! - Vector search
//! - BM25 search
//! - Hybrid search

use axum::{Json, extract::State};
use std::sync::Arc;

use crate::api::validation;
use cce_orchestrator::SearchResult as OrchestratorResultItem;
use cce_orchestrator::query::types::{
    ContentState, DowngradeReason, ExcludableContentType, SearchSources,
};
use cce_orchestrator::query::{SubQuery, types::ResultFilterConfig, types::SearchConfig};
use cce_utils::text::is_blank;

use cce_api::models::{
    AggregatedSearchRequest, ContentStateDto, ErrorResponse, SearchRequest, SearchResponse,
    SearchResultItem, error_codes,
};

use crate::api::response::ApiResult;

/// Unified response type for search handlers
pub type SearchApiResponse = ApiResult<SearchResponse>;

/// Best-effort relation version for the project, used to stamp search
/// responses so callers can judge freshness. Never fails the query.
async fn relation_version(
    state: &crate::api::state::AppState,
    project_id: i64,
) -> (Option<i64>, bool) {
    let runtime = match state.engine.get_relation_runtime(project_id).await {
        Ok(runtime) => runtime,
        Err(_) => return (None, false),
    };
    let info = runtime.get_capability_info().await;
    let epoch = (info.relation_epoch > 0).then_some(info.relation_epoch);
    (epoch, info.stale)
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
    // Parse query type to SearchSources
    let search_sources = match request.query_type.to_lowercase().as_str() {
        "vector" => SearchSources::none().with_vector(),
        "bm25" => SearchSources::none().with_bm25(),
        "hybrid" => SearchSources::default(), // vector + bm25
        "summary" => SearchSources::none().with_summary(),
        _ => {
            return SearchApiResponse::Error(ErrorResponse::new(
                error_codes::INVALID_REQUEST,
                format!("Invalid query type: {}", request.query_type),
            ));
        }
    };

    // Build query options with project_id
    let mut query_opts = cce_orchestrator::query::types::QueryConfigBuilder::new(project_id)
        .build(&request.query)
        .with_sources(search_sources)
        .with_limit(request.limit);

    // Merge the project's search and rerank configuration into the query options
    // so runtime parameters come from the config layer. The per-request limit
    // is preserved across the merge.
    query_opts.config.rerank = project_entry.config.rerank.clone();
    query_opts.config.vector = project_entry.config.search.vector.clone();
    query_opts.config.bm25 = project_entry.config.search.bm25.clone();
    query_opts.config.fusion = project_entry.config.search.fusion.clone();
    query_opts.config.summary = project_entry.config.search.summary.clone();
    query_opts.config.score = project_entry.config.search.score.clone();
    query_opts.config.boost = project_entry.config.search.boost.clone();
    query_opts.config.plugin = project_entry.config.search.plugin.clone();
    query_opts.config.annotation = project_entry.config.search.annotation.clone();
    query_opts.config.timeout_ms = project_entry.config.search.timeout_ms;
    query_opts.config.result = {
        let mut r = project_entry.config.search.result.clone();
        r.limit = request.limit;
        r
    };

    // Per-request relation-expansion opt-in. When requested, the neighbour
    // resolver is injected into the query options so the engine pass itself
    // resolves real neighbours and runs the batch annotation (single wrap,
    // cross-hit shared dedup). Expansion implies annotation, so both switches
    // are enabled together; without the request the annotation config is
    // passed through unchanged and default behavior is identical to before.
    let resolver = if request.expand.unwrap_or(false) {
        let searcher = match state.get_relation_searcher(project_id).await {
            Ok(searcher) => Some(searcher),
            Err(error) => {
                tracing::warn!(%error, "Expansion requested but relation searcher unavailable; skipping");
                None
            }
        };
        searcher.map(|searcher| {
            cce_orchestrator::query::expansion::NeighbourResolver::new(
                searcher,
                std::path::PathBuf::from(&project_entry.metadata.root_path),
            )
        })
    } else {
        None
    };
    if resolver.is_some() {
        query_opts.config.annotation.enable_annotation = true;
        query_opts.config.annotation.expansion_enabled = true;
    }
    query_opts = query_opts.with_neighbour_resolver(resolver);

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

    // Exclude content types: reject unknown values instead of ignoring them.
    if !request.exclude_content_types.is_empty() {
        for ct in &request.exclude_content_types {
            match ct.to_lowercase().as_str() {
                "test" | "tests" => {
                    query_opts = query_opts.add_exclude_content_type(ExcludableContentType::Test);
                }
                _ => {
                    return SearchApiResponse::Error(ErrorResponse::new(
                        error_codes::INVALID_INPUT,
                        format!("Unknown exclude content type: {}", ct),
                    ));
                }
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

    // Category filters: unknown names are rejected so typos stay visible.
    if !request.include_categories.is_empty() {
        let mut categories = Vec::with_capacity(request.include_categories.len());
        for raw in &request.include_categories {
            match cce_types::FileCategory::from_name(&raw.to_lowercase()) {
                Some(category) => categories.push(category),
                None => {
                    return SearchApiResponse::Error(ErrorResponse::new(
                        error_codes::INVALID_INPUT,
                        format!("Unknown include category: {}", raw),
                    ));
                }
            }
        }
        query_opts = query_opts.with_include_categories(categories);
    }
    if !request.exclude_categories.is_empty() {
        let mut categories = Vec::with_capacity(request.exclude_categories.len());
        for raw in &request.exclude_categories {
            match cce_types::FileCategory::from_name(&raw.to_lowercase()) {
                Some(category) => categories.push(category),
                None => {
                    return SearchApiResponse::Error(ErrorResponse::new(
                        error_codes::INVALID_INPUT,
                        format!("Unknown exclude category: {}", raw),
                    ));
                }
            }
        }
        query_opts = query_opts.with_exclude_categories(categories);
    }
    match request.query_intent.as_deref() {
        None | Some("") => {}
        Some(raw) => match parse_query_intent(Some(raw)) {
            Some(intent) => {
                query_opts = query_opts.with_query_intent(Some(intent));
            }
            None => {
                return SearchApiResponse::Error(ErrorResponse::new(
                    error_codes::INVALID_INPUT,
                    format!("Unknown query intent: {}", raw),
                ));
            }
        },
    }
    // Execute query using engine's search method with project_id
    let result = state.engine.search(project_id, &query_opts).await;

    match result {
        Ok(result) => {
            // Stable symbol IDs need the relation snapshot; unavailable
            // snapshots simply leave the field empty.
            let relation_searcher = state.get_relation_searcher(project_id).await.ok();
            let items: Vec<SearchResultItem> = result
                .items
                .into_iter()
                .map(|item| convert_orchestrator_result(item, relation_searcher.as_deref()))
                .collect();

            let (relation_epoch, relation_stale) = relation_version(&state, project_id).await;
            let response = SearchResponse {
                success: true,
                total: result.total,
                items,
                elapsed_ms: start.elapsed().as_millis() as u64,
                sources_used: result.sources,
                failed_sub_queries: result.failed_sub_queries,
                relation_epoch,
                relation_stale,
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
    // Build aggregated query options
    let mut sub_queries = Vec::with_capacity(request.sub_queries.len());
    for sq in &request.sub_queries {
        // Parse query type to SearchSources
        let search_sources = match sq.query_type.to_lowercase().as_str() {
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
        sub_queries.push(SubQuery {
            text: sq.text.clone(),
            sources: search_sources,
            weight: sq.weight,
        });
    }

    // Build global config from the project configuration so runtime
    // parameters come from the config layer. Expansion is applied once to
    // the merged hits; sub-queries run without annotation (see coordinator).
    let mut global_config = SearchConfig {
        result: ResultFilterConfig {
            limit: request.limit,
            ..Default::default()
        },
        ..Default::default()
    };
    global_config.vector = project_entry.config.search.vector.clone();
    global_config.bm25 = project_entry.config.search.bm25.clone();
    global_config.fusion = project_entry.config.search.fusion.clone();
    global_config.summary = project_entry.config.search.summary.clone();
    global_config.score = project_entry.config.search.score.clone();
    global_config.boost = project_entry.config.search.boost.clone();
    global_config.plugin = project_entry.config.search.plugin.clone();
    global_config.annotation = project_entry.config.search.annotation.clone();
    global_config.timeout_ms = project_entry.config.search.timeout_ms;
    global_config.rerank = project_entry.config.rerank.clone();
    // Per-request expansion opt-in for the merged hits. Expansion implies
    // annotation, so both switches are enabled together; without the request
    // the annotation config is passed through unchanged.
    let resolver = if request.expand.unwrap_or(false) {
        match state.get_relation_searcher(project_id).await {
            Ok(searcher) => {
                global_config.annotation.enable_annotation = true;
                global_config.annotation.expansion_enabled = true;
                Some(cce_orchestrator::query::expansion::NeighbourResolver::new(
                    searcher,
                    std::path::PathBuf::from(&project_entry.metadata.root_path),
                ))
            }
            Err(error) => {
                tracing::warn!(%error, "Expansion requested but relation searcher unavailable; skipping");
                None
            }
        }
    } else {
        None
    };
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

    // Build filters: any active filter field materializes the object so
    // category-only requests are not dropped.
    let has_filters = request.directory_prefix.is_some()
        || !request.exclude_content_types.is_empty()
        || !request.include_categories.is_empty()
        || !request.exclude_categories.is_empty();
    let mut exclude_types = Vec::with_capacity(request.exclude_content_types.len());
    for ct in &request.exclude_content_types {
        match ct.to_lowercase().as_str() {
            "test" | "tests" => exclude_types.push(ExcludableContentType::Test),
            _ => {
                return SearchApiResponse::Error(ErrorResponse::new(
                    error_codes::INVALID_INPUT,
                    format!("Unknown exclude content type: {}", ct),
                ));
            }
        }
    }
    let mut include_categories = Vec::with_capacity(request.include_categories.len());
    for raw in &request.include_categories {
        match cce_types::FileCategory::from_name(&raw.to_lowercase()) {
            Some(category) => include_categories.push(category),
            None => {
                return SearchApiResponse::Error(ErrorResponse::new(
                    error_codes::INVALID_INPUT,
                    format!("Unknown include category: {}", raw),
                ));
            }
        }
    }
    let mut exclude_categories = Vec::with_capacity(request.exclude_categories.len());
    for raw in &request.exclude_categories {
        match cce_types::FileCategory::from_name(&raw.to_lowercase()) {
            Some(category) => exclude_categories.push(category),
            None => {
                return SearchApiResponse::Error(ErrorResponse::new(
                    error_codes::INVALID_INPUT,
                    format!("Unknown exclude category: {}", raw),
                ));
            }
        }
    }
    let filters = if has_filters {
        Some(cce_orchestrator::query::FilterOptions {
            directory_prefix: request.directory_prefix,
            exclude_content_types: exclude_types,
            include_categories,
            exclude_categories,
        })
    } else {
        None
    };
    let aggregated_intent = match request.query_intent.as_deref() {
        None | Some("") => None,
        Some(raw) => match parse_query_intent(Some(raw)) {
            Some(intent) => Some(intent),
            None => {
                return SearchApiResponse::Error(ErrorResponse::new(
                    error_codes::INVALID_INPUT,
                    format!("Unknown query intent: {}", raw),
                ));
            }
        },
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
        query_intent: aggregated_intent,
        neighbour_resolver: resolver,
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
            cce_codegraph::CallChainQuery::new(),
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
        relation_searcher.clone(),
        capabilities,
        project_id,
    )
    .with_metrics(cce_metrics::QueryMetrics::new(
        state.engine.metrics_registry(),
        project_id,
    ));

    // Execute aggregated search
    match query_coordinator.search_aggregated(&agg_options).await {
        Ok(result) => {
            // Stable symbol IDs need the relation snapshot; unavailable
            // snapshots simply leave the field empty.
            let relation_searcher = state.get_relation_searcher(project_id).await.ok();
            let items: Vec<SearchResultItem> = result
                .items
                .into_iter()
                .map(|item| convert_orchestrator_result(item, relation_searcher.as_deref()))
                .collect();

            let (relation_epoch, relation_stale) = relation_version(&state, project_id).await;
            SearchApiResponse::Success(SearchResponse {
                success: true,
                total: result.total,
                items,
                elapsed_ms: start.elapsed().as_millis() as u64,
                sources_used: result.sources,
                failed_sub_queries: result.failed_sub_queries,
                relation_epoch,
                relation_stale,
            })
        }
        Err(e) => {
            tracing::error!("Aggregated search failed: {}", e);
            SearchApiResponse::Error(ErrorResponse::new(error_codes::QUERY_ERROR, e.to_string()))
        }
    }
}

/// Parse an optional query intent label. Empty means auto-detect.
fn parse_query_intent(raw: Option<&str>) -> Option<cce_orchestrator::query::QueryIntent> {
    use cce_orchestrator::query::QueryIntent;
    match raw {
        None | Some("") => None,
        Some(label) => match label.to_lowercase().as_str() {
            "semantic" => Some(QueryIntent::Semantic),
            "keyword" => Some(QueryIntent::Keyword),
            "hybrid" => Some(QueryIntent::Hybrid),
            "entity" => Some(QueryIntent::Entity),
            _ => None,
        },
    }
}

/// Convert orchestrator result item to API result item.
///
/// `relation_searcher` is optional: when a relation snapshot is available the
/// hit's entity IDs are converted to stable symbol IDs for joinable responses.
fn convert_orchestrator_result(
    item: OrchestratorResultItem,
    relation_searcher: Option<&cce_orchestrator::query::RelationSearcher>,
) -> SearchResultItem {
    // Get entity type from kind
    let entity_type = if item.kind.is_empty() {
        None
    } else {
        Some(item.kind)
    };
    let content_state = match item.content_state {
        ContentState::Full => ContentStateDto::Full,
        ContentState::Reference(DowngradeReason::OverLimit) => ContentStateDto::ReferenceOverLimit,
        ContentState::Reference(DowngradeReason::FileMissing) => {
            ContentStateDto::ReferenceFileMissing
        }
        ContentState::Reference(DowngradeReason::FileLevel) => ContentStateDto::ReferenceFileLevel,
        ContentState::Reference(DowngradeReason::ChunkMissing) => {
            ContentStateDto::ReferenceChunkMissing
        }
    };

    SearchResultItem {
        file_path: item.file_path,
        code_chunk: item.content,
        start_line: item.start_line,
        end_line: item.end_line,
        entity_type,
        entity_names: if item.name.is_empty() {
            Vec::new()
        } else {
            vec![item.name.clone()]
        },
        entity_ids: item.entity_ids.iter().map(|id| id.0 as i64).collect(),
        stable_symbol_ids: relation_searcher
            .map(|searcher| {
                item.entity_ids
                    .iter()
                    .map(|id| cce_orchestrator::query::expansion::stable_id_of(searcher, *id))
                    .collect()
            })
            .unwrap_or_default(),
        score: item.score,
        sources: item.sources,
        content_state,
        expanded: item.metadata.get("expanded").is_some_and(|v| v == "true"),
        expanded_nodes: metadata_count(&item.metadata, "expanded_nodes"),
        forward_nodes: metadata_count(&item.metadata, "forward_nodes"),
        backward_nodes: metadata_count(&item.metadata, "backward_nodes"),
        omitted_self_evident: metadata_count(&item.metadata, "omitted_self_evident"),
    }
}

/// Read a numeric expansion counter from hit metadata, defaulting to zero
/// when the hit went through no expansion pass.
fn metadata_count(metadata: &std::collections::HashMap<String, String>, key: &str) -> usize {
    metadata.get(key).and_then(|v| v.parse().ok()).unwrap_or(0)
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

    #[test]
    fn test_convert_carries_expansion_counters() {
        let mut metadata = std::collections::HashMap::new();
        metadata.insert("expanded".to_string(), "true".to_string());
        metadata.insert("expanded_nodes".to_string(), "3".to_string());
        metadata.insert("forward_nodes".to_string(), "2".to_string());
        metadata.insert("backward_nodes".to_string(), "1".to_string());
        metadata.insert("omitted_self_evident".to_string(), "1".to_string());
        let item = OrchestratorResultItem {
            content: "fn a() {}".to_string(),
            metadata,
            ..Default::default()
        };
        let converted = convert_orchestrator_result(item, None);
        assert!(converted.expanded);
        assert_eq!(converted.expanded_nodes, 3);
        assert_eq!(converted.forward_nodes, 2);
        assert_eq!(converted.backward_nodes, 1);
        assert_eq!(converted.omitted_self_evident, 1);
    }

    #[test]
    fn test_convert_defaults_counters_without_expansion() {
        let item = OrchestratorResultItem {
            content: "fn a() {}".to_string(),
            ..Default::default()
        };
        let converted = convert_orchestrator_result(item, None);
        assert!(!converted.expanded);
        assert_eq!(converted.expanded_nodes, 0);
        assert_eq!(converted.forward_nodes, 0);
        assert_eq!(converted.backward_nodes, 0);
        assert_eq!(converted.omitted_self_evident, 0);
    }
}
