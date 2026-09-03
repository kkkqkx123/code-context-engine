//! Local symbol table (file-level)
//!
//! Manages symbols within a single file, providing fast local lookup.
//!
//! # Memory Optimization
//! Uses `Arc` for shared data to minimize memory footprint when the same
//! data is referenced from multiple places (e.g., ParsedFile).

use crate::symbol::{SymbolMetadata, SymbolRef};
use cce_types::language::Language;
use cce_types::{Entity, EntityId, EntityKind};
use std::collections::HashMap;
use std::sync::Arc;

/// Default maximum scope chain depth, matching
/// `SymbolResolutionConfig::max_scope_chain_depth` default.
const DEFAULT_MAX_SCOPE_CHAIN_DEPTH: usize = 100;

/// Local symbol table for file-level symbol management
///
/// Uses `Arc` for efficient data sharing:
/// - `file_path`: Shared string for file path
/// - `entities`: Shared entity storage (can be shared with ParsedFile)
///
/// # Scope-aware resolution
///
/// Maintains a `scope_index` that maps `(parent_id, name)` to entity IDs,
/// enabling scope chain resolution to correctly handle name shadowing.
#[derive(Debug, Clone)]
pub struct LocalSymbolTable {
    /// File path (Arc<str> for efficient sharing)
    pub file_path: Arc<str>,

    /// Language
    pub language: Language,

    /// Entity storage: EntityId -> Entity (Arc for sharing with ParsedFile)
    entities: Arc<HashMap<EntityId, Entity>>,

    /// Entity IDs for local lookup (avoids duplicating Entity data)
    entity_ids: Vec<EntityId>,

    /// Name index: name -> Vec<EntityId>
    name_index: HashMap<Arc<str>, Vec<EntityId>>,

    /// Scope index: (parent_id, name) -> Vec<EntityId>
    /// Enables scope chain resolution from inner to outer scopes.
    /// parent_id = None means top-level scope (depth=0).
    scope_index: HashMap<(Option<EntityId>, Arc<str>), Vec<EntityId>>,

    /// Symbol metadata cache: EntityId -> SymbolMetadata
    metadata_cache: HashMap<EntityId, SymbolMetadata>,

    /// Maximum scope chain depth, backed by
    /// `SymbolResolutionConfig::max_scope_chain_depth`.
    max_scope_chain_depth: usize,
}

impl LocalSymbolTable {
    /// Create a new local symbol table
    pub fn new(file_path: impl Into<Arc<str>>, language: Language) -> Self {
        Self {
            file_path: file_path.into(),
            language,
            entities: Arc::new(HashMap::new()),
            entity_ids: Vec::new(),
            name_index: HashMap::new(),
            scope_index: HashMap::new(),
            metadata_cache: HashMap::new(),
            max_scope_chain_depth: DEFAULT_MAX_SCOPE_CHAIN_DEPTH,
        }
    }

    /// Create a local symbol table with shared entity storage
    ///
    /// This is useful when the entities are already stored in a ParsedFile
    /// and we want to avoid duplicating the data.
    pub fn with_shared_entities(
        file_path: impl Into<Arc<str>>,
        language: Language,
        entities: Arc<HashMap<EntityId, Entity>>,
    ) -> Self {
        let mut entity_ids = Vec::with_capacity(entities.len());
        let mut name_index: HashMap<Arc<str>, Vec<EntityId>> = HashMap::new();
        let mut scope_index: HashMap<(Option<EntityId>, Arc<str>), Vec<EntityId>> = HashMap::new();

        for (id, entity) in entities.iter() {
            entity_ids.push(*id);
            let name: Arc<str> = entity.name.clone().into();
            name_index.entry(name.clone()).or_default().push(*id);
            scope_index
                .entry((entity.parent, name))
                .or_default()
                .push(*id);
        }

        Self {
            file_path: file_path.into(),
            language,
            entities,
            entity_ids,
            name_index,
            scope_index,
            metadata_cache: HashMap::new(),
            max_scope_chain_depth: DEFAULT_MAX_SCOPE_CHAIN_DEPTH,
        }
    }

    /// Set the maximum scope chain depth (backed by
    /// `SymbolResolutionConfig::max_scope_chain_depth`).
    pub fn set_max_scope_chain_depth(&mut self, depth: usize) {
        self.max_scope_chain_depth = depth;
    }

    /// Add an entity to the table
    pub fn add_entity(&mut self, entity: Entity) {
        let id = entity.id;
        let parent = entity.parent;
        let name: Arc<str> = entity.name.clone().into();

        // Insert into the shared entity map, cloning lazily only when other
        // references exist. `Arc::make_mut` is infallible (it clones on
        // demand), so no `Arc::get_mut().expect` is needed here.
        Arc::make_mut(&mut self.entities).insert(id, entity);

        // Track entity ID
        self.entity_ids.push(id);

        // Index by name
        self.name_index.entry(name.clone()).or_default().push(id);

        // Index by scope
        self.scope_index.entry((parent, name)).or_default().push(id);
    }

