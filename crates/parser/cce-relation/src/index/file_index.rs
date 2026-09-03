//! File index operations for RelationIndex
//!
//! This module provides file-related operations as extension traits.
//! It handles file metadata, imports, exports, and file-level operations.

use cce_types::{Entity, EntityId, FileInfo, ImportTable};

use super::super::types::ExportInfo;
use super::core::RelationIndex;

/// File index operations extension trait
///
/// Provides methods for managing file metadata in the relation index.
pub trait FileIndexOps {
    /// Add a file to the index
    fn add_file(&self, file: FileInfo);

    /// Get file info by ID
    fn get_file(&self, file_id: &str) -> Option<FileInfo>;

    /// Check if a file exists
    fn contains_file(&self, file_id: &str) -> bool;

    /// Get total number of files
    fn file_count(&self) -> usize;
}

impl FileIndexOps for RelationIndex {
    fn add_file(&self, file: FileInfo) {
        let file_id = file.id.clone();
        self.file_records
            .write()
            .entry(file_id.clone())
            .or_default()
            .info = file;
        self.record_affected_files(std::iter::once(file_id));
        self.bump_version();
    }

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
}

/// Import index operations extension trait
///
/// Provides methods for managing import tables in the relation index.
pub trait ImportIndexOps {
    /// Add an import table to the index
    fn add_import_table(&self, file_id: String, import_table: ImportTable);

    /// Get import table by file ID
    fn get_import_table(&self, file_id: &str) -> Option<ImportTable>;

    /// Check if a file has imports
    fn has_imports(&self, file_id: &str) -> bool;

    /// Get total number of import tables
    fn import_count(&self) -> usize;
}

impl ImportIndexOps for RelationIndex {
    fn add_import_table(&self, file_id: String, import_table: ImportTable) {
        self.file_records
            .write()
            .entry(file_id.clone())
            .or_default()
            .imports = import_table;
        self.record_affected_files(std::iter::once(file_id));
        self.bump_version();
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
}

/// Export index operations extension trait
///
/// Provides methods for managing exports in the relation index.
pub trait ExportIndexOps {
    /// Add exports to the index
    fn add_exports(&self, file_id: String, exports: Vec<ExportInfo>);

    /// Add a single export to the index
    fn add_export(&self, file_id: &str, export: ExportInfo);

    /// Get exports by file ID
    fn get_exports(&self, file_id: &str) -> Option<Vec<ExportInfo>>;

    /// Find export by function name in a file
    fn find_export_by_name(&self, file_id: &str, function_name: &str) -> Option<ExportInfo>;
}

impl ExportIndexOps for RelationIndex {
    fn add_exports(&self, file_id: String, exports: Vec<ExportInfo>) {
        self.file_records
            .write()
            .entry(file_id.clone())
            .or_default()
            .exports = exports.into();
        self.record_affected_files(std::iter::once(file_id));
        self.bump_version();
    }

    fn add_export(&self, file_id: &str, export: ExportInfo) {
        self.file_records
            .write()
            .entry(file_id.to_string())
            .or_default()
            .exports
            .push(export);
        self.record_affected_files(std::iter::once(file_id.to_string()));
        self.bump_version();
    }

    fn get_exports(&self, file_id: &str) -> Option<Vec<ExportInfo>> {
        self.file_records
            .read()
            .get(file_id)
            .map(|r| r.exports.iter().cloned().collect())
    }

    fn find_export_by_name(&self, file_id: &str, function_name: &str) -> Option<ExportInfo> {
        self.file_records
            .read()
            .get(file_id)?
            .exports
            .iter()
            .find(|e| e.function_name == function_name)
            .cloned()
    }
}

/// File-level operations extension trait
///
/// Provides methods for file-level operations like getting entities by file,
/// removing files, and updating files.
pub trait FileLevelOps {
    /// Get entity IDs belonging to a file
    fn get_entity_ids_by_file(&self, file_id: &str) -> Vec<EntityId>;

