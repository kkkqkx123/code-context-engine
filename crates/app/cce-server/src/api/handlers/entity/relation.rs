//! Relation handlers
//!
//! Provides call chain queries, call path finding, and inheritance relations.

use axum::extract::{Path, Query as QueryParams, State};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use cce_orchestrator::query::RelationQueryOptions;
use cce_relation::index::snapshot_query::{SnapshotEntityQueryOps, SnapshotSymbolQueryOps};
use cce_types::EntityId;

use cce_api::models::{
    CallChainNode, CallChainResponse, CallPathQuery, CallPathResponse,
    ClassImplementationsResponse, ClassInheritanceResponse, ErrorResponse, error_codes,
};

use crate::api::response::ApiResult;

/// Success payload variants for relation queries
#[derive(Serialize)]
#[serde(untagged)]
pub enum RelationSuccess {
    CallChain(CallChainResponse),
    CallPath(CallPathResponse),
    ClassInheritance(ClassInheritanceResponse),
    ClassImplementations(ClassImplementationsResponse),
}

/// Unified response type for relation handlers
pub type RelationApiResponse = ApiResult<RelationSuccess>;

/// Helper: get relation snapshot for a project
pub(crate) async fn get_snapshot(
    state: &crate::api::state::AppState,
    project_id: i64,
) -> Result<Arc<crate::runtime::PublishedSnapshot>, RelationApiResponse> {
    if project_id <= 0 {
        return Err(RelationApiResponse::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "Invalid project_id".to_string(),
        )));
    }

    let runtime = match state.engine.get_relation_runtime(project_id).await {
        Ok(rt) => rt,
        Err(e) => {
            return Err(RelationApiResponse::Error(ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                format!("Failed to get relation runtime: {}", e),
            )));
        }
    };

    if !runtime.can_serve_queries().await {
        let info = runtime.get_capability_info().await;
        return Err(RelationApiResponse::Error(ErrorResponse::new(
            error_codes::SERVICE_UNAVAILABLE,
            format!(
                "Relation index not available: {:?}, epoch: {}",
                info.state, info.relation_epoch
            ),
        )));
    }

    match runtime.get_snapshot().await {
        Some(s) => Ok(s),
        None => Err(RelationApiResponse::Error(ErrorResponse::new(
            error_codes::SERVICE_UNAVAILABLE,
            "No relation snapshot available".to_string(),
        ))),
    }
}

/// Return the relation capability info as a JSON map when the served
/// snapshot is stale (runtime degraded or updating); `None` when the
/// snapshot is fresh. Lets the API layer hint that the answered data may
/// lag the latest published epoch.
async fn stale_relation_info(
    state: &crate::api::state::AppState,
    project_id: i64,
) -> Option<serde_json::Value> {
    let runtime = state.engine.get_relation_runtime(project_id).await.ok()?;
    let info = runtime.get_capability_info().await;
    info.stale.then(|| info.to_json_map())
}

async fn relation_query_config(
    state: &crate::api::state::AppState,
    project_id: i64,
) -> Result<cce_config::RelationConfig, RelationApiResponse> {
    state
        .engine
        .project_registry()
        .get_or_load(project_id)
        .await
        .map(|entry| entry.config.relation.clone())
        .map_err(|error| {
            RelationApiResponse::Error(ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                format!("Failed to load relation configuration: {error}"),
            ))
        })
}

