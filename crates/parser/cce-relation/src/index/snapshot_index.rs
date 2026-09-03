//! Immutable snapshot views over a relation index.
//!
//! Two read-only views share the same query surface:
//!
//! - [`RelationSnapshotIndex`]: a frozen snapshot of a concrete
//!   [`RelationIndex`] (zero-copy via shared maps, or deep-copied on demand).
//! - [`LayeredSnapshotIndex`]: a base snapshot overlaid with an ordered chain
//!   of incremental deltas, merging them at read time.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use parking_lot::RwLock;
use smallvec::SmallVec;

use super::stores::FileRecord;
use super::stores::diagnostics::RelationDiagnostics;

use crate::dependency_graph::FileDependencyGraph;
use crate::index::core::{RelationEdgeSet, RelationIndex, SymbolKey};
use crate::index::view::{RelationIndexView, active_file_set, fingerprint_in_files_from_maps};
use crate::types::ExportInfo;
use cce_types::{Entity, EntityId, ImportTable, ResolvedRelation};
use dashmap::DashMap;

mod query_optimized;
mod snapshot_builder;

pub use query_optimized::{QueryOptimizedIndex, TransitiveFileDeps};

/// Immutable, query-only snapshot of a relation index.
///
/// This type is designed to be safely shared across threads without risk
/// of mutation. It is created by freezing a mutable `RelationIndex` and
/// exposes only read operations.
#[derive(Debug, Clone)]
pub struct RelationSnapshotIndex {
    /// Function index: EntityId -> Entity.
    pub(super) function_index: Arc<DashMap<EntityId, Entity>>,
    /// Name index: entity name -> EntityId list, shared from the source
    /// `RelationIndex` so name lookups stay O(1) on the snapshot.
    pub(super) name_index: Arc<DashMap<String, SmallVec<[EntityId; 2]>>>,
    pub(super) entity_file_index: Arc<DashMap<EntityId, String>>,
    pub(super) resolved_relation_index: Arc<DashMap<EntityId, RelationEdgeSet>>,
    pub(super) reverse_callee_index: Arc<DashMap<EntityId, Vec<EntityId>>>,
    pub(super) file_relation_index: Arc<DashMap<String, RelationEdgeSet>>,
    pub(super) file_callers_by_callee: Arc<DashMap<EntityId, HashSet<String>>>,
    /// Unified file-level metadata shared with the source `RelationIndex`.
    pub(super) file_records: Arc<RwLock<HashMap<String, FileRecord>>>,
    pub(super) dependency_graph: Arc<FileDependencyGraph>,
    pub(super) symbol_key_to_entity: Arc<RwLock<HashMap<SymbolKey, EntityId>>>,
    pub(super) entity_to_symbol_key: Arc<RwLock<HashMap<EntityId, SymbolKey>>>,
    pub(super) stable_id_to_entity: Arc<RwLock<HashMap<String, EntityId>>>,
    /// File -> symbol keys reverse index shared with the source
    /// `RelationIndex`; backs O(scope) `stable_symbol_keys_in_files`.
    pub(super) file_symbol_keys: Arc<RwLock<HashMap<String, Vec<SymbolKey>>>>,
    /// File -> ordered (start row, EntityId) index shared with the source
    /// `RelationIndex`; backs O(entities-of-file) `entities_of_file`.
    pub(super) file_entities_by_start: super::FileEntitiesByStart,
    /// Grouped diagnostics shared with the source `RelationIndex`.
    pub(super) diagnostics: Arc<RelationDiagnostics>,
    /// Version of the source `RelationIndex` at snapshot creation time.
    /// Used for staleness detection and CoW snapshot creation.
    pub(super) version: u64,
    pub(super) query_optimized: Arc<QueryOptimizedIndex>,
    pub(super) transitive_deps: Arc<TransitiveFileDeps>,
}

impl RelationSnapshotIndex {
    /// Create a snapshot index from a mutable `RelationIndex`.
    ///
    /// This deep-copies the whole index and is only required when the source
    /// index stays alive and keeps being mutated after publication (the
    /// full-index builder is cleared on the next run). Cold-start and
    /// compaction paths own their index exclusively and should use
    /// [`Self::from_index_shared`] instead.
    pub fn from_index(index: &RelationIndex) -> Self {
        let index = index.detached_clone();
        Self::from_index_shared(&index)
    }