    /// Get all entities belonging to a file
    fn get_entities_by_file(&self, file_id: &str) -> Vec<(EntityId, Entity)>;

    /// Get all resolved relations belonging to a file
    fn get_resolved_relations_by_file(
        &self,
        file_id: &str,
    ) -> Vec<(EntityId, Vec<cce_types::ResolvedRelation>)>;

    /// Remove a file and all its associated data from the index
    fn remove_file(&self, file_id: &str);

    /// Update a file in the index (remove old data, add new data)
    fn update_file(&self, file_id: &str);
}

impl FileLevelOps for RelationIndex {
    fn get_entity_ids_by_file(&self, file_id: &str) -> Vec<EntityId> {
        self.entity_file_index
            .iter()
            .filter(|entry| entry.value().as_str() == file_id)
            .map(|entry| *entry.key())
            .collect()
    }

    fn get_entities_by_file(&self, file_id: &str) -> Vec<(EntityId, Entity)> {
        let entity_ids = self.get_entity_ids_by_file(file_id);
        entity_ids
            .into_iter()
            .filter_map(|id| {
                self.function_index
                    .get(&id)
                    .map(|entity| (id, entity.clone()))
            })
            .collect()
    }

    fn get_resolved_relations_by_file(
        &self,
        file_id: &str,
    ) -> Vec<(EntityId, Vec<cce_types::ResolvedRelation>)> {
        let entity_ids = self.get_entity_ids_by_file(file_id);
        entity_ids
            .into_iter()
            .filter_map(|id| {
                self.resolved_relation_index
                    .get(&id)
                    .map(|relations| (id, relations.edges.clone()))
            })
            .collect()
    }

    fn remove_file(&self, file_id: &str) {
        // Record the file for selective CoW refresh before removal.
        self.record_affected_files(std::iter::once(file_id.to_string()));

        // 1. Get all entity IDs belonging to this file
        let entities_to_remove = self.get_entity_ids_by_file(file_id);

        // 2. Remove from function_index (and name index)
        for entity_id in &entities_to_remove {
            self.remove_function(entity_id);
        }

        // 3. Remove from entity_file_index
        for entity_id in &entities_to_remove {
            self.entity_file_index.remove(entity_id);
        }

        // 3.5 Remove stable symbol mappings before another candidate is
        // resolved. Otherwise a deleted entity could be selected as an
        // internal target by a later incremental build.
        for entity_id in &entities_to_remove {
            if let Some(symbol) = self.entity_to_symbol_key.write().remove(entity_id) {
                self.symbol_key_to_entity.write().remove(&symbol);
                self.stable_id_to_entity
                    .write()
                    .remove(&symbol.stable_id().0);
            }
        }

        // 4. Remove entities from all indices using inverse mapping
        //    instead of scanning the entire index (replaces alter_all calls)
        for entity_id in &entities_to_remove {
            let entity_id = *entity_id;

            // 4a. Remove entity's own forward relations and clean up
            //     reverse index (entity_id appears as caller - remove it
            //     from each callee's callers list).
            //     Collect callee IDs first to avoid borrowing issues.
            let callee_ids: Vec<EntityId> = self
                .resolved_relation_index
                .get(&entity_id)
                .map(|entry| entry.iter().filter_map(|r| r.callee_id).collect())
                .unwrap_or_default();
            self.resolved_relation_index.remove(&entity_id);
            for callee_id in callee_ids {
                self.untrack_reverse_caller(callee_id, entity_id);
                if let Some(mut callee_entry) = self.resolved_relation_index.get_mut(&callee_id) {
                    callee_entry.remove_caller(&entity_id);
                    if callee_entry.is_empty() && callee_entry.is_callers_empty() {
                        drop(callee_entry);
                        self.resolved_relation_index.remove(&callee_id);
                    }
                }
            }
            // Drop reverse entry where this entity is the callee (incoming).
            self.reverse_callee_index.remove(&entity_id);

            // 4b. Remove from reverse index (entity_id as callee) - legacy
            //     embedded reverse path: this gives us all callers that
            //     referenced this entity via the embedded list.
            if let Some((_, callee_entry)) = self.resolved_relation_index.remove(&entity_id) {
                for caller in callee_entry.callers() {
                    self.untrack_reverse_caller(entity_id, *caller);
                    // Only update the specific caller's forward index
                    if let Some(mut relations) = self.resolved_relation_index.get_mut(caller) {
                        relations.retain(|r| r.callee_id != Some(entity_id));
                        if relations.is_empty() && relations.is_callers_empty() {
                            drop(relations);
                            self.resolved_relation_index.remove(caller);
                        }
                        // Forward edge removed, so reverse already untracked above;
                        // also ensure no stale reverse for this caller->callee.
                        self.maybe_untrack_reverse_caller(entity_id, *caller);
                    }
                }
            }
        }

        // 5. Remove from file_records and reconcile file-level relations.
        self.file_records.write().remove(file_id);
        self.take_file_relations(file_id);

        // 6. Remove from dependency graph
        self.dependency_graph.remove_file(file_id);

        // 7. Drop the per-file entity-ID remap so it cannot accumulate stale
        //    parsed-local -> global mappings.
        self.entity_id_remaps.write().remove(file_id);

        self.bump_version();
    }

