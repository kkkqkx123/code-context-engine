//! Graph retrieval handlers
//!
//! Project-scoped graph traversal over the relation snapshot:
//! ego neighborhoods, two-point paths, explicit subgraphs,
//! connected components, full export, and file impact.

mod analysis;
mod traversal;

pub use analysis::{
    __path_handle_graph_cycles, __path_handle_graph_entity_impact, __path_handle_graph_impact,
    __path_handle_graph_module, __path_handle_graph_structural, handle_graph_cycles,
    handle_graph_entity_impact, handle_graph_impact, handle_graph_module, handle_graph_structural,
};
pub use traversal::{
    __path_handle_graph_components, __path_handle_graph_ego, __path_handle_graph_export,
    __path_handle_graph_path, __path_handle_graph_subgraph, handle_graph_components,
    handle_graph_ego, handle_graph_export, handle_graph_path, handle_graph_subgraph,
};

use std::sync::Arc;

use cce_api::models::{ErrorResponse, GraphEdge, GraphNode, error_codes};
use cce_orchestrator::query::{GraphDirection, GraphFilter, GraphPagination, SubGraph};

/// Return the relation capability info as a JSON map when the served
/// snapshot is stale; `None` when the snapshot is fresh.
async fn stale_relation_info(
    runtime: &cce_server_shared::runtime::RelationRuntime,
) -> Option<serde_json::Value> {
    let info = runtime.get_capability_info().await;
    info.stale.then(|| info.to_json_map())
}

async fn relation_max_depth(
    state: &cce_server_shared::context::SharedContext,
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
            callsite: edge.callsite.clone(),
            edge_id: edge.edge_id.clone(),
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
    state: &cce_server_shared::context::SharedContext,
    project_id: i64,
) -> Result<
    (
        Arc<cce_server_shared::runtime::PublishedSnapshot>,
        Arc<cce_orchestrator::query::RelationSearcher>,
        Arc<cce_server_shared::runtime::RelationRuntime>,
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
