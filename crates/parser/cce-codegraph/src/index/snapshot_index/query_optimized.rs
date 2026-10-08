use std::collections::{HashMap, HashSet};

use cce_types::{EntityId, ResolvedRelation};
use dashmap::DashMap;

use crate::index::compact::CompactRelationIndex;
use crate::index::core::{RelationEdgeSet, RelationIndex};

/// Precomputed read-only indexes for high-concurrency queries.
///
/// Built once during snapshot creation (`O(entries)`) and then shared via
/// `Arc` for lock-free `O(1)` lookups. Avoids per-query `DashMap` read locks
/// and full-index scans for callee->callers reverse lookups.
#[derive(Debug, Clone, Default)]
pub struct QueryOptimizedIndex {
    callers_by_callee: HashMap<EntityId, Vec<EntityId>>,
    callees_by_caller: HashMap<EntityId, Vec<ResolvedRelation>>,
}

impl QueryOptimizedIndex {
    pub fn from_relation_index(index: &RelationIndex) -> Self {
        Self::from_resolved_map(&index.resolved_relation_index)
    }

    pub fn from_resolved_map(resolved: &DashMap<EntityId, RelationEdgeSet>) -> Self {
        let mut callers_by_callee: HashMap<EntityId, Vec<EntityId>> = HashMap::new();
        let mut callees_by_caller: HashMap<EntityId, Vec<ResolvedRelation>> = HashMap::new();
        for entry in resolved.iter() {
            let (caller, relations) = entry.pair();
            let edges = relations.edges.clone();
            callees_by_caller.insert(*caller, edges.clone());
            for rel in edges {
                if let Some(callee_id) = rel.callee_id {
                    callers_by_callee
                        .entry(callee_id)
                        .or_default()
                        .push(*caller);
                }
            }
        }
        for callers in callers_by_callee.values_mut() {
            callers.sort();
            callers.dedup();
        }
        Self {
            callers_by_callee,
            callees_by_caller,
        }
    }

    /// Build from a compact index without touching `DashMap` locks.
    pub fn from_compact_index(index: &CompactRelationIndex) -> Self {
        let mut callers_by_callee: HashMap<EntityId, Vec<EntityId>> = HashMap::new();
        let mut callees_by_caller: HashMap<EntityId, Vec<ResolvedRelation>> = HashMap::new();
        for (caller, relations) in &index.resolved_relation_index {
            callees_by_caller.insert(*caller, relations.edges.clone());
            for rel in &relations.edges {
                if let Some(callee_id) = rel.callee_id {
                    callers_by_callee
                        .entry(callee_id)
                        .or_default()
                        .push(*caller);
                }
            }
        }
        for callers in callers_by_callee.values_mut() {
            callers.sort();
            callers.dedup();
        }
        Self {
            callers_by_callee,
            callees_by_caller,
        }
    }

    pub fn empty() -> Self {
        Self::default()
    }

    pub fn get_callees(&self, caller_id: EntityId) -> Option<&Vec<ResolvedRelation>> {
        self.callees_by_caller.get(&caller_id)
    }

    pub fn get_callers(&self, callee_id: EntityId) -> Option<&Vec<EntityId>> {
        self.callers_by_callee.get(&callee_id)
    }
}

#[derive(Debug, Clone, Default)]
pub struct TransitiveFileDeps {
    transitive_dependents: HashMap<String, HashSet<String>>,
    transitive_dependencies: HashMap<String, HashSet<String>>,
}

impl TransitiveFileDeps {
    /// Build from a compact index.
    pub fn from_compact_index(index: &CompactRelationIndex, max_depth: usize) -> Self {
        let mut transitive_dependents: HashMap<String, HashSet<String>> = HashMap::new();
        let mut transitive_dependencies: HashMap<String, HashSet<String>> = HashMap::new();

        for file in index.file_records.keys() {
            transitive_dependents.entry(file.clone()).or_default();
            transitive_dependencies.entry(file.clone()).or_default();
        }
        for file in index.dependency_forward.keys() {
            transitive_dependents.entry(file.clone()).or_default();
            transitive_dependencies.entry(file.clone()).or_default();
        }
        for file in index.dependency_reverse.keys() {
            transitive_dependents.entry(file.clone()).or_default();
            transitive_dependencies.entry(file.clone()).or_default();
        }
        for file in index.file_relation_index.keys() {
            transitive_dependents.entry(file.clone()).or_default();
            transitive_dependencies.entry(file.clone()).or_default();
        }

        for (file, deps) in &index.dependency_forward {
            for dep in deps {
                transitive_dependents
                    .entry(dep.clone())
                    .or_default()
                    .insert(file.clone());
                transitive_dependencies
                    .entry(file.clone())
                    .or_default()
                    .insert(dep.clone());
            }
        }

        for (caller_file, relations) in &index.file_relation_index {
            for rel in relations.iter() {
                if let Some(callee_id) = rel.callee_id {
                    if let Some(callee_file) = index.entity_file_index.get(&callee_id) {
                        if caller_file != callee_file {
                            transitive_dependents
                                .entry(callee_file.clone())
                                .or_default()
                                .insert(caller_file.clone());
                            transitive_dependencies
                                .entry(caller_file.clone())
                                .or_default()
                                .insert(callee_file.clone());
                        }
                    }
                }
            }
        }

        for (caller_id, relations) in &index.resolved_relation_index {
            if let Some(caller_file) = index.entity_file_index.get(caller_id) {
                for rel in relations.iter() {
                    if let Some(callee_id) = rel.callee_id {
                        if let Some(callee_file) = index.entity_file_index.get(&callee_id) {
                            if caller_file != callee_file {
                                transitive_dependents
                                    .entry(callee_file.clone())
                                    .or_default()
                                    .insert(caller_file.clone());
                                transitive_dependencies
                                    .entry(caller_file.clone())
                                    .or_default()
                                    .insert(callee_file.clone());
                            }
                        }
                    }
                }
            }
        }

        Self::propagate(&mut transitive_dependents, max_depth);
        Self::propagate(&mut transitive_dependencies, max_depth);

        Self {
            transitive_dependents,
            transitive_dependencies,
        }
    }

