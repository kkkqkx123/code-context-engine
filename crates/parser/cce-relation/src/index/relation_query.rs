//! Relation query operations for RelationIndex
//!
//! This module provides query-related operations as extension traits.
//! It handles forward/reverse lookups, hierarchy queries, and frontend queries.

use crate::error::IndexError;
use cce_types::{EntityId, RelationType, ResolvedRelation};
use dashmap::DashMap;

use super::core::{RelationEdgeSet, RelationIndex};

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

    /// Get total number of call relations
    fn call_count(&self) -> usize;

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

        // Get relations
        self.resolved_relation_index
            .get(&caller_id)
            .map(|r| r.edges.clone())
            .ok_or_else(|| {
                IndexError::inconsistent_state(format!(
                    "Entity {:?} exists but has no relation entry",
                    caller_id
                ))
            })
    }

    fn get_callers_by_callee_entity(&self, callee_id: EntityId) -> Vec<EntityId> {
        if let Some(callers) = self.reverse_callee_index.get(&callee_id) {
            return callers.clone();
        }
        // Fallback: legacy embedded list for indexes built before reverse
        // was introduced (e.g. deserialized snapshots).
        if let Some(entry) = self.resolved_relation_index.get(&callee_id) {
            let callers = entry.callers();
            if !callers.is_empty() {
                return callers.to_vec();
            }
        }
        // For callee-only entities without reverse entry, scan.
        let mut result: Vec<EntityId> = self
            .resolved_relation_index
            .iter()
            .filter(|entry| entry.value().iter().any(|r| r.callee_id == Some(callee_id)))
            .map(|entry| *entry.key())
            .collect();
        result.sort();
        result.dedup();
        result
    }

    fn get_callers_by_callee_entity_checked(
        &self,
        callee_id: EntityId,
    ) -> Result<Vec<EntityId>, IndexError> {
        Ok(self.get_callers_by_callee_entity(callee_id))
    }

    fn get_callers_by_callee_and_type(
        &self,
        callee_id: EntityId,
        relation_type: RelationType,
    ) -> Vec<EntityId> {
        // Prefer reverse index O(1) lookup, filter by type without full scan.
        if let Some(callers) = self.reverse_callee_index.get(&callee_id) {
            return callers
                .iter()
                .filter(|caller_id| {
                    self.resolved_relation_index
                        .get(*caller_id)
                        .is_some_and(|relations| {
                            relations.iter().any(|r| {
                                r.callee_id == Some(callee_id) && r.relation_type == relation_type
                            })
                        })
                })
                .copied()
                .collect();
        }
        let mut callers = Vec::new();
        if let Some(entry) = self.resolved_relation_index.get(&callee_id) {
            for caller_id in entry.callers() {
                if let Some(relations) = self.resolved_relation_index.get(caller_id) {
                    if relations
                        .iter()
                        .any(|r| r.callee_id == Some(callee_id) && r.relation_type == relation_type)
                    {
                        callers.push(*caller_id);
                    }
                }
            }
        }
        if callers.is_empty() {
            for entry in self.resolved_relation_index.iter() {
                if entry
                    .value()
                    .iter()
                    .any(|r| r.callee_id == Some(callee_id) && r.relation_type == relation_type)
                {
                    callers.push(*entry.key());
                }
            }
        }
        callers
    }

    fn get_relations_to_entity(&self, callee_id: EntityId) -> Vec<ResolvedRelation> {
        // Use reverse index to enumerate callers O(k) without full scan.
        let callers: Vec<EntityId> = if let Some(v) = self.reverse_callee_index.get(&callee_id) {
            v.clone()
        } else if let Some(entry) = self.resolved_relation_index.get(&callee_id) {
            let c = entry.callers();
            if !c.is_empty() {
                c.to_vec()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };
        if !callers.is_empty() {
            let mut result = Vec::new();
            for caller_id in callers {
                if let Some(relations) = self.resolved_relation_index.get(&caller_id) {
                    for r in relations.iter() {
                        if r.callee_id == Some(callee_id) {
                            result.push(r.clone());
                        }
                    }
                }
            }
            if !result.is_empty() {
                return result;
            }
        }
        // Fallback full scan for legacy indexes.
        let mut result = Vec::new();
        for entry in self.resolved_relation_index.iter() {
            for r in entry.value().iter() {
                if r.callee_id == Some(callee_id) {
                    result.push(r.clone());
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
        let callers: Vec<EntityId> = if let Some(v) = self.reverse_callee_index.get(&callee_id) {
            v.clone()
        } else if let Some(entry) = self.resolved_relation_index.get(&callee_id) {
            let c = entry.callers();
            if !c.is_empty() {
                c.to_vec()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };
        if !callers.is_empty() {
            let mut result = Vec::new();
            for caller_id in callers {
                if let Some(relations) = self.resolved_relation_index.get(&caller_id) {
                    for r in relations.iter() {
                        if r.callee_id == Some(callee_id) && r.relation_type == relation_type {
                            result.push(r.clone());
                        }
                    }
                }
            }
            if !result.is_empty() {
                return result;
            }
        }
        let mut result = Vec::new();
        for entry in self.resolved_relation_index.iter() {
            for r in entry.value().iter() {
                if r.callee_id == Some(callee_id) && r.relation_type == relation_type {
                    result.push(r.clone());
                }
            }
        }
        result
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

    fn call_count(&self) -> usize {
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
