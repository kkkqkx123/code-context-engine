//! Graph retrieval handlers
//!
//! Project-scoped graph traversal over the relation snapshot:
//! ego neighborhoods, two-point paths, explicit subgraphs,
//! connected components, full export, and file impact.

use axum::extract::{Path, Query as QueryParams, State};
use std::sync::Arc;

use cce_api::models::{
    ComponentsQuery, CyclesQuery, EgoQuery, EntityImpactQuery, ErrorResponse, ExportQuery,
    GraphComponentsResponse, GraphCycle, GraphCyclesResponse, GraphEdge, GraphEntityImpactResponse,
    GraphImpactResponse, GraphModuleResponse, GraphNode, GraphPathQuery, GraphPathResponse,
    GraphStructuralResponse, GraphSubgraphResponse, ImpactQuery, ModuleQuery, ModuleRelation,
    StructuralQuery, StructuralRelation, SubgraphQuery, error_codes,
};
use cce_orchestrator::query::{
    GraphDirection, GraphFilter, GraphPagination, GraphService, SubGraph,
};
use cce_relation::index::snapshot_query::{SnapshotEntityQueryOps, SnapshotSymbolQueryOps};

use crate::api::handlers::entity::seed::resolve_symbol_seed;
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

fn convert_subgraph(graph: &SubGraph) -> Result<(Vec<GraphNode>, Vec<GraphEdge>), ErrorResponse> {
    let nodes = graph
        .nodes
        .iter()
        .map(|node| GraphNode {
            id: node.id.clone(),
            label: node.label.clone(),
            kind: node.kind.clone(),
            source_file: node.source_file.clone(),
            source_location: node.source_location.clone(),
            scoped_name: node.scoped_name.clone(),
            signature: node.signature.clone(),
        })
        .collect::<Vec<_>>();
    let edges = graph
        .edges
        .iter()
        .map(|edge| GraphEdge {
            source: edge.source.clone(),
            target: edge.target.clone(),
            relation: edge.relation.clone(),
            domain: edge.domain.clone(),
            confidence: edge.confidence.to_string(),
            call_context: edge.call_context.clone(),
            is_external: edge.is_external,
            weight: edge.weight,
            cfg_condition: edge.cfg_condition.clone(),
        })
        .collect::<Vec<_>>();

    // Validate every node and edge before shipping it. A malformed entry
    // should bubble up as an internal error rather than silently corrupting
    // the client-side graph store.
    for node in &nodes {
        node.validate()
            .map_err(|msg| ErrorResponse::new(error_codes::INTERNAL_ERROR, msg))?;
    }
    for edge in &edges {
        edge.validate()
            .map_err(|msg| ErrorResponse::new(error_codes::INTERNAL_ERROR, msg))?;
    }
    Ok((nodes, edges))
}

fn parse_comma_list(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| part.to_lowercase())
        .collect()
}

fn parse_graph_filter(domains: &str, relation_types: &str, include_external: bool) -> GraphFilter {
    GraphFilter {
        relation_domains: parse_comma_list(domains),
        relation_types: parse_comma_list(relation_types),
        include_external,
    }
}

fn parse_graph_pagination(offset: usize, limit: usize) -> GraphPagination {
    GraphPagination { offset, limit }
}

fn convert_paged_subgraph(
    graph: &cce_orchestrator::query::PagedSubGraph,
) -> Result<(Vec<GraphNode>, Vec<GraphEdge>), ErrorResponse> {
    let staged = SubGraph {
        nodes: graph.nodes.clone(),
        edges: graph.edges.clone(),
    };
    convert_subgraph(&staged)
}

