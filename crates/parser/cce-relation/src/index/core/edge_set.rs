//! Edge set data structures for relation index.
//!
//! Contains the ordered edge set with O(1) dedup, its identity types, and
//! auxiliary file-level record types. These are pure data structures without
//! direct dependency on `RelationIndex` storage.

use std::collections::HashSet;

use smallvec::SmallVec;

use cce_types::{EntityId, RelationType, ResolvedRelation, Span};

/// File-level relation record with explicit file path.
///
/// File-level relations have no specific entity caller; they are attributed
/// to the file itself rather than a placeholder entity. The file path is
/// stored explicitly (as the map key and inside the record) so query logic
/// never needs to handle a sentinel caller.
#[derive(Debug, Clone, Default)]
pub struct FileRelationRecord {
    pub relations: RelationEdgeSet,
    pub file_path: String,
}

/// Quality report for relation index diagnostics.
#[derive(Debug, Clone)]
pub struct QualityReport {
    pub summary: crate::index::stores::diagnostics::DiagnosticSummary,
    pub quality_score: f64,
    pub file_count: usize,
    pub entity_count: usize,
    pub relation_count: usize,
}

/// Identity of a relation edge for diff and dedup purposes.
///
/// Internal edges are identified by `(caller, callee_id, relation_type)`.
/// Edges without a callee ID participate as well: external edges
/// by callee name and classification, unresolved edges by raw target.
///
/// Call-class edges (`relation_type.is_call()`) additionally carry their
/// source span, so repeated calls to the same callee from different call
/// sites are kept as distinct edges instead of collapsing into the first
/// one. Non-call edges keep the span-free identity: their
/// meaning does not vary by call site.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RelationEdgeIdentity {
    pub caller: EntityId,
    pub kind: RelationEdgeKind,
    /// Source span discriminator, populated only for call-class edges.
    pub callsite: Option<Span>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RelationEdgeKind {
    Internal {
        callee_id: EntityId,
        relation_type: RelationType,
    },
    External {
        relation_type: RelationType,
        classification: Option<cce_types::ExternalCallType>,
        callee_name: String,
    },
    Unresolved {
        relation_type: RelationType,
        raw_target: String,
    },
}

/// Compute the diff identity of a relation edge.
///
/// Internal edges are identified by `(caller, callee_id, relation_type)`;
/// external edges (`callee_id = None`, `is_external`) additionally carry their
/// callee name and classification, and unresolved edges carry their raw
/// target. Classification participates only to separate genuinely distinct
/// call sites on the same external symbol; the name is the primary
/// discriminator so different external symbols never fold into one edge.
///
/// Call-class edges additionally carry their source span, so each call site
/// is a distinct edge. This is the single funnel for index dedup
/// (`RelationEdgeSet`), incremental diff (`index::delta`), and layered
/// snapshot views, so all three observe the same per-callsite semantics.
pub fn relation_identity(relation: &ResolvedRelation) -> RelationEdgeIdentity {
    let kind = match relation.callee_id {
        Some(callee_id) => RelationEdgeKind::Internal {
            callee_id,
            relation_type: relation.relation_type,
        },
        None if relation.is_external => RelationEdgeKind::External {
            relation_type: relation.relation_type,
            classification: relation.external_type.clone(),
            callee_name: relation.callee_name.clone(),
        },
        None => RelationEdgeKind::Unresolved {
            relation_type: relation.relation_type,
            raw_target: relation.callee_name.clone(),
        },
    };
    RelationEdgeIdentity {
        caller: relation.caller,
        kind,
        callsite: relation.relation_type.is_call().then_some(relation.span),
    }
}

/// Ordered edge set with O(1) identity dedup.
///
/// Holds `edges` in insertion order (preserving the existing
/// `RelationIndexView::relations_of` contract) and a parallel
/// `identities` set for O(1) duplicate checks.
///
/// The `callers` field embeds the reverse index (callee → callers)
/// directly inside the entry, eliminating the separate `callee_index`
/// DashMap. For a callee entity, `callers` holds the sorted list of
/// entity IDs that call it; for a caller entity, `callers` is typically
/// empty (callers are identified by their forward edges).
#[derive(Debug, Clone, Default)]
pub struct RelationEdgeSet {
    pub edges: Vec<ResolvedRelation>,
    pub identities: HashSet<RelationEdgeIdentity>,
    pub callers: SmallVec<[EntityId; 8]>,
}

impl RelationEdgeSet {
    pub(crate) fn len(&self) -> usize {
        self.edges.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.edges.is_empty()
    }

