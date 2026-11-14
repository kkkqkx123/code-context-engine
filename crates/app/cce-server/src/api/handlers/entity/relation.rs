//! Relation handlers
//!
//! Provides call chain queries, call path finding, and inheritance relations.
//! Every handler decodes its wire parameters and maps the shared relation
//! service result onto the response shape.

use axum::extract::{Path, Query as QueryParams, State};

use cce_api::models::{
    CallChainDirectionParams, CallChainNode, CallChainResponse, CallPathQuery, CallPathResponse,
    ClassImplementationsResponse, ClassInheritanceResponse, ClassRelation, ErrorResponse,
    InterfaceRelation, RelationFilterParams, error_codes,
};
use cce_server_shared::services::relation::{self, CallDirection, ChainNode};

use crate::api::response::{ApiResult, relation_error};

use super::calls::params_to_spec;

fn chain_node(node: ChainNode) -> CallChainNode {
    CallChainNode {
        function_id: node.entity.stable_id,
        function_name: node.entity.name,
        file_path: node.entity.file_path,
        depth: node.depth,
        relation_type: node.relation_type,
        call_line: node.call_line,
    }
}

/// Handle call chain request
#[utoipa::path(
    get, path = "/api/project/{project_id}/call-chain/{id}", tag = "Entity",
    params(CallChainDirectionParams, ("project_id" = i64, Path, description = "Project id"), ("id" = String, Path, description = "Entity id")),
    responses(
        (status = 200, body = CallChainResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 503, body = ErrorResponse, description = "Index unavailable"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_call_chain(
    State(state): State<cce_server_shared::context::SharedContext>,
    Path((project_id, id)): Path<(i64, String)>,
    QueryParams(params): QueryParams<CallChainDirectionParams>,
) -> ApiResult<CallChainResponse> {
    let Some(direction) = CallDirection::parse(&params.direction) else {
        return ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "direction must be one of down, forward, up, or backward".to_string(),
        ));
    };
    let options = params_to_spec(RelationFilterParams {
        exclude_tests: params.exclude_tests,
        directory_prefix: params.directory_prefix,
        excluded_files: params.excluded_files,
        domains: params.domains,
        include_external: params.include_external,
    })
    .into_options(params.max_depth, params.offset.unwrap_or(0), params.limit);
    match relation::call_chain(&state, project_id, &id, direction, options).await {
        Ok(page) => ApiResult::Success(CallChainResponse {
            success: true,
            relation_epoch: page.stamp.relation_epoch,
            function_id: id,
            function_name: page.function.name,
            direction: params.direction,
            call_chain: page.items.into_iter().map(chain_node).collect(),
            relation_info: page.stamp.stale,
        }),
        Err(error) => ApiResult::Error(relation_error(error)),
    }
}

/// Handle call path request
#[utoipa::path(
    get, path = "/api/project/{project_id}/call-path", tag = "Entity",
    params(CallPathQuery, ("project_id" = i64, Path, description = "Project id")),
    responses(
        (status = 200, body = CallPathResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 503, body = ErrorResponse, description = "Index unavailable"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_call_path(
    State(state): State<cce_server_shared::context::SharedContext>,
    Path(project_id): Path<i64>,
    QueryParams(params): QueryParams<CallPathQuery>,
) -> ApiResult<CallPathResponse> {
    match relation::call_path(
        &state,
        project_id,
        &params.start_id,
        &params.end_id,
        params.max_depth,
    )
    .await
    {
        Ok(result) => {
            let path = result
                .path
                .map(|nodes| nodes.into_iter().map(chain_node).collect());
            let path_found = path.is_some();
            let path_length = path.as_ref().map(Vec::len).unwrap_or_default();
            ApiResult::Success(CallPathResponse {
                success: true,
                relation_epoch: result.stamp.relation_epoch,
                start_function_id: params.start_id,
                end_function_id: params.end_id,
                path_found,
                path: path.unwrap_or_default(),
                path_length,
                relation_info: result.stamp.stale,
            })
        }
        Err(error) => ApiResult::Error(relation_error(error)),
    }
}

/// Handle class inheritance request
#[utoipa::path(
    get, path = "/api/project/{project_id}/class/{id}/inheritance", tag = "Entity",
    params(("project_id" = i64, Path, description = "Project id"), ("id" = String, Path, description = "Class id")),
    responses(
        (status = 200, body = ClassInheritanceResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 503, body = ErrorResponse, description = "Index unavailable"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_class_inheritance(
    State(state): State<cce_server_shared::context::SharedContext>,
    Path((project_id, id)): Path<(i64, String)>,
) -> ApiResult<ClassInheritanceResponse> {
    match relation::class_hierarchy(&state, project_id, &id).await {
        Ok(hierarchy) => ApiResult::Success(ClassInheritanceResponse {
            success: true,
            relation_epoch: hierarchy.stamp.relation_epoch,
            class_id: id,
            class_name: hierarchy.class.name,
            base_classes: hierarchy
                .ancestors
                .into_iter()
                .map(|(entity, depth)| ClassRelation {
                    class_id: entity.stable_id,
                    class_name: entity.name,
                    file_path: entity.file_path,
                    depth,
                })
                .collect(),
            derived_classes: hierarchy
                .descendants
                .into_iter()
                .map(|(entity, depth)| ClassRelation {
                    class_id: entity.stable_id,
                    class_name: entity.name,
                    file_path: entity.file_path,
                    depth,
                })
                .collect(),
            relation_info: hierarchy.stamp.stale,
        }),
        Err(error) => ApiResult::Error(relation_error(error)),
    }
}

/// Handle class implementations request
#[utoipa::path(
    get, path = "/api/project/{project_id}/class/{id}/implementations", tag = "Entity",
    params(("project_id" = i64, Path, description = "Project id"), ("id" = String, Path, description = "Class id")),
    responses(
        (status = 200, body = ClassImplementationsResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 503, body = ErrorResponse, description = "Index unavailable"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_class_implementations(
    State(state): State<cce_server_shared::context::SharedContext>,
    Path((project_id, id)): Path<(i64, String)>,
) -> ApiResult<ClassImplementationsResponse> {
    match relation::class_implementations(&state, project_id, &id).await {
        Ok(relations) => ApiResult::Success(ClassImplementationsResponse {
            success: true,
            relation_epoch: relations.stamp.relation_epoch,
            class_id: id,
            class_name: relations.class.name,
            implemented_interfaces: relations
                .interfaces
                .into_iter()
                .map(|entity| InterfaceRelation {
                    interface_id: entity.stable_id,
                    interface_name: entity.name,
                    file_path: entity.file_path,
                })
                .collect(),
            implementing_classes: relations
                .implementors
                .into_iter()
                .map(|entity| ClassRelation {
                    class_id: entity.stable_id,
                    class_name: entity.name,
                    file_path: entity.file_path,
                    depth: 0,
                })
                .collect(),
            relation_info: relations.stamp.stale,
        }),
        Err(error) => ApiResult::Error(relation_error(error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_nodes_carry_stable_identities() {
        let node = ChainNode {
            entity: relation::EntityRef {
                stable_id: "sym_a".to_string(),
                name: "a".to_string(),
                file_path: "src/a.rs".to_string(),
            },
            depth: 2,
            relation_type: "call".to_string(),
            call_line: Some(7),
        };
        let mapped = chain_node(node);
        assert_eq!(mapped.function_id, "sym_a");
        assert_eq!(mapped.file_path, "src/a.rs");
        assert_eq!(mapped.call_line, Some(7));
    }
}