fn parse_direction(raw: &str) -> Result<GraphDirection, ErrorResponse> {
    match raw.to_lowercase().as_str() {
        "in" | "backward" | "up" => Ok(GraphDirection::Backward),
        "out" | "forward" | "down" => Ok(GraphDirection::Forward),
        "both" | "bidirectional" => Ok(GraphDirection::Both),
        _ => Err(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "direction must be one of in, out, or both".to_string(),
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
    let entity_id = match resolve_symbol_seed(snapshot.index.as_ref(), &params.entity_id) {
        Ok(id) => id,
        Err(e) => return ApiResult::Error(e),
    };
    let service = GraphService::new(searcher);
    let filter = parse_graph_filter(
        &params.domains,
        &params.relation_types,
        params.include_external,
    );
    let pagination = parse_graph_pagination(params.offset, params.limit.max(1));
    let graph = match service.ego_graph_with_options(
        entity_id,
        params.depth.min(max_depth),
        direction,
        &filter,
        pagination,
    ) {
        Ok(graph) => graph,
        Err(e) => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                e.to_string(),
            ));
        }
    };
    let (nodes, edges) = match convert_paged_subgraph(&graph) {
        Ok(v) => v,
        Err(e) => return ApiResult::Error(e),
    };
    let truncated = graph.total_nodes > nodes.len() || graph.total_edges > edges.len();
    ApiResult::Success(GraphSubgraphResponse {
        success: true,
        relation_epoch: snapshot.relation_epoch,
        nodes,
        edges,
        total_nodes: graph.total_nodes,
        total_edges: graph.total_edges,
        truncated,
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
    let start = match resolve_symbol_seed(snapshot.index.as_ref(), &params.start) {
        Ok(id) => id,
        Err(e) => return ApiResult::Error(e),
    };
    let end = match resolve_symbol_seed(snapshot.index.as_ref(), &params.end) {
        Ok(id) => id,
        Err(e) => return ApiResult::Error(e),
    };
    let service = GraphService::new(searcher);
    let filter = parse_graph_filter(
        &params.domains,
        &params.relation_types,
        params.include_external,
    );
    let path = match service.shortest_path_with_options(
        start,
        end,
        params.max_depth.min(max_depth),
        &filter,
    ) {
        Ok(path) => path,
        Err(e) => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                e.to_string(),
            ));
        }
    };
    let (nodes, edges) = match path.as_ref() {
        Some(p) => match convert_subgraph(p) {
            Ok(v) => v,
            Err(e) => return ApiResult::Error(e),
        },
        None => Default::default(),
    };
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
        match resolve_symbol_seed(snapshot.index.as_ref(), raw) {
            Ok(id) => entity_ids.push(id),
            Err(e) => return ApiResult::Error(e),
        }
    }
    let service = GraphService::new(searcher);
    let filter = parse_graph_filter(
        &params.domains,
        &params.relation_types,
        params.include_external,
    );
    let pagination = parse_graph_pagination(params.offset, params.limit.max(1));
    let graph = match service.subgraph_with_options(&entity_ids, &filter, pagination) {
        Ok(graph) => graph,
        Err(e) => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                e.to_string(),
            ));
        }
    };
    let (nodes, edges) = match convert_paged_subgraph(&graph) {
        Ok(v) => v,
        Err(e) => return ApiResult::Error(e),
    };
    let truncated = graph.total_nodes > nodes.len() || graph.total_edges > edges.len();
    ApiResult::Success(GraphSubgraphResponse {
        success: true,
        relation_epoch: snapshot.relation_epoch,
        nodes,
        edges,
        total_nodes: graph.total_nodes,
        total_edges: graph.total_edges,
        truncated,
        relation_info: stale_relation_info(&runtime).await,
    })
}

