//! Graph retrieval handlers
//!
//! Project-scoped graph traversal over the relation snapshot:
//! ego neighborhoods, two-point paths, explicit subgraphs,
//! connected components, full export, and file impact.

use axum::extract::{Path, Query as QueryParams, State};
use std::sync::Arc;

use cce_api::models::{
    EgoQuery, ErrorResponse, ExportQuery, GraphComponentsResponse, GraphEdge, GraphImpactResponse,
    GraphNode, GraphPathQuery, GraphPathResponse, GraphSubgraphResponse, ImpactQuery,
    SubgraphQuery, error_codes,
};
use cce_orchestrator::query::{GraphDirection, GraphService, SubGraph};
use cce_relation::index::snapshot_query::SnapshotSymbolQueryOps;

use crate::api::response::ApiResult;

/// Return the relation capability info as a JSON map when the served
/// snapshot is stale; `None` when the snapshot is fresh.
async fn stale_relation_info(
    runtime: &crate::runtime::RelationRuntime,
) -> Option<serde_json::Value> {
    let info = runtime.get_capability_info().await;
    info.stale.then(|| info.to_json_map())
}

async fn relation_max_depth(
    state: &crate::api::state::AppState,
    project_id: i64,
) -> Result<usize, ErrorResponse> {
    state
        .engine
        .project_registry()
        .get_or_load(project_id)
        .await
        .map(|entry| entry.config.relation.max_call_depth)
        .map_err(|error| {
            ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                format!("Failed to load relation configuration: {error}"),
            )
        })
}

fn convert_subgraph(graph: &SubGraph) -> (Vec<GraphNode>, Vec<GraphEdge>) {
    let nodes = graph
        .nodes
        .iter()
        .map(|node| GraphNode {
            id: node.id.clone(),
            label: node.label.clone(),
            kind: node.kind.clone(),
            source_file: node.source_file.clone(),
            source_location: node.source_location.clone(),
        })
        .collect();
    let edges = graph
        .edges
        .iter()
        .map(|edge| GraphEdge {
            source: edge.source.clone(),
            target: edge.target.clone(),
            relation: edge.relation.clone(),
            confidence: edge.confidence.to_string(),
        })
        .collect();
    (nodes, edges)
}

fn parse_direction(raw: &str) -> Result<GraphDirection, ErrorResponse> {
    match raw.to_lowercase().as_str() {
        "forward" | "down" => Ok(GraphDirection::Forward),
        "backward" | "up" => Ok(GraphDirection::Backward),
        "both" | "bidirectional" => Ok(GraphDirection::Both),
        _ => Err(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "direction must be one of forward, backward, or both".to_string(),
        )),
    }
}

/// Shared snapshot + searcher setup for graph handlers.
async fn graph_context(
    state: &crate::api::state::AppState,
    project_id: i64,
) -> Result<
    (
        Arc<crate::runtime::PublishedSnapshot>,
        Arc<cce_orchestrator::query::RelationSearcher>,
        Arc<crate::runtime::RelationRuntime>,
    ),
    ErrorResponse,
> {
    if project_id <= 0 {
        return Err(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "Invalid project_id".to_string(),
        ));
    }
    let runtime = match state.engine.get_relation_runtime(project_id).await {
        Ok(rt) => rt,
        Err(e) => {
            return Err(ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                format!("Failed to get relation runtime: {}", e),
            ));
        }
    };
    if !runtime.can_serve_queries().await {
        let info = runtime.get_capability_info().await;
        return Err(ErrorResponse::new(
            error_codes::SERVICE_UNAVAILABLE,
            format!(
                "Relation index not available: {:?}, epoch: {}",
                info.state, info.relation_epoch
            ),
        ));
    }
    let snapshot = match runtime.get_snapshot().await {
        Some(s) => s,
        None => {
            return Err(ErrorResponse::new(
                error_codes::SERVICE_UNAVAILABLE,
                "No relation snapshot available".to_string(),
            ));
        }
    };
    let searcher = match state.get_relation_searcher(project_id).await {
        Ok(s) => s,
        Err(e) => {
            return Err(ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                format!("Failed to get relation searcher: {}", e),
            ));
        }
    };
    Ok((snapshot, searcher, runtime))
}

fn resolve_entity(
    snapshot: &crate::runtime::PublishedSnapshot,
    stable_id: &str,
) -> Result<cce_types::EntityId, ErrorResponse> {
    snapshot
        .index
        .get_entity_id_by_stable_symbol_id(stable_id)
        .ok_or_else(|| {
            ErrorResponse::new(
                error_codes::INVALID_REQUEST,
                format!("Unknown stable symbol ID: {stable_id}"),
            )
        })
}

