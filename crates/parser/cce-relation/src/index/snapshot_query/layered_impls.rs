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

use std::collections::{HashMap, HashSet};

use cce_types::{
    Entity, EntityId, ExternalCallType, FileInfo, ImportTable, RelationType, ResolvedRelation,
};

use super::{
    SnapshotEntityQueryOps, SnapshotFileQueryOps, SnapshotFrontendQueryOps,
    SnapshotHierarchyQueryOps, SnapshotRelationQueryOps, SnapshotSymbolQueryOps,
};
use crate::error::IndexError;
use crate::index::core::SymbolKey;
use crate::index::delta::relation_identity;
use crate::index::snapshot_index::LayeredSnapshotIndex;
use crate::types::ExportInfo;

// ---------------------------------------------------------------------------
// LayeredSnapshotIndex: delta-aware reads (removed hidden, added visible).
//
// Every method returns the same result as `base` merged with `delta`, without
// materializing the merged index. `compute_delta`'s dangling-reference
// cleanup guarantees the delta schedules every edge that points at a
// removed entity, so read paths can trust the delta without re-verification.
// ---------------------------------------------------------------------------

impl SnapshotEntityQueryOps for LayeredSnapshotIndex {
    fn get_function_by_entity_id(&self, entity_id: EntityId) -> Option<Entity> {
        // Walk deltas from last to first: the most recent delta wins.
        for d in self.deltas.iter().rev() {
            if d.removed_entities.contains(&entity_id) {
                return None;
            }
            if let Some(added) = d
                .added_entities
                .iter()
                .find(|added| added.entity.id == entity_id)
            {
                return Some(added.entity.clone());
            }
        }
        self.base.get_function_by_entity_id(entity_id)
    }

    fn get_function_ids_by_name(&self, name: &str) -> Vec<EntityId> {
        let mut ids = self.base.get_function_ids_by_name(name);
        for d in &self.deltas {
            ids.retain(|id| !d.removed_entities.contains(id));
            for added in &d.added_entities {
                if added.entity.name == name && !ids.contains(&added.entity.id) {
                    ids.push(added.entity.id);
                }
            }
        }
        ids
    }

    fn contains_function(&self, entity_id: EntityId) -> bool {
        self.contains_entity(entity_id)
    }

    fn function_count(&self) -> usize {
        LayeredSnapshotIndex::function_count(self)
    }

    fn get_file_path_by_entity(&self, entity_id: EntityId) -> Option<String> {
        // Walk deltas from last to first: the most recent delta wins.
        for d in self.deltas.iter().rev() {
            if d.removed_entities.contains(&entity_id) {
                return None;
            }
            if let Some(added) = d
                .added_entities
                .iter()
                .find(|added| added.entity.id == entity_id)
            {
                return Some(added.file_path.clone());
            }
        }
        self.base.get_file_path_by_entity(entity_id)
    }

    fn get_entities_in_line_range(
        &self,
        file_path: &str,
        start_line: usize,
        end_line: usize,
    ) -> Vec<EntityId> {
        let mut ids = self
            .base
            .get_entities_in_line_range(file_path, start_line, end_line);
        for d in &self.deltas {
            ids.retain(|id| !d.removed_entities.contains(id));
            for added in &d.added_entities {
                if added.file_path != file_path {
                    continue;
                }
                let span = &added.entity.span;
                if span.start_position.row <= end_line
                    && span.end_position.row >= start_line
                    && !ids.contains(&added.entity.id)
                {
                    ids.push(added.entity.id);
                }
            }
        }
        ids
    }
}

