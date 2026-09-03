//! Entity index operations for RelationIndex
//!
//! This module provides entity-related operations as an extension trait.
//! It handles function entity storage, lookup, and basic entity management.

use cce_types::{Entity, EntityId};
use dashmap::DashMap;

use super::core::RelationIndex;

/// Entity index operations extension trait
///
/// Provides methods for managing function entities in the relation index.
pub trait EntityIndexOps {
    /// Add a function entity to the index
    ///
    /// The entity must be a function-like entity (Function, Method, Constructor, etc.)
    fn add_function(&self, entity_id: EntityId, entity: Entity);

    /// Add a function entity with file path to the index
    ///
    /// This is the preferred method when the file path is known.
    fn add_function_with_path(&self, entity_id: EntityId, entity: Entity, file_path: String);

    /// Add multiple function entities to the index
    fn add_functions(&self, functions: Vec<(EntityId, Entity)>);

    /// Add multiple function entities with file paths to the index
    fn add_functions_with_paths(&self, functions: Vec<(EntityId, Entity, String)>);

    /// Get function entity by EntityId
    ///
    /// Returns a reference to the entity to avoid cloning.
    fn get_function_by_entity_id(
        &self,
        entity_id: EntityId,
    ) -> Option<dashmap::mapref::one::Ref<'_, EntityId, Entity>>;

    /// Get function IDs by name
    ///
    /// O(1) lookup through the `name_index` inverted index (entity name ->
    /// EntityId list), maintained on every function insert/remove. Previously
    /// this computed the result on-demand by scanning the whole `function_index`
    /// per lookup
    fn get_function_ids_by_name(&self, name: &str) -> Vec<EntityId>;

    /// Check if a function exists
    fn contains_function(&self, entity_id: EntityId) -> bool;

    /// Get total number of functions
    fn function_count(&self) -> usize;

    /// Get reference to function index
    fn function_index(&self) -> &DashMap<EntityId, Entity>;

    /// Get file path for an entity
    fn get_file_path_by_entity(&self, entity_id: EntityId) -> Option<String>;

    /// Get entities within a line range for a specific file
    ///
    /// Returns entity IDs that overlap with the given line range [start_line, end_line].
    ///
    /// # Implementation Note
    /// This method computes results on-demand from the function_index to avoid
    /// data duplication and cache invalidation complexity. The canonical source
    /// of truth is `entity.span` in the function_index.
    ///
    /// Performance: O(n) where n = entities in the file.
    /// For most files (< 1000 entities), this is fast enough.
    /// If profiling shows this is a bottleneck, consider adding an optimized index later.
    fn get_entities_in_line_range(
        &self,
        file_path: &str,
        start_line: usize,
        end_line: usize,
    ) -> Vec<EntityId>;
}

impl EntityIndexOps for RelationIndex {
    fn add_function(&self, entity_id: EntityId, entity: Entity) {
        self.insert_function(entity_id, entity);
        self.bump_version();
    }

    fn add_function_with_path(&self, entity_id: EntityId, entity: Entity, file_path: String) {
        let row = entity.span.start_position.row as u32;
        // If this ID already exists with a different row, untrack the old.
        if let Some(existing) = self.function_index.get(&entity_id) {
            let old_row = existing.span.start_position.row as u32;
            if let Some(old_file) = self.entity_file_index.get(&entity_id).map(|v| v.clone()) {
                self.untrack_file_entity(&old_file, entity_id);
                // Keep old_row for debug? not needed.
                let _ = old_row;
            }
        }
        self.add_function(entity_id, entity);
        self.entity_file_index.insert(entity_id, file_path.clone());
        self.track_file_entity(&file_path, row, entity_id);
    }

    fn add_functions(&self, functions: Vec<(EntityId, Entity)>) {
        for (entity_id, entity) in functions {
            self.add_function(entity_id, entity);
        }
    }

    fn add_functions_with_paths(&self, functions: Vec<(EntityId, Entity, String)>) {
        for (entity_id, entity, file_path) in functions {
            self.add_function_with_path(entity_id, entity, file_path);
        }
    }

    fn get_function_by_entity_id(
        &self,
        entity_id: EntityId,
    ) -> Option<dashmap::mapref::one::Ref<'_, EntityId, Entity>> {
        self.function_index.get(&entity_id)
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

    fn function_index(&self) -> &DashMap<EntityId, Entity> {
        &self.function_index
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
        // O(log n + k) via per-file ordered start-row index.
        // The index is maintained by `add_function_with_path` / `remove_function`.
        // Threshold: files with < 2k entities or low QPS still fall back to
        // O(n) scan without loss; the ordered index is always kept up-to-date
        // regardless, so it is used when present.
        if let Some(vec) = self.file_entities_by_start.read().get(file_path) {
            if !vec.is_empty() {
                // Upper bound: first entry with start > end_line (O(log n)).
                let upper = vec.partition_point(|(row, _)| *row as usize <= end_line);
                let mut result = Vec::new();
                for (_, entity_id) in &vec[..upper] {
                    if let Some(entity_ref) = self.function_index.get(entity_id) {
                        let entity_start = entity_ref.span.start_position.row;
                        let entity_end = entity_ref.span.end_position.row;
                        if entity_start <= end_line && entity_end >= start_line {
                            result.push(*entity_id);
                        }
                    }
                }
                return result;
            }
        }
        // Fallback: O(n) scan for files without an ordered index entry
        // (e.g. entities added via `add_function` without a file path).
        let file_entity_ids: Vec<EntityId> = self
            .entity_file_index
            .iter()
            .filter(|entry| entry.value().as_str() == file_path)
            .map(|entry| *entry.key())
            .collect();

        file_entity_ids
            .into_iter()
            .filter(|entity_id| {
                if let Some(entity_ref) = self.function_index.get(entity_id) {
                    let entity_start = entity_ref.span.start_position.row;
                    let entity_end = entity_ref.span.end_position.row;
                    entity_start <= end_line && entity_end >= start_line
                } else {
                    false
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::{EntityKind, Span};
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
    fn test_entity_index_ops() {
        let index = RelationIndex::new();

        // Test add_function
        index.add_function(EntityId(1), create_test_entity(1, "func_a"));
        assert!(index.contains_function(EntityId(1)));
        assert_eq!(index.function_count(), 1);

        // Test add_function_with_path
        index.add_function_with_path(
            EntityId(2),
            create_test_entity(2, "func_b"),
            "test.rs".to_string(),
        );
        assert_eq!(
            index.get_file_path_by_entity(EntityId(2)),
            Some("test.rs".to_string())
        );

        // Test get_function_ids_by_name
        let ids = index.get_function_ids_by_name("func_a");
        assert_eq!(ids.len(), 1);
        assert_eq!(ids[0], EntityId(1));
    }
}