/// Handle ego neighborhood request.
#[utoipa::path(
    get, path = "/api/project/{project_id}/graph/ego", tag = "Graph",
    params(EgoQuery, ("project_id" = i64, Path, description = "Project id")),
    responses(
        (status = 200, body = GraphSubgraphResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 503, body = ErrorResponse, description = "Index unavailable"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_graph_ego(
    State(state): State<crate::api::state::AppState>,
    Path(project_id): Path<i64>,
    QueryParams(params): QueryParams<EgoQuery>,
) -> ApiResult<GraphSubgraphResponse> {
    let (snapshot, searcher, runtime) = match graph_context(&state, project_id).await {
        Ok(ctx) => ctx,
        Err(e) => return ApiResult::Error(e),
    };
    let max_depth = match relation_max_depth(&state, project_id).await {
        Ok(depth) => depth,
        Err(e) => return ApiResult::Error(e),
    };
    let direction = match parse_direction(&params.direction) {
        Ok(direction) => direction,
        Err(e) => return ApiResult::Error(e),
    };
    let entity_id = match resolve_entity(&snapshot, &params.entity_id) {
        Ok(id) => id,
        Err(e) => return ApiResult::Error(e),
    };
    let service = GraphService::new(searcher);
    let graph = match service.ego_graph(entity_id, params.depth.min(max_depth), direction) {
        Ok(graph) => graph,
        Err(e) => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                e.to_string(),
            ));
        }
    };
    let (nodes, edges) = convert_subgraph(&graph);
    ApiResult::Success(GraphSubgraphResponse {
        success: true,
        relation_epoch: snapshot.relation_epoch,
        nodes,
        edges,
        relation_info: stale_relation_info(&runtime).await,
    })
}

/// Handle two-point path request.
#[utoipa::path(
    get, path = "/api/project/{project_id}/graph/path", tag = "Graph",
    params(GraphPathQuery, ("project_id" = i64, Path, description = "Project id")),
    responses(
        (status = 200, body = GraphPathResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 503, body = ErrorResponse, description = "Index unavailable"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_graph_path(
    State(state): State<crate::api::state::AppState>,
    Path(project_id): Path<i64>,
    QueryParams(params): QueryParams<GraphPathQuery>,
) -> ApiResult<GraphPathResponse> {
    let (snapshot, searcher, runtime) = match graph_context(&state, project_id).await {
        Ok(ctx) => ctx,
        Err(e) => return ApiResult::Error(e),
    };
    let max_depth = match relation_max_depth(&state, project_id).await {
        Ok(depth) => depth,
        Err(e) => return ApiResult::Error(e),
    };
    let start = match resolve_entity(&snapshot, &params.start) {
        Ok(id) => id,
        Err(e) => return ApiResult::Error(e),
    };
    let end = match resolve_entity(&snapshot, &params.end) {
        Ok(id) => id,
        Err(e) => return ApiResult::Error(e),
    };
    let service = GraphService::new(searcher);
    let path = match service.shortest_path(start, end, params.max_depth.min(max_depth)) {
        Ok(path) => path,
        Err(e) => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                e.to_string(),
            ));
        }
    };
    let (nodes, edges) = path.as_ref().map(convert_subgraph).unwrap_or_default();
    ApiResult::Success(GraphPathResponse {
        success: true,
        relation_epoch: snapshot.relation_epoch,
        path_found: path.is_some(),
        nodes,
        edges,
        relation_info: stale_relation_info(&runtime).await,
    })
}

/// Handle explicit subgraph request.
#[utoipa::path(
    get, path = "/api/project/{project_id}/graph/subgraph", tag = "Graph",
    params(SubgraphQuery, ("project_id" = i64, Path, description = "Project id")),
    responses(
        (status = 200, body = GraphSubgraphResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 503, body = ErrorResponse, description = "Index unavailable"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_graph_subgraph(
    State(state): State<crate::api::state::AppState>,
    Path(project_id): Path<i64>,
    QueryParams(params): QueryParams<SubgraphQuery>,
) -> ApiResult<GraphSubgraphResponse> {
    const MAX_IDS: usize = 200;
    let (snapshot, searcher, runtime) = match graph_context(&state, project_id).await {
        Ok(ctx) => ctx,
        Err(e) => return ApiResult::Error(e),
    };
    let raw_ids: Vec<String> = params
        .ids
        .split(',')
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .collect();
    if raw_ids.is_empty() || raw_ids.len() > MAX_IDS {
        return ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            format!("ids must list 1-{MAX_IDS} stable symbol IDs"),
        ));
    }
    let mut entity_ids = Vec::with_capacity(raw_ids.len());
    for raw in &raw_ids {
        match resolve_entity(&snapshot, raw) {
            Ok(id) => entity_ids.push(id),
            Err(e) => return ApiResult::Error(e),
        }
    }
    let service = GraphService::new(searcher);
    let graph = match service.subgraph(&entity_ids) {
        Ok(graph) => graph,
        Err(e) => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                e.to_string(),
            ));
        }
    };
    let (nodes, edges) = convert_subgraph(&graph);
    ApiResult::Success(GraphSubgraphResponse {
        success: true,
        relation_epoch: snapshot.relation_epoch,
        nodes,
        edges,
        relation_info: stale_relation_info(&runtime).await,
    })
}