    /// Add multiple entities
    pub fn add_entities(&mut self, entities: Vec<Entity>) {
        for entity in entities {
            self.add_entity(entity);
        }
    }

    /// Get entity by ID
    pub fn get_entity(&self, id: EntityId) -> Option<&Entity> {
        self.entities.get(&id)
    }

    /// Get entities by name
    pub fn get_by_name(&self, name: &str) -> Vec<&Entity> {
        self.name_index
            .get(name)
            .map(|ids| ids.iter().filter_map(|id| self.entities.get(id)).collect())
            .unwrap_or_default()
    }

    /// Get first entity by name (for common case of no overloading)
    pub fn get_first_by_name(&self, name: &str) -> Option<&Entity> {
        self.name_index
            .get(name)
            .and_then(|ids| ids.first())
            .and_then(|id| self.entities.get(id))
    }

    /// Check if a name exists (has any entities)
    pub fn has_name(&self, name: &str) -> bool {
        self.name_index.contains_key(name)
    }

    /// Get all entity IDs with a given kind
    pub fn get_by_kind(&self, kind: EntityKind) -> Vec<&Entity> {
        self.entities.values().filter(|e| e.kind == kind).collect()
    }

    /// Get all entity names
    pub fn all_names(&self) -> Vec<&str> {
        self.name_index.keys().map(|s| s.as_ref()).collect()
    }

    /// Get all entities
    pub fn all_entities(&self) -> Vec<&Entity> {
        self.entities.values().collect()
    }

    /// Get entity count
    pub fn len(&self) -> usize {
        self.entity_ids.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.entity_ids.is_empty()
    }

    /// Create a SymbolRef for an entity
    ///
    /// This method ensures metadata consistency by always using cached metadata
    /// when available. If no cached metadata exists, it creates new metadata
    /// and caches it for future use.
    ///
    /// The returned `SymbolRef` uses the entity's own `EntityId` as the symbol ID,
    /// eliminating the separate ID space.
    pub fn create_symbol_ref(&self, entity_id: EntityId) -> Option<SymbolRef> {
        let entity = self.entities.get(&entity_id)?;

        // Check if metadata is already cached
        let metadata = if let Some(cached) = self.metadata_cache.get(&entity_id) {
            cached.clone()
        } else {
            // Create new metadata and cache it for consistency
            let location = crate::symbol::SymbolLocation::new(
                self.file_path.clone(),
                entity.span,
                self.language,
            );
            SymbolMetadata::new(entity.name.clone(), entity.kind, location)
        };

        Some(SymbolRef::new(entity_id, metadata))
    }

    /// Create a SymbolRef with guaranteed cached metadata
    ///
    /// This method ensures the metadata is cached before creating the SymbolRef.
    /// Use this when you want to guarantee metadata consistency across multiple calls.
    pub fn create_symbol_ref_with_cache(&mut self, entity_id: EntityId) -> Option<SymbolRef> {
        let entity = self.entities.get(&entity_id)?.clone();

        // Ensure metadata is cached
        if !self.metadata_cache.contains_key(&entity_id) {
            let location = crate::symbol::SymbolLocation::new(
                self.file_path.clone(),
                entity.span,
                self.language,
            );
            let metadata = SymbolMetadata::new(entity.name.clone(), entity.kind, location);
            self.metadata_cache.insert(entity_id, metadata);
        }

        let metadata = self.metadata_cache.get(&entity_id)?.clone();
        Some(SymbolRef::new(entity_id, metadata))
    }

    /// Build scope chain for an entity by walking the parent chain.
    ///
    /// Returns an ordered list from outermost (root) to innermost (the entity itself).
    /// The entity_id itself is the last element.
    /// The depth is bounded by `max_scope_chain_depth` (backed by
    /// `SymbolResolutionConfig::max_scope_chain_depth`). Callers that need a
    /// custom limit should use `build_scope_chain_with_limit`.
    pub fn build_scope_chain(&self, entity_id: EntityId) -> Vec<EntityId> {
        self.build_scope_chain_with_limit(entity_id, self.max_scope_chain_depth)
    }

    /// Build scope chain with an explicit depth limit.
    pub fn build_scope_chain_with_limit(
        &self,
        entity_id: EntityId,
        max_depth: usize,
    ) -> Vec<EntityId> {
        let mut chain: Vec<EntityId> = Vec::new();
        let mut current = Some(entity_id);

        while let Some(id) = current {
            chain.push(id);
            if chain.len() > max_depth {
                break;
            }
            current = self.entities.get(&id).and_then(|e| e.parent);
        }

        chain.reverse();
        chain
    }

