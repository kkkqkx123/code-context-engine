//! Query result cache with LRU eviction.
//!
//! Caches frequent relation queries to avoid repeated graph traversal.
//! Uses a simple HashMap + VecDeque for LRU ordering without external
//! dependencies.

use std::collections::{HashMap, VecDeque};
use std::hash::Hash;

use cce_types::{EntityId, ResolvedRelation};

use crate::index::core::CallChainNode;

/// Simple LRU cache.
#[derive(Debug)]
struct LruCache<K, V> {
    capacity: usize,
    map: HashMap<K, V>,
    order: VecDeque<K>,
}

impl<K, V> LruCache<K, V>
where
    K: Eq + Hash + Clone,
{
    fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            map: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    fn get(&mut self, key: &K) -> Option<&V> {
        if self.map.contains_key(key) {
            self.promote(key);
            self.map.get(key)
        } else {
            None
        }
    }

    fn put(&mut self, key: K, value: V) {
        if self.map.contains_key(&key) {
            self.promote(&key);
            self.map.insert(key, value);
            return;
        }
        if self.map.len() >= self.capacity {
            if let Some(old) = self.order.pop_front() {
                self.map.remove(&old);
            }
        }
        self.order.push_back(key.clone());
        self.map.insert(key, value);
    }

    fn promote(&mut self, key: &K) {
        if let Some(pos) = self.order.iter().position(|k| k == key) {
            self.order.remove(pos);
            self.order.push_back(key.clone());
        }
    }

    fn len(&self) -> usize {
        self.map.len()
    }

    fn clear(&mut self) {
        self.map.clear();
        self.order.clear();
    }
}

/// Query result cache.
///
/// Caches reference lookups, definition lookups, and call-chain traversals.
#[derive(Debug)]
pub struct QueryCache {
    reference_cache: LruCache<(EntityId, String), Vec<ResolvedRelation>>,
    call_chain_cache: LruCache<(EntityId, usize, bool), Vec<CallChainNode>>,
    callers_cache: LruCache<EntityId, Vec<EntityId>>,
}

impl QueryCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            reference_cache: LruCache::new(capacity),
            call_chain_cache: LruCache::new(capacity),
            callers_cache: LruCache::new(capacity),
        }
    }

    pub fn get_references(&mut self, key: (EntityId, String)) -> Option<&Vec<ResolvedRelation>> {
        self.reference_cache.get(&key)
    }

    pub fn put_references(&mut self, key: (EntityId, String), value: Vec<ResolvedRelation>) {
        self.reference_cache.put(key, value);
    }

    pub fn get_call_chain(&mut self, key: (EntityId, usize, bool)) -> Option<&Vec<CallChainNode>> {
        self.call_chain_cache.get(&key)
    }

    pub fn put_call_chain(&mut self, key: (EntityId, usize, bool), value: Vec<CallChainNode>) {
        self.call_chain_cache.put(key, value);
    }

    pub fn get_callers(&mut self, key: EntityId) -> Option<&Vec<EntityId>> {
        self.callers_cache.get(&key)
    }

    pub fn put_callers(&mut self, key: EntityId, value: Vec<EntityId>) {
        self.callers_cache.put(key, value);
    }

    pub fn len(&self) -> usize {
        self.reference_cache.len() + self.call_chain_cache.len() + self.callers_cache.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn clear(&mut self) {
        self.reference_cache.clear();
        self.call_chain_cache.clear();
        self.callers_cache.clear();
    }
}

impl Default for QueryCache {
    fn default() -> Self {
        Self::new(128)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::EntityId;

    #[test]
    fn lru_eviction() {
        let mut cache = QueryCache::new(2);
        cache.put_callers(EntityId(1), vec![EntityId(2)]);
        cache.put_callers(EntityId(2), vec![EntityId(3)]);
        assert_eq!(cache.callers_cache.len(), 2);
        cache.put_callers(EntityId(3), vec![EntityId(4)]);
        // LRU eviction should have removed EntityId(1)
        assert!(cache.callers_cache.get(&EntityId(1)).is_none());
        assert!(cache.callers_cache.get(&EntityId(2)).is_some());
    }

    #[test]
    fn reference_cache_hit() {
        let mut cache = QueryCache::new(4);
        let key = (EntityId(1), "file.rs".to_string());
        cache.put_references(key.clone(), vec![]);
        assert!(cache.get_references(key).is_some());
    }

    #[test]
    fn call_chain_cache_roundtrip() {
        let mut cache = QueryCache::new(4);
        let key = (EntityId(10), 3, true);
        let nodes = vec![];
        cache.put_call_chain(key, nodes);
        assert!(cache.get_call_chain((EntityId(10), 3, true)).is_some());
    }
}