impl SnapshotRelationQueryOps for LayeredSnapshotIndex {
    fn get_resolved_relations_by_caller(
        &self,
        caller_id: EntityId,
    ) -> Option<Vec<ResolvedRelation>> {
        // Check if any delta removes this caller.
        if self
            .deltas
            .iter()
            .any(|d| d.removed_entities.contains(&caller_id))
        {
            return None;
        }

        let mut relations = self
            .base
            .get_resolved_relations_by_caller(caller_id)
            .unwrap_or_default();

        // Apply each delta in order: remove then add.
        for d in &self.deltas {
            let removed: HashSet<_> = d
                .removed_relations
                .iter()
                .filter(|r| r.caller == caller_id)
                .map(relation_identity)
                .collect();
            relations.retain(|r| !removed.contains(&relation_identity(r)));
            relations.extend(
                d.added_relations
                    .iter()
                    .filter(|r| r.caller == caller_id)
                    .cloned(),
            );
        }

        if relations.is_empty() {
            None
        } else {
            Some(relations)
        }
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
        // Check if any delta removes this callee.
        if self
            .deltas
            .iter()
            .any(|d| d.removed_entities.contains(&callee_id))
        {
            return Vec::new();
        }

        let mut callers = self.base.get_callers_by_callee_entity(callee_id);

        // Apply each delta in order: remove then add.
        for d in &self.deltas {
            let removed_entities: HashSet<EntityId> = d.removed_entities.iter().copied().collect();
            callers.retain(|caller| {
                !removed_entities.contains(caller)
                    && !d
                        .removed_relations
                        .iter()
                        .any(|r| r.caller == *caller && r.callee_id == Some(callee_id))
            });
            for relation in d
                .added_relations
                .iter()
                .filter(|r| r.callee_id == Some(callee_id))
            {
                if !callers.contains(&relation.caller) {
                    callers.push(relation.caller);
                }
            }
        }
        callers
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
                    .is_some_and(|relations| {
                        relations.iter().any(|r| {
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
            .map(|relations| {
                relations
                    .into_iter()
                    .filter(|r| r.relation_type == relation_type)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn resolved_relation_count(&self) -> usize {
        LayeredSnapshotIndex::resolved_relation_count(self)
    }

    fn call_count(&self) -> usize {
        self.resolved_relation_count()
    }

    fn get_relations_by_classification(
        &self,
        classification: &ExternalCallType,
    ) -> Vec<ResolvedRelation> {
        let base = self.base.get_relations_by_classification(classification);
        let mut delta_added: Vec<ResolvedRelation> = self
            .deltas
            .iter()
            .rev()
            .flat_map(|d| d.added_relations.iter())
            .filter(|r| r.external_type.as_ref() == Some(classification))
            .cloned()
            .collect();
        let mut seen: HashSet<_> = base.iter().map(|r| (r.caller, r.callee_id)).collect();
        let mut result = base;
        for rel in delta_added.drain(..) {
            if seen.insert((rel.caller, rel.callee_id)) {
                result.push(rel);
            }
        }
        result
    }

    fn get_classification_stats(&self) -> HashMap<ExternalCallType, usize> {
        let mut stats = self.base.get_classification_stats();
        for delta in &self.deltas {
            for rel in &delta.added_relations {
                if let Some(ref ext_type) = rel.external_type {
                    *stats.entry(ext_type.clone()).or_insert(0) += 1;
                }
            }
        }
        for delta in &self.deltas {
            for rel in &delta.removed_relations {
                if let Some(ref ext_type) = rel.external_type {
                    if let Some(count) = stats.get_mut(ext_type) {
                        *count = count.saturating_sub(1);
                    }
                }
            }
        }
        stats
    }
}

impl SnapshotHierarchyQueryOps for LayeredSnapshotIndex {
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
            .map(|relations| {
                relations
                    .into_iter()
                    .filter(|r| r.relation_type == RelationType::Inheritance)
                    .filter_map(|r| r.callee_id)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn get_implemented_interfaces(&self, class_id: EntityId) -> Vec<EntityId> {
        self.get_resolved_relations_by_caller(class_id)
            .map(|relations| {
                relations
                    .into_iter()
                    .filter(|r| r.relation_type == RelationType::Implementation)
                    .filter_map(|r| r.callee_id)
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl SnapshotFrontendQueryOps for LayeredSnapshotIndex {
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

impl SnapshotFileQueryOps for LayeredSnapshotIndex {
    fn get_file(&self, file_id: &str) -> Option<FileInfo> {
        // Walk deltas from last to first: the most recent delta wins.
        for d in self.deltas.iter().rev() {
            if d.removed_files.iter().any(|f| f == file_id) {
                return None;
            }
            if let Some(file) = d
                .added_files
                .iter()
                .find(|f| f.path == file_id || f.id == file_id)
            {
                return Some(file.clone());
            }
        }
        self.base.get_file(file_id)
    }

    fn contains_file(&self, file_id: &str) -> bool {
        for d in self.deltas.iter().rev() {
            if d.removed_files.iter().any(|f| f == file_id) {
                return false;
            }
            if d.added_files
                .iter()
                .any(|f| f.path == file_id || f.id == file_id)
            {
                return true;
            }
        }
        self.base.contains_file(file_id)
    }

    fn file_count(&self) -> usize {
        let mut count = self.base.file_count() as i64;
        for d in &self.deltas {
            let removed_existing = d
                .removed_files
                .iter()
                .filter(|f| self.base.contains_file(f))
                .count();
            count -= removed_existing as i64;
            count += d.added_files.len() as i64;
        }
        count.max(0) as usize
    }

    fn get_import_table(&self, file_id: &str) -> Option<ImportTable> {
        let mut table = self.base.get_import_table(file_id)?;
        // Apply import diffs from each delta in order.
        for d in &self.deltas {
            if let Some(diff) = d.import_diffs.iter().find(|diff| diff.file_path == file_id) {
                table
                    .standardized_imports
                    .retain(|i| !diff.removed_imports.contains(i));
                for import in &diff.added_imports {
                    if !table.standardized_imports.contains(import) {
                        table.standardized_imports.push(import.clone());
                    }
                }
            }
        }
        // Final state wins: a file removed by a later delta is gone even if a
        // prior delta touched it; a re-added file keeps the merged table.
        if !self.is_file_active(file_id) {
            return None;
        }
        Some(table)
    }

    fn has_imports(&self, file_id: &str) -> bool {
        if !self.is_file_active(file_id) {
            return false;
        }
        self.base.has_imports(file_id)
    }

    fn import_count(&self) -> usize {
        let mut count = self.base.import_count() as i64;
        for d in &self.deltas {
            let removed_tables = d
                .removed_files
                .iter()
                .filter(|f| self.base.has_imports(f))
                .count();
            count -= removed_tables as i64;
        }
        count.max(0) as usize
    }

    fn get_exports(&self, file_id: &str) -> Option<Vec<ExportInfo>> {
        let mut exports = self.base.get_exports(file_id)?;
        // Apply export diffs from each delta in order.
        for d in &self.deltas {
            if let Some(diff) = d.export_diffs.iter().find(|diff| diff.file_path == file_id) {
                let removed: HashSet<&str> = diff
                    .removed_exports
                    .iter()
                    .map(|e| e.symbol.scoped_name.as_str())
                    .collect();
                exports.retain(|e| !removed.contains(e.function_name.as_str()));

                let existing: HashSet<String> =
                    exports.iter().map(|e| e.function_name.clone()).collect();
                for export in &diff.added_exports {
                    if existing.contains(export.symbol.scoped_name.as_str()) {
                        continue;
                    }
                    // Skip unresolvable exports rather than falling back to EntityId(0),
                    // which could corrupt the call graph by aliasing a real entity.
                    let Some(entity_id) = self.get_entity_id_by_symbol_key(&export.symbol) else {
                        tracing::debug!(
                            scoped_name = %export.symbol.scoped_name,
                            "export symbol key unresolvable in layered snapshot, skipping"
                        );
                        continue;
                    };
                    let export_type = match export.export_type.as_str() {
                        "default" => crate::types::ExportType::Default,
                        "wildcard" => crate::types::ExportType::Wildcard,
                        _ => crate::types::ExportType::Named,
                    };
                    exports.push(ExportInfo {
                        function_id: entity_id,
                        function_name: export.symbol.scoped_name.clone(),
                        export_type,
                    });
                }
            }
        }
        // Final state wins: a file removed by a later delta is gone even if a
        // prior delta touched it; a re-added file keeps the merged exports.
        if !self.is_file_active(file_id) {
            return None;
        }
        Some(exports)
    }

    fn find_export_by_name(&self, file_id: &str, function_name: &str) -> Option<ExportInfo> {
        self.get_exports(file_id)?
            .iter()
            .find(|e| e.function_name == function_name)
            .cloned()
    }

    fn get_entity_ids_by_file(&self, file_id: &str) -> Vec<EntityId> {
        let mut ids = self.base.get_entity_ids_by_file(file_id);
        for d in &self.deltas {
            ids.retain(|id| !d.removed_entities.contains(id));
            for added in &d.added_entities {
                if added.file_path == file_id && !ids.contains(&added.entity.id) {
                    ids.push(added.entity.id);
                }
            }
        }
        ids
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

impl SnapshotSymbolQueryOps for LayeredSnapshotIndex {
    fn get_entity_id_by_symbol_key(&self, key: &SymbolKey) -> Option<EntityId> {
        // Walk deltas from last to first: the most recent delta wins.
        for d in self.deltas.iter().rev() {
            if let Some(added) = d.added_entities.iter().find(|a| a.symbol_key == *key) {
                return Some(added.entity.id);
            }
        }
        // Exact key lookup in the base snapshot.
        if let Some(base_id) = self.base.get_entity_id_by_symbol_key(key) {
            // Check if any delta removes this entity.
            if self
                .deltas
                .iter()
                .any(|d| d.removed_entities.contains(&base_id))
            {
                return None;
            }
            return Some(base_id);
        }
        // Fallback: name-based lookup in the base's function_index.
        // This handles cases where the symbol key's file_path differs from the
        // entity's registered file path (e.g. exports referencing an entity
        // registered under a different file).
        for entry in self.base.function_index.iter() {
            if entry.value().name == key.scoped_name {
                let entity_id = *entry.key();
                if !self
                    .deltas
                    .iter()
                    .any(|d| d.removed_entities.contains(&entity_id))
                {
                    return Some(entity_id);
                }
            }
        }
        None
    }

    fn get_entity_id_by_stable_symbol_id(&self, stable_id: &str) -> Option<EntityId> {
        // Walk deltas from last to first: the most recent delta wins.
        for d in self.deltas.iter().rev() {
            if let Some(added) = d
                .added_entities
                .iter()
                .find(|a| a.symbol_key.stable_id().0 == stable_id)
            {
                return Some(added.entity.id);
            }
        }
        let base_id = self.base.get_entity_id_by_stable_symbol_id(stable_id)?;
        // Check if any delta removes this entity.
        if self
            .deltas
            .iter()
            .any(|d| d.removed_entities.contains(&base_id))
        {
            return None;
        }
        Some(base_id)
    }

    fn get_symbol_key_by_entity_id(&self, entity_id: EntityId) -> Option<SymbolKey> {
        // Walk deltas from last to first: the most recent delta wins.
        for d in self.deltas.iter().rev() {
            if d.removed_entities.contains(&entity_id) {
                return None;
            }
            if let Some(added) = d.added_entities.iter().find(|a| a.entity.id == entity_id) {
                return Some(added.symbol_key.clone());
            }
        }
        self.base.get_symbol_key_by_entity_id(entity_id)
    }

    fn stable_symbol_keys(&self) -> Vec<SymbolKey> {
        // Collect all removed entity IDs across the chain.
        let all_removed: HashSet<EntityId> = self
            .deltas
            .iter()
            .flat_map(|d| d.removed_entities.iter().copied())
            .collect();

        let removed_keys: Vec<SymbolKey> = all_removed
            .iter()
            .filter_map(|id| self.base.get_symbol_key_by_entity_id(*id))
            .collect();
        let mut keys = self.base.stable_symbol_keys();
        keys.retain(|k| !removed_keys.contains(k));
        // Add keys from all deltas.
        for d in &self.deltas {
            keys.extend(d.added_entities.iter().map(|a| a.symbol_key.clone()));
        }
        keys
    }
}

impl super::TransitiveFileQueryOps for LayeredSnapshotIndex {
    fn transitive_dependents_of(&self, file_path: &str) -> Vec<String> {
        let mut result: std::collections::HashSet<String> = self
            .base
            .transitive_dependents_of(file_path)
            .into_iter()
            .collect();
        let mut dependents_direct: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        for d in &self.deltas {
            for diff in &d.dependency_diffs {
                for added in &diff.added_dependencies {
                    dependents_direct
                        .entry(added.clone())
                        .or_default()
                        .push(diff.source_file.clone());
                }
            }
        }
        let mut queue: Vec<String> = result.iter().cloned().collect();
        let mut visited = result.clone();
        visited.insert(file_path.to_string());
        let mut depth = 0;
        while !queue.is_empty() && depth < 10 {
            let mut next_queue = Vec::new();
            for cur in &queue {
                if let Some(direct) = dependents_direct.get(cur) {
                    for dep in direct {
                        if visited.insert(dep.clone()) {
                            result.insert(dep.clone());
                            next_queue.push(dep.clone());
                        }
                    }
                }
                for dep in self.base.dependency_graph.get_dependents(cur) {
                    if visited.insert(dep.clone()) {
                        result.insert(dep.clone());
                        next_queue.push(dep);
                    }
                }
            }
            queue = next_queue;
            depth += 1;
        }
        let mut v: Vec<String> = result.into_iter().collect();
        v.sort();
        v
    }

    fn transitive_dependencies_of(&self, file_path: &str) -> Vec<String> {
        let mut result: std::collections::HashSet<String> = self
            .base
            .transitive_dependencies_of(file_path)
            .into_iter()
            .collect();
        let mut dependencies_direct: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        for d in &self.deltas {
            for diff in &d.dependency_diffs {
                dependencies_direct
                    .entry(diff.source_file.clone())
                    .or_default()
                    .extend(diff.added_dependencies.clone());
            }
        }
        let mut queue: Vec<String> = result.iter().cloned().collect();
        let mut visited = result.clone();
        visited.insert(file_path.to_string());
        let mut depth = 0;
        while !queue.is_empty() && depth < 10 {
            let mut next_queue = Vec::new();
            for cur in &queue {
                if let Some(direct) = dependencies_direct.get(cur) {
                    for dep in direct {
                        if visited.insert(dep.clone()) {
                            result.insert(dep.clone());
                            next_queue.push(dep.clone());
                        }
                    }
                }
                for dep in self.base.dependency_graph.get_dependencies(cur) {
                    if visited.insert(dep.clone()) {
                        result.insert(dep.clone());
                        next_queue.push(dep);
                    }
                }
            }
            queue = next_queue;
            depth += 1;
        }
        let mut v: Vec<String> = result.into_iter().collect();
        v.sort();
        v
    }
}
