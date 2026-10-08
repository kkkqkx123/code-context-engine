//! Relation store grouping.
//!
//! Owns:
//! - `resolved_relation_index: DashMap<EntityId, RelationEdgeSet>`
//! - `file_relation_index: DashMap<String, RelationEdgeSet>`
//! - `file_callers_by_callee: DashMap<EntityId, HashSet<String>>`

use dashmap::DashMap;
use parking_lot::RwLock;
use smallvec::SmallVec;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use cce_types::{EntityId, FileInfo, ImportTable};

use crate::index::core::{RelationEdgeSet, SymbolKey};
use crate::types::ExportInfo;

/// Grouped relation maps.
#[derive(Debug, Default)]
pub struct RelationStore {
    pub resolved_relation_index: Arc<DashMap<EntityId, RelationEdgeSet>>,
    pub reverse_callee_index: Arc<DashMap<EntityId, Vec<EntityId>>>,
    pub file_relation_index: Arc<DashMap<String, RelationEdgeSet>>,
    pub file_callers_by_callee: Arc<DashMap<EntityId, HashSet<String>>>,
}

impl Clone for RelationStore {
    fn clone(&self) -> Self {
        Self {
            resolved_relation_index: Arc::clone(&self.resolved_relation_index),
            reverse_callee_index: Arc::clone(&self.reverse_callee_index),
            file_relation_index: Arc::clone(&self.file_relation_index),
            file_callers_by_callee: Arc::clone(&self.file_callers_by_callee),
        }
    }
}

impl RelationStore {
    /// Create a deep, fully independent copy of this store.
    pub fn deep_clone(&self) -> Self {
        let resolved_relation_index = Arc::new(DashMap::new());
        for entry in self.resolved_relation_index.iter() {
            resolved_relation_index.insert(*entry.key(), entry.value().clone());
        }

        let reverse_callee_index = Arc::new(DashMap::new());
        for entry in self.reverse_callee_index.iter() {
            reverse_callee_index.insert(*entry.key(), entry.value().clone());
        }

        let file_relation_index = Arc::new(DashMap::new());
        for entry in self.file_relation_index.iter() {
            file_relation_index.insert(entry.key().clone(), entry.value().clone());
        }

        let file_callers_by_callee = Arc::new(DashMap::new());
        for entry in self.file_callers_by_callee.iter() {
            file_callers_by_callee.insert(*entry.key(), entry.value().clone());
        }

        Self {
            resolved_relation_index,
            reverse_callee_index,
            file_relation_index,
            file_callers_by_callee,
        }
    }
}

/// Symbol registry grouping.
#[derive(Debug, Default)]
pub struct SymbolRegistry {
    pub symbol_key_to_entity: Arc<RwLock<HashMap<SymbolKey, EntityId>>>,
    pub entity_to_symbol_key: Arc<RwLock<HashMap<EntityId, SymbolKey>>>,
    pub stable_id_to_entity: Arc<RwLock<HashMap<String, EntityId>>>,
    pub file_symbol_keys: Arc<RwLock<HashMap<String, Vec<SymbolKey>>>>,
}

impl Clone for SymbolRegistry {
    fn clone(&self) -> Self {
        Self {
            symbol_key_to_entity: Arc::clone(&self.symbol_key_to_entity),
            entity_to_symbol_key: Arc::clone(&self.entity_to_symbol_key),
            stable_id_to_entity: Arc::clone(&self.stable_id_to_entity),
            file_symbol_keys: Arc::clone(&self.file_symbol_keys),
        }
    }
}

impl SymbolRegistry {
    /// Create a deep, fully independent copy of this registry.
    pub fn deep_clone(&self) -> Self {
        let symbol_key_to_entity = Arc::new(RwLock::new(HashMap::new()));
        for (k, v) in self.symbol_key_to_entity.read().iter() {
            symbol_key_to_entity.write().insert(k.clone(), *v);
        }

        let entity_to_symbol_key = Arc::new(RwLock::new(HashMap::new()));
        for (k, v) in self.entity_to_symbol_key.read().iter() {
            entity_to_symbol_key.write().insert(*k, v.clone());
        }

        let stable_id_to_entity = Arc::new(RwLock::new(HashMap::new()));
        for (k, v) in self.stable_id_to_entity.read().iter() {
            stable_id_to_entity.write().insert(k.clone(), *v);
        }

        let file_symbol_keys = Arc::new(RwLock::new(HashMap::new()));
        for (k, v) in self.file_symbol_keys.read().iter() {
            file_symbol_keys.write().insert(k.clone(), v.clone());
        }

        Self {
            symbol_key_to_entity,
            entity_to_symbol_key,
            stable_id_to_entity,
            file_symbol_keys,
        }
    }
}

/// Unified file-level metadata record.
///
/// Merges `file_index`, `import_index`, and `export_index` into a single
/// per-file entry, eliminating two redundant String key copies and two
/// extra heap allocations per file.
#[derive(Debug, Clone, Default)]
pub struct FileRecord {
    pub info: FileInfo,
    pub imports: ImportTable,
    pub exports: SmallVec<[ExportInfo; 2]>,
}

/// File store grouping — single unified map.
#[derive(Debug, Default)]
pub struct FileStore {
    pub file_records: Arc<RwLock<HashMap<String, FileRecord>>>,
}

impl Clone for FileStore {
    fn clone(&self) -> Self {
        Self {
            file_records: Arc::clone(&self.file_records),
        }
    }
}

impl FileStore {
    /// Create a deep, fully independent copy of this store.
    pub fn deep_clone(&self) -> Self {
        let file_records = Arc::new(RwLock::new(HashMap::new()));
        for (k, v) in self.file_records.read().iter() {
            file_records.write().insert(k.clone(), v.clone());
        }

        Self { file_records }
    }
}
