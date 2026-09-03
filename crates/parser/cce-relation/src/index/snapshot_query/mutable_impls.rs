//! Read-only query operations shared by every queryable relation index type.
//!
//! This is the query-side counterpart of the write-side extension traits
//! (`EntityIndexOps`, `RelationQueryOps`, ...). It contains only the pure
//! read methods of those traits and is implemented on all three index
//! surfaces:
//!
//! - [`RelationIndex`]: the mutable build-time index (delegates to the
//!   write-side traits' maps);
//! - [`RelationSnapshotIndex`]: the immutable published snapshot — every
//!   lookup is a direct `&self` read of the shared maps, zero-copy;
//! - [`LayeredSnapshotIndex`]: base + delta — each method merges the delta
//!   at read time (removed entries hidden, added entries visible) without
//!   materializing a merged index.
//!
//! Write methods (`add_*`, `remove_*`, ...) deliberately stay on the
//! write-side traits; the snapshot types never expose them. This is enforced
//! by the type system (snapshots hold only `Arc` maps) rather than by
//! convention.

use std::collections::HashMap;

use cce_types::{
    Entity, EntityId, ExternalCallType, FileInfo, ImportTable, RelationType, ResolvedRelation,
};

use super::{
    SnapshotEntityQueryOps, SnapshotFileQueryOps, SnapshotFrontendQueryOps,
    SnapshotHierarchyQueryOps, SnapshotRelationQueryOps, SnapshotSymbolQueryOps,
};
use crate::error::IndexError;
use crate::index::core::{RelationIndex, SymbolKey};
use crate::types::ExportInfo;

// ---------------------------------------------------------------------------
// RelationIndex: read-only view over the mutable index.
// ---------------------------------------------------------------------------

impl SnapshotEntityQueryOps for RelationIndex {
    fn get_function_by_entity_id(&self, entity_id: EntityId) -> Option<Entity> {
        self.function_index
            .get(&entity_id)
            .map(|entry| entry.value().clone())
    }

    fn get_function_ids_by_name(&self, name: &str) -> Vec<EntityId> {
        self.name_index
            .get(name)
            .map(|ids| ids.to_vec())
            .unwrap_or_default()
    }

    fn contains_function(&self, entity_id: EntityId) -> bool {
        self.function_index.contains_key(&entity_id)
    }

    fn function_count(&self) -> usize {
        self.function_index.len()
    }

    fn get_file_path_by_entity(&self, entity_id: EntityId) -> Option<String> {
        self.entity_file_index.get(&entity_id).map(|v| v.clone())
    }

    fn get_entities_in_line_range(
        &self,
        file_path: &str,
        start_line: usize,
        end_line: usize,
    ) -> Vec<EntityId> {
        self.entity_file_index
            .iter()
            .filter(|entry| entry.value().as_str() == file_path)
            .map(|entry| *entry.key())
            .filter(|entity_id| {
                self.function_index.get(entity_id).is_some_and(|entity| {
                    let start = entity.span.start_position.row;
                    let end = entity.span.end_position.row;
                    start <= end_line && end >= start_line
                })
            })
            .collect()
    }
}

impl SnapshotRelationQueryOps for RelationIndex {
    fn get_resolved_relations_by_caller(
        &self,
        caller_id: EntityId,
    ) -> Option<Vec<ResolvedRelation>> {
        self.resolved_relation_index
            .get(&caller_id)
            .map(|entry| entry.edges.clone())
    }

    fn get_resolved_relations_by_caller_checked(
        &self,
        caller_id: EntityId,
    ) -> Result<Vec<ResolvedRelation>, IndexError> {
        if !self.function_index.contains_key(&caller_id) {
            return Err(IndexError::entity_not_found(caller_id));
        }
        self.resolved_relation_index
            .get(&caller_id)
            .map(|entry| entry.edges.clone())
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
        if let Some(entry) = self.resolved_relation_index.get(&callee_id) {
            let callers = entry.callers();
            if !callers.is_empty() {
                return callers.to_vec();
            }
        }
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
                if self
                    .resolved_relation_index
                    .get(caller_id)
                    .is_some_and(|relations| {
                        relations.iter().any(|r| {
                            r.callee_id == Some(callee_id) && r.relation_type == relation_type
                        })
                    })
                {
                    callers.push(*caller_id);
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
        self.get_callers_by_callee_entity(callee_id)
            .into_iter()
            .flat_map(|caller| {
                self.get_resolved_relations_by_caller(caller)
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|r| r.callee_id == Some(callee_id))
            })
            .collect()
    }

    fn get_relations_to_entity_by_type(
        &self,
        callee_id: EntityId,
        relation_type: RelationType,
    ) -> Vec<ResolvedRelation> {
        self.get_callers_by_callee_entity(callee_id)
            .into_iter()
            .flat_map(|caller| {
                self.get_resolved_relations_by_caller(caller)
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|r| r.callee_id == Some(callee_id) && r.relation_type == relation_type)
            })
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
        self.resolved_relation_index
            .iter()
            .map(|entry| entry.len())
            .sum()
    }

    fn call_count(&self) -> usize {
        self.resolved_relation_count()
    }

    fn get_relations_by_classification(
        &self,
        classification: &ExternalCallType,
    ) -> Vec<ResolvedRelation> {
        let mut result = Vec::new();
        for entry in self.resolved_relation_index.iter() {
            for relation in entry.iter() {
                if relation.external_type.as_ref() == Some(classification) {
                    result.push(relation.clone());
                }
            }
        }
        result
    }

    fn get_classification_stats(&self) -> HashMap<ExternalCallType, usize> {
        let mut stats = HashMap::new();
        for entry in self.resolved_relation_index.iter() {
            for relation in entry.iter() {
                if let Some(ref ext_type) = relation.external_type {
                    *stats.entry(ext_type.clone()).or_insert(0) += 1;
                }
            }
        }
        stats
    }
}

