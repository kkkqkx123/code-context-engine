//! Function calls handlers
//!
//! Provides function callees and callers query functionality.

use axum::extract::{Path, Query as QueryParams, State};

use cce_orchestrator::query::RelationQueryOptions;
use cce_relation::index::snapshot_query::{SnapshotEntityQueryOps, SnapshotSymbolQueryOps};

use cce_api::models::{
    CallChainNode, CallChainQueryParams, ErrorResponse, FunctionCallersResponse,
    FunctionCallsResponse, RelationFilterParams, error_codes,
};

use crate::api::response::ApiResult;

/// Apply the HTTP filter parameters onto query options.
fn apply_filters(
    filter: RelationFilterParams,
    options: RelationQueryOptions,
) -> RelationQueryOptions {
    let mut options = options.with_exclude_tests(filter.exclude_tests.unwrap_or(false));
    if let Some(prefix) = filter.directory_prefix {
        options = options.with_directory_prefix(prefix);
    }
    if let Some(files) = filter.excluded_files {
        options = options.with_excluded_files(files);
    }
    options
}

fn params_to_options(params: CallChainQueryParams) -> RelationQueryOptions {
    apply_filters(
        RelationFilterParams {
            exclude_tests: params.exclude_tests,
            directory_prefix: params.directory_prefix,
            excluded_files: params.excluded_files,
        },
        RelationQueryOptions::new()
            .with_max_depth(params.max_depth)
            .with_offset(params.offset.unwrap_or(0))
            .with_limit(params.limit),
    )
}

/// Return the relation capability info as a JSON map when the served
/// snapshot is stale (runtime degraded or updating); `None` when the
/// snapshot is fresh. Lets the API layer hint that the answered data may
/// lag the latest published epoch.
async fn stale_relation_info(
    runtime: &crate::runtime::RelationRuntime,
) -> Option<serde_json::Value> {
    let info = runtime.get_capability_info().await;
    info.stale.then(|| info.to_json_map())
}

/// Handle function calls request (get callees)
#[utoipa::path(
    get, path = "/api/project/{project_id}/function/{id}/calls", tag = "Entity",
    params(CallChainQueryParams, ("project_id" = i64, Path, description = "Project id"), ("id" = String, Path, description = "Function id")),
    responses(
        (status = 200, body = FunctionCallsResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 503, body = ErrorResponse, description = "Index unavailable"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_function_calls(
    State(state): State<crate::api::state::AppState>,
    Path((project_id, id)): Path<(i64, String)>,
    QueryParams(params): QueryParams<CallChainQueryParams>,
) -> ApiResult<FunctionCallsResponse> {
    // Validate project_id
    if project_id <= 0 {
        return ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "Invalid project_id".to_string(),
        ));
    }

    let options: RelationQueryOptions = params_to_options(params);

    // Get relation runtime for this project
    let runtime = match state.engine.get_relation_runtime(project_id).await {
        Ok(rt) => rt,
        Err(e) => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                format!("Failed to get relation runtime: {}", e),
            ));
        }
    };

    // Check if runtime can serve queries
    if !runtime.can_serve_queries().await {
        let info = runtime.get_capability_info().await;
        return ApiResult::Error(ErrorResponse::new(
            error_codes::SERVICE_UNAVAILABLE,
            format!(
                "Relation index not available: {:?}, epoch: {}",
                info.state, info.relation_epoch
            ),
        ));
    }

    // Get snapshot for epoch / symbol lookup
    let snapshot = match runtime.get_snapshot().await {
        Some(s) => s,
        None => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::SERVICE_UNAVAILABLE,
                "No relation snapshot available".to_string(),
            ));
        }
    };

    let entity_id = match snapshot.index.get_entity_id_by_stable_symbol_id(&id) {
        Some(eid) => eid,
        None => {
            // Try parsing as numeric ID for backwards compatibility
            if let Ok(numeric_id) = id.parse::<u64>() {
                cce_types::EntityId(numeric_id)
            } else {
                return ApiResult::Error(ErrorResponse::new(
                    error_codes::INVALID_REQUEST,
                    "Unknown stable symbol ID".to_string(),
                ));
            }
        }
    };

    // Use cached RelationSearcher (LRU) instead of per-request CallChainQuery
    let searcher = match state.get_relation_searcher(project_id).await {
        Ok(s) => s,
        Err(e) => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                format!("Failed to get relation searcher: {}", e),
            ));
        }
    };
    let filtered_callees: Vec<cce_types::ResolvedRelation> =
        searcher.filter_callees(entity_id, &options);
    // Total after filtering, before pagination
    let total = filtered_callees.len();
    let resolved_callees: Vec<cce_types::ResolvedRelation> = filtered_callees
        .into_iter()
        .skip(options.offset)
        .take(options.limit)
        .collect();
    // Convert ResolvedRelation to CallChainNode
    let callees: Vec<CallChainNode> = resolved_callees
        .into_iter()
        .map(|r| {
            // Try to get callee info from relation index
            let (function_name, file_path) = if let Some(callee_id) = r.callee_id {
                searcher
                    .query()
                    .index()
                    .get_function_by_entity_id(callee_id)
                    .map(|entity| {
                        let path = searcher
                            .query()
                            .index()
                            .get_file_path_by_entity(callee_id)
                            .unwrap_or_else(|| "Unknown".to_string());
                        (entity.name.clone(), path)
                    })
                    .unwrap_or_else(|| ("Unknown".to_string(), "Unknown".to_string()))
            } else {
                ("Unknown".to_string(), "Unknown".to_string())
            };

            CallChainNode {
                function_id: r
                    .callee_id
                    .and_then(|callee_id| {
                        searcher
                            .query()
                            .index()
                            .get_symbol_key_by_entity_id(callee_id)
                    })
                    .map(|key| key.stable_id().0)
                    .unwrap_or_default(),
                function_name,
                file_path,
                depth: 0,
                relation_type: format!("{:?}", r.relation_type),
                call_line: None,
            }
        })
        .collect();

    // Get caller function name
    let function_name = searcher
        .query()
        .index()
        .get_function_by_entity_id(entity_id)
        .map(|entity| entity.name.clone())
        .unwrap_or_else(|| "Unknown".to_string());

    let response = FunctionCallsResponse {
        success: true,
        relation_epoch: snapshot.relation_epoch,
        function_id: id,
        function_name,
        callees,
        total_callees: total,
        relation_info: stale_relation_info(&runtime).await,
    };

    ApiResult::Success(response)
}

