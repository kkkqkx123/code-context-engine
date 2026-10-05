//! Graph data model for independent relation retrieval.
//!
//! The model mirrors the node-link shape used for graph exchange:
//! a flat node list plus a flat edge list. It is the shared contract
//! between the in-memory graph service, the HTTP graph endpoints,
//! and future offline consumers.

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

/// Coarse relation domain carried alongside the raw relation string.
///
/// Derived from `RelationType` so the frontend does not have to re-derive
/// the classification from relation strings. Raw string edges without a
/// parsed `RelationType` (for example plugin-provided relations) map to
/// `"other"`.
pub fn relation_domain(relation_type: &cce_types::RelationType) -> &'static str {
    if relation_type.is_call() {
        "call"
    } else if relation_type.is_dependency() {
        "dependency"
    } else if relation_type.is_structural() {
        "structural"
    } else if relation_type.is_reference() {
        "reference"
    } else if relation_type.is_template_relation() {
        "template"
    } else {
        "other"
    }
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
    /// Whether to keep edges pointing outside the indexed project.
    pub include_external: bool,
}

impl GraphFilter {
    /// Create a filter that keeps everything.
    pub fn allow_all() -> Self {
        Self {
            relation_domains: Vec::new(),
            include_external: true,
        }
    }

    /// Whether the filter drops nothing.
    pub fn is_empty(&self) -> bool {
        self.relation_domains.is_empty() && self.include_external
    }

    /// Whether a materialized edge passes the filter.
    pub fn matches_edge(&self, edge: &GraphEdge) -> bool {
        if !self.include_external && edge.is_external {
            return false;
        }
        if self.relation_domains.is_empty() {
            return true;
        }
        self.relation_domains
            .iter()
            .any(|domain| domain == &edge.domain)
    }
}

/// Pagination over a materialized subgraph.
#[derive(Debug, Clone, Copy)]
pub struct GraphPagination {
    /// Number of leading nodes and edges to skip.
    pub offset: usize,
    /// Maximum number of nodes and edges to keep.
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
    /// Edges after pagination, restricted to surviving nodes.
    pub edges: Vec<GraphEdge>,
    /// Node count before pagination.
    pub total_nodes: usize,
    /// Edge count before pagination.
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
