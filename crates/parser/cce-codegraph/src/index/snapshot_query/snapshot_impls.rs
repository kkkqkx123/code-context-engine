//! Read-only query operations for the published snapshot facade.

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
use crate::index::relation_query::MAX_REVERSE_FANIN;
use crate::index::snapshot_index::RelationSnapshotIndex;
use crate::types::ExportInfo;

impl SnapshotEntityQueryOps for RelationSnapshotIndex {
    fn get_function_by_entity_id(&self, entity_id: EntityId) -> Option<Entity> {
        self.get_function(entity_id)
    }

    fn get_function_ids_by_name(&self, name: &str) -> Vec<EntityId> {
        self.function_ids_by_name(name)
    }

    fn contains_function(&self, entity_id: EntityId) -> bool {
        self.contains_function(entity_id)
    }

    fn function_count(&self) -> usize {
        RelationSnapshotIndex::function_count(self)
    }

    fn get_file_path_by_entity(&self, entity_id: EntityId) -> Option<String> {
        self.file_path_of(entity_id)
    }

    fn get_entities_in_line_range(
        &self,
        file_path: &str,
        start_line: usize,
        end_line: usize,
    ) -> Vec<EntityId> {
        self.entities_in_line_range(file_path, start_line, end_line)
    }
}

impl SnapshotRelationQueryOps for RelationSnapshotIndex {
    fn get_resolved_relations_by_caller(
        &self,
        caller_id: EntityId,
    ) -> Option<Vec<ResolvedRelation>> {
        self.relations_of(caller_id)
    }

    fn get_resolved_relations_by_caller_checked(
        &self,
        caller_id: EntityId,
    ) -> Result<Vec<ResolvedRelation>, IndexError> {
        self.relations_of_checked(caller_id)
    }

    fn get_callers_by_callee_entity(&self, callee_id: EntityId) -> Vec<EntityId> {
        let mut result = self.callers_of(callee_id);
        result.sort();
        result.dedup();
        result.truncate(MAX_REVERSE_FANIN);
        result
    }

    fn get_callers_by_callee_entity_checked(
        &self,
        callee_id: EntityId,
    ) -> Result<Vec<EntityId>, IndexError> {
        self.callers_of_checked(callee_id).map(|mut callers| {
            callers.sort();
            callers.dedup();
            callers.truncate(MAX_REVERSE_FANIN);
            callers
        })
    }

    fn get_callers_by_callee_and_type(
        &self,
        callee_id: EntityId,
        relation_type: RelationType,
    ) -> Vec<EntityId> {
        let mut seen = std::collections::HashSet::new();
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
        self.relations_to_entity(callee_id)
    }

    fn get_relations_to_entity_by_type(
        &self,
        callee_id: EntityId,
        relation_type: RelationType,
    ) -> Vec<ResolvedRelation> {
        self.relations_to_entity(callee_id)
            .into_iter()
            .filter(|relation| relation.relation_type == relation_type)
            .collect()
    }

    fn get_relations_from_entity_by_type(
        &self,
        caller_id: EntityId,
        relation_type: RelationType,
    ) -> Vec<ResolvedRelation> {
        self.relations_from_entity_by_type(caller_id, relation_type)
    }

    fn resolved_relation_count(&self) -> usize {
        RelationSnapshotIndex::resolved_relation_count(self)
    }

    fn get_relations_by_classification(
        &self,
        classification: &ExternalCallType,
    ) -> Vec<ResolvedRelation> {
        self.relations_by_classification(classification)
    }

    fn get_classification_stats(&self) -> HashMap<ExternalCallType, usize> {
        self.classification_stats()
    }
}

impl SnapshotHierarchyQueryOps for RelationSnapshotIndex {
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
        self.relations_from_entity_by_type(class_id, RelationType::Inheritance)
            .into_iter()
            .filter_map(|relation| relation.callee_id)
            .collect()
    }

    fn get_implemented_interfaces(&self, class_id: EntityId) -> Vec<EntityId> {
        self.relations_from_entity_by_type(class_id, RelationType::Implementation)
            .into_iter()
            .filter_map(|relation| relation.callee_id)
            .collect()
    }
}

impl SnapshotFrontendQueryOps for RelationSnapshotIndex {
    fn get_child_elements(&self, parent_id: EntityId) -> Vec<EntityId> {
        self.relations_from_entity_by_type(parent_id, RelationType::ElementContains)
            .into_iter()
            .filter_map(|relation| relation.callee_id)
            .collect()
    }

