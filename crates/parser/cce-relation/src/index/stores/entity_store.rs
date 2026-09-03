//! Entity store grouping.
//!
//! Owns:
//! - `function_index: DashMap<EntityId, Entity>`
//! - `name_index: DashMap<String, SmallVec<[EntityId; 2]>>`
//! - `entity_file_index: DashMap<EntityId, String>`
//! - `file_entities_by_start: RwLock<HashMap<String, SmallVec<[(u32, EntityId); 8]>>>`
//! - `entity_id_counter: AtomicU64`
//! - `entity_id_remaps: RwLock<HashMap<String, HashMap<EntityId, EntityId>>>`
//!
//! Provides `deep_clone()` for creating independent mutable copies (used by
//! `detached_clone` and snapshot creation).

use dashmap::DashMap;
use parking_lot::RwLock;
use smallvec::SmallVec;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use cce_types::{Entity, EntityId};

/// Grouped entity maps.
#[derive(Debug, Default)]
pub struct EntityStore {
    pub function_index: Arc<DashMap<EntityId, Entity>>,
    pub name_index: Arc<DashMap<String, SmallVec<[EntityId; 2]>>>,
    pub entity_file_index: Arc<DashMap<EntityId, String>>,
    pub file_entities_by_start: super::super::FileEntitiesByStart,
    pub entity_id_counter: Arc<AtomicU64>,
    pub entity_id_remaps: Arc<RwLock<HashMap<String, HashMap<EntityId, EntityId>>>>,
}

impl Clone for EntityStore {
    fn clone(&self) -> Self {
        Self {
            function_index: Arc::clone(&self.function_index),
            name_index: Arc::clone(&self.name_index),
            entity_file_index: Arc::clone(&self.entity_file_index),
            file_entities_by_start: Arc::clone(&self.file_entities_by_start),
            entity_id_counter: Arc::clone(&self.entity_id_counter),
            entity_id_remaps: Arc::clone(&self.entity_id_remaps),
        }
    }
}

impl EntityStore {
    /// Create a deep, fully independent copy of this store.
    ///
    /// Every map is cloned entry-by-entry into a fresh map, and the
    /// atomic counter is copied by value. The resulting store shares no
    /// runtime state with the source.
    pub fn deep_clone(&self) -> Self {
        let function_index = Arc::new(DashMap::new());
        let name_index: Arc<DashMap<String, SmallVec<[EntityId; 2]>>> = Arc::new(DashMap::new());
        for entry in self.function_index.iter() {
            function_index.insert(*entry.key(), entry.value().clone());
            name_index
                .entry(entry.value().name.clone())
                .or_default()
                .push(*entry.key());
        }

        let entity_file_index = Arc::new(DashMap::new());
        for entry in self.entity_file_index.iter() {
            entity_file_index.insert(*entry.key(), entry.value().clone());
        }

        let file_entities_by_start = Arc::new(RwLock::new(HashMap::new()));
        for (k, v) in self.file_entities_by_start.read().iter() {
            file_entities_by_start.write().insert(k.clone(), v.clone());
        }

        let entity_id_counter = Arc::new(AtomicU64::new(
            self.entity_id_counter.load(Ordering::Relaxed),
        ));

        let entity_id_remaps = Arc::new(RwLock::new(HashMap::new()));
        for (k, v) in self.entity_id_remaps.read().iter() {
            entity_id_remaps.write().insert(k.clone(), v.clone());
        }

        Self {
            function_index,
            name_index,
            entity_file_index,
            file_entities_by_start,
            entity_id_counter,
            entity_id_remaps,
        }
    }

    /// Create an empty store with the entity ID counter starting at `start`.
    pub fn new_with_entity_id_start(start: u64) -> Self {
        let store = Self::default();
        store.entity_id_counter.store(start, Ordering::Relaxed);
        store
    }
}