    pub fn from_relation_index(index: &RelationIndex, max_depth: usize) -> Self {
        let mut transitive_dependents: HashMap<String, HashSet<String>> = HashMap::new();
        let mut transitive_dependencies: HashMap<String, HashSet<String>> = HashMap::new();

        for file in index.file_records.read().keys() {
            transitive_dependents.entry(file.clone()).or_default();
            transitive_dependencies.entry(file.clone()).or_default();
        }
        for file in index.dependency_graph.get_all_files() {
            transitive_dependents.entry(file.clone()).or_default();
            transitive_dependencies.entry(file).or_default();
        }
        for entry in index.file_relation_index.iter() {
            let callee_file_key = entry.key().clone();
            transitive_dependents
                .entry(callee_file_key.clone())
                .or_default();
            transitive_dependencies.entry(callee_file_key).or_default();
        }

        for file in index.dependency_graph.get_all_files() {
            for dep in index.dependency_graph.get_dependencies(&file) {
                transitive_dependents
                    .entry(dep.clone())
                    .or_default()
                    .insert(file.clone());
                transitive_dependencies
                    .entry(file.clone())
                    .or_default()
                    .insert(dep.clone());
            }
        }

        for entry in index.file_relation_index.iter() {
            let caller_file = entry.key().clone();
            for rel in entry.value().iter() {
                if let Some(callee_id) = rel.callee_id {
                    if let Some(callee_file) =
                        index.entity_file_index.get(&callee_id).map(|v| v.clone())
                    {
                        if caller_file != callee_file {
                            transitive_dependents
                                .entry(callee_file.clone())
                                .or_default()
                                .insert(caller_file.clone());
                            transitive_dependencies
                                .entry(caller_file.clone())
                                .or_default()
                                .insert(callee_file.clone());
                        }
                    }
                }
            }
        }

        for entry in index.resolved_relation_index.iter() {
            let caller_id = *entry.key();
            if let Some(caller_file) = index.entity_file_index.get(&caller_id).map(|v| v.clone()) {
                for rel in entry.value().iter() {
                    if let Some(callee_id) = rel.callee_id {
                        if let Some(callee_file) =
                            index.entity_file_index.get(&callee_id).map(|v| v.clone())
                        {
                            if caller_file != callee_file {
                                transitive_dependents
                                    .entry(callee_file.clone())
                                    .or_default()
                                    .insert(caller_file.clone());
                                transitive_dependencies
                                    .entry(caller_file.clone())
                                    .or_default()
                                    .insert(callee_file);
                            }
                        }
                    }
                }
            }
        }

        Self::propagate(&mut transitive_dependents, max_depth);
        Self::propagate(&mut transitive_dependencies, max_depth);

        Self {
            transitive_dependents,
            transitive_dependencies,
        }
    }

    fn propagate(deps: &mut HashMap<String, HashSet<String>>, max_depth: usize) {
        for _ in 0..max_depth {
            let mut changed = false;
            let keys: Vec<_> = deps.keys().cloned().collect();
            for key in keys {
                let current = deps.get(&key).cloned().unwrap_or_default();
                let mut new_deps = current.clone();
                for dep in &current {
                    if let Some(dep_deps) = deps.get(dep) {
                        new_deps.extend(dep_deps.iter().cloned());
                    }
                }
                new_deps.remove(&key);
                if new_deps.len() > current.len() {
                    deps.insert(key, new_deps);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
    }

    pub fn transitive_dependents_of(&self, file_path: &str) -> Vec<String> {
        self.transitive_dependents
            .get(file_path)
            .map(|s| {
                let mut v: Vec<String> = s.iter().cloned().collect();
                v.sort();
                v
            })
            .unwrap_or_default()
    }

    pub fn transitive_dependencies_of(&self, file_path: &str) -> Vec<String> {
        self.transitive_dependencies
            .get(file_path)
            .map(|s| {
                let mut v: Vec<String> = s.iter().cloned().collect();
                v.sort();
                v
            })
            .unwrap_or_default()
    }

    pub fn empty() -> Self {
        Self::default()
    }
}