    fn get_parent_element(&self, child_id: EntityId) -> Vec<EntityId> {
        self.get_callers_by_callee_and_type(child_id, RelationType::ElementContains)
    }

    fn get_event_handlers(&self, element_id: EntityId) -> Vec<ResolvedRelation> {
        self.relations_from_entity_by_type(element_id, RelationType::EventCallback)
    }

    fn get_elements_by_handler(&self, handler_id: EntityId) -> Vec<EntityId> {
        self.get_callers_by_callee_and_type(handler_id, RelationType::EventCallback)
    }

    fn get_parameter_bindings(&self, component_id: EntityId) -> Vec<ResolvedRelation> {
        self.relations_from_entity_by_type(component_id, RelationType::ParameterBinding)
    }

    fn get_template_references(&self, element_id: EntityId) -> Vec<ResolvedRelation> {
        self.relations_from_entity_by_type(element_id, RelationType::TemplateReference)
    }

    fn get_elements_by_template_ref(&self, target_id: EntityId) -> Vec<EntityId> {
        self.get_callers_by_callee_and_type(target_id, RelationType::TemplateReference)
    }
}

impl SnapshotFileQueryOps for RelationSnapshotIndex {
    fn get_file(&self, file_id: &str) -> Option<FileInfo> {
        self.get_file_info(file_id)
    }

    fn contains_file(&self, file_id: &str) -> bool {
        RelationSnapshotIndex::contains_file(self, file_id)
    }

    fn file_count(&self) -> usize {
        RelationSnapshotIndex::file_count(self)
    }

    fn get_import_table(&self, file_id: &str) -> Option<ImportTable> {
        self.get_import_table(file_id)
    }

    fn has_imports(&self, file_id: &str) -> bool {
        RelationSnapshotIndex::contains_file(self, file_id)
    }

    fn import_count(&self) -> usize {
        RelationSnapshotIndex::file_count(self)
    }

    fn get_exports(&self, file_id: &str) -> Option<Vec<ExportInfo>> {
        self.get_exports(file_id)
    }

    fn find_export_by_name(&self, file_id: &str, function_name: &str) -> Option<ExportInfo> {
        self.get_exports(file_id)?
            .iter()
            .find(|export| export.function_name == function_name)
            .cloned()
    }

    fn get_entity_ids_by_file(&self, file_id: &str) -> Vec<EntityId> {
        self.entity_ids_of_file(file_id)
    }

    fn get_entities_by_file(&self, file_id: &str) -> Vec<(EntityId, Entity)> {
        self.entity_ids_of_file(file_id)
            .into_iter()
            .filter_map(|id| self.get_function(id).map(|entity| (id, entity)))
            .collect()
    }

    fn get_resolved_relations_by_file(
        &self,
        file_id: &str,
    ) -> Vec<(EntityId, Vec<ResolvedRelation>)> {
        self.entity_ids_of_file(file_id)
            .into_iter()
            .filter_map(|id| self.relations_of(id).map(|relations| (id, relations)))
            .collect()
    }
}

impl SnapshotSymbolQueryOps for RelationSnapshotIndex {
    fn get_entity_id_by_symbol_key(&self, key: &SymbolKey) -> Option<EntityId> {
        self.entity_id_by_symbol_key(key)
    }

    fn get_entity_id_by_stable_symbol_id(&self, stable_id: &str) -> Option<EntityId> {
        self.entity_id_by_stable_symbol_id(stable_id)
    }

    fn get_symbol_key_by_entity_id(&self, entity_id: EntityId) -> Option<SymbolKey> {
        self.symbol_key_of(entity_id)
    }

    fn stable_symbol_keys(&self) -> Vec<SymbolKey> {
        RelationSnapshotIndex::stable_symbol_keys(self)
    }
}

impl super::TransitiveFileQueryOps for RelationSnapshotIndex {
    fn transitive_dependents_of(&self, file_path: &str) -> Vec<String> {
        RelationSnapshotIndex::transitive_dependents_of(self, file_path)
    }

    fn transitive_dependencies_of(&self, file_path: &str) -> Vec<String> {
        RelationSnapshotIndex::transitive_dependencies_of(self, file_path)
    }
}
