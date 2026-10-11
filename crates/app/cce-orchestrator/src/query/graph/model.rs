//! Graph data model for independent relation retrieval.
//!
//! The model mirrors the node-link shape used for graph exchange:
//! a flat node list plus a flat edge list. It is the shared contract
//! between the in-memory graph service, the HTTP graph endpoints,
//! and future offline consumers.

use sha2::{Digest, Sha256};

use cce_types::{ResolvedRelation, relation::CallContext};
use serde::{Deserialize, Serialize};

/// Confidence of a graph edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Confidence {
    /// Relationship stated explicitly in source (direct call, import).
    Extracted,
    /// Relationship deduced during resolution (inference, cross-file link).
    Inferred,
    /// Relationship pointing outside the indexed project.
    External,
}

impl std::fmt::Display for Confidence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Extracted => write!(f, "EXTRACTED"),
            Self::Inferred => write!(f, "INFERRED"),
            Self::External => write!(f, "EXTERNAL"),
        }
    }
}

/// Map a resolved relation onto an edge confidence.
///
/// External references are always external. Direct calls that survived
/// resolution without indirection are explicit. Everything else
/// (method dispatch, inferred links) is an inference.
pub fn confidence_of(relation: &ResolvedRelation) -> Confidence {
    if relation.is_external {
        Confidence::External
    } else if relation.relation_type.is_call()
        && matches!(relation.call_context, CallContext::Direct)
    {
        Confidence::Extracted
    } else {
        Confidence::Inferred
    }
}

/// A node in the relation graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    /// Stable identifier (symbol id when available, entity fallback otherwise).
    pub id: String,
    /// Human-readable name.
    pub label: String,
    /// Entity kind (`function`, `class`, `external`, ...).
    pub kind: String,
    /// Source file path (empty for synthetic external nodes).
    pub source_file: String,
    /// Source location (`L<line>`, empty when unknown).
    pub source_location: String,
    /// Scoped symbol name from the symbol table (`Module::item`), when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scoped_name: Option<String>,
    /// Formatted signature of the defining entity, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

/// An edge in the relation graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    /// Source node id.
    pub source: String,
    /// Target node id.
    pub target: String,
    /// Relation name (stable relation type string).
    pub relation: String,
    /// Coarse domain of the relation (`call`, `dependency`, `structural`,
    /// `reference`, `template`, `other`); authoritative classification.
    pub domain: String,
    /// Edge confidence.
    pub confidence: Confidence,
    /// How the call site invokes the target (`direct`, `instance_method`, ...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call_context: Option<String>,
    /// True when the edge points outside the indexed project.
    #[serde(default)]
    pub is_external: bool,
    /// Structural bearing for weighted traversal: relation strength scaled by
    /// call heat (1.0 when neither applies).
    #[serde(default = "default_edge_weight")]
    pub weight: f32,
    /// Conditional-compilation guard the caller entity carries. The edge
    /// exists only under that predicate, so consumers that expand or prune a
    /// graph by platform can exclude it without re-reading the source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cfg_condition: Option<String>,
    /// Call-site byte span (`start-end`) for call-class edges; `None` for
    /// non-call edges whose identity does not vary by call site.
    ///
    /// Callers use this to tell repeated calls between one entity pair apart.
    /// Edge counts are per call site when this is present, per entity pair
    /// otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub callsite: Option<String>,
    /// Stable edge identity: content digest over
    /// `(source, target, relation[, callsite])`.
    ///
    /// Endpoints are node ids, so the identity survives the run-local entity
    /// numbering. Call-class edges fold the call site in, matching the index
    /// `RelationEdgeIdentity`; non-call edges hash the triple only.
    /// Consumers use it to tell one edge emitted from both directions apart
    /// from genuinely distinct edges.
    pub edge_id: String,
}

/// Content-addressed identity of a graph edge.
///
/// Hashes the `(source, target, relation)` triple with the same digest shape
/// as node ids, so edge and node ids share one encoding convention.
pub fn edge_id(source: &str, target: &str, relation: &str) -> String {
    edge_id_with_callsite(source, target, relation, None)
}

/// Content-addressed identity with an optional call-site discriminator.
///
/// Call-class edges pass their `start-end` span so each call site is a
/// distinct edge; non-call edges pass `None` and collapse to the triple.
pub fn edge_id_with_callsite(
    source: &str,
    target: &str,
    relation: &str,
    callsite: Option<&str>,
) -> String {
    let mut hasher = Sha256::new();
    for component in [source, target, relation] {
        hasher.update((component.len() as u64).to_le_bytes());
        hasher.update(component.as_bytes());
    }
    if let Some(site) = callsite {
        hasher.update((site.len() as u64).to_le_bytes());
        hasher.update(site.as_bytes());
    }
    format!("edge_{:x}", hasher.finalize())
}

/// Render a call-site span for edge identity and display.
pub fn callsite_of(span: &cce_types::Span) -> String {
    format!("{}-{}", span.start_byte, span.end_byte)
}

fn default_edge_weight() -> f32 {
    1.0
}

/// A materialized subgraph: nodes plus induced edges.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SubGraph {
    /// Nodes in encounter order, deduplicated by id.
    pub nodes: Vec<GraphNode>,
    /// Edges whose endpoints are both present.
    pub edges: Vec<GraphEdge>,
}

