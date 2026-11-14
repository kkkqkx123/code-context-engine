//! Graph traversal handlers over the relation snapshot.

use axum::extract::{Path, Query as QueryParams, State};

use cce_api::models::{
    ComponentsQuery, EgoQuery, ErrorResponse, ExportQuery, GraphComponentsResponse, GraphPathQuery,
    GraphPathResponse, GraphSubgraphResponse, SubgraphQuery, error_codes,
};
use cce_server_shared::services::graph::{self, GraphDirectionSpec, GraphView};

use crate::api::response::{ApiResult, relation_error};

use super::{convert_graph, parse_graph_filter, parse_graph_pagination};

fn parse_direction(raw: &str) -> Result<GraphDirectionSpec, ErrorResponse> {
    GraphDirectionSpec::parse(raw).ok_or_else(|| {
        ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "direction must be one of in, out, or both".to_string(),
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
    State(state): State<cce_server_shared::context::SharedContext>,
    Path(project_id): Path<i64>,
    QueryParams(params): QueryParams<EgoQuery>,
) -> ApiResult<GraphSubgraphResponse> {
    let direction = match parse_direction(&params.direction) {
        Ok(direction) => direction,
        Err(error) => return ApiResult::Error(error),
    };
    let filter = parse_graph_filter(
        &params.domains,
        &params.relation_types,
        params.include_external,
    );
    let page = parse_graph_pagination(params.offset, params.limit.max(1));
    let graph = match graph::ego(
        &state,
        project_id,
        &params.entity_id,
        params.depth,
        direction,
        &filter,
        page,
    )
    .await
    {
        Ok(graph) => graph,
        Err(error) => return ApiResult::Error(relation_error(error)),
    };
    let GraphView {
        nodes,
        edges,
        total_nodes,
        total_edges,
        stamp,
    } = graph;
    let truncated = total_nodes > nodes.len() || total_edges > edges.len();
    let (nodes, edges) = match convert_graph(nodes, edges) {
        Ok(pair) => pair,
        Err(error) => return ApiResult::Error(error),
    };
    ApiResult::Success(GraphSubgraphResponse {
        success: true,
        relation_epoch: stamp.relation_epoch,
        nodes,
        edges,
        total_nodes,
        total_edges,
        truncated,
        relation_info: stamp.stale,
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
    let filter = parse_graph_filter(
        &params.domains,
        &params.relation_types,
        params.include_external,
    );
    let path = match graph::path(
        &state,
        project_id,
        &params.start,
        &params.end,
        params.max_depth,
        &filter,
    )
    .await
    {
        Ok(path) => path,
        Err(error) => return ApiResult::Error(relation_error(error)),
    };
    let found = path.found;
    let found_stamp = path.stamp;
    let (nodes, edges) = match convert_graph(path.nodes, path.edges) {
        Ok(pair) => pair,
        Err(error) => return ApiResult::Error(error),
    };
    ApiResult::Success(GraphPathResponse {
        success: true,
        relation_epoch: found_stamp.relation_epoch,
        path_found: found,
        nodes,
        edges,
        relation_info: found_stamp.stale,
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
    let seeds: Vec<String> = params
        .ids
        .split(',')
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .collect();
    let filter = parse_graph_filter(
        &params.domains,
        &params.relation_types,
        params.include_external,
    );
    let page = parse_graph_pagination(params.offset, params.limit.max(1));
    let graph = match graph::subgraph(&state, project_id, &seeds, &filter, page).await {
        Ok(graph) => graph,
        Err(error) => return ApiResult::Error(relation_error(error)),
    };
    let GraphView {
        nodes,
        edges,
        total_nodes,
        total_edges,
        stamp,
    } = graph;
    let truncated = total_nodes > nodes.len() || total_edges > edges.len();
    let (nodes, edges) = match convert_graph(nodes, edges) {
        Ok(pair) => pair,
        Err(error) => return ApiResult::Error(error),
    };
    ApiResult::Success(GraphSubgraphResponse {
        success: true,
        relation_epoch: stamp.relation_epoch,
        nodes,
        edges,
        total_nodes,
        total_edges,
        truncated,
        relation_info: stamp.stale,
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
    let filter = parse_graph_filter(
        &params.domains,
        &params.relation_types,
        params.include_external,
    );
    let page = parse_graph_pagination(params.offset, params.limit);
    let paged = match graph::components(&state, project_id, &filter, page).await {
        Ok(paged) => paged,
        Err(error) => return ApiResult::Error(relation_error(error)),
    };
    ApiResult::Success(GraphComponentsResponse {
        success: true,
        relation_epoch: paged.stamp.relation_epoch,
        components: paged.components,
        total_components: paged.total_components,
        relation_info: paged.stamp.stale,
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
    let filter = parse_graph_filter(
        &params.domains,
        &params.relation_types,
        params.include_external,
    );
    let page = parse_graph_pagination(params.offset, params.limit.max(1));
    let graph = match graph::export(&state, project_id, params.limit, &filter, page).await {
        Ok(graph) => graph,
        Err(error) => return ApiResult::Error(relation_error(error)),
    };
    let GraphView {
        nodes,
        edges,
        total_nodes,
        total_edges,
        stamp,
    } = graph;
    let truncated = total_nodes > nodes.len() || total_edges > edges.len();
    let (nodes, edges) = match convert_graph(nodes, edges) {
        Ok(pair) => pair,
        Err(error) => return ApiResult::Error(error),
    };
    ApiResult::Success(GraphSubgraphResponse {
        success: true,
        relation_epoch: stamp.relation_epoch,
        nodes,
        edges,
        total_nodes,
        total_edges,
        truncated,
        relation_info: stamp.stale,
    })
}