    pub(crate) fn iter(&self) -> std::slice::Iter<'_, ResolvedRelation> {
        self.edges.iter()
    }

    #[allow(dead_code)]
    pub(crate) fn contains_identity(&self, identity: &RelationEdgeIdentity) -> bool {
        self.identities.contains(identity)
    }

    /// Insert an edge if its identity is not already present.
    /// Returns true if inserted, false if duplicate.
    pub(crate) fn insert(&mut self, relation: ResolvedRelation) -> bool {
        let identity = relation_identity(&relation);
        if self.identities.contains(&identity) {
            return false;
        }
        self.identities.insert(identity);
        self.edges.push(relation);
        true
    }

    /// Remove an edge by its identity, returning true if removed.
    pub(crate) fn remove_by_identity(&mut self, identity: &RelationEdgeIdentity) -> bool {
        if !self.identities.remove(identity) {
            return false;
        }
        self.edges
            .retain(|existing| relation_identity(existing) != *identity);
        true
    }

    /// Retain only edges satisfying predicate, incrementally updating the identity set.
    ///
    /// Unlike the previous implementation that cleared and rebuilt the entire
    /// identity set, this version removes identities incrementally as edges
    /// are filtered out, avoiding O(n) re-hashing when most edges are retained.
    pub(crate) fn retain<F>(&mut self, mut f: F)
    where
        F: FnMut(&ResolvedRelation) -> bool,
    {
        let mut i = 0;
        while i < self.edges.len() {
            if !f(&self.edges[i]) {
                let removed = self.edges.remove(i);
                self.identities.remove(&relation_identity(&removed));
            } else {
                i += 1;
            }
        }
    }

    /// Add a caller to the reverse index, maintaining sorted order.
    pub(crate) fn add_caller(&mut self, caller: EntityId) {
        if let Err(pos) = self.callers.binary_search(&caller) {
            self.callers.insert(pos, caller);
        }
    }

    /// Remove a caller from the reverse index. Returns true if removed.
    pub(crate) fn remove_caller(&mut self, caller: &EntityId) -> bool {
        if let Ok(pos) = self.callers.binary_search(caller) {
            self.callers.remove(pos);
            true
        } else {
            false
        }
    }

    /// Returns the sorted list of callers.
    pub(crate) fn callers(&self) -> &[EntityId] {
        &self.callers
    }

    /// Returns true if there are no callers.
    pub(crate) fn is_callers_empty(&self) -> bool {
        self.callers.is_empty()
    }
}

impl std::ops::Deref for RelationEdgeSet {
    type Target = Vec<ResolvedRelation>;
    fn deref(&self) -> &Self::Target {
        &self.edges
    }
}

impl<'a> IntoIterator for &'a RelationEdgeSet {
    type Item = &'a ResolvedRelation;
    type IntoIter = std::slice::Iter<'a, ResolvedRelation>;
    fn into_iter(self) -> Self::IntoIter {
        self.edges.iter()
    }
}

impl IntoIterator for RelationEdgeSet {
    type Item = ResolvedRelation;
    type IntoIter = std::vec::IntoIter<ResolvedRelation>;
    fn into_iter(self) -> Self::IntoIter {
        self.edges.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::Position;
    use cce_types::types::relation::CallContext;

    fn span(start: usize, end: usize) -> Span {
        Span {
            start_byte: start,
            end_byte: end,
            start_position: Position::new(0, start),
            end_position: Position::new(0, end),
        }
    }

    fn call_edge(caller: u64, callee: u64, span: Span) -> ResolvedRelation {
        ResolvedRelation {
            caller: EntityId(caller),
            callee_id: Some(EntityId(callee)),
            callee_name: "combine".to_string(),
            relation_type: RelationType::DirectCall,
            span,
            is_external: false,
            external_type: None,
            callee_symbol: None,
            stdlib_category: None,
            owner_type: None,
            call_context: CallContext::default(),
            overload_signature: None,
        }
    }

    #[test]
    fn call_edges_at_distinct_callsites_are_distinct() {
        // Calls to the same callee from distinct spans stay distinct.
        let mut set = RelationEdgeSet::default();
        assert!(set.insert(call_edge(1, 2, span(100, 110))));
        assert!(set.insert(call_edge(1, 2, span(120, 130))));
        assert!(set.insert(call_edge(1, 2, span(140, 150))));
        assert_eq!(set.len(), 3);
    }

    #[test]
    fn duplicate_call_at_same_span_still_dedups() {
        let mut set = RelationEdgeSet::default();
        assert!(set.insert(call_edge(1, 2, span(100, 110))));
        assert!(!set.insert(call_edge(1, 2, span(100, 110))));
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn non_call_edges_keep_span_free_identity() {
        // Inheritance edges do not vary by call site: same
        // (caller, callee, type) with different spans still collapses.
        let mut inherit = call_edge(1, 2, span(100, 110));
        inherit.relation_type = RelationType::Inheritance;
        let mut other = inherit.clone();
        other.span = span(500, 510);
        let mut set = RelationEdgeSet::default();
        assert!(set.insert(inherit));
        assert!(!set.insert(other));
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn remove_by_identity_targets_single_callsite() {
        let mut set = RelationEdgeSet::default();
        let first = call_edge(1, 2, span(100, 110));
        let second = call_edge(1, 2, span(120, 130));
        set.insert(first.clone());
        set.insert(second);
        assert!(set.remove_by_identity(&relation_identity(&first)));
        assert_eq!(set.len(), 1);
        assert_eq!(set.edges[0].span.start_byte, 120);
    }
}