    /// Resolve a name using the scope chain (inner-to-outer).
    ///
    /// Traverses the scope_chain from innermost to outermost scope,
    /// returning the first matching entity. This correctly handles
    /// name shadowing (inner declarations shadow outer ones).
    ///
    /// Returns None if no entity with the given name is found in any scope.
    pub fn resolve_by_scope(&self, name: &str, scope_chain: &[EntityId]) -> Option<&Entity> {
        // Traverse scope chain from innermost (last) to outermost (first)
        for scope_id in scope_chain.iter().rev() {
            let key = (Some(*scope_id), Arc::<str>::from(name));
            if let Some(ids) = self.scope_index.get(&key) {
                if let Some(first_id) = ids.first() {
                    if let Some(entity) = self.entities.get(first_id) {
                        return Some(entity);
                    }
                }
            }
        }
        // Also check top-level scope (parent = None)
        let top_key = (None, Arc::<str>::from(name));
        if let Some(ids) = self.scope_index.get(&top_key) {
            if let Some(first_id) = ids.first() {
                if let Some(entity) = self.entities.get(first_id) {
                    return Some(entity);
                }
            }
        }
        None
    }

    /// Get reference to the entity map
    pub fn get_entity_map(&self) -> &Arc<HashMap<EntityId, Entity>> {
        &self.entities
    }

    /// Apply an entity ID remap to all internal indexes.
    ///
    /// This is called after `index_file_core` assigns globally unique EntityIds
    /// to replace ParsedFile-local IDs in the name_index, scope_index, and
    /// entity_ids. This ensures the LocalSymbolTable uses the same ID space
    /// as the RelationIndex.
    pub fn apply_entity_remap(&mut self, remap: &HashMap<EntityId, EntityId>) {
        // Rebuild entity storage with remapped IDs
        let old_entities = Arc::make_mut(&mut self.entities);
        let mut new_entities = HashMap::with_capacity(old_entities.len());
        for (old_id, entity) in old_entities.drain() {
            let new_id = remap.get(&old_id).copied().unwrap_or(old_id);
            let mut entity = entity;
            entity.id = new_id;
            if let Some(pid) = entity.parent {
                entity.parent = remap.get(&pid).copied();
            }
            entity.children = entity
                .children
                .iter()
                .filter_map(|cid| remap.get(cid).copied())
                .collect();
            new_entities.insert(new_id, entity);
        }
        *old_entities = new_entities;

        // Rebuild entity_ids
        self.entity_ids = self.entities.keys().copied().collect();

        // Rebuild name_index
        self.name_index.clear();
        for (id, entity) in self.entities.iter() {
            let name: Arc<str> = entity.name.clone().into();
            self.name_index.entry(name).or_default().push(*id);
        }

        // Rebuild scope_index
        self.scope_index.clear();
        for (id, entity) in self.entities.iter() {
            let name: Arc<str> = entity.name.clone().into();
            self.scope_index
                .entry((entity.parent, name))
                .or_default()
                .push(*id);
        }

        // Remap metadata_cache keys
        let old_cache = std::mem::take(&mut self.metadata_cache);
        for (old_id, metadata) in old_cache {
            let new_id = remap.get(&old_id).copied().unwrap_or(old_id);
            self.metadata_cache.insert(new_id, metadata);
        }
    }

    /// Cache symbol metadata
    pub fn cache_metadata(&mut self, entity_id: EntityId, metadata: SymbolMetadata) {
        self.metadata_cache.insert(entity_id, metadata);
    }

    /// Build from ParsedFile
    ///
    /// Creates a LocalSymbolTable with shared entity storage from a ParsedFile.
    /// This avoids duplicating entity data in memory.
    pub fn from_parsed_file(file: &cce_types::entity::ParsedFile) -> Self {
        // Convert entities Vec to HashMap for efficient lookup
        let mut entities_map = HashMap::with_capacity(file.entities.len());
        for entity in &file.entities {
            entities_map.insert(entity.id, entity.clone());
        }

        Self::with_shared_entities(file.path.clone(), file.language, Arc::new(entities_map))
    }

    /// Build from ParsedFile with shared Arc entities
    ///
    /// This is the most memory-efficient way to create a LocalSymbolTable
    /// when you already have entities in an Arc<HashMap>.
    pub fn from_parsed_file_shared(
        file_path: impl Into<Arc<str>>,
        language: Language,
        entities: Arc<HashMap<EntityId, Entity>>,
    ) -> Self {
        Self::with_shared_entities(file_path, language, entities)
    }