    /// Create a snapshot index by taking ownership of a `RelationIndex`.
    ///
    /// Uses `snapshot_take` to drain the source index cheaply (O(1) per map
    /// instead of O(entries)). The source is left empty after this call.
    /// Prefer this over [`from_index`] when you own the index exclusively
    /// and will drop it immediately afterward.
    pub fn from_index_owned(index: &mut RelationIndex) -> Self {
        let index = index.snapshot_take();
        Self::from_index_shared(&index)
    }

    /// Create a zero-copy snapshot view over an existing `RelationIndex`.
    ///
    /// The snapshot shares the underlying maps with `index` instead of
    /// deep-copying them. The caller must guarantee that `index` is never
    /// mutated while the returned snapshot (or anything derived from it) is
    /// alive. This is the contract of the process-internal base cache used by
    /// hot updates: the cache is the canonical owner and only
    /// hands out read-only shallow clones or deep-copied working copies.
    pub fn from_index_shared(index: &RelationIndex) -> Self {
        let query_optimized = Arc::new(QueryOptimizedIndex::from_relation_index(index));
        let transitive_deps = Arc::new(TransitiveFileDeps::from_relation_index(index, 10));
        Self {
            function_index: Arc::clone(&index.function_index),
            name_index: Arc::clone(&index.name_index),
            entity_file_index: Arc::clone(&index.entity_file_index),
            resolved_relation_index: Arc::clone(&index.resolved_relation_index),
            reverse_callee_index: Arc::clone(&index.reverse_callee_index),
            file_relation_index: Arc::clone(&index.file_relation_index),
            file_callers_by_callee: Arc::clone(&index.file_callers_by_callee),
            file_records: Arc::clone(&index.file_records),
            dependency_graph: Arc::clone(&index.dependency_graph),
            symbol_key_to_entity: Arc::clone(&index.symbol_key_to_entity),
            entity_to_symbol_key: Arc::clone(&index.entity_to_symbol_key),
            stable_id_to_entity: Arc::clone(&index.stable_id_to_entity),
            file_symbol_keys: Arc::clone(&index.file_symbol_keys),
            file_entities_by_start: Arc::clone(&index.file_entities_by_start),
            diagnostics: Arc::clone(&index.diagnostics),
            version: index.version(),
            query_optimized,
            transitive_deps,
        }
    }

    /// Version of the source `RelationIndex` at snapshot creation time.
    ///
    /// Useful for staleness detection: if the source index's current version
    /// is greater than this value, the snapshot is stale and a new snapshot
    /// should be created.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Check whether this snapshot is stale relative to a source index.
    ///
    /// Returns `true` if the source index's current version is greater than
    /// the version recorded at snapshot creation time, indicating that the
    /// source has been mutated since the snapshot was taken.
    pub fn is_stale(&self, source_version: u64) -> bool {
        source_version > self.version
    }

    /// Create a fresh snapshot from a stale one by deep-copying the source.
    ///
    /// This is the safe fallback for CoW: when the source has been mutated
    /// since the snapshot was taken, a full deep copy is required because
    /// the snapshot's shared maps may contain stale data. For better
    /// performance on small mutations, consider using `LayeredSnapshotIndex`
    /// with an explicit `SnapshotDelta`.
    pub fn cow_from_stale(source: &RelationIndex) -> Self {
        Self::from_index(source)
    }

    pub fn transitive_dependents_of(&self, file_path: &str) -> Vec<String> {
        self.transitive_deps.transitive_dependents_of(file_path)
    }

    pub fn transitive_dependencies_of(&self, file_path: &str) -> Vec<String> {
        self.transitive_deps.transitive_dependencies_of(file_path)
    }

    /// Get entity count.
    pub fn function_count(&self) -> usize {
        self.function_index.len()
    }

    /// Access diagnostics.
    pub fn diagnostics(&self) -> &RelationDiagnostics {
        &self.diagnostics
    }

    /// Get quality report for this snapshot.
    pub fn quality_report(&self) -> crate::index::core::QualityReport {
        let summary = self.diagnostics.summary();
        let quality_score = self.diagnostics.quality_score(self.function_index.len());
        crate::index::core::QualityReport {
            summary,
            quality_score,
            file_count: self.file_records.read().len(),
            entity_count: self.function_index.len(),
            relation_count: self.resolved_relation_count(),
        }
    }

    /// Access the unified file records map.
    pub fn file_records(&self) -> &RwLock<HashMap<String, FileRecord>> {
        &self.file_records
    }

    /// Get relation count (sum of all caller relation vectors).
    pub fn resolved_relation_count(&self) -> usize {
        self.resolved_relation_index
            .iter()
            .map(|entry| entry.len())
            .sum()
    }