/// Filter for graph expansion and materialization.
#[derive(Debug, Clone, Default)]
pub struct GraphFilter {
    /// Keep only relations in these coarse domains. Empty means no filtering.
    pub relation_domains: Vec<String>,
    /// Keep only relations whose exact type is in this set. Empty means no
    /// fine-grained filtering. Applied after `relation_domains`.
    pub relation_types: Vec<String>,
    /// Whether to keep edges pointing outside the indexed project.
    pub include_external: bool,
}

impl GraphFilter {
    /// Create a filter that keeps everything.
    pub fn allow_all() -> Self {
        Self {
            relation_domains: Vec::new(),
            relation_types: Vec::new(),
            include_external: true,
        }
    }

    /// Whether the filter drops nothing.
    pub fn is_empty(&self) -> bool {
        self.relation_domains.is_empty() && self.relation_types.is_empty() && self.include_external
    }

    /// Whether a materialized edge passes the filter.
    pub fn matches_edge(&self, edge: &GraphEdge) -> bool {
        if !self.include_external && edge.is_external {
            return false;
        }
        if !self.relation_domains.is_empty()
            && !self
                .relation_domains
                .iter()
                .any(|domain| domain == &edge.domain)
        {
            return false;
        }
        if !self.relation_types.is_empty()
            && !self.relation_types.iter().any(|t| t == &edge.relation)
        {
            return false;
        }
        true
    }
}

/// Pagination over a materialized subgraph.
///
/// Addresses nodes only: the resulting edges are every edge induced on the
/// selected node page.
#[derive(Debug, Clone, Copy)]
pub struct GraphPagination {
    /// Number of leading nodes to skip.
    pub offset: usize,
    /// Maximum number of nodes to keep.
    pub limit: usize,
}

impl Default for GraphPagination {
    fn default() -> Self {
        Self {
            offset: 0,
            limit: usize::MAX,
        }
    }
}

/// A subgraph slice with totals before pagination.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PagedSubGraph {
    /// Nodes after pagination.
    pub nodes: Vec<GraphNode>,
    /// Every filtered edge whose endpoints are both present in `nodes`.
    pub edges: Vec<GraphEdge>,
    /// Node count before pagination.
    pub total_nodes: usize,
    /// Filtered edge count before pagination, over the whole node set.
    pub total_edges: usize,
}

impl PagedSubGraph {
    /// Drop pagination metadata, keeping only the materialized slice.
    pub fn into_subgraph(self) -> SubGraph {
        SubGraph {
            nodes: self.nodes,
            edges: self.edges,
        }
    }
}

/// A page of connected components with the total before pagination.
///
/// Components are ordered largest first so a truncated page keeps the
/// structurally significant ones; members are sorted within each component.
#[derive(Debug, Clone, Default)]
pub struct PagedComponents {
    /// Components after pagination.
    pub components: Vec<Vec<cce_types::EntityId>>,
    /// Component count before pagination.
    pub total_components: usize,
}

impl SubGraph {
    /// Serialize the subgraph to node-link JSON bytes.
    pub fn to_node_link_json(&self) -> serde_json::Result<Vec<u8>> {
        serde_json::to_vec(&serde_json::json!({
            "nodes": self.nodes,
            "links": self.edges,
        }))
    }
}

/// Render an entity kind with its serialized name.
pub fn kind_label(kind: &cce_types::EntityKind) -> String {
    serde_json::to_value(kind)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::{EntityId, RelationType};

    fn resolved(caller: u64, callee: Option<u64>, external: bool) -> ResolvedRelation {
        ResolvedRelation {
            caller: EntityId(caller),
            callee_id: callee.map(EntityId),
            callee_name: "target".to_string(),
            relation_type: RelationType::DirectCall,
            span: cce_types::Span::default(),
            is_external: external,
            external_type: None,
            callee_symbol: None,
            stdlib_category: None,
            owner_type: None,
            call_context: CallContext::Direct,
            overload_signature: None,
            call_frequency: 1,
            cfg_condition: None,
        }
    }

    #[test]
    fn test_confidence_mapping() {
        assert_eq!(
            confidence_of(&resolved(1, Some(2), false)),
            Confidence::Extracted
        );
        assert_eq!(
            confidence_of(&resolved(1, None, true)),
            Confidence::External
        );
        let mut inferred = resolved(1, Some(2), false);
        inferred.call_context = CallContext::InstanceMethod {
            receiver_type: "Foo".to_string(),
        };
        assert_eq!(confidence_of(&inferred), Confidence::Inferred);
    }

    #[test]
    fn test_subgraph_node_link_shape() {
        let graph = SubGraph {
            nodes: vec![GraphNode {
                id: "a".to_string(),
                label: "a".to_string(),
                kind: "function".to_string(),
                source_file: "src/a.rs".to_string(),
                source_location: "L1".to_string(),
                scoped_name: None,
                signature: None,
            }],
            edges: vec![],
        };
        let bytes = graph.to_node_link_json().expect("serialize");
        let value: serde_json::Value = serde_json::from_slice(&bytes).expect("parse");
        assert!(value.get("nodes").is_some());
        assert!(value.get("links").is_some());
    }
}