    /// Find entities that match a pattern (for fuzzy matching)
    pub fn find_matching(&self, pattern: &str) -> Vec<&Entity> {
        let pattern_lower = pattern.to_lowercase();
        self.entities
            .values()
            .filter(|e| e.name.to_lowercase().contains(&pattern_lower))
            .collect()
    }

    /// Get entities in a specific line range
    pub fn get_in_range(&self, start_line: usize, end_line: usize) -> Vec<&Entity> {
        self.entities
            .values()
            .filter(|e| {
                let entity_start = e.span.start_position.row;
                let entity_end = e.span.end_position.row;
                entity_start >= start_line && entity_end <= end_line
            })
            .collect()
    }
}

impl Default for LocalSymbolTable {
    fn default() -> Self {
        Self::new(String::new(), Language::Unknown)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::Span;

    fn create_test_entity(id: u32, name: &str, kind: EntityKind) -> Entity {
        Entity {
            id: EntityId(id.into()),
            name: name.to_string(),
            kind,
            signature: String::new(),
            parameters: Vec::new(),
            return_type: None,
            span: Span {
                start_byte: 0,
                end_byte: 10,
                start_position: Default::default(),
                end_position: Default::default(),
            },
            depth: 0,
            parent: None,
            children: Vec::new(),
            doc_comment: None,
            modifiers: Vec::new(),
            attributes: std::collections::HashMap::new(),
            metadata: std::collections::HashMap::new(),
            is_stdlib: false,
            subtype: None,
            stdlib_category: None,
        }
    }

    #[test]
    fn test_add_and_get_entity() {
        let mut table = LocalSymbolTable::new("src/lib.rs".to_string(), Language::Rust);

        let entity = create_test_entity(1, "test_func", EntityKind::Function);
        table.add_entity(entity);

        assert_eq!(table.len(), 1);
        assert!(table.get_entity(EntityId(1)).is_some());
    }

    #[test]
    fn test_get_by_name() {
        let mut table = LocalSymbolTable::new("src/lib.rs".to_string(), Language::Rust);

        table.add_entity(create_test_entity(1, "func_a", EntityKind::Function));
        table.add_entity(create_test_entity(2, "func_b", EntityKind::Function));
        table.add_entity(create_test_entity(3, "func_a", EntityKind::Function)); // Overload

        let func_a = table.get_by_name("func_a");
        assert_eq!(func_a.len(), 2);

        let func_c = table.get_by_name("func_c");
        assert!(func_c.is_empty());
    }

    #[test]
    fn test_get_first_by_name() {
        let mut table = LocalSymbolTable::new("src/lib.rs".to_string(), Language::Rust);

        table.add_entity(create_test_entity(1, "test", EntityKind::Function));

        let found = table.get_first_by_name("test");
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, EntityId(1));
    }

    #[test]
    fn test_get_by_kind() {
        let mut table = LocalSymbolTable::new("src/lib.rs".to_string(), Language::Rust);

        table.add_entity(create_test_entity(1, "MyStruct", EntityKind::Struct));
        table.add_entity(create_test_entity(2, "my_func", EntityKind::Function));
        table.add_entity(create_test_entity(3, "MyClass", EntityKind::Class));

        let types = table.get_by_kind(EntityKind::Struct);
        assert_eq!(types.len(), 1);

        let funcs = table.get_by_kind(EntityKind::Function);
        assert_eq!(funcs.len(), 1);
    }

    #[test]
    fn test_create_symbol_ref() {
        let mut table = LocalSymbolTable::new("src/lib.rs".to_string(), Language::Rust);

        table.add_entity(create_test_entity(1, "test", EntityKind::Function));

        let symbol_ref = table.create_symbol_ref(EntityId(1));
        assert!(symbol_ref.is_some());
        // SymbolRef now uses EntityId directly
        assert_eq!(symbol_ref.unwrap().symbol_id().0, 1);
    }

    #[test]
    fn test_apply_entity_remap() {
        let mut table = LocalSymbolTable::new("src/lib.rs".to_string(), Language::Rust);

        table.add_entity(create_test_entity(1, "func_a", EntityKind::Function));
        table.add_entity(create_test_entity(2, "func_b", EntityKind::Function));

        let mut remap = HashMap::new();
        remap.insert(EntityId(1), EntityId(100));
        remap.insert(EntityId(2), EntityId(200));

        table.apply_entity_remap(&remap);

        // Old IDs should be gone
        assert!(table.get_entity(EntityId(1)).is_none());
        assert!(table.get_entity(EntityId(2)).is_none());

        // New IDs should be present
        assert!(table.get_entity(EntityId(100)).is_some());
        assert!(table.get_entity(EntityId(200)).is_some());

        // Name index should work with new IDs
        let func_a = table.get_by_name("func_a");
        assert_eq!(func_a.len(), 1);
        assert_eq!(func_a[0].id, EntityId(100));
    }
}