/// Query parameters for call chain direction
#[derive(Debug, Clone, Deserialize)]
pub struct CallChainDirectionParams {
    #[serde(default = "default_direction")]
    pub direction: String,
    #[serde(default = "default_max_depth")]
    pub max_depth: usize,
    #[serde(default)]
    pub offset: Option<usize>,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_direction() -> String {
    "down".to_string()
}

fn default_max_depth() -> usize {
    3
}

fn default_limit() -> usize {
    20
}

fn stable_id<I: SnapshotSymbolQueryOps>(index: &I, entity_id: EntityId) -> String {
    index
        .get_symbol_key_by_entity_id(entity_id)
        .map(|key| key.stable_id().0)
        .unwrap_or_default()
}

/// Handle call chain request
pub async fn handle_call_chain(
    State(state): State<crate::api::state::AppState>,
    Path((project_id, id)): Path<(i64, String)>,
    QueryParams(params): QueryParams<CallChainDirectionParams>,
) -> RelationApiResponse {
    let snapshot = match get_snapshot(&state, project_id).await {
        Ok(s) => s,
        Err(e) => return e,
    };
    let relation_config = match relation_query_config(&state, project_id).await {
        Ok(config) => config,
        Err(error) => return error,
    };
    if !relation_config.index.resolve_call_chains {
        return RelationApiResponse::Error(ErrorResponse::new(
            error_codes::SERVICE_UNAVAILABLE,
            "Call chain resolution is disabled for this project".to_string(),
        ));
    }
    let max_depth = params.max_depth.min(relation_config.max_call_depth);

    let Some(entity_id) = snapshot.index.get_entity_id_by_stable_symbol_id(&id) else {
        return RelationApiResponse::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "Unknown stable symbol ID".to_string(),
        ));
    };
    let searcher = match state.get_relation_searcher(project_id).await {
        Ok(s) => s,
        Err(e) => {
            return RelationApiResponse::Error(ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                format!("Failed to get relation searcher: {}", e),
            ));
        }
    };
    let direction = params.direction.to_lowercase();
    let options = RelationQueryOptions::new()
        .with_max_depth(max_depth)
        .with_offset(params.offset.unwrap_or(0))
        .with_limit(params.limit);
    let nodes_result = match direction.as_str() {
        "down" | "forward" => searcher.query_forward_paginated(entity_id, &options),
        "up" | "backward" => searcher.query_backward_paginated(entity_id, &options),
        _ => {
            return RelationApiResponse::Error(ErrorResponse::new(
                error_codes::INVALID_REQUEST,
                "direction must be one of down, forward, up, or backward".to_string(),
            ));
        }
    };
    let call_chain: Vec<CallChainNode> = match nodes_result {
        Ok(nodes) => nodes
            .into_iter()
            .map(|node| CallChainNode {
                function_id: stable_id(searcher.query().index(), node.function_id),
                function_name: node.function_name,
                file_path: node.file_path,
                depth: node.depth,
                relation_type: format!("{:?}", node.relation_type),
                call_line: node.call_line,
            })
            .collect(),
        Err(error) => {
            return RelationApiResponse::Error(ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                error.to_string(),
            ));
        }
    };
    let response = CallChainResponse {
        success: true,
        relation_epoch: snapshot.relation_epoch,
        function_id: id,
        function_name: "Unknown".to_string(),
        direction: params.direction,
        call_chain,
        relation_info: stale_relation_info(&state, project_id).await,
    };

    RelationApiResponse::Success(RelationSuccess::CallChain(response))
}

/// Handle call path request
pub async fn handle_call_path(
    State(state): State<crate::api::state::AppState>,
    Path(project_id): Path<i64>,
    QueryParams(params): QueryParams<CallPathQuery>,
) -> RelationApiResponse {
    let snapshot = match get_snapshot(&state, project_id).await {
        Ok(s) => s,
        Err(e) => return e,
    };
    let relation_config = match relation_query_config(&state, project_id).await {
        Ok(config) => config,
        Err(error) => return error,
    };
    if !relation_config.index.resolve_call_chains {
        return RelationApiResponse::Error(ErrorResponse::new(
            error_codes::SERVICE_UNAVAILABLE,
            "Call chain resolution is disabled for this project".to_string(),
        ));
    }

    let max_depth = params.max_depth.min(relation_config.max_call_depth);
    let Some(start_id) = snapshot
        .index
        .get_entity_id_by_stable_symbol_id(&params.start_id)
    else {
        return RelationApiResponse::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "Unknown start stable symbol ID".to_string(),
        ));
    };
    let Some(end_id) = snapshot
        .index
        .get_entity_id_by_stable_symbol_id(&params.end_id)
    else {
        return RelationApiResponse::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "Unknown end stable symbol ID".to_string(),
        ));
    };
    let searcher = match state.get_relation_searcher(project_id).await {
        Ok(s) => s,
        Err(e) => {
            return RelationApiResponse::Error(ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                format!("Failed to get relation searcher: {}", e),
            ));
        }
    };
    let options = cce_orchestrator::query::PathQueryOptions::new().with_max_depth(max_depth);

    match searcher.find_path(start_id, end_id, &options) {
        Ok(Some(path)) => {
            let call_chain_nodes: Vec<CallChainNode> = path
                .into_iter()
                .map(|node| CallChainNode {
                    function_id: stable_id(snapshot.index.as_ref(), node.function_id),
                    function_name: node.function_name,
                    file_path: node.file_path,
                    depth: node.depth,
                    relation_type: format!("{:?}", node.relation_type),
                    call_line: node.call_line,
                })
                .collect();

            let response = CallPathResponse {
                success: true,
                relation_epoch: snapshot.relation_epoch,
                start_function_id: params.start_id.clone(),
                end_function_id: params.end_id.clone(),
                path_found: true,
                path: call_chain_nodes.clone(),
                path_length: call_chain_nodes.len(),
                relation_info: stale_relation_info(&state, project_id).await,
            };
            RelationApiResponse::Success(RelationSuccess::CallPath(response))
        }
        Ok(None) => {
            let response = CallPathResponse {
                success: true,
                relation_epoch: snapshot.relation_epoch,
                start_function_id: params.start_id,
                end_function_id: params.end_id,
                path_found: false,
                path: Vec::new(),
                path_length: 0,
                relation_info: stale_relation_info(&state, project_id).await,
            };
            RelationApiResponse::Success(RelationSuccess::CallPath(response))
        }
        Err(e) => RelationApiResponse::Error(ErrorResponse::new(
            error_codes::INTERNAL_ERROR,
            e.to_string(),
        )),
    }
}

