//! Unified snapshot index merging base and incremental deltas.
//!
//! `UnifiedSnapshotIndex` replaces the previous two-layer hierarchy
//! (`RelationSnapshotIndex` → `LayeredSnapshotIndex`) with a single type that
//! holds a compact base and an ordered delta chain. Query-optimized indexes
//! are precomputed from the merged view so per-query work is O(1).

use std::collections::HashSet;
use std::sync::Arc;

use cce_types::{EntityId, SnapshotDelta};

use super::compact::CompactRelationIndex;
use super::core::RelationIndex;
use super::snapshot_index::{QueryOptimizedIndex, TransitiveFileDeps};

/// Unified immutable snapshot.
///
/// Holds a compact base, an ordered delta chain, and precomputed
/// query-optimized indexes over the merged view.
#[derive(Debug, Clone)]
pub struct UnifiedSnapshotIndex {
    /// Compact base snapshot (immutable).
    pub base: Arc<CompactRelationIndex>,
    /// Ordered delta chain applied on top of `base`.
    pub deltas: Vec<Arc<SnapshotDelta>>,
    /// Merged compact view (base + deltas applied) cached for O(1) queries.
    merged: Arc<CompactRelationIndex>,
    /// Query-optimized caller/callee maps over `merged`.
    pub query_optimized: Arc<QueryOptimizedIndex>,
    /// Precomputed transitive file deps over `merged`.
    pub transitive_deps: Arc<TransitiveFileDeps>,
}

impl UnifiedSnapshotIndex {
    /// Create from a compact base with no deltas.
    pub fn from_base(base: CompactRelationIndex) -> Self {
        let query_optimized = Arc::new(QueryOptimizedIndex::from_compact_index(&base));
        let transitive_deps = Arc::new(TransitiveFileDeps::from_compact_index(&base, 10));
        let base = Arc::new(base);
        let merged = Arc::clone(&base);
        Self {
            base,
            deltas: Vec::new(),
            merged,
            query_optimized,
            transitive_deps,
        }
    }

    /// Create from a live `RelationIndex` (compacted internally).
    pub fn from_relation_index(index: &RelationIndex) -> Self {
        Self::from_base(CompactRelationIndex::from_relation_index(index))
    }

    /// Create an empty snapshot.
    pub fn empty() -> Self {
        Self::from_base(CompactRelationIndex::default())
    }

    /// Apply a delta, returning a new snapshot with rebuilt indexes.
    ///
    /// The merged compact is recomputed by applying the new delta onto the
    /// previous merged view; query indexes are rebuilt from the new merged
    /// compact.
    pub fn apply_delta(&self, delta: SnapshotDelta) -> Self {
        let mut new_deltas = self.deltas.clone();
        new_deltas.push(Arc::new(delta));
        let mut merged = (*self.merged).clone();
        // Apply only the new delta onto the previous merged (incremental).
        let last = new_deltas.last().expect("just pushed");
        merged.apply_delta(last);
        let query_optimized = Arc::new(QueryOptimizedIndex::from_compact_index(&merged));
        let transitive_deps = Arc::new(TransitiveFileDeps::from_compact_index(&merged, 10));
        Self {
            base: Arc::clone(&self.base),
            deltas: new_deltas,
            merged: Arc::new(merged),
            query_optimized,
            transitive_deps,
        }
    }

    /// Merge all layers into a single compact snapshot.
    pub fn merge_all(&self) -> CompactRelationIndex {
        (*self.merged).clone()
    }

    /// Number of entities in the merged view.
    pub fn function_count(&self) -> usize {
        self.merged.function_index.len()
    }

    /// Number of relations in the merged view.
    pub fn resolved_relation_count(&self) -> usize {
        self.merged
            .resolved_relation_index
            .values()
            .map(|s| s.len())
            .sum()
    }

    /// Check whether an entity exists in the merged view.
    pub fn contains_entity(&self, id: EntityId) -> bool {
        self.merged.function_index.contains_key(&id)
    }

    /// Check whether a file is active in the merged view.
    pub fn is_file_active(&self, file_id: &str) -> bool {
        self.merged.file_records.contains_key(file_id)
    }

    /// Files affected by the delta chain.
    pub fn files_affected_by_deltas(&self) -> HashSet<String> {
        let mut affected = HashSet::new();
        for d in &self.deltas {
            for f in &d.removed_files {
                affected.insert(f.clone());
            }
            for f in &d.added_files {
                affected.insert(f.path.clone());
            }
            for e in &d.added_entities {
                affected.insert(e.file_path.clone());
            }
            for diff in &d.import_diffs {
                affected.insert(diff.file_path.clone());
            }
            for diff in &d.export_diffs {
                affected.insert(diff.file_path.clone());
            }
            for diff in &d.file_relation_diffs {
                affected.insert(diff.file_path.clone());
            }
            for diff in &d.dependency_diffs {
                affected.insert(diff.source_file.clone());
                affected.extend(diff.added_dependencies.iter().cloned());
            }
        }
        affected
    }

    /// Access the merged compact index.
    pub fn merged_compact(&self) -> &CompactRelationIndex {
        &self.merged
    }