/// Handle connected components request.
#[utoipa::path(
    get, path = "/api/project/{project_id}/graph/components", tag = "Graph",
    params(ComponentsQuery, ("project_id" = i64, Path, description = "Project id")),
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
    QueryParams(params): QueryParams<ComponentsQuery>,
) -> ApiResult<GraphComponentsResponse> {
    const MAX_COMPONENTS: usize = 5_000;
    let (snapshot, searcher, runtime) = match graph_context(&state, project_id).await {
        Ok(ctx) => ctx,
        Err(e) => return ApiResult::Error(e),
    };
    if params.limit == 0 || params.limit > MAX_COMPONENTS {
        return ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            format!("limit must be within 1-{MAX_COMPONENTS}"),
        ));
    }
    let service = GraphService::new(searcher);
    let filter = parse_graph_filter(
        &params.domains,
        &params.relation_types,
        params.include_external,
    );
    let pagination = parse_graph_pagination(params.offset, params.limit);
    let paged = match service.connected_components_with_options(&filter, pagination) {
        Ok(paged) => paged,
        Err(e) => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                e.to_string(),
            ));
        }
    };
    let groups = paged
        .components
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
        total_components: paged.total_components,
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
    let filter = parse_graph_filter(
        &params.domains,
        &params.relation_types,
        params.include_external,
    );
    let pagination = parse_graph_pagination(params.offset, params.limit.max(1));
    let graph = match service.export_full_with_options(params.limit, &filter, pagination) {
        Ok(graph) => graph,
        Err(e) => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                e.to_string(),
            ));
        }
    };
    let (nodes, edges) = match convert_paged_subgraph(&graph) {
        Ok(v) => v,
        Err(e) => return ApiResult::Error(e),
    };
    let truncated = graph.total_nodes > nodes.len() || graph.total_edges > edges.len();
    ApiResult::Success(GraphSubgraphResponse {
        success: true,
        relation_epoch: snapshot.relation_epoch,
        nodes,
        edges,
        total_nodes: graph.total_nodes,
        total_edges: graph.total_edges,
        truncated,
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
        changed_file: impact.changed,
        direct_dependents: impact.direct_dependents,
        indirect_dependents: impact.indirect_dependents,
        impact_score: impact.impact_score,
        relation_info: stale_relation_info(&runtime).await,
    })
}

/// Handle entity impact request.
///
/// Reports which entities break when one entity changes, split into disjoint
/// first-hop callers and deeper callers.
#[utoipa::path(
    get, path = "/api/project/{project_id}/graph/entity-impact", tag = "Graph",
    params(EntityImpactQuery, ("project_id" = i64, Path, description = "Project id")),
    responses(
        (status = 200, body = GraphEntityImpactResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 503, body = ErrorResponse, description = "Index unavailable"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_graph_entity_impact(
    State(state): State<crate::api::state::AppState>,
    Path(project_id): Path<i64>,
    QueryParams(params): QueryParams<EntityImpactQuery>,
) -> ApiResult<GraphEntityImpactResponse> {
    const MAX_IMPACT_DEPTH: usize = 50;
    let (snapshot, searcher, runtime) = match graph_context(&state, project_id).await {
        Ok(ctx) => ctx,
        Err(e) => return ApiResult::Error(e),
    };
    if params.max_depth == 0 || params.max_depth > MAX_IMPACT_DEPTH {
        return ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            format!("max_depth must be within 1-{MAX_IMPACT_DEPTH}"),
        ));
    }
    let entity_id = match resolve_symbol_seed(snapshot.index.as_ref(), &params.entity_id) {
        Ok(id) => id,
        Err(e) => return ApiResult::Error(e),
    };
    let impact = searcher.get_entity_impact(entity_id, params.max_depth, "entity");
    let name_of = |id: cce_types::EntityId| {
        snapshot
            .index
            .get_symbol_key_by_entity_id(id)
            .map(|key| key.stable_id().0)
            .unwrap_or_else(|| format!("entity:{}", id.0))
    };
    ApiResult::Success(GraphEntityImpactResponse {
        success: true,
        relation_epoch: snapshot.relation_epoch,
        changed_entity: name_of(entity_id),
        direct_dependents: impact
            .direct_dependents
            .iter()
            .map(|s| s.as_str().to_string())
            .collect(),
        indirect_dependents: impact
            .indirect_dependents
            .iter()
            .map(|s| s.as_str().to_string())
            .collect(),
        impact_score: impact.impact_score,
        relation_info: stale_relation_info(&runtime).await,
    })
}