/// Handle class inheritance request
pub async fn handle_class_inheritance(
    State(state): State<crate::api::state::AppState>,
    Path((project_id, id)): Path<(i64, String)>,
) -> RelationApiResponse {
    let snapshot = match get_snapshot(&state, project_id).await {
        Ok(s) => s,
        Err(e) => return e,
    };

    let Some(entity_id) = snapshot.index.get_entity_id_by_stable_symbol_id(&id) else {
        return RelationApiResponse::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "Unknown stable symbol ID".to_string(),
        ));
    };
    let searcher = match state.get_relation_searcher(project_id).await {
        Ok(s) => s,
        Err(e) => {
            return RelationApiResponse::Error(ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                format!("Failed to get relation searcher: {}", e),
            ));
        }
    };
    let base_classes = searcher.get_base_classes(entity_id);
    let derived_classes = searcher.get_derived_classes(entity_id);

    let response = ClassInheritanceResponse {
        success: true,
        relation_epoch: snapshot.relation_epoch,
        class_id: id,
        class_name: searcher
            .query()
            .index()
            .get_function_by_entity_id(entity_id)
            .map(|e| e.name.clone())
            .unwrap_or_else(|| "Unknown".to_string()),
        base_classes: base_classes
            .into_iter()
            .map(|id| {
                let (class_name, file_path) = searcher
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

                cce_api::models::ClassRelation {
                    class_id: stable_id(searcher.query().index(), id),
                    class_name,
                    file_path,
                    depth: 0,
                }
            })
            .collect(),
        derived_classes: derived_classes
            .into_iter()
            .map(|id| {
                let (class_name, file_path) = searcher
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

                cce_api::models::ClassRelation {
                    class_id: stable_id(searcher.query().index(), id),
                    class_name,
                    file_path,
                    depth: 0,
                }
            })
            .collect(),
        relation_info: stale_relation_info(&state, project_id).await,
    };

    RelationApiResponse::Success(RelationSuccess::ClassInheritance(response))
}

/// Handle class implementations request
pub async fn handle_class_implementations(
    State(state): State<crate::api::state::AppState>,
    Path((project_id, id)): Path<(i64, String)>,
) -> RelationApiResponse {
    let snapshot = match get_snapshot(&state, project_id).await {
        Ok(s) => s,
        Err(e) => return e,
    };

    let Some(entity_id) = snapshot.index.get_entity_id_by_stable_symbol_id(&id) else {
        return RelationApiResponse::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "Unknown stable symbol ID".to_string(),
        ));
    };
    let searcher = match state.get_relation_searcher(project_id).await {
        Ok(s) => s,
        Err(e) => {
            return RelationApiResponse::Error(ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                format!("Failed to get relation searcher: {}", e),
            ));
        }
    };
    let implemented_interfaces = searcher.get_implemented_interfaces(entity_id);
    let implementing_classes = searcher.get_implementing_classes(entity_id);

    let response = ClassImplementationsResponse {
        success: true,
        relation_epoch: snapshot.relation_epoch,
        class_id: id,
        class_name: searcher
            .query()
            .index()
            .get_function_by_entity_id(entity_id)
            .map(|e| e.name.clone())
            .unwrap_or_else(|| "Unknown".to_string()),
        implemented_interfaces: implemented_interfaces
            .into_iter()
            .map(|id| {
                let (interface_name, file_path) = searcher
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

                cce_api::models::InterfaceRelation {
                    interface_id: stable_id(searcher.query().index(), id),
                    interface_name,
                    file_path,
                }
            })
            .collect(),
        implementing_classes: implementing_classes
            .into_iter()
            .map(|id| {
                let (class_name, file_path) = searcher
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

                cce_api::models::ClassRelation {
                    class_id: stable_id(searcher.query().index(), id),
                    class_name,
                    file_path,
                    depth: 0,
                }
            })
            .collect(),
        relation_info: stale_relation_info(&state, project_id).await,
    };

    RelationApiResponse::Success(RelationSuccess::ClassImplementations(response))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_direction() {
        assert_eq!(default_direction(), "down");
    }
}