/// Handle function callers request
#[utoipa::path(
    get, path = "/api/project/{project_id}/function/{id}/callers", tag = "Entity",
    params(CallChainQueryParams, ("project_id" = i64, Path, description = "Project id"), ("id" = String, Path, description = "Function id")),
    responses(
        (status = 200, body = FunctionCallersResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 503, body = ErrorResponse, description = "Index unavailable"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_function_callers(
    State(state): State<crate::api::state::AppState>,
    Path((project_id, id)): Path<(i64, String)>,
    QueryParams(params): QueryParams<CallChainQueryParams>,
) -> ApiResult<FunctionCallersResponse> {
    // Validate project_id
    if project_id <= 0 {
        return ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "Invalid project_id".to_string(),
        ));
    }

    let options: RelationQueryOptions = params_to_options(params);

    // Get relation runtime for this project
    let runtime = match state.engine.get_relation_runtime(project_id).await {
        Ok(rt) => rt,
        Err(e) => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                format!("Failed to get relation runtime: {}", e),
            ));
        }
    };

    // Check if runtime can serve queries
    if !runtime.can_serve_queries().await {
        let info = runtime.get_capability_info().await;
        return ApiResult::Error(ErrorResponse::new(
            error_codes::SERVICE_UNAVAILABLE,
            format!(
                "Relation index not available: {:?}, epoch: {}",
                info.state, info.relation_epoch
            ),
        ));
    }

    // Get snapshot for stable-id lookup and epoch
    let snapshot = match runtime.get_snapshot().await {
        Some(s) => s,
        None => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::SERVICE_UNAVAILABLE,
                "No relation snapshot available".to_string(),
            ));
        }
    };

    let entity_id = match snapshot.index.get_entity_id_by_stable_symbol_id(&id) {
        Some(eid) => eid,
        None => {
            // Try parsing as numeric ID for backwards compatibility
            if let Ok(numeric_id) = id.parse::<u64>() {
                cce_types::EntityId(numeric_id)
            } else {
                return ApiResult::Error(ErrorResponse::new(
                    error_codes::INVALID_REQUEST,
                    "Unknown stable symbol ID".to_string(),
                ));
            }
        }
    };
    let searcher = match state.get_relation_searcher(project_id).await {
        Ok(s) => s,
        Err(e) => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                format!("Failed to get relation searcher: {}", e),
            ));
        }
    };
    let filtered_callers: Vec<cce_types::EntityId> = searcher.filter_callers(entity_id, &options);
    let total_callers = filtered_callers.len();
    let caller_ids: Vec<cce_types::EntityId> = filtered_callers
        .into_iter()
        .skip(options.offset)
        .take(options.limit)
        .collect();

    // Convert EntityId to CallChainNode
    let callers: Vec<CallChainNode> = caller_ids
        .into_iter()
        .map(|id| {
            let (function_name, file_path) = searcher
                .query()
                .index()
                .get_function_by_entity_id(id)
                .map(|entity| {
                    let path = searcher
                        .query()
                        .index()
                        .get_file_path_by_entity(id)
                        .unwrap_or_else(|| "Unknown".to_string());
                    (entity.name.clone(), path)
                })
                .unwrap_or_else(|| ("Unknown".to_string(), "Unknown".to_string()));

            CallChainNode {
                function_id: searcher
                    .query()
                    .index()
                    .get_symbol_key_by_entity_id(id)
                    .map(|key| key.stable_id().0)
                    .unwrap_or_default(),
                function_name,
                file_path,
                depth: 0,
                relation_type: "caller".to_string(),
                call_line: None,
            }
        })
        .collect();

    // Get callee function name
    let function_name = searcher
        .query()
        .index()
        .get_function_by_entity_id(entity_id)
        .map(|entity| entity.name.clone())
        .unwrap_or_else(|| "Unknown".to_string());

    let response = FunctionCallersResponse {
        success: true,
        relation_epoch: snapshot.relation_epoch,
        function_id: id,
        function_name,
        callers,
        total_callers,
        relation_info: stale_relation_info(&runtime).await,
    };

    ApiResult::Success(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_params_to_options() {
        let params = CallChainQueryParams {
            max_depth: 5,
            offset: Some(10),
            limit: 50,
            exclude_tests: Some(true),
            directory_prefix: Some("src/flask".to_string()),
            excluded_files: Some(vec!["src/flask/app.py".to_string()]),
        };
        let opts: RelationQueryOptions = params_to_options(params);
        assert_eq!(opts.max_depth, 5);
        assert_eq!(opts.offset, 10);
        assert_eq!(opts.limit, 50);
        assert!(
            opts.exclude_content_types
                .contains(&cce_orchestrator::query::ExcludableContentType::Test)
        );
        assert_eq!(opts.directory_prefix.as_deref(), Some("src/flask"));
        assert_eq!(opts.excluded_files.len(), 1);
    }
}
