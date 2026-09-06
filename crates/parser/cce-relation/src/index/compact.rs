//! Compact immutable relation index for snapshots and queries.
//!
//! `CompactRelationIndex` is a memory-efficient, immutable counterpart to
//! `RelationIndex`. It is built once during snapshot creation by draining the
//! concurrent `DashMap`/`RwLock` structures into plain `HashMap`s, eliminating
//! per-entry lock and `DashMap` shard overhead.
//!
//! Build path: `RelationIndex` (concurrent DashMap) → `CompactRelationIndex`
//! (plain HashMap) → `QueryOptimizedIndex` / `TransitiveFileDeps`.
//!
//! The compact form is intended for read-only snapshots. Mutations continue to
//! use `RelationIndex` with `DashMap` for concurrent writes.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use smallvec::SmallVec;

use super::core::{RelationEdgeSet, RelationIndex};
use super::stores::{FileRecord, diagnostics::RelationDiagnostics};
use crate::dependency_graph::{EntityDependencyGraph, FileDependencyGraph};
use crate::index::core::SymbolKey;
use cce_types::{Entity, EntityId};

/// Compact, immutable relation index for snapshots.
///
/// Uses plain `HashMap`/`HashSet` instead of `DashMap` and `RwLock` where
/// possible, reducing per-entry memory overhead for read-heavy snapshot
/// queries.
#[derive(Debug, Clone, Default)]
pub struct CompactRelationIndex {
    pub function_index: HashMap<EntityId, Entity>,
    pub resolved_relation_index: HashMap<EntityId, RelationEdgeSet>,
    pub entity_file_index: HashMap<EntityId, String>,
    pub file_relation_index: HashMap<String, RelationEdgeSet>,
    pub file_callers_by_callee: HashMap<EntityId, HashSet<String>>,
    pub file_records: HashMap<String, FileRecord>,
    pub symbol_key_to_entity: HashMap<SymbolKey, EntityId>,
    pub entity_to_symbol_key: HashMap<EntityId, SymbolKey>,
    pub stable_id_to_entity: HashMap<String, EntityId>,
    pub file_symbol_keys: HashMap<String, Vec<SymbolKey>>,
    pub file_entities_by_start: HashMap<String, SmallVec<[(u32, EntityId); 8]>>,
    /// Forward edges of the file dependency graph.
    pub dependency_forward: HashMap<String, HashSet<String>>,
    /// Reverse edges of the file dependency graph.
    pub dependency_reverse: HashMap<String, HashSet<String>>,
    /// Entity-level dependency graph snapshot.
    pub entity_dependency_graph: EntityDependencyGraph,
    /// Diagnostic counters snapshot (cloned, not shared).
    pub diagnostics_snapshot: Arc<RelationDiagnostics>,
}

impl CompactRelationIndex {
    /// Build a compact index from a live `RelationIndex`.
    ///
    /// Batches iteration to reduce lock contention: each `DashMap`/`RwLock`
    /// is iterated once and collected into a plain `HashMap`.
    pub fn from_relation_index(index: &RelationIndex) -> Self {
        let function_index = index
            .function_index
            .iter()
            .map(|e| (*e.key(), e.value().clone()))
            .collect::<HashMap<_, _>>();

        let resolved_relation_index = index
            .resolved_relation_index
            .iter()
            .map(|e| (*e.key(), e.value().clone()))
            .collect::<HashMap<_, _>>();

        let entity_file_index = index
            .entity_file_index
            .iter()
            .map(|e| (*e.key(), e.value().clone()))
            .collect::<HashMap<_, _>>();

        let file_relation_index = index
            .file_relation_index
            .iter()
            .map(|e| (e.key().clone(), e.value().clone()))
            .collect::<HashMap<_, _>>();

        let file_callers_by_callee = index
            .file_callers_by_callee
            .iter()
            .map(|e| (*e.key(), e.value().clone()))
            .collect::<HashMap<_, _>>();

        let file_records = index.file_records.read().clone();

        let symbol_key_to_entity = index.symbol_key_to_entity.read().clone();
        let entity_to_symbol_key = index.entity_to_symbol_key.read().clone();
        let stable_id_to_entity = index.stable_id_to_entity.read().clone();
        let file_symbol_keys = index.file_symbol_keys.read().clone();
        let file_entities_by_start = index.file_entities_by_start.read().clone();

        // Snapshot dependency graph edges via public API (get_all_files + get_dependencies/get_dependents)
        let mut dependency_forward: HashMap<String, HashSet<String>> = HashMap::new();
        let mut dependency_reverse: HashMap<String, HashSet<String>> = HashMap::new();
        for file in index.dependency_graph.get_all_files() {
            let deps = index.dependency_graph.get_dependencies(&file);
            if !deps.is_empty() {
                dependency_forward.insert(file.clone(), deps.into_iter().collect());
            }
            let dependents = index.dependency_graph.get_dependents(&file);
            if !dependents.is_empty() {
                dependency_reverse.insert(file.clone(), dependents.into_iter().collect());
            }
        }

        let entity_dependency_graph = index.entity_dependency_graph.read().clone();

        Self {
            function_index,
            resolved_relation_index,
            entity_file_index,
            file_relation_index,
            file_callers_by_callee,
            file_records,
            symbol_key_to_entity,
            entity_to_symbol_key,
            stable_id_to_entity,
            file_symbol_keys,
            file_entities_by_start,
            dependency_forward,
            dependency_reverse,
            entity_dependency_graph,
            diagnostics_snapshot: Arc::clone(&index.diagnostics),
        }
    }

