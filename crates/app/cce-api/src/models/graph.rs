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
    /// Scoped symbol name from the symbol table, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scoped_name: Option<String>,
    /// Formatted signature of the defining entity, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
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
    /// Coarse relation domain derived by the backend (`call`, `dependency`,
    /// `structural`, `reference`, `template`, `other`); authoritative
    /// classification, frontend must not re-derive it from `relation`.
    pub domain: String,
    pub confidence: String,
    /// How the call site invokes the target (`direct`, `instance_method`, ...),
    /// when the edge carries call context.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call_context: Option<String>,
    /// True when the edge points outside the indexed project.
    #[serde(default)]
    pub is_external: bool,
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

/// A materialized subgraph: nodes plus induced edges.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct SubGraph {
    /// Nodes in encounter order, deduplicated by id.
    pub nodes: Vec<GraphNode>,
    /// Edges whose endpoints are both present.
    pub edges: Vec<GraphEdge>,
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
    #[serde(default)]
    pub offset: usize,
    #[serde(default = "default_graph_page_limit")]
    pub limit: usize,
    #[serde(default)]
    pub domains: String,
    #[serde(default = "default_true")]
    pub include_external: bool,
}

/// Two-point path query parameters.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct GraphPathQuery {
    pub start: String,
    pub end: String,
    #[serde(default = "default_path_depth")]
    pub max_depth: usize,
    #[serde(default)]
    pub domains: String,
    #[serde(default = "default_true")]
    pub include_external: bool,
}

/// Explicit entity set query parameters (comma-separated stable ids).
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SubgraphQuery {
    pub ids: String,
    #[serde(default)]
    pub offset: usize,
    #[serde(default = "default_graph_page_limit")]
    pub limit: usize,
    #[serde(default)]
    pub domains: String,
    #[serde(default = "default_true")]
    pub include_external: bool,
}

/// Full export query parameters.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ExportQuery {
    #[serde(default = "default_export_limit")]
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
    #[serde(default)]
    pub domains: String,
    #[serde(default = "default_true")]
    pub include_external: bool,
}

/// Connected component query parameters.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ComponentsQuery {
    #[serde(default)]
    pub offset: usize,
    #[serde(default = "default_graph_page_limit")]
    pub limit: usize,
    #[serde(default)]
    pub domains: String,
    #[serde(default = "default_true")]
    pub include_external: bool,
}

/// File impact query parameters.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ImpactQuery {
    pub file: String,
}

/// Entity impact query parameters.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct EntityImpactQuery {
    pub entity_id: String,
    #[serde(default = "default_impact_depth")]
    pub max_depth: usize,
    /// `entity` (default) uses entity-level edges; `file` uses file-level
    /// dependency graph after locating the entity's file.
    #[serde(default = "default_impact_scope")]
    pub scope: String,
}

fn default_impact_scope() -> String {
    "entity".to_string()
}

/// Dependency cycle query parameters.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct CyclesQuery {
    /// `entity` or `file`.
    #[serde(default = "default_cycle_level")]
    pub level: String,
    #[serde(default = "default_cycle_limit")]
    pub limit: usize,
}

/// Structural relation query parameters.
///
/// Selects one of the typed structural/frontend relation families rooted at an
/// entity.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct StructuralQuery {
    pub entity_id: String,
    /// Relation family: `trait_bound`, `child_elements`, `parent_element`,
    /// `event_handlers`, `handler_elements`, `parameter_bindings`,
    /// `template_references`, `template_ref_owners`.
    pub kind: String,
    /// `out` follows the family forward, `in` follows it in reverse.
    #[serde(default = "default_structural_direction")]
    pub direction: String,
    #[serde(default = "default_structural_limit")]
    pub limit: usize,
}

/// Subgraph response (ego, subgraph, export).
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GraphSubgraphResponse {
    pub success: bool,
    pub relation_epoch: i64,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    #[serde(default)]
    pub total_nodes: usize,
    #[serde(default)]
    pub total_edges: usize,
    /// Whether the result was truncated due to safety limits.
    #[serde(default)]
    pub truncated: bool,
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
    /// Component count before pagination.
    #[serde(default)]
    pub total_components: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relation_info: Option<serde_json::Value>,
}

