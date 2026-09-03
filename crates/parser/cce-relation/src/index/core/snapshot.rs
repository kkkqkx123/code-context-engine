//! Snapshot and copy operations for RelationIndex.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::Ordering;

use dashmap::DashMap;
use parking_lot::RwLock;
use smallvec::SmallVec;

use super::RelationIndex;
use crate::index::snapshot_generation::SnapshotGeneration;
use crate::index::stores::diagnostics::RelationDiagnostics;
use cce_types::EntityId;

impl RelationIndex {
    /// Produce a CoW copy that only deep-copies maps touched by `affected_files`.
    ///
    /// Maps entirely outside the affected scope are shared via `Arc::clone`
    /// (O(1) per map). Maps that contain entries for affected files are
    /// deep-cloned entry by entry, retaining entries outside the scope from
    /// the shared original.
    ///
    /// Cost: O(affected_entries) instead of O(total_entries).
    pub fn selective_cow_copy(&self, affected_files: &HashSet<String>) -> Self {
        // Entity store: copy function_index, name_index, entity_file_index
        // for entities whose file is in affected_files; share the rest.
        let function_index = if affected_files.is_empty() {
            Arc::clone(&self.function_index)
        } else {
            let new_map = Arc::new(DashMap::new());
            for entry in self.function_index.iter() {
                if let Some(file) = self.entity_file_index.get(entry.key()) {
                    if affected_files.contains(file.value()) {
                        new_map.insert(*entry.key(), entry.value().clone());
                    }
                }
            }
            new_map
        };

        let name_index = if affected_files.is_empty() {
            Arc::clone(&self.name_index)
        } else {
            let new_map: Arc<DashMap<String, SmallVec<[EntityId; 2]>>> = Arc::new(DashMap::new());
            for entry in self.function_index.iter() {
                if let Some(file) = self.entity_file_index.get(entry.key()) {
                    if affected_files.contains(file.value()) {
                        new_map
                            .entry(entry.value().name.clone())
                            .or_default()
                            .push(*entry.key());
                    }
                }
            }
            new_map
        };

        let entity_file_index = if affected_files.is_empty() {
            Arc::clone(&self.entity_file_index)
        } else {
            let new_map = Arc::new(DashMap::new());
            for entry in self.entity_file_index.iter() {
                if affected_files.contains(entry.value()) {
                    new_map.insert(*entry.key(), entry.value().clone());
                }
            }
            new_map
        };

        // Relation store: copy resolved_relation_index for affected callers
        let resolved_relation_index = if affected_files.is_empty() {
            Arc::clone(&self.resolved_relation_index)
        } else {
            let new_map = Arc::new(DashMap::new());
            for entry in self.resolved_relation_index.iter() {
                if let Some(file) = self.entity_file_index.get(entry.key()) {
                    if affected_files.contains(file.value()) {
                        new_map.insert(*entry.key(), entry.value().clone());
                    }
                }
            }
            new_map
        };

        let reverse_callee_index = if affected_files.is_empty() {
            Arc::clone(&self.reverse_callee_index)
        } else {
            let new_map = Arc::new(DashMap::new());
            for entry in self.reverse_callee_index.iter() {
                // Reverse index is keyed by callee; include it when the
                // callee's file is affected or any caller is affected.
                let callee = *entry.key();
                let callee_file = self.entity_file_index.get(&callee).map(|v| v.clone());
                let dominated_by_caller = entry.value().iter().any(|caller| {
                    self.entity_file_index
                        .get(caller)
                        .is_some_and(|f| affected_files.contains(f.value()))
                });
                if callee_file.is_some_and(|f| affected_files.contains(&f)) || dominated_by_caller {
                    new_map.insert(callee, entry.value().clone());
                }
            }
            new_map
        };

        // File-level maps: copy entries for affected files
        let file_relation_index = if affected_files.is_empty() {
            Arc::clone(&self.file_relation_index)
        } else {
            let new_map = Arc::new(DashMap::new());
            for entry in self.file_relation_index.iter() {
                if affected_files.contains(entry.key()) {
                    new_map.insert(entry.key().clone(), entry.value().clone());
                }
            }
            new_map
        };

        let file_callers_by_callee = Arc::clone(&self.file_callers_by_callee);

        let file_records = if affected_files.is_empty() {
            Arc::clone(&self.file_records)
        } else {
            let new_map = Arc::new(RwLock::new(HashMap::new()));
            for (k, v) in self.file_records.read().iter() {
                if affected_files.contains(k) {
                    new_map.write().insert(k.clone(), v.clone());
                }
            }
            new_map
        };

        // Symbol registry: copy for affected files
        let symbol_key_to_entity = Arc::clone(&self.symbol_key_to_entity);
        let entity_to_symbol_key = Arc::clone(&self.entity_to_symbol_key);
        let stable_id_to_entity = Arc::clone(&self.stable_id_to_entity);

        let file_symbol_keys = if affected_files.is_empty() {
            Arc::clone(&self.file_symbol_keys)
        } else {
            let new_map = Arc::new(RwLock::new(HashMap::new()));
            for (k, v) in self.file_symbol_keys.read().iter() {
                if affected_files.contains(k) {
                    new_map.write().insert(k.clone(), v.clone());
                }
            }
            new_map
        };

        let file_entities_by_start = if affected_files.is_empty() {
            Arc::clone(&self.file_entities_by_start)
        } else {
            let new_map = Arc::new(RwLock::new(HashMap::new()));
            for (k, v) in self.file_entities_by_start.read().iter() {
                if affected_files.contains(k) {
                    new_map.write().insert(k.clone(), v.clone());
                }
            }
            new_map
        };

        // Diagnostics, dependency graphs: always deep-cloned (cheap).
        let diagnostics = Arc::new(RelationDiagnostics::new());
        diagnostics.symbol_key_conflict_count.store(
            self.diagnostics
                .symbol_key_conflict_count
                .load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
        diagnostics.entity_derived_key_count.store(
            self.diagnostics
                .entity_derived_key_count
                .load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
        diagnostics.relation_derived_key_count.store(
            self.diagnostics
                .relation_derived_key_count
                .load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
        diagnostics.delta_export_unresolved_count.store(
            self.diagnostics
                .delta_export_unresolved_count
                .load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
        if let Ok(guard) = self.diagnostics.symbol_key_conflict_samples.lock() {
            if let Ok(mut target) = diagnostics.symbol_key_conflict_samples.lock() {
                target.extend(guard.iter().cloned());
            }
        }

        let dependency_graph = Arc::new((*self.dependency_graph).clone());
        let entity_dependency_graph =
            Arc::new(RwLock::new(self.entity_dependency_graph.read().clone()));

        Self {
            function_index,
            name_index,
            entity_file_index,
            resolved_relation_index,
            reverse_callee_index,
            file_relation_index,
            file_callers_by_callee,
            file_records,
            dependency_graph,
            entity_dependency_graph,
            symbol_key_to_entity,
            entity_to_symbol_key,
            stable_id_to_entity,
            entity_id_counter: Arc::clone(&self.entity_id_counter),
            entity_id_remaps: Arc::clone(&self.entity_id_remaps),
            diagnostics,
            file_symbol_keys,
            file_entities_by_start,
            generation: Arc::new(SnapshotGeneration::new()),
            last_affected_files: std::sync::Mutex::new(None),
        }
    }
}
