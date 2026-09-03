//! Query ops for `UnifiedSnapshotIndex`.
//!
//! The unified snapshot stores the merged compact view; all reads are O(1)
//! over `HashMap` without per-query delta walks.

use std::collections::HashMap;

use cce_types::{
    Entity, EntityId, ExternalCallType, FileInfo, ImportTable, RelationType, ResolvedRelation,
};

use super::{
    SnapshotEntityQueryOps, SnapshotFileQueryOps, SnapshotFrontendQueryOps,
    SnapshotHierarchyQueryOps, SnapshotRelationQueryOps, SnapshotSymbolQueryOps,
};
use crate::error::IndexError;
use crate::index::core::SymbolKey;
use crate::index::unified_snapshot::UnifiedSnapshotIndex;
use crate::types::ExportInfo;

impl SnapshotEntityQueryOps for UnifiedSnapshotIndex {
    fn get_function_by_entity_id(&self, entity_id: EntityId) -> Option<Entity> {
        self.merged_compact()
            .function_index
            .get(&entity_id)
            .cloned()
    }

    fn get_function_ids_by_name(&self, name: &str) -> Vec<EntityId> {
        self.merged_compact()
            .function_index
            .iter()
            .filter(|(_, e)| e.name == name)
            .map(|(id, _)| *id)
            .collect()
    }

    fn contains_function(&self, entity_id: EntityId) -> bool {
        self.merged_compact()
            .function_index
            .contains_key(&entity_id)
    }

    fn function_count(&self) -> usize {
        UnifiedSnapshotIndex::function_count(self)
    }

    fn get_file_path_by_entity(&self, entity_id: EntityId) -> Option<String> {
        self.merged_compact()
            .entity_file_index
            .get(&entity_id)
            .cloned()
    }

    fn get_entities_in_line_range(
        &self,
        file_path: &str,
        start_line: usize,
        end_line: usize,
    ) -> Vec<EntityId> {
        // Use per-file ordered index if present.
        if let Some(rows) = self.merged_compact().file_entities_by_start.get(file_path) {
            let upper = rows.partition_point(|(row, _)| *row as usize <= end_line);
            let mut result = Vec::new();
            for (_, entity_id) in &rows[..upper] {
                if let Some(entity) = self.merged_compact().function_index.get(entity_id) {
                    if entity.span.start_position.row <= end_line
                        && entity.span.end_position.row >= start_line
                    {
                        result.push(*entity_id);
                    }
                }
            }
            return result;
        }
        // Fallback: scan entity_file_index.
        self.merged_compact()
            .entity_file_index
            .iter()
            .filter(|(_, f)| f.as_str() == file_path)
            .map(|(id, _)| *id)
            .filter(|id| {
                self.merged_compact()
                    .function_index
                    .get(id)
                    .is_some_and(|e| {
                        e.span.start_position.row <= end_line
                            && e.span.end_position.row >= start_line
                    })
            })
            .collect()
    }
}

impl SnapshotRelationQueryOps for UnifiedSnapshotIndex {
    fn get_resolved_relations_by_caller(
        &self,
        caller_id: EntityId,
    ) -> Option<Vec<ResolvedRelation>> {
        if let Some(callees) = self.query_optimized.get_callees(caller_id) {
            return Some(callees.clone());
        }
        self.merged_compact()
            .resolved_relation_index
            .get(&caller_id)
            .map(|s| s.edges.clone())
    }

    fn get_resolved_relations_by_caller_checked(
        &self,
        caller_id: EntityId,
    ) -> Result<Vec<ResolvedRelation>, IndexError> {
        if !self.contains_function(caller_id) {
            return Err(IndexError::entity_not_found(caller_id));
        }
        self.get_resolved_relations_by_caller(caller_id)
            .ok_or_else(|| {
                IndexError::inconsistent_state(format!(
                    "Entity {:?} exists but has no relation entry",
                    caller_id
                ))
            })
    }

