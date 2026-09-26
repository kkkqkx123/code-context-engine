//! Graph query models
//!
//! Response shapes for the project-scoped graph retrieval endpoints.
//! Nodes and edges follow the node-link layout shared with the
//! orchestrator graph service.

use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// A node in a returned subgraph.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub source_file: String,
    pub source_location: String,
}

/// An edge in a returned subgraph.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub relation: String,
    pub confidence: String,
}

/// Ego neighborhood query parameters.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct EgoQuery {
    pub entity_id: String,
    #[serde(default = "default_ego_depth")]
    pub depth: usize,
    #[serde(default = "default_ego_direction")]
    pub direction: String,
}

/// Two-point path query parameters.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct GraphPathQuery {
    pub start: String,
    pub end: String,
    #[serde(default = "default_path_depth")]
    pub max_depth: usize,
}

/// Explicit entity set query parameters (comma-separated stable ids).
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SubgraphQuery {
    pub ids: String,
}

/// Full export query parameters.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ExportQuery {
    #[serde(default = "default_export_limit")]
    pub limit: usize,
}

/// File impact query parameters.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ImpactQuery {
    pub file: String,
}

/// Subgraph response (ego, subgraph, export).
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GraphSubgraphResponse {
    pub success: bool,
    pub relation_epoch: i64,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relation_info: Option<serde_json::Value>,
}

/// Path response.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GraphPathResponse {
    pub success: bool,
    pub relation_epoch: i64,
    pub path_found: bool,
    #[serde(default)]
    pub nodes: Vec<GraphNode>,
    #[serde(default)]
    pub edges: Vec<GraphEdge>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relation_info: Option<serde_json::Value>,
}

/// Connected components response (stable id groups).
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GraphComponentsResponse {
    pub success: bool,
    pub relation_epoch: i64,
    pub components: Vec<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relation_info: Option<serde_json::Value>,
}

/// File impact response.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GraphImpactResponse {
    pub success: bool,
    pub relation_epoch: i64,
    pub changed_file: String,
    pub direct_dependents: Vec<String>,
    pub transitive_dependents: Vec<String>,
    pub impact_score: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relation_info: Option<serde_json::Value>,
}

fn default_ego_depth() -> usize {
    2
}

fn default_ego_direction() -> String {
    "both".to_string()
}

fn default_path_depth() -> usize {
    10
}

fn default_export_limit() -> usize {
    2000
}
