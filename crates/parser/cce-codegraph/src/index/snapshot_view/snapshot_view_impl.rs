use std::collections::{HashMap, HashSet};

use crate::index::core::SymbolKey;
use crate::index::snapshot_index::RelationSnapshotIndex;
use crate::index::view::RelationIndexView;
use crate::types::ExportInfo;
use cce_types::{Entity, EntityId, FileInfo, ImportTable, ResolvedRelation};

impl RelationIndexView for RelationSnapshotIndex {
    fn file_contains(&self, path: &str) -> bool {
        RelationSnapshotIndex::contains_file(self, path)
    }

    fn for_each_file<F: FnMut(&str, &FileInfo)>(&self, mut f: F) {
        self.for_each_file_record(|path, record| f(path, &record.info));
    }

    fn function_contains(&self, id: EntityId) -> bool {
        RelationSnapshotIndex::contains_function(self, id)
    }

    fn for_each_function<F: FnMut(EntityId, &Entity)>(&self, f: F) {
        self.for_each_entity(f);
    }

    fn entity_file_of(&self, id: EntityId) -> Option<String> {
        self.file_path_of(id)
    }

    fn relations_of(&self, caller: EntityId) -> Option<Vec<ResolvedRelation>> {
        RelationSnapshotIndex::relations_of(self, caller)
    }

    fn for_each_resolved_relation<F: FnMut(EntityId, &[ResolvedRelation])>(&self, f: F) {
        self.for_each_relation_group(f);
    }

    fn file_relations_of(&self, path: &str) -> Vec<ResolvedRelation> {
        RelationSnapshotIndex::file_relations_of(self, path)
    }

    fn for_each_file_relation<F: FnMut(&str, &[ResolvedRelation])>(&self, f: F) {
        self.for_each_file_relation(f);
    }

    fn callers_of(&self, callee: EntityId) -> Vec<EntityId> {
        RelationSnapshotIndex::callers_of(self, callee)
    }

    fn imports_of(&self, path: &str) -> Option<ImportTable> {
        RelationSnapshotIndex::get_import_table(self, path)
    }

    fn for_each_import<F: FnMut(&str, &ImportTable)>(&self, mut f: F) {
        self.for_each_file_record(|path, record| f(path, &record.imports));
    }

    fn exports_of(&self, path: &str) -> Option<Vec<ExportInfo>> {
        RelationSnapshotIndex::get_exports(self, path)
    }

    fn for_each_export<F: FnMut(&str, &[ExportInfo])>(&self, mut f: F) {
        self.for_each_file_record(|path, record| {
            let exports: Vec<ExportInfo> = record.exports.iter().cloned().collect();
            f(path, &exports);
        });
    }

    fn symbol_key_of(&self, id: EntityId) -> Option<SymbolKey> {
        RelationSnapshotIndex::symbol_key_of(self, id)
    }

    fn dependency_files(&self) -> Vec<String> {
        RelationSnapshotIndex::dependency_files(self)
    }

    fn dependencies_of(&self, source: &str) -> Vec<String> {
        RelationSnapshotIndex::dependencies_of(self, source)
    }

    fn dependents_of(&self, file: &str) -> Vec<String> {
        RelationSnapshotIndex::dependents_of(self, file)
    }

    fn collect_transitive_dependents(&self, file: &str, max_depth: usize) -> Vec<String> {
        RelationSnapshotIndex::collect_transitive_dependents(self, file, max_depth)
    }

    fn collect_transitive_dependencies(&self, file: &str, max_depth: usize) -> Vec<String> {
        RelationSnapshotIndex::collect_transitive_dependencies(self, file, max_depth)
    }

    fn entities_by_file(&self) -> HashMap<String, Vec<Entity>> {
        let mut grouped: HashMap<String, Vec<Entity>> = HashMap::new();
        self.for_each_entity(|id, entity| {
            if let Some(file) = self.file_path_of(id) {
                grouped.entry(file).or_default().push(entity.clone());
            }
        });
        grouped
    }

    fn entities_of_file(&self, path: &str) -> Vec<Entity> {
        self.entity_ids_of_file(path)
            .into_iter()
            .filter_map(|id| self.get_function(id))
            .collect()
    }

    fn file_callers_of(&self, callee: EntityId) -> Vec<String> {
        RelationSnapshotIndex::file_callers_of(self, callee)
    }

    fn stable_symbol_keys(&self) -> Vec<SymbolKey> {
        RelationSnapshotIndex::stable_symbol_keys(self)
    }

    fn stable_symbol_keys_in_files(&self, files: &HashSet<String>) -> Vec<SymbolKey> {
        RelationSnapshotIndex::stable_symbol_keys_in_files(self, files)
    }

    fn max_entity_id(&self) -> u64 {
        RelationSnapshotIndex::max_entity_id(self)
    }

    fn fingerprint_in_files(&self, files: &HashSet<String>) -> String {
        RelationSnapshotIndex::fingerprint_in_files(self, files)
    }
}