    fn update_file(&self, file_id: &str) {
        self.remove_file(file_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ExportType;
    use crate::index::EntityIndexOps;
    use cce_types::entity::ParseStatus;

    fn create_test_file(id: &str, path: &str) -> FileInfo {
        FileInfo {
            id: id.to_string(),
            path: path.to_string(),
            language: "rust".to_string(),
            file_hash: String::new(),
            file_size: 0,
            modified_time: 0,
            parse_status: ParseStatus::Pending,
            parse_errors: Vec::new(),
            parse_version: 0,
            entity_count: 0,
            relation_count: 0,
            export_count: 0,
            import_count: 0,
            depends_on: Vec::new(),
        }
    }

    #[test]
    fn test_file_index_ops() {
        let index = RelationIndex::new();

        index.add_file(create_test_file("file_1", "src/main.rs"));

        assert_eq!(index.file_count(), 1);
        assert!(index.contains_file("file_1"));
        assert!(index.get_file("file_1").is_some());
    }

    #[test]
    fn test_import_index_ops() {
        let index = RelationIndex::new();

        let import_table = ImportTable {
            file_id: "file_1".to_string(),
            ..Default::default()
        };

        index.add_import_table("file_1".to_string(), import_table);

        assert_eq!(index.import_count(), 1);
        assert!(index.has_imports("file_1"));
    }

    #[test]
    fn test_export_index_ops() {
        let index = RelationIndex::new();

        let exports = vec![ExportInfo {
            function_id: EntityId(1),
            function_name: "foo".to_string(),
            export_type: ExportType::Named,
        }];

        index.add_exports("file_1".to_string(), exports);

        let retrieved = index.get_exports("file_1").expect("Should have exports");
        assert_eq!(retrieved.len(), 1);

        let found = index.find_export_by_name("file_1", "foo");
        assert!(found.is_some());
    }

    #[test]
    fn remove_file_removes_stable_symbol_mappings() {
        let index = RelationIndex::new();
        let entity_id = EntityId(7);
        let entity = Entity::new(
            entity_id,
            cce_types::EntityKind::Function,
            "deleted_function".to_string(),
            Default::default(),
        )
        .with_signature("fn deleted_function()".to_string());
        index.add_function_with_path(entity_id, entity.clone(), "src/deleted.rs".to_string());
        index.register_symbol_key("src/deleted.rs", "deleted_function", &entity, entity_id);

        let key = index
            .get_symbol_key_by_entity_id(entity_id)
            .expect("stable symbol should be registered");
        index.remove_file("src/deleted.rs");

        assert!(!index.contains_function(entity_id));
        assert!(index.get_symbol_key_by_entity_id(entity_id).is_none());
        assert!(index.get_entity_id_by_symbol_key(&key).is_none());
    }
}