    /// Compute fingerprint over the snapshot contents.
    ///
    /// Byte-identical to [`RelationIndex::compute_fingerprint`] over the same
    /// maps: canonical components are built from stable symbol keys + raw
    /// targets and hashed through `fingerprint_from_components`; runtime
    /// entity IDs never participate.
    pub fn compute_fingerprint(&self) -> String {
        let files = active_file_set(&self.file_records, &self.entity_file_index);
        fingerprint_in_files_from_maps(
            &self.function_index,
            &self.entity_file_index,
            &self.entity_to_symbol_key,
            &self.resolved_relation_index,
            &self.file_relation_index,
            &self.file_records,
            &self.dependency_graph,
            &files,
        )
    }
}

/// Immutable base snapshot overlaid with an ordered chain of incremental deltas.
///
/// Queries merge `base` + `deltas` at read time, avoiding a full rebuild
/// on every hot update. When `deltas` is empty, queries pass through
/// directly to `base`.
#[derive(Debug, Clone)]
pub struct LayeredSnapshotIndex {
    pub base: Arc<RelationSnapshotIndex>,
    pub deltas: Vec<Arc<cce_types::SnapshotDelta>>,
}

impl LayeredSnapshotIndex {
    pub fn new(base: Arc<RelationSnapshotIndex>) -> Self {
        Self {
            base,
            deltas: Vec::new(),
        }
    }

    pub fn with_delta(
        base: Arc<RelationSnapshotIndex>,
        delta: Arc<cce_types::SnapshotDelta>,
    ) -> Self {
        Self {
            base,
            deltas: vec![delta],
        }
    }

    /// Create a layered snapshot with an ordered delta chain.
    pub fn with_deltas(
        base: Arc<RelationSnapshotIndex>,
        deltas: Vec<Arc<cce_types::SnapshotDelta>>,
    ) -> Self {
        Self { base, deltas }
    }

    /// Create an empty layered snapshot (no base data, no deltas).
    pub fn empty() -> Self {
        Self::new(Arc::new(RelationSnapshotIndex::from_index_shared(
            &RelationIndex::new(),
        )))
    }

    /// Files whose entity or relation content was modified by the delta chain.
    ///
    /// Returns the union of:
    /// - Files added or removed by any delta
    /// - Files whose entities were added or removed by any delta
    /// - Files whose relations or imports/exports were diffed by any delta
    ///
    /// Useful for file-scoped snapshot creation: only these files need to be
    /// re-queried when building a file-scoped view.
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

