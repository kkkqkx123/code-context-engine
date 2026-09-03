use std::collections::{HashMap, HashSet};

use crate::index::core::SymbolKey;
use crate::index::view::{RelationIndexView, fingerprint_in_files_from_maps};
use crate::types::ExportInfo;
use cce_types::{Entity, EntityId, FileInfo, ImportTable, ResolvedRelation};

impl RelationIndexView for crate::index::snapshot_index::RelationSnapshotIndex {
    fn file_contains(&self, path: &str) -> bool {
        self.file_records.read().contains_key(path)
    }

    fn for_each_file<F: FnMut(&str, &FileInfo)>(&self, mut f: F) {
        let guard = self.file_records.read();
        for (path, record) in guard.iter() {
            f(path, &record.info);
        }
    }

    fn function_contains(&self, id: EntityId) -> bool {
        self.function_index.contains_key(&id)
    }

    fn for_each_function<F: FnMut(EntityId, &Entity)>(&self, mut f: F) {
        for entry in self.function_index.iter() {
            f(*entry.key(), entry.value());
        }
    }

    fn entity_file_of(&self, id: EntityId) -> Option<String> {
        self.entity_file_index.get(&id).map(|v| v.clone())
    }

    fn relations_of(&self, caller: EntityId) -> Option<Vec<ResolvedRelation>> {
        self.resolved_relation_index
            .get(&caller)
            .map(|v| v.edges.clone())
    }

    fn for_each_resolved_relation<F: FnMut(EntityId, &[ResolvedRelation])>(&self, mut f: F) {
        for entry in self.resolved_relation_index.iter() {
            f(*entry.key(), &entry.value().edges);
        }
    }

    fn file_relations_of(&self, path: &str) -> Vec<ResolvedRelation> {
        self.file_relation_index
            .get(path)
            .map(|v| v.edges.clone())
            .unwrap_or_default()
    }

    fn for_each_file_relation<F: FnMut(&str, &[ResolvedRelation])>(&self, mut f: F) {
        for entry in self.file_relation_index.iter() {
            f(entry.key(), &entry.value().edges);
        }
    }

    fn callers_of(&self, callee: EntityId) -> Vec<EntityId> {
        if let Some(callers) = self.reverse_callee_index.get(&callee) {
            return callers.clone();
        }
        if let Some(callers) = self.query_optimized.get_callers(callee) {
            return callers.clone();
        }
        if let Some(entry) = self.resolved_relation_index.get(&callee) {
            let callers = entry.callers();
            if !callers.is_empty() {
                return callers.to_vec();
            }
        }
        let mut result: Vec<EntityId> = self
            .resolved_relation_index
            .iter()
            .filter(|entry| entry.value().iter().any(|r| r.callee_id == Some(callee)))
            .map(|entry| *entry.key())
            .collect();
        result.sort();
        result.dedup();
        result
    }

    fn imports_of(&self, path: &str) -> Option<ImportTable> {
        self.file_records
            .read()
            .get(path)
            .map(|r| r.imports.clone())
    }

    fn for_each_import<F: FnMut(&str, &ImportTable)>(&self, mut f: F) {
        let guard = self.file_records.read();
        for (path, record) in guard.iter() {
            f(path, &record.imports);
        }
    }

    fn exports_of(&self, path: &str) -> Option<Vec<ExportInfo>> {
        self.file_records
            .read()
            .get(path)
            .map(|r| r.exports.iter().cloned().collect())
    }

    fn for_each_export<F: FnMut(&str, &[ExportInfo])>(&self, mut f: F) {
        let guard = self.file_records.read();
        for (path, record) in guard.iter() {
            f(path, &record.exports);
        }
    }

    fn symbol_key_of(&self, id: EntityId) -> Option<SymbolKey> {
        self.entity_to_symbol_key.read().get(&id).cloned()
    }

    fn dependency_files(&self) -> Vec<String> {
        self.dependency_graph.get_all_files()
    }

    fn dependencies_of(&self, source: &str) -> Vec<String> {
        self.dependency_graph.get_dependencies(source)
    }

    fn dependents_of(&self, file: &str) -> Vec<String> {
        self.dependency_graph.get_dependents(file)
    }

    fn collect_transitive_dependents(&self, file: &str, max_depth: usize) -> Vec<String> {
        self.dependency_graph
            .collect_transitive_dependents(file, max_depth)
    }

    fn collect_transitive_dependencies(&self, file: &str, max_depth: usize) -> Vec<String> {
        self.dependency_graph
            .collect_transitive_dependencies(file, max_depth)
    }

    fn entities_by_file(&self) -> HashMap<String, Vec<Entity>> {
        let mut grouped: HashMap<String, Vec<Entity>> = HashMap::new();
        for entry in self.entity_file_index.iter() {
            let entity_id = *entry.key();
            let file_path = entry.value();
            if !self.function_index.contains_key(&entity_id) {
                continue;
            }
            if let Some(entity) = self.function_index.get(&entity_id) {
                grouped
                    .entry(file_path.clone())
                    .or_default()
                    .push(entity.value().clone());
            }
        }
        grouped
    }

    fn entities_of_file(&self, path: &str) -> Vec<Entity> {
        let feb_guard = self.file_entities_by_start.read();
        let Some(rows) = feb_guard.get(path) else {
            return Vec::new();
        };
        rows.iter()
            .filter_map(|(_, id)| self.function_index.get(id).map(|e| e.value().clone()))
            .collect()
    }

    fn file_callers_of(&self, callee: EntityId) -> Vec<String> {
        self.file_callers_by_callee
            .get(&callee)
            .map(|v| v.iter().cloned().collect())
            .unwrap_or_default()
    }

    fn stable_symbol_keys(&self) -> Vec<SymbolKey> {
        self.symbol_key_to_entity.read().keys().cloned().collect()
    }

    fn stable_symbol_keys_in_files(&self, files: &HashSet<String>) -> Vec<SymbolKey> {
        let mut keys = Vec::new();
        let guard = self.file_symbol_keys.read();
        for file in files {
            if let Some(vec) = guard.get(file.as_str()) {
                keys.extend(vec.iter().cloned());
            }
        }
        keys
    }

    fn max_entity_id(&self) -> u64 {
        self.function_index
            .iter()
            .map(|entry| entry.key().0)
            .max()
            .unwrap_or(0)
    }

    fn fingerprint_in_files(&self, files: &HashSet<String>) -> String {
        fingerprint_in_files_from_maps(
            &self.function_index,
            &self.entity_file_index,
            &self.entity_to_symbol_key,
            &self.resolved_relation_index,
            &self.file_relation_index,
            &self.file_records,
            &self.dependency_graph,
            files,
        )
    }
}