/// Handle connected components request.
#[utoipa::path(
    get, path = "/api/project/{project_id}/graph/components", tag = "Graph",
    params(("project_id" = i64, Path, description = "Project id")),
    responses(
        (status = 200, body = GraphComponentsResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 503, body = ErrorResponse, description = "Index unavailable"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_graph_components(
    State(state): State<crate::api::state::AppState>,
    Path(project_id): Path<i64>,
) -> ApiResult<GraphComponentsResponse> {
    let (snapshot, searcher, runtime) = match graph_context(&state, project_id).await {
        Ok(ctx) => ctx,
        Err(e) => return ApiResult::Error(e),
    };
    let service = GraphService::new(searcher);
    let components = match service.connected_components() {
        Ok(components) => components,
        Err(e) => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                e.to_string(),
            ));
        }
    };
    let groups = components
        .into_iter()
        .map(|group| {
            group
                .into_iter()
                .map(|id| {
                    snapshot
                        .index
                        .get_symbol_key_by_entity_id(id)
                        .map(|key| key.stable_id().0)
                        .unwrap_or_else(|| format!("entity:{}", id.0))
                })
                .collect()
        })
        .collect();
    ApiResult::Success(GraphComponentsResponse {
        success: true,
        relation_epoch: snapshot.relation_epoch,
        components: groups,
        relation_info: stale_relation_info(&runtime).await,
    })
}

/// Handle full export request.
#[utoipa::path(
    get, path = "/api/project/{project_id}/graph/export", tag = "Graph",
    params(ExportQuery, ("project_id" = i64, Path, description = "Project id")),
    responses(
        (status = 200, body = GraphSubgraphResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 503, body = ErrorResponse, description = "Index unavailable"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_graph_export(
    State(state): State<crate::api::state::AppState>,
    Path(project_id): Path<i64>,
    QueryParams(params): QueryParams<ExportQuery>,
) -> ApiResult<GraphSubgraphResponse> {
    const MAX_EXPORT_NODES: usize = 10_000;
    let (snapshot, searcher, runtime) = match graph_context(&state, project_id).await {
        Ok(ctx) => ctx,
        Err(e) => return ApiResult::Error(e),
    };
    if params.limit == 0 || params.limit > MAX_EXPORT_NODES {
        return ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            format!("limit must be within 1-{MAX_EXPORT_NODES}"),
        ));
    }
    let service = GraphService::new(searcher);
    let graph = match service.export_full(params.limit) {
        Ok(graph) => graph,
        Err(e) => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                e.to_string(),
            ));
        }
    };
    let (nodes, edges) = convert_subgraph(&graph);
    ApiResult::Success(GraphSubgraphResponse {
        success: true,
        relation_epoch: snapshot.relation_epoch,
        nodes,
        edges,
        relation_info: stale_relation_info(&runtime).await,
    })
}

/// Handle file impact request.
#[utoipa::path(
    get, path = "/api/project/{project_id}/graph/impact", tag = "Graph",
    params(ImpactQuery, ("project_id" = i64, Path, description = "Project id")),
    responses(
        (status = 200, body = GraphImpactResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 503, body = ErrorResponse, description = "Index unavailable"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_graph_impact(
    State(state): State<crate::api::state::AppState>,
    Path(project_id): Path<i64>,
    QueryParams(params): QueryParams<ImpactQuery>,
) -> ApiResult<GraphImpactResponse> {
    let (snapshot, searcher, runtime) = match graph_context(&state, project_id).await {
        Ok(ctx) => ctx,
        Err(e) => return ApiResult::Error(e),
    };
    if params.file.trim().is_empty() {
        return ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "file must not be empty".to_string(),
        ));
    }
    let impact = searcher.get_change_impact(&params.file);
    ApiResult::Success(GraphImpactResponse {
        success: true,
        relation_epoch: snapshot.relation_epoch,
        changed_file: impact.changed_file,
        direct_dependents: impact.direct_dependents,
        transitive_dependents: impact.transitive_dependents,
        impact_score: impact.impact_score,
        relation_info: stale_relation_info(&runtime).await,
    })
}