    /// Create a file-scoped snapshot that only includes data for the specified files.
    ///
    /// This is useful for incremental processing where you only need to
    /// snapshot certain files. The returned `LayeredSnapshotIndex` shares
    /// the same base and deltas but can be filtered at query time.
    ///
    /// Note: This creates a filtered view, not a new snapshot. The base
    /// and deltas are still shared. For a truly independent file-scoped
    /// snapshot, use `materialize_merged_index()` and then create a new
    /// snapshot from the materialized index.
    pub fn file_scoped(&self, files: &HashSet<String>) -> FileScopedSnapshot<'_> {
        FileScopedSnapshot {
            layered: self,
            files: files.clone(),
        }
    }

    /// Number of entities visible through this layer.
    pub fn function_count(&self) -> usize {
        let mut count = self.base.function_count() as i64;
        for d in &self.deltas {
            count += d.added_entities.len() as i64;
            count -= d.removed_entities.len() as i64;
        }
        count.max(0) as usize
    }

    /// Number of relations visible through this layer.
    ///
    /// Delta-aware: the base count minus the edges owned by removed entities
    /// and the explicitly removed edges, plus the added edges. A delta never
    /// removes edges owned by a removed caller (they are dropped wholesale
    /// with the caller), and `removed_relations` never contains them either,
    /// so the two subtractions are disjoint across deltas.
    pub fn resolved_relation_count(&self) -> usize {
        let mut count = self.base.resolved_relation_count() as i64;
        for d in &self.deltas {
            let owned_by_removed: usize = d
                .removed_entities
                .iter()
                .filter_map(|id| self.base.resolved_relation_index.get(id))
                .map(|entry| entry.len())
                .sum();
            count -= owned_by_removed as i64;
            count -= d.removed_relations.len() as i64;
            count += d.added_relations.len() as i64;
        }
        count.max(0) as usize
    }

    /// Check whether an entity exists, respecting removals/additions across
    /// the entire delta chain.
    pub fn contains_entity(&self, id: EntityId) -> bool {
        // Walk deltas from last to first: the most recent delta wins.
        for d in self.deltas.iter().rev() {
            if d.removed_entities.contains(&id) {
                return false;
            }
            if d.added_entities.iter().any(|added| added.entity.id == id) {
                return true;
            }
        }
        self.base.function_index.contains_key(&id)
    }

    /// Check whether a file is active in the final state, respecting
    /// removals/additions across the entire delta chain (last operation wins).
    pub(crate) fn is_file_active(&self, file_id: &str) -> bool {
        let mut active = self.base.file_records.read().contains_key(file_id);
        for d in &self.deltas {
            if d.removed_files.iter().any(|f| f == file_id) {
                active = false;
            }
            if d.added_files
                .iter()
                .any(|f| f.path == file_id || f.id == file_id)
            {
                active = true;
            }
        }
        active
    }

    /// Compute the merged fingerprint of the visible layer (base + deltas).
    ///
    /// Verification-only: never on the per-request query path. The merged
    /// canonical snapshot is produced read-only over the base and the deltas.
    pub fn compute_fingerprint(&self) -> String {
        if let Ok(snapshot) = self.to_canonical_snapshot(String::new()) {
            return snapshot.fingerprint();
        }

        // Fallback: materialize the merged index and fingerprint it with
        // stable symbol keys (never runtime entity IDs).
        let merged = self.materialize_merged_index();
        let files = active_file_set(&merged.file_records, &merged.entity_file_index);
        merged.fingerprint_in_files(&files)
    }

    /// Deterministic, file-scoped fingerprint of the visible layer.
    ///
    /// Verification-only. Materializes base + deltas into a concrete index
    /// (identical to `apply_delta` semantics) and delegates, so the result is
    /// byte-identical to the materialized index fingerprint over the same
    /// file set.
    pub fn fingerprint_in_files(&self, files: &HashSet<String>) -> String {
        let merged = self.materialize_merged_index();
        merged.fingerprint_in_files(files)
    }

    /// Get quality report for the merged view.
    pub fn quality_report(&self) -> crate::index::core::QualityReport {
        self.base.quality_report()
    }
}

/// A file-scoped view over a `LayeredSnapshotIndex`.
///
/// Created by `LayeredSnapshotIndex::file_scoped()`, this struct provides
/// the same query surface as `RelationIndexView` but filters results to
/// only include data for the specified files. Useful for incremental
/// processing where you only need to snapshot certain files.
pub struct FileScopedSnapshot<'a> {
    layered: &'a LayeredSnapshotIndex,
    files: HashSet<String>,
}

impl<'a> FileScopedSnapshot<'a> {
    /// The set of files this snapshot is scoped to.
    pub fn scoped_files(&self) -> &HashSet<String> {
        &self.files
    }

    /// Check if a file is in scope.
    pub fn contains_file(&self, path: &str) -> bool {
        self.files.contains(path)
    }

    /// Get entities for a specific file.
    pub fn entities_of_file(&self, path: &str) -> Vec<cce_types::Entity> {
        if self.files.contains(path) {
            self.layered.entities_of_file(path)
        } else {
            Vec::new()
        }
    }

    /// Get relations for a specific file.
    pub fn file_relations_of(&self, path: &str) -> Vec<ResolvedRelation> {
        if self.files.contains(path) {
            self.layered.file_relations_of(path)
        } else {
            Vec::new()
        }
    }

    /// Get imports for a specific file.
    pub fn imports_of(&self, path: &str) -> Option<ImportTable> {
        if self.files.contains(path) {
            self.layered.imports_of(path)
        } else {
            None
        }
    }

    /// Get exports for a specific file.
    pub fn exports_of(&self, path: &str) -> Option<Vec<ExportInfo>> {
        if self.files.contains(path) {
            self.layered.exports_of(path)
        } else {
            None
        }
    }

    /// Get all entities across scoped files.
    pub fn entities_by_file(&self) -> HashMap<String, Vec<cce_types::Entity>> {
        let mut result = HashMap::new();
        for path in &self.files {
            let entities = self.layered.entities_of_file(path);
            if !entities.is_empty() {
                result.insert(path.clone(), entities);
            }
        }
        result
    }

    /// Iterate over all scoped files and their entities.
    pub fn for_each_file_entity<F: FnMut(&str, &[cce_types::Entity])>(&self, mut f: F) {
        for path in &self.files {
            let entities = self.layered.entities_of_file(path);
            if !entities.is_empty() {
                f(path, &entities);
            }
        }
    }
}
