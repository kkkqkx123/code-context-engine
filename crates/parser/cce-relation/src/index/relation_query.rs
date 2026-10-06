//! Relation query operations for RelationIndex
//!
//! This module provides query-related operations as extension traits.
//! It handles forward/reverse lookups, hierarchy queries, and frontend queries.

use crate::error::IndexError;
use cce_types::{EntityId, RelationType, ResolvedRelation};
use dashmap::DashMap;
use std::collections::HashSet;

use super::core::{RelationEdgeSet, RelationIndex};

/// Upper bound for reverse fan-in materialization.
///
/// Reverse lookups with extremely large caller sets are truncated to keep
/// query memory bounded; callers beyond the cap are dropped deterministically
/// after sorting. No type-partition table is built.
pub const MAX_REVERSE_FANIN: usize = 10_000;

/// Relation query operations extension trait
///
/// Provides methods for querying relations in the index.
pub trait RelationQueryOps {
    /// Get resolved relations by caller EntityId
    fn get_resolved_relations_by_caller(
        &self,
        caller_id: EntityId,
    ) -> Option<dashmap::mapref::one::Ref<'_, EntityId, RelationEdgeSet>>;

    /// Get resolved relations by caller EntityId with validation
    ///
    /// Returns an error if the caller doesn't exist.
    fn get_resolved_relations_by_caller_checked(
        &self,
        caller_id: EntityId,
    ) -> Result<Vec<ResolvedRelation>, IndexError>;

    /// Get callers by callee EntityId (uses reverse index)
    fn get_callers_by_callee_entity(&self, callee_id: EntityId) -> Vec<EntityId>;

    /// Get callers by callee EntityId with validation
    ///
    /// Returns an error if the callee doesn't exist.
    fn get_callers_by_callee_entity_checked(
        &self,
        callee_id: EntityId,
    ) -> Result<Vec<EntityId>, IndexError>;

    /// Get callers by callee EntityId and relation type
    fn get_callers_by_callee_and_type(
        &self,
        callee_id: EntityId,
        relation_type: RelationType,
    ) -> Vec<EntityId>;

    /// Get relations targeting a specific entity
    fn get_relations_to_entity(&self, callee_id: EntityId) -> Vec<ResolvedRelation>;

    /// Get relations targeting a specific entity by type
    fn get_relations_to_entity_by_type(
        &self,
        callee_id: EntityId,
        relation_type: RelationType,
    ) -> Vec<ResolvedRelation>;

    /// Get relations from a specific entity by type
    fn get_relations_from_entity_by_type(
        &self,
        caller_id: EntityId,
        relation_type: RelationType,
    ) -> Vec<ResolvedRelation>;

    /// Get total number of resolved relations
    fn resolved_relation_count(&self) -> usize;

    /// Get reference to resolved relation index
    fn resolved_relation_index(&self) -> &DashMap<EntityId, RelationEdgeSet>;
}

