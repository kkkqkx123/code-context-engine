//! Function calls handlers
//!
//! Provides function callees and callers query functionality. Both handlers
//! decode the wire parameters, delegate to the shared relation service, and map
//! the neutral edge list onto the response shape.

use axum::extract::{Path, Query as QueryParams, State};

use cce_api::models::{
    CallChainNode, CallChainQueryParams, ErrorResponse, FunctionCallersResponse,
    FunctionCallsResponse, RelationFilterParams, error_codes,
};
use cce_server_shared::services::relation::{
    self, CallEdge, CallEdges, RelationFilterSpec, RelationStamp,
};

use crate::api::response::{ApiResult, relation_error};

/// Translate the wire filter parameters into the shared filter spec.
///
/// Pagination stays with the caller: the spec only carries filter values
/// and the service builds the final query options in one step.
pub(super) fn params_to_spec(filter: RelationFilterParams) -> RelationFilterSpec {
    RelationFilterSpec {
        exclude_tests: filter.exclude_tests.unwrap_or(false),
        directory_prefix: filter.directory_prefix,
        excluded_files: filter.excluded_files,
        domains: filter.domains,
        include_external: filter.include_external,
    }
}

/// Describe the edge endpoint a listing is about: the callee for callees, the
/// caller for callers.
fn edge_node(edge: &CallEdge, callers: bool) -> CallChainNode {
    let primary = if callers {
        Some(&edge.caller)
    } else {
        edge.callee.as_ref()
    };
    match primary {
        Some(entity) => CallChainNode {
            function_id: entity.stable_id.clone(),
            function_name: entity.name.clone(),
            file_path: entity.file_path.clone(),
            depth: 0,
            relation_type: edge.relation_type.clone(),
            call_line: None,
        },
        None => CallChainNode {
            function_id: format!("external:{}", edge.target_name),
            function_name: edge.target_name.clone(),
            file_path: "Unknown".to_string(),
            depth: 0,
            relation_type: edge.relation_type.clone(),
            call_line: None,
        },
    }
}

fn stamp_fields(stamp: RelationStamp) -> (i64, Option<serde_json::Value>) {
    (stamp.relation_epoch, stamp.stale)
}

/// Invalid project id shared by the entity handlers.
fn invalid_project() -> ErrorResponse {
    ErrorResponse::new(
        error_codes::INVALID_REQUEST,
        "Invalid project_id".to_string(),
    )
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
    State(state): State<cce_server_shared::context::SharedContext>,
    Path((project_id, id)): Path<(i64, String)>,
    QueryParams(params): QueryParams<CallChainQueryParams>,
) -> ApiResult<FunctionCallsResponse> {
    if project_id <= 0 {
        return ApiResult::Error(invalid_project());
    }
    let options = params_to_spec(RelationFilterParams {
        exclude_tests: params.exclude_tests,
        directory_prefix: params.directory_prefix,
        excluded_files: params.excluded_files,
        domains: params.domains,
        include_external: params.include_external,
    })
    .into_options(params.max_depth, params.offset.unwrap_or(0), params.limit);
    let CallEdges {
        subject,
        total,
        items,
        stamp,
    } = match relation::callees(&state, project_id, &id, &options).await {
        Ok(edges) => edges,
        Err(error) => return ApiResult::Error(relation_error(error)),
    };
    let (relation_epoch, relation_info) = stamp_fields(stamp);
    ApiResult::Success(FunctionCallsResponse {
        success: true,
        relation_epoch,
        function_id: id,
        function_name: subject.name,
        callees: items.iter().map(|edge| edge_node(edge, false)).collect(),
        total_callees: total,
        relation_info,
    })
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
    State(state): State<cce_server_shared::context::SharedContext>,
    Path((project_id, id)): Path<(i64, String)>,
    QueryParams(params): QueryParams<CallChainQueryParams>,
) -> ApiResult<FunctionCallersResponse> {
    if project_id <= 0 {
        return ApiResult::Error(invalid_project());
    }
    let options = params_to_spec(RelationFilterParams {
        exclude_tests: params.exclude_tests,
        directory_prefix: params.directory_prefix,
        excluded_files: params.excluded_files,
        domains: params.domains,
        include_external: params.include_external,
    })
    .into_options(params.max_depth, params.offset.unwrap_or(0), params.limit);
    let CallEdges {
        subject,
        total,
        items,
        stamp,
    } = match relation::callers(&state, project_id, &id, &options).await {
        Ok(edges) => edges,
        Err(error) => return ApiResult::Error(relation_error(error)),
    };
    let (relation_epoch, relation_info) = stamp_fields(stamp);
    ApiResult::Success(FunctionCallersResponse {
        success: true,
        relation_epoch,
        function_id: id,
        function_name: subject.name,
        callers: items.iter().map(|edge| edge_node(edge, true)).collect(),
        total_callers: total,
        relation_info,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> CallChainQueryParams {
        CallChainQueryParams {
            max_depth: 5,
            offset: Some(10),
            limit: 50,
            exclude_tests: Some(true),
            directory_prefix: Some("src/flask".to_string()),
            excluded_files: Some("src/flask/app.py".to_string()),
            domains: Some("call".to_string()),
            include_external: false,
        }
    }

    #[test]
    fn test_params_to_options() {
        let params = params();
        let opts = params_to_spec(RelationFilterParams {
            exclude_tests: params.exclude_tests,
            directory_prefix: params.directory_prefix,
            excluded_files: params.excluded_files,
            domains: params.domains,
            include_external: params.include_external,
        })
        .into_options(params.max_depth, params.offset.unwrap_or(0), params.limit);
        assert_eq!(opts.max_depth, 5);
        assert_eq!(opts.offset, 10);
        assert_eq!(opts.limit, 50);
        assert!(!opts.exclude_content_types.is_empty());
        assert_eq!(opts.directory_prefix.as_deref(), Some("src/flask"));
        assert_eq!(opts.excluded_files.len(), 1);
        assert_eq!(opts.relation_domains, vec!["call".to_string()]);
        assert!(!opts.include_external);
    }
}