    /// Reconstruct a `FileDependencyGraph` from the stored edge maps.
    pub fn to_dependency_graph(&self) -> FileDependencyGraph {
        let graph = FileDependencyGraph::new();
        for (from, tos) in &self.dependency_forward {
            for to in tos {
                graph.add_dependency(from, to);
            }
        }
        graph
    }

    /// Number of entities in the compact index.
    pub fn entity_count(&self) -> usize {
        self.function_index.len()
    }

    /// Number of resolved relations (sum of all caller vectors).
    pub fn relation_count(&self) -> usize {
        self.resolved_relation_index.values().map(|s| s.len()).sum()
    }

    /// Apply a `SnapshotDelta` to this compact index in place.
    ///
    /// Used by `UnifiedSnapshotIndex::merge_all` to materialize the merged view.
    /// This is a lightweight in-memory mutation that mirrors `RelationIndex::apply_delta`
    /// semantics without touching concurrent maps.
    pub fn apply_delta(&mut self, delta: &cce_types::SnapshotDelta) {
        use crate::index::delta::RelationDeltaOps;
        // Convert to a temporary RelationIndex, apply, then convert back.
        // This reuses the existing delta logic without duplicating it.
        let temp = self.to_relation_index();
        temp.apply_delta(delta);
        *self = Self::from_relation_index(&temp);
    }

    /// Convert back to a `RelationIndex` (e.g., for delta application or testing).
    pub fn to_relation_index(&self) -> RelationIndex {
        let index = RelationIndex::new();
        for (id, entity) in &self.function_index {
            index.insert_function(*id, entity.clone());
        }
        for (id, path) in &self.entity_file_index {
            index.entity_file_index.insert(*id, path.clone());
        }
        for (id, set) in &self.resolved_relation_index {
            index.resolved_relation_index.insert(*id, set.clone());
        }
        // Rebuild reverse callee index from forward edges, since compact does
        // not store it separately and direct insertion bypasses the incremental
        // maintenance in `add_resolved_relation`.
        index.rebuild_reverse_callee_index();
        for (path, set) in &self.file_relation_index {
            index.file_relation_index.insert(path.clone(), set.clone());
        }
        for (id, callers) in &self.file_callers_by_callee {
            index.file_callers_by_callee.insert(*id, callers.clone());
        }
        {
            let mut guard = index.file_records.write();
            *guard = self.file_records.clone();
        }
        {
            let mut guard = index.symbol_key_to_entity.write();
            *guard = self.symbol_key_to_entity.clone();
        }
        {
            let mut guard = index.entity_to_symbol_key.write();
            *guard = self.entity_to_symbol_key.clone();
        }
        {
            let mut guard = index.stable_id_to_entity.write();
            *guard = self.stable_id_to_entity.clone();
        }
        {
            let mut guard = index.file_symbol_keys.write();
            *guard = self.file_symbol_keys.clone();
        }
        {
            let mut guard = index.file_entities_by_start.write();
            *guard = self.file_entities_by_start.clone();
        }
        for (from, tos) in &self.dependency_forward {
            for to in tos {
                index.dependency_graph.add_dependency(from, to);
            }
        }
        {
            let mut guard = index.entity_dependency_graph.write();
            *guard = self.entity_dependency_graph.clone();
        }
        index
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::entity_index::EntityIndexOps;
    use cce_types::relation::CallContext;
    use cce_types::{EntityKind, RelationType, ResolvedRelation, Span};
    use std::collections::HashMap;

    fn make_entity(id: u64, name: &str) -> Entity {
        Entity {
            id: EntityId(id),
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
            stdlib_category: None,
            subtype: None,
        }
    }

    #[test]
    fn compact_roundtrip_preserves_counts() {
        let index = RelationIndex::new();
        index.add_function_with_path(EntityId(1), make_entity(1, "a"), "src/a.rs".to_string());
        index.add_function_with_path(EntityId(2), make_entity(2, "b"), "src/a.rs".to_string());
        index.add_resolved_relation(ResolvedRelation {
            caller: EntityId(1),
            callee_id: Some(EntityId(2)),
            callee_name: "b".to_string(),
            relation_type: RelationType::DirectCall,
            span: Span::default(),
            is_external: false,
            external_type: None,
            callee_symbol: None,
            stdlib_category: None,
            owner_type: None,
            call_context: CallContext::Direct,
            overload_signature: None,
        });
        index
            .dependency_graph
            .add_dependency("src/a.rs", "src/b.rs");

        let compact = CompactRelationIndex::from_relation_index(&index);
        assert_eq!(compact.entity_count(), 2);
        assert_eq!(compact.relation_count(), 1);
        assert_eq!(compact.entity_file_index.len(), 2);
        assert_eq!(compact.dependency_forward.len(), 1);

        let restored = compact.to_relation_index();
        assert_eq!(restored.function_index.len(), 2);
        assert_eq!(restored.resolved_relation_index.len(), 1);
    }
}