    /// Compute fingerprint over the merged view.
    pub fn compute_fingerprint(&self) -> String {
        // Delegate to a temporary RelationIndex to reuse existing fingerprint logic.
        let temp = self.merged.to_relation_index();
        temp.compute_fingerprint()
    }

    /// Quality report for the merged view.
    pub fn quality_report(&self) -> super::core::QualityReport {
        let temp = self.merged.to_relation_index();
        temp.quality_report()
    }
}

// ---------------------------------------------------------------------------
// Compatibility shims: allow existing code using LayeredSnapshotIndex /
// RelationSnapshotIndex to migrate gradually. UnifiedSnapshotIndex can be
// converted to/from the legacy types via the compact representation.
// ---------------------------------------------------------------------------

impl From<UnifiedSnapshotIndex> for super::snapshot_index::LayeredSnapshotIndex {
    fn from(unified: UnifiedSnapshotIndex) -> Self {
        // Materialize the merged compact into a RelationIndex, then wrap as
        // LayeredSnapshotIndex with no deltas (already merged).
        let merged_index = unified.merged.to_relation_index();
        let base_snapshot = Arc::new(
            super::snapshot_index::RelationSnapshotIndex::from_index_shared(&merged_index),
        );
        super::snapshot_index::LayeredSnapshotIndex::new(base_snapshot)
    }
}

impl From<super::snapshot_index::LayeredSnapshotIndex> for UnifiedSnapshotIndex {
    fn from(layered: super::snapshot_index::LayeredSnapshotIndex) -> Self {
        let merged = layered.materialize_merged_index();
        Self::from_relation_index(&merged)
    }
}

/// Snapshot manager for unified snapshots.
///
/// Holds the active snapshot and a bounded history, matching the plan's
/// `SnapshotManager` design.
pub struct SnapshotManager {
    active: parking_lot::RwLock<Option<Arc<UnifiedSnapshotIndex>>>,
    history: parking_lot::RwLock<std::collections::VecDeque<Arc<UnifiedSnapshotIndex>>>,
    max_history: usize,
}

impl SnapshotManager {
    pub fn new() -> Self {
        Self {
            active: parking_lot::RwLock::new(None),
            history: parking_lot::RwLock::new(std::collections::VecDeque::new()),
            max_history: 10,
        }
    }

    pub fn with_capacity(max_history: usize) -> Self {
        Self {
            active: parking_lot::RwLock::new(None),
            history: parking_lot::RwLock::new(std::collections::VecDeque::new()),
            max_history,
        }
    }

    pub fn publish(&self, snapshot: UnifiedSnapshotIndex) {
        let snapshot = Arc::new(snapshot);
        let mut active = self.active.write();
        if let Some(prev) = active.take() {
            let mut history = self.history.write();
            history.push_back(prev);
            if history.len() > self.max_history {
                history.pop_front();
            }
        }
        *active = Some(snapshot);
    }

    pub fn get_active(&self) -> Option<Arc<UnifiedSnapshotIndex>> {
        self.active.read().clone()
    }

    pub fn history_len(&self) -> usize {
        self.history.read().len()
    }

    pub fn clear(&self) {
        *self.active.write() = None;
        self.history.write().clear();
    }
}

impl Default for SnapshotManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::entity_index::EntityIndexOps;
    use cce_types::relation::CallContext;
    use cce_types::{Entity, EntityKind, RelationType, ResolvedRelation, Span};
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
    fn unified_from_base_and_apply_delta() {
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

        let unified = UnifiedSnapshotIndex::from_relation_index(&index);
        assert_eq!(unified.function_count(), 2);
        assert_eq!(unified.resolved_relation_count(), 1);
        assert!(unified.query_optimized.get_callees(EntityId(1)).is_some());
        assert_eq!(
            unified
                .query_optimized
                .get_callers(EntityId(2))
                .unwrap()
                .len(),
            1
        );

        // Apply an empty delta (should preserve state).
        let delta = cce_types::SnapshotDelta {
            epoch: 0,
            base_epoch: 0,
            config_fingerprint: String::new(),
            removed_files: Vec::new(),
            added_files: Vec::new(),
            removed_entities: Vec::new(),
            added_entities: Vec::new(),
            removed_relations: Vec::new(),
            added_relations: Vec::new(),
            file_relation_diffs: Vec::new(),
            import_diffs: Vec::new(),
            export_diffs: Vec::new(),
            dependency_diffs: Vec::new(),
            relation_edges_dropped_unbounded: 0,
            renamed_entities: Vec::new(),
        };
        let unified2 = unified.apply_delta(delta);
        assert_eq!(unified2.function_count(), 2);
        assert_eq!(unified2.deltas.len(), 1);
    }

    #[test]
    fn snapshot_manager_publish_and_history() {
        let manager = SnapshotManager::new();
        let snap1 = UnifiedSnapshotIndex::empty();
        manager.publish(snap1);
        assert!(manager.get_active().is_some());
        assert_eq!(manager.history_len(), 0);
        let snap2 = UnifiedSnapshotIndex::empty();
        manager.publish(snap2);
        assert_eq!(manager.history_len(), 1);
    }
}