impl SnapshotHierarchyQueryOps for RelationIndex {
    fn get_derived_classes(&self, class_id: EntityId) -> Vec<EntityId> {
        self.get_callers_by_callee_and_type(class_id, RelationType::Inheritance)
    }

    fn get_implementing_classes(&self, interface_id: EntityId) -> Vec<EntityId> {
        self.get_callers_by_callee_and_type(interface_id, RelationType::Implementation)
    }

    fn get_types_with_trait_bound(&self, trait_id: EntityId) -> Vec<EntityId> {
        self.get_callers_by_callee_and_type(trait_id, RelationType::TraitBound)
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

impl SnapshotFrontendQueryOps for RelationIndex {
    fn get_child_elements(&self, parent_id: EntityId) -> Vec<EntityId> {
        self.get_relations_from_entity_by_type(parent_id, RelationType::ElementContains)
            .into_iter()
            .filter_map(|r| r.callee_id)
            .collect()
    }

    fn get_parent_element(&self, child_id: EntityId) -> Vec<EntityId> {
        self.get_callers_by_callee_and_type(child_id, RelationType::ElementContains)
    }

    fn get_event_handlers(&self, element_id: EntityId) -> Vec<ResolvedRelation> {
        self.get_relations_from_entity_by_type(element_id, RelationType::EventCallback)
    }

    fn get_elements_by_handler(&self, handler_id: EntityId) -> Vec<EntityId> {
        self.get_callers_by_callee_and_type(handler_id, RelationType::EventCallback)
    }

    fn get_parameter_bindings(&self, component_id: EntityId) -> Vec<ResolvedRelation> {
        self.get_relations_from_entity_by_type(component_id, RelationType::ParameterBinding)
    }

    fn get_template_references(&self, element_id: EntityId) -> Vec<ResolvedRelation> {
        self.get_relations_from_entity_by_type(element_id, RelationType::TemplateReference)
    }

    fn get_elements_by_template_ref(&self, target_id: EntityId) -> Vec<EntityId> {
        self.get_callers_by_callee_and_type(target_id, RelationType::TemplateReference)
    }
}

impl SnapshotFileQueryOps for RelationIndex {
    fn get_file(&self, file_id: &str) -> Option<FileInfo> {
        self.file_records
            .read()
            .get(file_id)
            .map(|r| r.info.clone())
    }

    fn contains_file(&self, file_id: &str) -> bool {
        self.file_records.read().contains_key(file_id)
    }

    fn file_count(&self) -> usize {
        self.file_records.read().len()
    }

    fn get_import_table(&self, file_id: &str) -> Option<ImportTable> {
        self.file_records
            .read()
            .get(file_id)
            .map(|r| r.imports.clone())
    }

    fn has_imports(&self, file_id: &str) -> bool {
        self.file_records.read().contains_key(file_id)
    }

    fn import_count(&self) -> usize {
        self.file_records.read().len()
    }

    fn get_exports(&self, file_id: &str) -> Option<Vec<ExportInfo>> {
        self.file_records
            .read()
            .get(file_id)
            .map(|r| r.exports.iter().cloned().collect())
    }

    fn find_export_by_name(&self, file_id: &str, function_name: &str) -> Option<ExportInfo> {
        self.get_exports(file_id)?
            .iter()
            .find(|e| e.function_name == function_name)
            .cloned()
    }

    fn get_entity_ids_by_file(&self, file_id: &str) -> Vec<EntityId> {
        self.entity_file_index
            .iter()
            .filter(|entry| entry.value().as_str() == file_id)
            .map(|entry| *entry.key())
            .collect()
    }

    fn get_entities_by_file(&self, file_id: &str) -> Vec<(EntityId, Entity)> {
        self.get_entity_ids_by_file(file_id)
            .into_iter()
            .filter_map(|id| {
                self.get_function_by_entity_id(id)
                    .map(|entity| (id, entity))
            })
            .collect()
    }

    fn get_resolved_relations_by_file(
        &self,
        file_id: &str,
    ) -> Vec<(EntityId, Vec<ResolvedRelation>)> {
        self.get_entity_ids_by_file(file_id)
            .into_iter()
            .filter_map(|id| {
                self.get_resolved_relations_by_caller(id)
                    .map(|relations| (id, relations))
            })
            .collect()
    }
}

impl SnapshotSymbolQueryOps for RelationIndex {
    fn get_entity_id_by_symbol_key(&self, key: &SymbolKey) -> Option<EntityId> {
        self.symbol_key_to_entity.read().get(key).copied()
    }

    fn get_entity_id_by_stable_symbol_id(&self, stable_id: &str) -> Option<EntityId> {
        self.stable_id_to_entity.read().get(stable_id).copied()
    }

    fn get_symbol_key_by_entity_id(&self, entity_id: EntityId) -> Option<SymbolKey> {
        self.entity_to_symbol_key.read().get(&entity_id).cloned()
    }

    fn stable_symbol_keys(&self) -> Vec<SymbolKey> {
        self.symbol_key_to_entity.read().keys().cloned().collect()
    }
}

impl super::TransitiveFileQueryOps for RelationIndex {
    fn transitive_dependents_of(&self, file_path: &str) -> Vec<String> {
        self.dependency_graph
            .collect_transitive_dependents(file_path, 10)
    }

    fn transitive_dependencies_of(&self, file_path: &str) -> Vec<String> {
        self.dependency_graph
            .collect_transitive_dependencies(file_path, 10)
    }
}
