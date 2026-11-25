//! Bounded build-time view over relation entities.
//!
//! Full builds no longer retain a complete mutable `RelationIndex`. Registration
//! and resolution operate through these contracts so the same logic can write
//! into a disk-backed build store while incremental builds keep their
//! in-memory index.

use std::collections::HashMap;

use cce_types::{Entity, EntityId, FileInfo, ImportTable, ParsedFile, ResolvedRelation};

use crate::types::ExportInfo;

/// Read surface required by relation resolution.
///
/// Every query returns owned values so implementations may read from a
/// disk-backed store without leaking map guards into the resolver.
pub trait BuildEntityView: Send + Sync {
    fn entity_id_remap_for(&self, path: &str) -> Option<HashMap<EntityId, EntityId>>;
    fn get_function_ids_by_name(&self, name: &str) -> Vec<EntityId>;
    fn get_function_by_entity_id(&self, id: EntityId) -> Option<Entity>;
    fn get_file_path_by_entity(&self, id: EntityId) -> Option<String>;
}

/// Write surface used by registration, resolution and plugin injection during
/// a full build.
pub trait BuildIndexSink: BuildEntityView {
    fn add_file(&self, file: FileInfo);
    fn allocate_entity_id(&self) -> EntityId;
    fn add_entities_with_paths(&self, entities: Vec<(EntityId, Entity, String)>);
    fn function_contains(&self, id: EntityId) -> bool;
    fn register_symbol_key(
        &self,
        file_path: &str,
        scoped_name: &str,
        entity: &Entity,
        entity_id: EntityId,
    ) -> bool;
    fn set_entity_remap(&self, path: String, remap: HashMap<EntityId, EntityId>);
    fn add_import_table(&self, path: String, table: ImportTable);
    fn add_exports(&self, path: String, exports: Vec<ExportInfo>);
    fn add_dependency(&self, source: &str, target: &str);
    fn add_resolved_relation(&self, relation: ResolvedRelation);
    fn add_file_relation(&self, path: &str, relation: ResolvedRelation);
    fn get_resolved_relations_by_file(&self, path: &str) -> Vec<(EntityId, Vec<ResolvedRelation>)>;
    fn relations_of_caller(&self, caller: EntityId) -> Vec<ResolvedRelation>;
    fn for_each_import<F: FnMut(&str, &ImportTable)>(&self, visit: F);
    fn entities_of_file(&self, path: &str) -> Vec<Entity>;
    fn caller_entity_ids(&self, file: &ParsedFile) -> Vec<EntityId>;
}

impl BuildEntityView for super::core::RelationIndex {
    fn entity_id_remap_for(&self, path: &str) -> Option<HashMap<EntityId, EntityId>> {
        super::core::RelationIndex::entity_id_remap_for(self, path)
    }

    fn get_function_ids_by_name(&self, name: &str) -> Vec<EntityId> {
        use super::EntityIndexOps;
        EntityIndexOps::get_function_ids_by_name(self, name)
    }

    fn get_function_by_entity_id(&self, id: EntityId) -> Option<Entity> {
        use super::EntityIndexOps;
        EntityIndexOps::get_function_by_entity_id(self, id).map(|entry| entry.clone())
    }

    fn get_file_path_by_entity(&self, id: EntityId) -> Option<String> {
        use super::EntityIndexOps;
        EntityIndexOps::get_file_path_by_entity(self, id)
    }
}

impl BuildIndexSink for super::core::RelationIndex {
    fn add_file(&self, file: FileInfo) {
        use super::FileIndexOps;
        FileIndexOps::add_file(self, file);
    }

    fn allocate_entity_id(&self) -> EntityId {
        super::core::RelationIndex::allocate_entity_id(self)
    }

    fn add_entities_with_paths(&self, entities: Vec<(EntityId, Entity, String)>) {
        use super::EntityIndexOps;
        EntityIndexOps::add_functions_with_paths(self, entities);
    }

    fn function_contains(&self, id: EntityId) -> bool {
        use super::EntityIndexOps;
        EntityIndexOps::contains_function(self, id)
    }

    fn register_symbol_key(
        &self,
        file_path: &str,
        scoped_name: &str,
        entity: &Entity,
        entity_id: EntityId,
    ) -> bool {
        super::core::RelationIndex::register_symbol_key(
            self,
            file_path,
            scoped_name,
            entity,
            entity_id,
        )
    }

    fn set_entity_remap(&self, path: String, remap: HashMap<EntityId, EntityId>) {
        self.entity_id_remaps.write().insert(path, remap);
    }

    fn add_import_table(&self, path: String, table: ImportTable) {
        use super::ImportIndexOps;
        ImportIndexOps::add_import_table(self, path, table);
    }

    fn add_exports(&self, path: String, exports: Vec<ExportInfo>) {
        use super::ExportIndexOps;
        ExportIndexOps::add_exports(self, path, exports);
    }

    fn add_dependency(&self, source: &str, target: &str) {
        self.dependency_graph.add_dependency(source, target);
    }

    fn add_resolved_relation(&self, relation: ResolvedRelation) {
        super::core::RelationIndex::add_resolved_relation(self, relation);
    }

    fn add_file_relation(&self, path: &str, relation: ResolvedRelation) {
        super::core::RelationIndex::add_file_relation(self, path, relation);
    }

    fn get_resolved_relations_by_file(&self, path: &str) -> Vec<(EntityId, Vec<ResolvedRelation>)> {
        use super::FileLevelOps;
        FileLevelOps::get_resolved_relations_by_file(self, path)
    }

    fn for_each_import<F: FnMut(&str, &ImportTable)>(&self, mut visit: F) {
        let guard = self.file_records.read();
        for (path, record) in guard.iter() {
            visit(path, &record.imports);
        }
    }

    fn entities_of_file(&self, path: &str) -> Vec<Entity> {
        use super::RelationIndexView;
        RelationIndexView::entities_of_file(self, path)
    }

    fn relations_of_caller(&self, caller: EntityId) -> Vec<ResolvedRelation> {
        self.resolved_relation_index
            .get(&caller)
            .map(|entry| entry.value().edges.clone())
            .unwrap_or_default()
    }

    fn caller_entity_ids(&self, file: &ParsedFile) -> Vec<EntityId> {
        let path = cce_types::normalize_project_path(&file.path);
        let Some(remap) = self.entity_id_remaps.read().get(&path).cloned() else {
            return Vec::new();
        };
        let mut ids: Vec<EntityId> = remap.into_values().collect();
        ids.sort_by_key(|id| id.0);
        ids.dedup();
        ids
    }
}