/// Handle dependency cycle request.
///
/// Reports cycles in the call graph (`level=entity`) or in the file dependency
/// graph (`level=file`).
#[utoipa::path(
    get, path = "/api/project/{project_id}/graph/cycles", tag = "Graph",
    params(CyclesQuery, ("project_id" = i64, Path, description = "Project id")),
    responses(
        (status = 200, body = GraphCyclesResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 503, body = ErrorResponse, description = "Index unavailable"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_graph_cycles(
    State(state): State<crate::api::state::AppState>,
    Path(project_id): Path<i64>,
    QueryParams(params): QueryParams<CyclesQuery>,
) -> ApiResult<GraphCyclesResponse> {
    /// Hard ceiling; the detector stops early at this count.
    const MAX_CYCLES: usize = 5_000;
    let (snapshot, searcher, runtime) = match graph_context(&state, project_id).await {
        Ok(ctx) => ctx,
        Err(e) => return ApiResult::Error(e),
    };
    let level = params.level.to_lowercase();
    if level != "entity" && level != "file" {
        return ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "level must be one of entity, file".to_string(),
        ));
    }
    if params.limit == 0 || params.limit > MAX_CYCLES {
        return ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            format!("limit must be within 1-{MAX_CYCLES}"),
        ));
    }
    // Ask for one more than requested so `truncated` reflects reality instead
    // of assuming the page happened to be the last one.
    let probe = params.limit + 1;
    let cycles = if level == "entity" {
        searcher
            .find_entity_cycles(probe)
            .into_iter()
            .map(|members| GraphCycle {
                members: members
                    .into_iter()
                    .map(|id| {
                        snapshot
                            .index
                            .get_symbol_key_by_entity_id(id)
                            .map(|key| key.stable_id().0)
                            .unwrap_or_else(|| format!("entity:{}", id.0))
                    })
                    .collect(),
            })
            .collect::<Vec<_>>()
    } else {
        searcher
            .find_file_cycles(probe)
            .into_iter()
            .map(|members| GraphCycle { members })
            .collect::<Vec<_>>()
    };
    let truncated = cycles.len() > params.limit;
    let total_cycles = cycles.len();
    let cycles = cycles.into_iter().take(params.limit).collect();
    ApiResult::Success(GraphCyclesResponse {
        success: true,
        relation_epoch: snapshot.relation_epoch,
        level,
        cycles,
        total_cycles,
        truncated,
        relation_info: stale_relation_info(&runtime).await,
    })
}