/// File impact response.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GraphImpactResponse {
    pub success: bool,
    pub relation_epoch: i64,
    pub changed_file: String,
    /// Dependents exactly one hop away.
    pub direct_dependents: Vec<String>,
    /// Dependents two or more hops away; disjoint from `direct_dependents`.
    pub indirect_dependents: Vec<String>,
    pub impact_score: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relation_info: Option<serde_json::Value>,
}

/// Entity impact response.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GraphEntityImpactResponse {
    pub success: bool,
    pub relation_epoch: i64,
    pub changed_entity: String,
    /// Callers exactly one hop away.
    pub direct_dependents: Vec<String>,
    /// Callers two or more hops away; disjoint from `direct_dependents`.
    pub indirect_dependents: Vec<String>,
    pub impact_score: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relation_info: Option<serde_json::Value>,
}

/// One dependency cycle: the members in traversal order, last member calling
/// back into the first.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GraphCycle {
    /// Stable symbol ids for `level=entity`, project paths for `level=file`.
    pub members: Vec<String>,
}

/// Dependency cycles response.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GraphCyclesResponse {
    pub success: bool,
    pub relation_epoch: i64,
    pub level: String,
    pub cycles: Vec<GraphCycle>,
    /// Cycle count reported, before the `limit` cap.
    #[serde(default)]
    pub total_cycles: usize,
    /// Whether more cycles exist than were reported.
    #[serde(default)]
    pub truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relation_info: Option<serde_json::Value>,
}

/// One relation inside a structural answer.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct StructuralRelation {
    /// Stable symbol id of the counterparty.
    pub entity_id: String,
    /// Human-readable name of the counterparty.
    pub label: String,
    /// Relation type string (`trait_bound`, `contains.element`, ...).
    pub relation: String,
    /// Coarse relation domain of `relation`.
    pub domain: String,
    /// Source file of the counterparty.
    pub source_file: String,
}

/// One structural relation family resolved for an entity.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GraphStructuralResponse {
    pub success: bool,
    pub relation_epoch: i64,
    /// The requested relation family.
    pub kind: String,
    /// The requested direction (`out` or `in`).
    pub direction: String,
    pub relations: Vec<StructuralRelation>,
    /// Relation count before the `limit` cap.
    #[serde(default)]
    pub total_relations: usize,
    /// Whether more relations exist than were returned.
    #[serde(default)]
    pub truncated: bool,
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

fn default_impact_depth() -> usize {
    10
}

fn default_cycle_level() -> String {
    "entity".to_string()
}

fn default_cycle_limit() -> usize {
    100
}

fn default_structural_direction() -> String {
    "out".to_string()
}

fn default_structural_limit() -> usize {
    200
}

fn default_export_limit() -> usize {
    2000
}

fn default_graph_page_limit() -> usize {
    2000
}

fn default_true() -> bool {
    true
}

/// File module relation query parameters.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ModuleQuery {
    pub file: String,
}

/// One module-level relation of a file.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ModuleRelation {
    /// Stable symbol id of the imported/calling entity, or empty for edges
    /// that originate at the file itself.
    #[serde(default)]
    pub entity_id: String,
    /// Raw relation target as written in the source.
    pub target: String,
    /// Relation type string (`dependency.import.standard`, `call.direct`, ...).
    pub relation: String,
    /// Coarse relation domain of `relation`.
    pub domain: String,
}

/// File module relations response.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GraphModuleResponse {
    pub success: bool,
    pub relation_epoch: i64,
    pub file: String,
    /// Stable symbol ids exported by the file.
    pub exports: Vec<String>,
    /// Files whose module-level edges target this file.
    pub caller_files: Vec<String>,
    /// Module-level edges originating at the file.
    pub imports: Vec<ModuleRelation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relation_info: Option<serde_json::Value>,
}