impl RelationQueryOps for RelationIndex {
    fn get_resolved_relations_by_caller(
        &self,
        caller_id: EntityId,
    ) -> Option<dashmap::mapref::one::Ref<'_, EntityId, RelationEdgeSet>> {
        self.resolved_relation_index.get(&caller_id)
    }

    fn get_resolved_relations_by_caller_checked(
        &self,
        caller_id: EntityId,
    ) -> Result<Vec<ResolvedRelation>, IndexError> {
        // Check if caller exists
        if !self.function_index.contains_key(&caller_id) {
            return Err(IndexError::entity_not_found(caller_id));
        }

        // Existing entities without outgoing edges return an empty list.
        Ok(self
            .resolved_relation_index
            .get(&caller_id)
            .map(|r| r.edges.clone())
            .unwrap_or_default())
    }

    fn get_callers_by_callee_entity(&self, callee_id: EntityId) -> Vec<EntityId> {
        // The reverse map is authoritative: an entry lists every caller, and a
        // miss means no callers. Never derive callers from the callee's own
        // outgoing edges — that answers a different question.
        let Some(callers) = self.reverse_callee_index.get(&callee_id) else {
            return Vec::new();
        };
        let mut result = callers.clone();
        result.sort();
        result.dedup();
        result.truncate(MAX_REVERSE_FANIN);
        result
    }

    fn get_callers_by_callee_entity_checked(
        &self,
        callee_id: EntityId,
    ) -> Result<Vec<EntityId>, IndexError> {
        if !self.function_index.contains_key(&callee_id) {
            return Err(IndexError::entity_not_found(callee_id));
        }
        Ok(self.get_callers_by_callee_entity(callee_id))
    }

    fn get_callers_by_callee_and_type(
        &self,
        callee_id: EntityId,
        relation_type: RelationType,
    ) -> Vec<EntityId> {
        // Single merged edge walk: derive callers from full edges filtered by
        // type instead of reverse-list plus per-caller forward re-verification.
        let mut seen: HashSet<EntityId> = HashSet::new();
        let mut callers = Vec::new();
        for relation in self.get_relations_to_entity_by_type(callee_id, relation_type) {
            if seen.insert(relation.caller) {
                callers.push(relation.caller);
                if callers.len() >= MAX_REVERSE_FANIN {
                    break;
                }
            }
        }
        callers
    }

    fn get_relations_to_entity(&self, callee_id: EntityId) -> Vec<ResolvedRelation> {
        // Enumerate callers through the reverse map, then resolve each
        // caller's edges. A reverse miss means nothing points at this callee,
        // so no scan and no fallback lookup is warranted.
        let Some(callers) = self.reverse_callee_index.get(&callee_id) else {
            return Vec::new();
        };
        let mut limited = callers.clone();
        limited.sort();
        limited.dedup();
        limited.truncate(MAX_REVERSE_FANIN);
        let mut result = Vec::new();
        for caller_id in limited {
            if let Some(relations) = self.resolved_relation_index.get(&caller_id) {
                for relation in relations.iter() {
                    if relation.callee_id == Some(callee_id) {
                        result.push(relation.clone());
                    }
                }
            }
        }
        result
    }

    fn get_relations_to_entity_by_type(
        &self,
        callee_id: EntityId,
        relation_type: RelationType,
    ) -> Vec<ResolvedRelation> {
        // Single merged edge walk with in-memory type filtering.
        self.get_relations_to_entity(callee_id)
            .into_iter()
            .filter(|r| r.relation_type == relation_type)
            .collect()
    }

    fn get_relations_from_entity_by_type(
        &self,
        caller_id: EntityId,
        relation_type: RelationType,
    ) -> Vec<ResolvedRelation> {
        self.resolved_relation_index
            .get(&caller_id)
            .map(|relations| {
                relations
                    .iter()
                    .filter(|r| r.relation_type == relation_type)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    fn resolved_relation_count(&self) -> usize {
        self.resolved_relation_index.iter().map(|v| v.len()).sum()
    }

    fn resolved_relation_index(&self) -> &DashMap<EntityId, RelationEdgeSet> {
        &self.resolved_relation_index
    }
}

/// Hierarchy query operations extension trait
///
/// Provides methods for querying class/interface hierarchy relationships.
pub trait HierarchyQueryOps {
    /// Get derived classes (classes that extend this class)
    fn get_derived_classes(&self, class_id: EntityId) -> Vec<EntityId>;

    /// Get implementing classes (classes that implement this interface)
    fn get_implementing_classes(&self, interface_id: EntityId) -> Vec<EntityId>;

    /// Get types with this trait bound (for Rust trait bounds)
    fn get_types_with_trait_bound(&self, trait_id: EntityId) -> Vec<EntityId>;

    /// Get base classes (classes this class extends)
    fn get_base_classes(&self, class_id: EntityId) -> Vec<EntityId>;

    /// Get implemented interfaces
    fn get_implemented_interfaces(&self, class_id: EntityId) -> Vec<EntityId>;
}

impl HierarchyQueryOps for RelationIndex {
    fn get_derived_classes(&self, class_id: EntityId) -> Vec<EntityId> {
        RelationQueryOps::get_callers_by_callee_and_type(self, class_id, RelationType::Inheritance)
    }

    fn get_implementing_classes(&self, interface_id: EntityId) -> Vec<EntityId> {
        RelationQueryOps::get_callers_by_callee_and_type(
            self,
            interface_id,
            RelationType::Implementation,
        )
    }

    fn get_types_with_trait_bound(&self, trait_id: EntityId) -> Vec<EntityId> {
        RelationQueryOps::get_callers_by_callee_and_type(self, trait_id, RelationType::TraitBound)
    }

    fn get_base_classes(&self, class_id: EntityId) -> Vec<EntityId> {
        self.resolved_relation_index
            .get(&class_id)
            .map(|relations| {
                relations
                    .iter()
                    .filter(|r| r.relation_type == RelationType::Inheritance)
                    .filter_map(|r| r.callee_id)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn get_implemented_interfaces(&self, class_id: EntityId) -> Vec<EntityId> {
        self.resolved_relation_index
            .get(&class_id)
            .map(|relations| {
                relations
                    .iter()
                    .filter(|r| r.relation_type == RelationType::Implementation)
                    .filter_map(|r| r.callee_id)
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// Frontend/Markup query operations extension trait
///
/// Provides methods for querying frontend component relationships.
pub trait FrontendQueryOps {
    /// Get child elements (via ElementContains relation)
    fn get_child_elements(&self, parent_id: EntityId) -> Vec<EntityId>;

    /// Get parent element (via reverse ElementContains relation)
    fn get_parent_element(&self, child_id: EntityId) -> Vec<EntityId>;

    /// Get event handlers bound to an element/component
    fn get_event_handlers(&self, element_id: EntityId) -> Vec<ResolvedRelation>;

    /// Get elements that use a specific event handler
    fn get_elements_by_handler(&self, handler_id: EntityId) -> Vec<EntityId>;

    /// Get parameter bindings (props) of a component
    fn get_parameter_bindings(&self, component_id: EntityId) -> Vec<ResolvedRelation>;

    /// Get template references (ref/bind:this) of an element
    fn get_template_references(&self, element_id: EntityId) -> Vec<ResolvedRelation>;

    /// Get components/elements that reference a specific entity via template reference
    fn get_elements_by_template_ref(&self, target_id: EntityId) -> Vec<EntityId>;
}

impl FrontendQueryOps for RelationIndex {
    fn get_child_elements(&self, parent_id: EntityId) -> Vec<EntityId> {
        RelationQueryOps::get_relations_from_entity_by_type(
            self,
            parent_id,
            RelationType::ElementContains,
        )
        .into_iter()
        .filter_map(|r| r.callee_id)
        .collect()
    }

    fn get_parent_element(&self, child_id: EntityId) -> Vec<EntityId> {
        RelationQueryOps::get_callers_by_callee_and_type(
            self,
            child_id,
            RelationType::ElementContains,
        )
    }

    fn get_event_handlers(&self, element_id: EntityId) -> Vec<ResolvedRelation> {
        RelationQueryOps::get_relations_from_entity_by_type(
            self,
            element_id,
            RelationType::EventCallback,
        )
    }

    fn get_elements_by_handler(&self, handler_id: EntityId) -> Vec<EntityId> {
        RelationQueryOps::get_callers_by_callee_and_type(
            self,
            handler_id,
            RelationType::EventCallback,
        )
    }

    fn get_parameter_bindings(&self, component_id: EntityId) -> Vec<ResolvedRelation> {
        RelationQueryOps::get_relations_from_entity_by_type(
            self,
            component_id,
            RelationType::ParameterBinding,
        )
    }

    fn get_template_references(&self, element_id: EntityId) -> Vec<ResolvedRelation> {
        RelationQueryOps::get_relations_from_entity_by_type(
            self,
            element_id,
            RelationType::TemplateReference,
        )
    }

    fn get_elements_by_template_ref(&self, target_id: EntityId) -> Vec<EntityId> {
        RelationQueryOps::get_callers_by_callee_and_type(
            self,
            target_id,
            RelationType::TemplateReference,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::entity_index::EntityIndexOps;
    use cce_types::{Entity, EntityKind, Span};
    use std::collections::HashMap;

    fn create_test_entity(id: u32, name: &str) -> Entity {
        Entity {
            id: EntityId(id.into()),
            kind: EntityKind::Function,
            name: name.to_string(),
            signature: format!("fn {}()", name),
            parameters: Vec::new(),
            return_type: None,
            span: Span::default(),
            depth: 0,
            parent: None,
            children: Vec::new(),
            doc_comment: None,
            modifiers: Vec::new(),
            attributes: HashMap::new(),
            metadata: HashMap::new(),
            is_stdlib: false,
            subtype: None,
            stdlib_category: None,
        }
    }

    /// A callee that itself calls something must not be reported as a caller
    /// of itself: reverse lookups read the reverse map only.
    #[test]
    fn reverse_lookup_ignores_the_callees_own_outgoing_edges() {
        let index = RelationIndex::new();
        index.add_function(EntityId(1), create_test_entity(1, "middle"));
        index.add_function(EntityId(2), create_test_entity(2, "target"));
        // 1 -> 2 only.
        index.add_resolved_relation(ResolvedRelation {
            caller: EntityId(1),
            callee_id: Some(EntityId(2)),
            callee_name: "target".to_string(),
            relation_type: RelationType::DirectCall,
            span: Span::default(),
            is_external: false,
            external_type: None,
            callee_symbol: None,
            stdlib_category: None,
            owner_type: None,
            call_context: cce_types::relation::CallContext::Direct,
            overload_signature: None,
        });

        assert_eq!(
            index.get_callers_by_callee_entity(EntityId(2)),
            vec![EntityId(1)]
        );
        assert!(index.get_callers_by_callee_entity(EntityId(1)).is_empty());
        assert_eq!(
            index.get_relations_to_entity(EntityId(2)).len(),
            1,
            "the single 1->2 edge is the only relation targeting 2"
        );
        assert!(
            index.get_relations_to_entity(EntityId(1)).is_empty(),
            "nothing targets 1; its own outgoing edge must not resurface"
        );
    }

    #[test]
    fn hierarchy_queries_only_follow_the_requested_family() {
        let index = RelationIndex::new();
        index.add_function(EntityId(1), create_test_entity(1, "base"));
        index.add_function(EntityId(2), create_test_entity(2, "derived"));
        index.add_function(EntityId(3), create_test_entity(3, "unrelated"));

        index.add_resolved_relation(ResolvedRelation {
            caller: EntityId(2),
            callee_id: Some(EntityId(1)),
            callee_name: "base".to_string(),
            relation_type: RelationType::Inheritance,
            span: Span::default(),
            is_external: false,
            external_type: None,
            callee_symbol: None,
            stdlib_category: None,
            owner_type: None,
            call_context: cce_types::relation::CallContext::Direct,
            overload_signature: None,
        });

        assert_eq!(index.get_derived_classes(EntityId(1)), vec![EntityId(2)]);
        assert_eq!(index.get_base_classes(EntityId(2)), vec![EntityId(1)]);
        // 2 inherits from 1 but implements nothing, so it is not an implementor.
        assert!(index.get_implementing_classes(EntityId(1)).is_empty());
        assert!(index.get_types_with_trait_bound(EntityId(1)).is_empty());
        assert!(index.get_derived_classes(EntityId(3)).is_empty());
    }

    #[test]
    fn test_hierarchy_queries() {
        let index = RelationIndex::new();

        index.add_function(EntityId(1), create_test_entity(1, "BaseClass"));
        index.add_function(EntityId(2), create_test_entity(2, "DerivedClass"));

        // Add inheritance relation
        index.add_resolved_relation(ResolvedRelation {
            caller: EntityId(2),
            callee_id: Some(EntityId(1)),
            callee_name: "BaseClass".to_string(),
            relation_type: RelationType::Inheritance,
            span: Span::default(),
            is_external: false,
            external_type: None,
            callee_symbol: None,
            stdlib_category: None,
            owner_type: None,
            call_context: cce_types::relation::CallContext::Direct,
            overload_signature: None,
        });

        // Test derived classes
        let derived = index.get_derived_classes(EntityId(1));
        assert_eq!(derived.len(), 1);
        assert_eq!(derived[0], EntityId(2));

        // Test base classes
        let bases = index.get_base_classes(EntityId(2));
        assert_eq!(bases.len(), 1);
        assert_eq!(bases[0], EntityId(1));
    }
}