/// Handle structural / frontend relation request.
///
/// Typed access to the relation families that the generic `domains` filter can
/// only approximate: Rust trait bounds and the markup relation set (element
/// containment, event callbacks, parameter bindings, template references).
#[utoipa::path(
    get, path = "/api/project/{project_id}/graph/structural", tag = "Graph",
    params(StructuralQuery, ("project_id" = i64, Path, description = "Project id")),
    responses(
        (status = 200, body = GraphStructuralResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 503, body = ErrorResponse, description = "Index unavailable"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_graph_structural(
    State(state): State<crate::api::state::AppState>,
    Path(project_id): Path<i64>,
    QueryParams(params): QueryParams<StructuralQuery>,
) -> ApiResult<GraphStructuralResponse> {
    use cce_orchestrator::query::{StructuralDirection, StructuralKind};

    const MAX_STRUCTURAL: usize = 2_000;
    let (snapshot, searcher, runtime) = match graph_context(&state, project_id).await {
        Ok(ctx) => ctx,
        Err(e) => return ApiResult::Error(e),
    };
    let kind = match StructuralKind::parse(&params.kind) {
        Some(kind) => kind,
        None => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::INVALID_REQUEST,
                format!(
                    "unknown kind '{}'; expected one of: {}",
                    params.kind,
                    StructuralKind::labels().join(", ")
                ),
            ));
        }
    };
    let direction = match params.direction.to_lowercase().as_str() {
        "out" | "outgoing" | "forward" => StructuralDirection::Outgoing,
        "in" | "incoming" | "backward" => StructuralDirection::Incoming,
        other => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::INVALID_REQUEST,
                format!("direction must be out or in, got '{other}'"),
            ));
        }
    };
    if params.limit == 0 || params.limit > MAX_STRUCTURAL {
        return ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            format!("limit must be within 1-{MAX_STRUCTURAL}"),
        ));
    }
    let entity_id = match resolve_symbol_seed(snapshot.index.as_ref(), &params.entity_id) {
        Ok(id) => id,
        Err(e) => return ApiResult::Error(e),
    };
    let resolved = searcher.structural_relations(entity_id, kind, direction);
    let total_relations = resolved.len();
    let truncated = total_relations > params.limit;
    let relations = resolved
        .into_iter()
        .take(params.limit)
        .map(|relation| StructuralRelation {
            entity_id: snapshot
                .index
                .get_symbol_key_by_entity_id(relation.entity_id)
                .map(|key| key.stable_id().0)
                .unwrap_or_else(|| format!("entity:{}", relation.entity_id.0)),
            label: relation.label,
            relation: relation.relation_type.to_string(),
            domain: cce_orchestrator::query::graph::relation_domain(&relation.relation_type)
                .to_string(),
            source_file: snapshot
                .index
                .as_ref()
                .get_file_path_by_entity(relation.entity_id)
                .unwrap_or_default(),
        })
        .collect();
    ApiResult::Success(GraphStructuralResponse {
        success: true,
        relation_epoch: snapshot.relation_epoch,
        kind: params.kind,
        direction: params.direction,
        relations,
        total_relations,
        truncated,
        relation_info: stale_relation_info(&runtime).await,
    })
}

/// Handle file module relations request.
///
/// Reports what a file pulls in (module-level imports/uses), what it exposes
/// (exports), and which files reach into it.
#[utoipa::path(
    get, path = "/api/project/{project_id}/graph/module", tag = "Graph",
    params(ModuleQuery, ("project_id" = i64, Path, description = "Project id")),
    responses(
        (status = 200, body = GraphModuleResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 503, body = ErrorResponse, description = "Index unavailable"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_graph_module(
    State(state): State<crate::api::state::AppState>,
    Path(project_id): Path<i64>,
    QueryParams(params): QueryParams<ModuleQuery>,
) -> ApiResult<GraphModuleResponse> {
    const MAX_MODULE_EDGES: usize = 5_000;
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
    let modules = searcher.get_module_relations(&params.file);
    let imports = modules
        .imports
        .into_iter()
        .take(MAX_MODULE_EDGES)
        .map(|relation| ModuleRelation {
            entity_id: relation
                .callee_id
                .map(|id| {
                    snapshot
                        .index
                        .get_symbol_key_by_entity_id(id)
                        .map(|key| key.stable_id().0)
                        .unwrap_or_default()
                })
                .unwrap_or_default(),
            target: relation.callee_name,
            relation: relation.relation_type.to_string(),
            domain: cce_orchestrator::query::relation_domain(&relation.relation_type).to_string(),
        })
        .collect();
    ApiResult::Success(GraphModuleResponse {
        success: true,
        relation_epoch: snapshot.relation_epoch,
        file: params.file,
        exports: modules
            .exports
            .into_iter()
            .map(|id| {
                snapshot
                    .index
                    .get_symbol_key_by_entity_id(id)
                    .map(|key| key.stable_id().0)
                    .unwrap_or_else(|| format!("entity:{}", id.0))
            })
            .collect(),
        caller_files: modules.callers,
        imports,
        relation_info: stale_relation_info(&runtime).await,
    })
}
