//! Graph traversal handlers over the relation snapshot.

use axum::extract::{Path, Query as QueryParams, State};

use cce_api::models::{
    ComponentsQuery, EgoQuery, ErrorResponse, ExportQuery, GraphComponentsResponse, GraphPathQuery,
    GraphPathResponse, GraphSubgraphResponse, SubgraphQuery, error_codes,
};
use cce_codegraph::index::snapshot_query::SnapshotSymbolQueryOps;
use cce_orchestrator::query::GraphService;

use crate::api::handlers::entity::seed::resolve_symbol_seed;
use crate::api::response::ApiResult;

use super::{
    convert_paged_subgraph, convert_subgraph, graph_context, parse_direction, parse_graph_filter,
    parse_graph_pagination, relation_max_depth, stale_relation_info,
};

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
    State(state): State<cce_server_shared::context::SharedContext>,
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
    State(state): State<cce_server_shared::context::SharedContext>,
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
    State(state): State<cce_server_shared::context::SharedContext>,
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
    State(state): State<cce_server_shared::context::SharedContext>,
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
    State(state): State<cce_server_shared::context::SharedContext>,
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
