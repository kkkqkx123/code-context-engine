//! Graph retrieval handlers
//!
//! Project-scoped graph traversal over the relation snapshot: ego
//! neighborhoods, two-point paths, explicit subgraphs, connected components,
//! full export, and file impact.
//!
//! This module keeps only wire decoding and response mapping; every traversal
//! runs in the shared graph service.

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

use cce_api::models::{ErrorResponse, GraphEdge, GraphNode, error_codes};
use cce_server_shared::services::graph::{GraphFilterSpec, GraphPageSpec};

/// Translate the comma-separated wire filters into a filter spec.
fn parse_comma_list(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| part.to_lowercase())
        .collect()
}

fn parse_graph_filter(
    domains: &str,
    relation_types: &str,
    include_external: bool,
) -> GraphFilterSpec {
    GraphFilterSpec {
        domains: parse_comma_list(domains),
        relation_types: parse_comma_list(relation_types),
        include_external,
    }
}

fn parse_graph_pagination(offset: usize, limit: usize) -> GraphPageSpec {
    GraphPageSpec { offset, limit }
}

/// Map the service graph onto wire nodes and edges.
///
/// Every entry is validated before shipping: a malformed entry must surface as
/// an internal error rather than silently corrupting the client-side graph
/// store.
fn convert_graph(
    nodes: Vec<cce_orchestrator::query::GraphNode>,
    edges: Vec<cce_orchestrator::query::GraphEdge>,
) -> Result<(Vec<GraphNode>, Vec<GraphEdge>), ErrorResponse> {
    let nodes = nodes
        .into_iter()
        .map(|node| GraphNode {
            id: node.id,
            label: node.label,
            kind: node.kind,
            source_file: node.source_file,
            source_location: node.source_location,
            scoped_name: node.scoped_name,
            signature: node.signature,
        })
        .collect::<Vec<_>>();
    let edges = edges
        .into_iter()
        .map(|edge| GraphEdge {
            source: edge.source,
            target: edge.target,
            relation: edge.relation,
            domain: edge.domain,
            confidence: edge.confidence.to_string(),
            call_context: edge.call_context,
            is_external: edge.is_external,
            weight: edge.weight,
            cfg_condition: edge.cfg_condition,
            callsite: edge.callsite,
            edge_id: edge.edge_id,
        })
        .collect::<Vec<_>>();
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