    fn get_callers_by_callee_entity(&self, callee_id: EntityId) -> Vec<EntityId> {
        if let Some(callers) = self.query_optimized.get_callers(callee_id) {
            return callers.clone();
        }
        // Fallback scan via merged compact.
        let mut result: Vec<EntityId> = self
            .merged_compact()
            .resolved_relation_index
            .iter()
            .filter(|(_, set)| set.iter().any(|r| r.callee_id == Some(callee_id)))
            .map(|(k, _)| *k)
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
        self.get_callers_by_callee_entity(callee_id)
            .into_iter()
            .filter(|caller| {
                self.get_resolved_relations_by_caller(*caller)
                    .is_some_and(|rels| {
                        rels.iter().any(|r| {
                            r.callee_id == Some(callee_id) && r.relation_type == relation_type
                        })
                    })
            })
            .collect()
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
        self.get_resolved_relations_by_caller(caller_id)
            .map(|rels| {
                rels.into_iter()
                    .filter(|r| r.relation_type == relation_type)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn resolved_relation_count(&self) -> usize {
        UnifiedSnapshotIndex::resolved_relation_count(self)
    }

    fn call_count(&self) -> usize {
        self.resolved_relation_count()
    }

    fn get_relations_by_classification(
        &self,
        classification: &ExternalCallType,
    ) -> Vec<ResolvedRelation> {
        self.merged_compact()
            .resolved_relation_index
            .values()
            .flat_map(|set| set.iter())
            .filter(|r| r.external_type.as_ref() == Some(classification))
            .cloned()
            .collect()
    }

    fn get_classification_stats(&self) -> HashMap<ExternalCallType, usize> {
        let mut stats = HashMap::new();
        for set in self.merged_compact().resolved_relation_index.values() {
            for r in set.iter() {
                if let Some(ref ext) = r.external_type {
                    *stats.entry(ext.clone()).or_insert(0) += 1;
                }
            }
        }
        stats
    }
}

impl SnapshotHierarchyQueryOps for UnifiedSnapshotIndex {
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
        self.get_resolved_relations_by_caller(class_id)
            .map(|rels| {
                rels.into_iter()
                    .filter(|r| r.relation_type == RelationType::Inheritance)
                    .filter_map(|r| r.callee_id)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn get_implemented_interfaces(&self, class_id: EntityId) -> Vec<EntityId> {
        self.get_resolved_relations_by_caller(class_id)
            .map(|rels| {
                rels.into_iter()
                    .filter(|r| r.relation_type == RelationType::Implementation)
                    .filter_map(|r| r.callee_id)
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl SnapshotFrontendQueryOps for UnifiedSnapshotIndex {
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

impl SnapshotFileQueryOps for UnifiedSnapshotIndex {
    fn get_file(&self, file_id: &str) -> Option<FileInfo> {
        self.merged_compact()
            .file_records
            .get(file_id)
            .map(|r| r.info.clone())
    }

    fn contains_file(&self, file_id: &str) -> bool {
        self.merged_compact().file_records.contains_key(file_id)
    }

    fn file_count(&self) -> usize {
        self.merged_compact().file_records.len()
    }

    fn get_import_table(&self, file_id: &str) -> Option<ImportTable> {
        self.merged_compact()
            .file_records
            .get(file_id)
            .map(|r| r.imports.clone())
    }

    fn has_imports(&self, file_id: &str) -> bool {
        self.merged_compact().file_records.contains_key(file_id)
    }

    fn import_count(&self) -> usize {
        self.merged_compact().file_records.len()
    }

    fn get_exports(&self, file_id: &str) -> Option<Vec<ExportInfo>> {
        self.merged_compact()
            .file_records
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
        self.merged_compact()
            .entity_file_index
            .iter()
            .filter(|(_, f)| f.as_str() == file_id)
            .map(|(id, _)| *id)
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

impl SnapshotSymbolQueryOps for UnifiedSnapshotIndex {
    fn get_entity_id_by_symbol_key(&self, key: &SymbolKey) -> Option<EntityId> {
        self.merged_compact().symbol_key_to_entity.get(key).copied()
    }

    fn get_entity_id_by_stable_symbol_id(&self, stable_id: &str) -> Option<EntityId> {
        self.merged_compact()
            .stable_id_to_entity
            .get(stable_id)
            .copied()
    }

    fn get_symbol_key_by_entity_id(&self, entity_id: EntityId) -> Option<SymbolKey> {
        self.merged_compact()
            .entity_to_symbol_key
            .get(&entity_id)
            .cloned()
    }

    fn stable_symbol_keys(&self) -> Vec<SymbolKey> {
        self.merged_compact()
            .symbol_key_to_entity
            .keys()
            .cloned()
            .collect()
    }
}

impl super::TransitiveFileQueryOps for UnifiedSnapshotIndex {
    fn transitive_dependents_of(&self, file_path: &str) -> Vec<String> {
        self.transitive_deps.transitive_dependents_of(file_path)
    }

    fn transitive_dependencies_of(&self, file_path: &str) -> Vec<String> {
        self.transitive_deps.transitive_dependencies_of(file_path)
    }
}
