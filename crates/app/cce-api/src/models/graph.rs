//! Graph query models
//!
//! Response shapes for the project-scoped graph retrieval endpoints.
//! Nodes and edges follow the node-link layout shared with the
//! orchestrator graph service.

use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// Maximum length of a stable entity id accepted by graph endpoints.
///
/// Entity ids are structured strings of the form `pkg::Type::fn@file.rs#60`.
/// Anything above this limit is either pathological input or attacker-supplied.
pub const MAX_ID_LEN: usize = 512;
/// Maximum label length; values above this are trimmed.
pub const MAX_LABEL_LEN: usize = 1024;
/// Maximum source file path length.
pub const MAX_SOURCE_PATH_LEN: usize = 1024;
/// Maximum relation string length.
pub const MAX_RELATION_LEN: usize = 128;

/// A node in a returned subgraph.
///
/// `kind` is the serde-serialized value of `cce_types::EntityKind`
/// (e.g. `"function"`, `"class"`, `"interface"`). It is validated against
/// the known kind set before being sent over the wire so callers never
/// encounter arbitrary strings for this field.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub source_file: String,
    pub source_location: String,
}

/// An edge in a returned subgraph.
///
/// `confidence` mirrors `cce_orchestrator::query::Confidence` serialized as
/// lowercase snake_case (`"extracted"`, `"inferred"`, `"external"`).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub relation: String,
    pub confidence: String,
}

impl GraphNode {
    /// Validate field sizes. Returns the node if every bound is respected,
    /// an error message otherwise.
    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_empty() {
            return Err("GraphNode.id must not be empty".into());
        }
        if self.id.len() > MAX_ID_LEN {
            return Err(format!(
                "GraphNode.id exceeds {MAX_ID_LEN} bytes: {}…",
                &self.id[..MAX_ID_LEN.min(self.id.len())]
            ));
        }
        if self.kind.is_empty() {
            return Err("GraphNode.kind must not be empty".into());
        }
        if self.kind.len() > 64 {
            return Err(format!("GraphNode.kind exceeds 64 bytes: {}", self.kind));
        }
        if self.label.len() > MAX_LABEL_LEN {
            return Err(format!(
                "GraphNode.label exceeds {MAX_LABEL_LEN} bytes for id {}",
                self.id
            ));
        }
        if self.source_file.len() > MAX_SOURCE_PATH_LEN {
            return Err(format!(
                "GraphNode.source_file exceeds {MAX_SOURCE_PATH_LEN} bytes for id {}",
                self.id
            ));
        }
        Ok(())
    }
}

impl GraphEdge {
    /// Validate edge field sizes.
    pub fn validate(&self) -> Result<(), String> {
        if self.source.is_empty() {
            return Err("GraphEdge.source must not be empty".into());
        }
        if self.target.is_empty() {
            return Err("GraphEdge.target must not be empty".into());
        }
        if self.source.len() > MAX_ID_LEN {
            return Err(format!("GraphEdge.source exceeds {MAX_ID_LEN} bytes"));
        }
        if self.target.len() > MAX_ID_LEN {
            return Err(format!("GraphEdge.target exceeds {MAX_ID_LEN} bytes"));
        }
        if self.relation.is_empty() {
            return Err("GraphEdge.relation must not be empty".into());
        }
        if self.relation.len() > MAX_RELATION_LEN {
            return Err(format!(
                "GraphEdge.relation exceeds {MAX_RELATION_LEN} bytes: {}",
                self.relation
            ));
        }
        if self.confidence.is_empty() {
            return Err("GraphEdge.confidence must not be empty".into());
        }
        Ok(())
    }
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
