//! Relation index for call chain queries
//!
//! Provides data structures for storing and indexing function call relationships.
//! This module defines the core index structures used by the query module.
//!
//! The relation index contains 5 core DashMap indexes (thread-safe):
//! 1. Function index: EntityId -> Entity (function-like entities)
//! 2. Call index: caller ID -> call relations
//! 3. Import index: file ID -> import table
//! 4. File index: file ID -> file info
//! 5. Export index: file ID -> export list
//! 6. Resolved relation index: caller EntityId -> resolved relations
//!
//! # Module Organization
//!
//! The functionality is split across several extension traits:
//! - [`EntityIndexOps`]: Entity-related operations
//! - [`RelationQueryOps`]: Query operations
//! - [`HierarchyQueryOps`]: Hierarchy queries
//! - [`FrontendQueryOps`]: Frontend component queries
//! - [`FileIndexOps`]: File metadata operations
//! - [`ImportIndexOps`]: Import operations
//! - [`ExportIndexOps`]: Export operations
//! - [`FileLevelOps`]: File-level operations
//! - [`RelationDeltaOps`]: Delta computation and application
//!
//! [`EntityIndexOps`]: super::entity_index::EntityIndexOps
//! [`RelationQueryOps`]: super::relation_query::RelationQueryOps
//! [`HierarchyQueryOps`]: super::relation_query::HierarchyQueryOps
//! [`FrontendQueryOps`]: super::relation_query::FrontendQueryOps
//! [`FileIndexOps`]: super::file_index::FileIndexOps
//! [`ImportIndexOps`]: super::file_index::ImportIndexOps
//! [`ExportIndexOps`]: super::file_index::ExportIndexOps
//! [`FileLevelOps`]: super::file_index::FileLevelOps
//! [`RelationDeltaOps`]: super::delta::RelationDeltaOps

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::RwLock;
use smallvec::SmallVec;

use super::snapshot_generation::{CoWSnapshotGuard, SnapshotGeneration};
use super::stores::FileRecord;
use super::stores::diagnostics::RelationDiagnostics;

use crate::dependency_graph::{EntityDependencyGraph, FileDependencyGraph};
use crate::index::snapshot_index::RelationSnapshotIndex;
use crate::index::view::{RelationIndexView, active_file_set};
use cce_metrics::domain::pipeline::RelationMetrics;
use cce_types::{
    CanonicalRelationSnapshot, Entity, EntityId, ResolvedRelation, SYMBOL_KEY_CONFLICT_SAMPLE_CAP,
    SymbolKeyConflictRecord,
};
use dashmap::DashMap;

mod edge_set;
pub use edge_set::{
    FileRelationRecord, QualityReport, RelationEdgeIdentity, RelationEdgeKind, RelationEdgeSet,
    relation_identity,
};

mod snapshot;

#[cfg(test)]
mod tests;

// Re-export types from the types module
pub use super::super::types::{CallChainNode, CallChainPath, ExportInfo, ExportType};

/// Stable symbol key for cross-session entity identification.
///
/// Composed of `(file_path, scoped_name, kind)` triple, uniquely identifying
/// a symbol within a project. Unlike `EntityId` (process-local counter),
/// `SymbolKey` survives restarts because it is derived from source-code
/// properties rather than a volatile counter.
pub type SymbolKey = cce_types::StableSymbolKey;

/// Relation index for call chain queries
///
/// This index is thread-safe and can be shared across threads without locks.
/// It uses DashMap for all internal maps, which provides better concurrency
/// than RwLock<HashMap>.
///
/// # Optimization Notes
///
/// This structure has been optimized to reduce memory redundancy:
/// - Removed `function_name_index` - computed on-demand from `function_index`
/// - Added reverse index embedded in `RelationEdgeSet` for efficient reverse lookups (callee -> callers)
/// - Removed `group_relation_index` and `reverse_group_relation_index` - computed on-demand from relations
///
/// # Usage
///
/// The core methods are available directly on `RelationIndex`. Additional
/// functionality is provided through extension traits:
///
/// ```ignore
/// use crate::index::{
/// RelationIndex, EntityIndexOps, RelationQueryOps, SmartQueryOps
/// };
///
/// let index = RelationIndex::new();
///
/// // Core methods
/// index.add_resolved_relation(relation);
///
/// // Extension trait methods
/// index.add_function(entity_id, entity); // EntityIndexOps
/// let callers = index.get_callers_by_callee_entity(callee_id); // RelationQueryOps
/// ```
// Manual Clone impl (AtomicU64 is not Clone).
#[derive(Debug)]
pub struct RelationIndex {
    /// Function index: EntityId -> Entity (stores function-like entities)
    /// This is the primary source of truth for function entities
    pub(super) function_index: Arc<DashMap<EntityId, Entity>>,

    /// Name index: entity name -> EntityId list (inverted index over
    /// `function_index`). Enables O(1) name lookups instead of full-index
    /// scans; maintained on every function insert/remove.
    /// SmallVec inline capacity of 2 covers the typical 1-2 entities per name
    /// without heap allocation.
    pub(super) name_index: Arc<DashMap<String, SmallVec<[EntityId; 2]>>>,

    /// Entity file index: EntityId -> file path
    /// Used for tracking which file an entity belongs to, enabling file-level operations
    pub(super) entity_file_index: Arc<DashMap<EntityId, String>>,

    /// Resolved relation index: caller EntityId -> RelationEdgeSet (vector + dedup set)
    /// This is the primary source of truth for relations (forward index)
    pub(super) resolved_relation_index: Arc<DashMap<EntityId, RelationEdgeSet>>,

    /// Reverse index: callee EntityId -> sorted list of caller EntityIds.
    /// Maintained alongside `resolved_relation_index` for O(1) reverse lookups
    /// without scanning the forward index. In-memory derived, rebuilt from
    /// forward edges on snapshot load.
    pub(super) reverse_callee_index: Arc<DashMap<EntityId, Vec<EntityId>>>,

    /// File-scoped relation index: normalized file path -> file-level
    /// RelationEdgeSet (imports, uses, module-level calls). File-level
    /// edges are attributed to the file instead of a placeholder entity, so
    /// entity-scoped queries and `function_index` never see them.
    pub(super) file_relation_index: Arc<DashMap<String, RelationEdgeSet>>,

    /// Reverse index over `file_relation_index`: callee EntityId ->
    /// caller file paths (unique, unordered). Lets hot-update
    /// dependency propagation resolve file-level callers with O(1) lookups
    /// instead of scanning every file-level edge. In-memory derived,
    /// maintained alongside `file_relation_index` mutations.
    pub(super) file_callers_by_callee: Arc<DashMap<EntityId, HashSet<String>>>,

    /// Unified file-level metadata: file_id -> FileRecord (info + imports + exports).
    /// Replaces the separate `file_index`, `import_index`, and `export_index` maps,
    /// eliminating two redundant String key copies and two extra heap allocations per file.
    pub(super) file_records: Arc<RwLock<HashMap<String, FileRecord>>>,

    /// File dependency graph for tracking cross-file dependencies
    /// Used during hot updates to determine which files need reprocessing
    pub dependency_graph: Arc<FileDependencyGraph>,

    /// Entity-level dependency graph for precise impact analysis.
    pub entity_dependency_graph: Arc<RwLock<EntityDependencyGraph>>,

    /// Symbol key -> EntityId mapping (stable cross-session identifier)
    /// Populated during index building and snapshot loading.
    pub(super) symbol_key_to_entity: Arc<RwLock<HashMap<SymbolKey, EntityId>>>,

    /// EntityId -> SymbolKey reverse mapping
    pub(super) entity_to_symbol_key: Arc<RwLock<HashMap<EntityId, SymbolKey>>>,

    /// StableSymbolId -> EntityId side map for O(1) stable-ID lookups.
    /// Maintained alongside `symbol_key_to_entity`.
    pub(super) stable_id_to_entity: Arc<RwLock<HashMap<String, EntityId>>>,

    /// Monotonic counter for globally unique entity ID allocation.
    /// Initialized to 0 for new indexes; set to `max_id + 1` after snapshot loading.
    /// Shared via `Arc` so that cloned `RelationIndex` instances (used across
    /// concurrent re-parse tasks) all increment the same counter.
    pub(super) entity_id_counter: Arc<AtomicU64>,

    /// Per-file entity ID remappings: normalized_file_path -> (parsed_file_local -> global).
    /// Populated by `index_file_core` and consumed by `process_relations`.
    pub(super) entity_id_remaps: Arc<RwLock<HashMap<String, HashMap<EntityId, EntityId>>>>,

    /// Grouped diagnostics (counters + samples + metrics sink).
    /// See `crate::index::stores::diagnostics::RelationDiagnostics`.
    pub(super) diagnostics: Arc<RelationDiagnostics>,

    /// Reverse index for `stable_symbol_keys_in_files`: file -> symbol keys.
    /// O(scope) aggregation instead of scanning the full `symbol_key_to_entity`.
    /// In-memory derived, not persisted — rebuilt via `register_symbol_key`.
    pub(super) file_symbol_keys: Arc<RwLock<HashMap<String, Vec<SymbolKey>>>>,

    /// File -> ordered entity start rows for `get_entities_in_line_range`.
    /// Each file's SmallVec is kept sorted by `span.start_position.row`.
    /// O(log n + k) range queries instead of O(n) full scan.
    /// In-memory derived, rebuilt via `add_function_with_path` / `remove_function`.
    /// SmallVec inline capacity of 8 covers typical 3-10 entities per file.
    pub(super) file_entities_by_start: super::FileEntitiesByStart,

    /// Shared generation counter for snapshot CoW tracking.
    /// Incremented on every mutation; snapshots record the generation at
    /// creation time so callers can detect stale snapshots.
    pub(super) generation: Arc<SnapshotGeneration>,

    /// Tracks which files were affected by the most recent mutation(s).
    /// Written by mutating entry points; read and cleared by
    /// `CowLayeredSnapshot::refresh()` to enable selective CoW copy.
    /// `None` means no file-level information is available (fallback to
    /// full copy).
    pub(super) last_affected_files: std::sync::Mutex<Option<HashSet<String>>>,
}

impl Clone for RelationIndex {
    fn clone(&self) -> Self {
        Self {
            function_index: Arc::clone(&self.function_index),
            name_index: Arc::clone(&self.name_index),
            entity_file_index: Arc::clone(&self.entity_file_index),
            resolved_relation_index: Arc::clone(&self.resolved_relation_index),
            reverse_callee_index: Arc::clone(&self.reverse_callee_index),
            file_relation_index: Arc::clone(&self.file_relation_index),
            file_callers_by_callee: Arc::clone(&self.file_callers_by_callee),
            file_records: Arc::clone(&self.file_records),
            dependency_graph: Arc::clone(&self.dependency_graph),
            entity_dependency_graph: Arc::clone(&self.entity_dependency_graph),
            symbol_key_to_entity: Arc::clone(&self.symbol_key_to_entity),
            entity_to_symbol_key: Arc::clone(&self.entity_to_symbol_key),
            stable_id_to_entity: Arc::clone(&self.stable_id_to_entity),
            entity_id_counter: Arc::clone(&self.entity_id_counter),
            entity_id_remaps: Arc::clone(&self.entity_id_remaps),
            diagnostics: Arc::clone(&self.diagnostics),
            file_symbol_keys: Arc::clone(&self.file_symbol_keys),
            file_entities_by_start: Arc::clone(&self.file_entities_by_start),
            generation: Arc::clone(&self.generation),
            last_affected_files: std::sync::Mutex::new(None),
        }
    }
}

impl Default for RelationIndex {
    fn default() -> Self {
        Self::new()
    }
}

/// Core methods for RelationIndex
///
/// These are the fundamental operations that are always available.
/// Additional functionality is provided through extension traits.
impl RelationIndex {
    /// Create a new empty relation index
    pub fn new() -> Self {
        Self {
            function_index: Arc::new(DashMap::new()),
            name_index: Arc::new(DashMap::new()),
            entity_file_index: Arc::new(DashMap::new()),
            resolved_relation_index: Arc::new(DashMap::new()),
            reverse_callee_index: Arc::new(DashMap::new()),
            file_relation_index: Arc::new(DashMap::new()),
            file_callers_by_callee: Arc::new(DashMap::new()),
            file_records: Arc::new(RwLock::new(HashMap::new())),
            dependency_graph: Arc::new(FileDependencyGraph::new()),
            entity_dependency_graph: Arc::new(RwLock::new(EntityDependencyGraph::new())),
            symbol_key_to_entity: Arc::new(RwLock::new(HashMap::new())),
            entity_to_symbol_key: Arc::new(RwLock::new(HashMap::new())),
            stable_id_to_entity: Arc::new(RwLock::new(HashMap::new())),
            entity_id_counter: Arc::new(AtomicU64::new(0)),
            entity_id_remaps: Arc::new(RwLock::new(HashMap::new())),
            diagnostics: Arc::new(RelationDiagnostics::new()),
            file_symbol_keys: Arc::new(RwLock::new(HashMap::new())),
            file_entities_by_start: Arc::new(RwLock::new(HashMap::new())),
            generation: Arc::new(SnapshotGeneration::new()),
            last_affected_files: std::sync::Mutex::new(None),
        }
    }

    /// Current mutation version of this index.
    ///
    /// Starts at 0 and increments on every mutating operation. Snapshots
    /// record the version at creation time so callers can detect stale
    /// snapshots and decide whether to re-snapshot.
    pub fn version(&self) -> u64 {
        self.generation.current()
    }

    /// Access the unified file records map.
    ///
    /// Returns a reference to the `RwLock<HashMap<String, FileRecord>>` that
    /// stores file info, imports, and exports in a single per-file entry.
    pub fn file_records(&self) -> &RwLock<HashMap<String, FileRecord>> {
        &self.file_records
    }

    /// Bump the version counter after a mutation.
    ///
    /// Called from all public mutating entry points. The increment is
    /// monotonic but not guaranteed to be exactly +1 under concurrent
    /// access (relaxed ordering is sufficient for staleness detection).
    pub(super) fn bump_version(&self) {
        self.generation.advance();
    }

    /// Whether any zero-copy snapshot is currently sharing this index's maps.
    ///
    /// When true, mutating methods should copy affected maps before writing.
    pub fn has_active_readers(&self) -> bool {
        self.generation.active_readers() > 0
    }

    /// Create a compact, immutable snapshot of this index.
    ///
    /// Converts all `DashMap`/`RwLock` structures into plain `HashMap`s in a
    /// single batched pass, reducing per-entry overhead for read-only queries.
    /// The compact snapshot shares no memory with the source; mutations to the
    /// source after this call do not affect the returned value.
    pub fn take_compact_snapshot(&self) -> super::compact::CompactRelationIndex {
        super::compact::CompactRelationIndex::from_relation_index(self)
    }

    /// Record that the given files were affected by a mutation.
    /// Called from mutating entry points to support selective CoW copy.
    pub(super) fn record_affected_files(&self, files: impl IntoIterator<Item = String>) {
        if let Ok(mut guard) = self.last_affected_files.lock() {
            match guard.as_mut() {
                Some(set) => {
                    for f in files {
                        set.insert(f);
                    }
                }
                None => {
                    *guard = Some(files.into_iter().collect());
                }
            }
        }
    }

    /// Take the accumulated affected files set, clearing it.
    /// Returns `None` if no file-level information was recorded
    /// (e.g., entity-only mutations), signalling that a full copy is needed.
    pub(super) fn take_affected_files(&self) -> Option<HashSet<String>> {
        self.last_affected_files
            .lock()
            .ok()
            .and_then(|mut guard| guard.take())
    }

    /// Create a CoW-aware snapshot: zero-copy, with active-reader tracking.
    ///
    /// The returned snapshot shares the source maps (O(1)) and holds a guard
    /// that increments the source's reader count. While the guard is alive,
    /// `source.has_active_readers()` returns true, and mutations will
    /// automatically copy affected maps before writing.
    pub fn cow_snapshot(&self) -> (RelationSnapshotIndex, CoWSnapshotGuard) {
        let snapshot = RelationSnapshotIndex::from_index_shared(self);
        let guard = CoWSnapshotGuard::new(Arc::clone(&self.generation));
        (snapshot, guard)
    }

    /// Attach a metrics sink for observability.
    pub fn set_metrics(&self, metrics: Arc<RelationMetrics>) {
        if let Ok(mut guard) = self.diagnostics.metrics_sink.write() {
            *guard = Some(metrics);
        }
    }

    /// Get diagnostic state.
    pub fn diagnostics(&self) -> &RelationDiagnostics {
        &self.diagnostics
    }

    /// Get quality report.
    pub fn quality_report(&self) -> QualityReport {
        let summary = self.diagnostics.summary();
        let quality_score = self.diagnostics.quality_score(self.function_index.len());
        QualityReport {
            summary,
            quality_score,
            file_count: self.file_records.read().len(),
            entity_count: self.function_index.len(),
            relation_count: self.resolved_relation_index.iter().map(|e| e.len()).sum(),
        }
    }

    /// Synthetic EntityId marker bit (high bit 63).
    ///
    /// Real entity IDs are allocated sequentially from 0 and never set this
    /// bit. Synthetic IDs produced by `ProjectSymbolTable::symbol_ref_for` set
    /// it, so the two spaces are disjoint and can be distinguished via
    /// `is_synthetic_id`.
    pub const SYNTHETIC_MARK: u64 = 1u64 << 63;

    /// Whether the given EntityId is a synthetic symbol-table ID.
    pub fn is_synthetic_id(id: EntityId) -> bool {
        id.0 & Self::SYNTHETIC_MARK != 0
    }

    /// Extract the low counter from a synthetic EntityId.
    pub fn synthetic_counter(id: EntityId) -> u64 {
        id.0 & !Self::SYNTHETIC_MARK
    }

    /// Allocate a globally unique entity ID.
    pub fn allocate_entity_id(&self) -> EntityId {
        EntityId(self.entity_id_counter.fetch_add(1, Ordering::Relaxed))
    }

    /// The entity ID the counter starts from: next allocatable ID.
    ///
    /// Used by `detached_clone` and by the sparse-candidate hot-update path
    /// (which starts an empty index from the base view's counter).
    /// O(1) via the atomic counter.
    pub(super) fn entity_id_counter_start(&self) -> u64 {
        self.entity_id_counter.load(Ordering::Relaxed)
    }

    /// Ensure the atomic counter is at least `next` (max+1).
    fn ensure_counter_at_least(&self, next: u64) {
        let mut current = self.entity_id_counter.load(Ordering::Relaxed);
        while next > current {
            match self.entity_id_counter.compare_exchange_weak(
                current,
                next,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(v) => current = v,
            }
        }
    }

    /// O(1) max entity ID via the atomic counter, with self-healing when a
    /// test or snapshot fixture inserted directly into `function_index` without
    /// bumping the counter.
    pub fn max_entity_id(&self) -> u64 {
        if self.function_index.is_empty() {
            return 0;
        }
        let counter = self.entity_id_counter.load(Ordering::Relaxed);
        // Fast path: counter observed via `insert_function` is authoritative.
        if counter > 0 {
            // Self-heal if a raw `function_index.insert` outran the counter
            // (legacy test fixtures). Scan only in this fallback and bump the
            // counter so the next query is O(1) again.
            let mut max_id: u64 = 0;
            for entry in self.function_index.iter() {
                let id = entry.key().0;
                if id > max_id {
                    max_id = id;
                }
            }
            if max_id + 1 > counter {
                self.ensure_counter_at_least(max_id + 1);
                if let Ok(guard) = self.diagnostics.metrics_sink.read() {
                    if let Some(metrics) = guard.as_ref() {
                        metrics
                            .relation_max_entity_id_scan_fallback_total
                            .increment();
                    }
                }
                tracing::warn!(
                    scanned_max = max_id,
                    counter,
                    "entity_id_counter behind scanned max; self-healed"
                );
                return max_id;
            }
            return counter.saturating_sub(1);
        }
        // Counter was never bumped (empty or raw inserts only) — scan once.
        let mut max_id: u64 = 0;
        for entry in self.function_index.iter() {
            let id = entry.key().0;
            if id > max_id {
                max_id = id;
            }
        }
        self.ensure_counter_at_least(max_id + 1);
        if let Ok(guard) = self.diagnostics.metrics_sink.read() {
            if let Some(metrics) = guard.as_ref() {
                metrics
                    .relation_max_entity_id_scan_fallback_total
                    .increment();
            }
        }
        max_id
    }

    /// Insert an entity into the per-file ordered start-row index.
    pub(super) fn track_file_entity(&self, file_path: &str, row: u32, entity_id: EntityId) {
        let mut map = self.file_entities_by_start.write();
        let vec = map.entry(file_path.to_string()).or_default();
        // Keep sorted by (row, entity_id) for deterministic range queries.
        let pos = vec
            .binary_search_by(|(r, id)| r.cmp(&row).then_with(|| id.0.cmp(&entity_id.0)))
            .unwrap_or_else(|p| p);
        // Avoid duplicate insertion if already present.
        if vec.get(pos).is_some_and(|(_, id)| *id == entity_id) {
            return;
        }
        vec.insert(pos, (row, entity_id));
    }

    /// Remove an entity from the per-file ordered index.
    pub(super) fn untrack_file_entity(&self, file_path: &str, entity_id: EntityId) {
        let mut map = self.file_entities_by_start.write();
        if let Some(vec) = map.get_mut(file_path) {
            if let Some(pos) = vec.iter().position(|(_, id)| *id == entity_id) {
                vec.remove(pos);
            }
            if vec.is_empty() {
                map.remove(file_path);
            }
        }
    }

    /// Create an empty relation index whose entity ID counter starts at
    /// `start`. Equivalent to `RelationIndex::new()` followed by storing
    /// `start` into `entity_id_counter`; used by the sparse-candidate
    /// hot-update path so new entities never collide with base IDs.
    pub fn new_with_entity_id_start(start: u64) -> Self {
        let index = Self::new();
        index.entity_id_counter.store(start, Ordering::Relaxed);
        index
    }

    /// Create a fully detached mutable copy. No map, symbol table, or
    /// dependency graph is shared with the source index.
    ///
    /// Uses store-level deep-clone helpers (`EntityStore::deep_clone`,
    /// `RelationStore::deep_clone`, `SymbolRegistry::deep_clone`,
    /// `FileStore::deep_clone`) to keep the copy logic co-located with the
    /// data it operates on, instead of spreading 19 entry-by-entry loops
    /// across this method.
    ///
    /// The entity_id_counter is copied atomically from the source O(1).
    pub fn detached_clone(&self) -> Self {
        use super::stores::{EntityStore, FileStore, RelationStore, SymbolRegistry};

        let entity_store = EntityStore {
            function_index: Arc::clone(&self.function_index),
            name_index: Arc::clone(&self.name_index),
            entity_file_index: Arc::clone(&self.entity_file_index),
            file_entities_by_start: Arc::clone(&self.file_entities_by_start),
            entity_id_counter: Arc::clone(&self.entity_id_counter),
            entity_id_remaps: Arc::clone(&self.entity_id_remaps),
        };
        let cloned_entities = entity_store.deep_clone();

        let relation_store = RelationStore {
            resolved_relation_index: Arc::clone(&self.resolved_relation_index),
            reverse_callee_index: Arc::clone(&self.reverse_callee_index),
            file_relation_index: Arc::clone(&self.file_relation_index),
            file_callers_by_callee: Arc::clone(&self.file_callers_by_callee),
        };
        let cloned_relations = relation_store.deep_clone();

        let symbol_registry = SymbolRegistry {
            symbol_key_to_entity: Arc::clone(&self.symbol_key_to_entity),
            entity_to_symbol_key: Arc::clone(&self.entity_to_symbol_key),
            stable_id_to_entity: Arc::clone(&self.stable_id_to_entity),
            file_symbol_keys: Arc::clone(&self.file_symbol_keys),
        };
        let cloned_symbols = symbol_registry.deep_clone();

        let file_store = FileStore {
            file_records: Arc::clone(&self.file_records),
        };
        let cloned_files = file_store.deep_clone();

        // Diagnostics: copy atomic counters and mutex-protected samples.
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
        // Propagate metrics sink so detached observers emit to the same registry.
        if let Ok(guard) = self.diagnostics.metrics_sink.read() {
            if let Some(metrics) = guard.as_ref() {
                if let Ok(mut target) = diagnostics.metrics_sink.write() {
                    *target = Some(Arc::clone(metrics));
                }
            }
        }

        Self {
            function_index: cloned_entities.function_index,
            name_index: cloned_entities.name_index,
            entity_file_index: cloned_entities.entity_file_index,
            resolved_relation_index: cloned_relations.resolved_relation_index,
            reverse_callee_index: cloned_relations.reverse_callee_index,
            file_relation_index: cloned_relations.file_relation_index,
            file_callers_by_callee: cloned_relations.file_callers_by_callee,
            file_records: cloned_files.file_records,
            dependency_graph: Arc::new((*self.dependency_graph).clone()),
            entity_dependency_graph: Arc::new(RwLock::new(
                self.entity_dependency_graph.read().clone(),
            )),
            symbol_key_to_entity: cloned_symbols.symbol_key_to_entity,
            entity_to_symbol_key: cloned_symbols.entity_to_symbol_key,
            stable_id_to_entity: cloned_symbols.stable_id_to_entity,
            entity_id_counter: Arc::new(AtomicU64::new(self.entity_id_counter_start())),
            entity_id_remaps: cloned_entities.entity_id_remaps,
            diagnostics,
            file_symbol_keys: cloned_symbols.file_symbol_keys,
            file_entities_by_start: cloned_entities.file_entities_by_start,
            generation: Arc::new(SnapshotGeneration::new()),
            last_affected_files: std::sync::Mutex::new(None),
        }
    }

    /// Drain the source index into a fully independent snapshot, leaving the
    /// source empty.
    ///
    /// This is cheaper than `detached_clone` when the caller will drop the
    /// source immediately afterward (e.g. `RelationSnapshotIndex::from_index`
    /// followed by dropping the builder). Instead of cloning every entry, we
    /// take ownership of the DashMap data by swapping with empty maps.
    pub fn snapshot_take(&mut self) -> Self {
        fn take_map<K, V>(map: &mut Arc<DashMap<K, V>>) -> Arc<DashMap<K, V>>
        where
            K: std::hash::Hash + Eq,
        {
            std::mem::replace(map, Arc::new(DashMap::new()))
        }
        fn take_rwlock_map<K, V>(map: &mut Arc<RwLock<HashMap<K, V>>>) -> Arc<RwLock<HashMap<K, V>>>
        where
            K: std::hash::Hash + Eq,
        {
            std::mem::replace(map, Arc::new(RwLock::new(HashMap::new())))
        }

        let function_index = take_map(&mut self.function_index);
        let name_index = take_map(&mut self.name_index);
        let entity_file_index = take_map(&mut self.entity_file_index);
        let resolved_relation_index = take_map(&mut self.resolved_relation_index);
        let reverse_callee_index = take_map(&mut self.reverse_callee_index);
        let file_relation_index = take_map(&mut self.file_relation_index);
        let file_callers_by_callee = take_map(&mut self.file_callers_by_callee);
        let file_records = take_rwlock_map(&mut self.file_records);
        let symbol_key_to_entity = take_rwlock_map(&mut self.symbol_key_to_entity);
        let entity_to_symbol_key = take_rwlock_map(&mut self.entity_to_symbol_key);
        let stable_id_to_entity = take_rwlock_map(&mut self.stable_id_to_entity);
        let file_symbol_keys = take_rwlock_map(&mut self.file_symbol_keys);
        let file_entities_by_start = take_rwlock_map(&mut self.file_entities_by_start);
        let entity_id_remaps = take_rwlock_map(&mut self.entity_id_remaps);

        let entity_id_counter = Arc::new(AtomicU64::new(
            self.entity_id_counter.load(Ordering::Relaxed),
        ));
        // Leave the source counter at 0 (source is now empty).
        self.entity_id_counter.store(0, Ordering::Relaxed);

        let dependency_graph = Arc::new((*self.dependency_graph).clone());
        self.dependency_graph = Arc::new(FileDependencyGraph::new());
        let entity_dependency_graph =
            Arc::new(RwLock::new(self.entity_dependency_graph.read().clone()));
        *self.entity_dependency_graph.write() = EntityDependencyGraph::new();

        // Take diagnostics counters and samples.
        let diagnostics = Arc::new(RelationDiagnostics::new());
        diagnostics.symbol_key_conflict_count.store(
            self.diagnostics
                .symbol_key_conflict_count
                .swap(0, Ordering::Relaxed),
            Ordering::Relaxed,
        );
        diagnostics.entity_derived_key_count.store(
            self.diagnostics
                .entity_derived_key_count
                .swap(0, Ordering::Relaxed),
            Ordering::Relaxed,
        );
        diagnostics.relation_derived_key_count.store(
            self.diagnostics
                .relation_derived_key_count
                .swap(0, Ordering::Relaxed),
            Ordering::Relaxed,
        );
        diagnostics.delta_export_unresolved_count.store(
            self.diagnostics
                .delta_export_unresolved_count
                .swap(0, Ordering::Relaxed),
            Ordering::Relaxed,
        );
        if let Ok(mut guard) = self.diagnostics.symbol_key_conflict_samples.lock() {
            if let Ok(mut target) = diagnostics.symbol_key_conflict_samples.lock() {
                target.extend(guard.drain(..));
            }
        }
        // Propagate metrics sink so snapshot observers emit to the same registry.
        if let Ok(guard) = self.diagnostics.metrics_sink.read() {
            if let Some(metrics) = guard.as_ref() {
                if let Ok(mut target) = diagnostics.metrics_sink.write() {
                    *target = Some(Arc::clone(metrics));
                }
            }
        }

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
            entity_id_counter,
            entity_id_remaps,
            diagnostics,
            file_symbol_keys,
            file_entities_by_start,
            generation: Arc::new(SnapshotGeneration::new()),
            last_affected_files: std::sync::Mutex::new(None),
        }
    }

    /// Remove a function entity from the function index, keeping the name
    /// index consistent. Returns the removed entity, if any.
    pub(super) fn remove_function(&self, entity_id: &EntityId) -> Option<Entity> {
        // Capture file path before the entity_file_index is cleared by caller.
        let file_path = self.entity_file_index.get(entity_id).map(|v| v.clone());
        // Record the file for selective CoW refresh.
        if let Some(ref fp) = file_path {
            self.record_affected_files(std::iter::once(fp.clone()));
        }
        // Capture symbol key for file_symbol_keys cleanup before map removal.
        let symbol_key = self.entity_to_symbol_key.read().get(entity_id).cloned();
        let removed = self.function_index.remove(entity_id).map(|(_, e)| e);
        if let Some(entity) = &removed {
            {
                if let Some(mut bucket) = self.name_index.get_mut(&entity.name) {
                    bucket.retain(|id| id != entity_id);
                    if bucket.is_empty() {
                        drop(bucket);
                        self.name_index.remove(&entity.name);
                    }
                }
            }
            if let Some(fp) = file_path {
                self.untrack_file_entity(&fp, *entity_id);
            }
            // Keep file_symbol_keys consistent even when callers bypass delta's
            // explicit cleanup (idempotent with delta's follow-up removal).
            if let Some(key) = symbol_key {
                // Leave primary symbol maps to delta's caller for now; just keep
                // the reverse index consistent if we are the sole remover.
                let mut fsk_map = self.file_symbol_keys.write();
                if let Some(vec) = fsk_map.get_mut(&key.file_path) {
                    vec.retain(|k| k != &key);
                }
                if fsk_map.get(&key.file_path).is_some_and(|b| b.is_empty()) {
                    fsk_map.remove(&key.file_path);
                }
            }
        }
        removed
    }

    /// Add a function entity, maintaining the name index. If the entity ID is
    /// already registered under a different name, the stale name entry is
    /// removed first. Re-registering the same ID with the same name is a no-op.
    pub(super) fn insert_function(&self, entity_id: EntityId, entity: Entity) {
        if let Some(existing) = self.function_index.get(&entity_id)
            && existing.value().name != entity.name
        {
            if let Some(mut bucket) = self.name_index.get_mut(&existing.value().name) {
                bucket.retain(|id| *id != entity_id);
                if bucket.is_empty() {
                    drop(bucket);
                    self.name_index.remove(&existing.value().name);
                }
            }
        }
        let name = entity.name.clone();
        self.function_index.insert(entity_id, entity);
        let mut bucket = self.name_index.entry(name).or_default();
        if !bucket.contains(&entity_id) {
            bucket.push(entity_id);
        }
        self.ensure_counter_at_least(entity_id.0 + 1);
    }

    /// Register a name -> entity and entity -> file lookup for an entity that
    /// lives outside this index (a base-project entity during a sparse hot
    /// update). The entity is NOT added to `function_index`, so it does not
    /// count as part of the candidate; the registration only lets the resolver
    /// map a cross-file callee to its real EntityId in the base snapshot.
    pub(super) fn register_external_entity_name(
        &self,
        name: &str,
        entity_id: EntityId,
        file_path: &str,
    ) {
        {
            let mut bucket = self.name_index.entry(name.to_string()).or_default();
            if !bucket.contains(&entity_id) {
                bucket.push(entity_id);
            }
        }
        self.entity_file_index
            .insert(entity_id, file_path.to_string());
    }

    /// Add a resolved relation to the index
    ///
    /// Updates both forward index (caller -> relations) and the global
    /// reverse index (callee -> callers).
    /// Dedup is O(1) via the per-caller `RelationEdgeSet` identity set
    /// and the sorted reverse caller list.
    pub fn add_resolved_relation(&self, relation: ResolvedRelation) {
        let caller = relation.caller;
        let callee = relation.callee_id;
        let rel_type = relation.relation_type;

        // Insert into forward index (caller -> edges). The entry guard is
        // dropped before we touch the reverse index to avoid a deadlock
        // when caller and callee hash to the same DashMap shard.
        {
            let mut set = self.resolved_relation_index.entry(caller).or_default();
            if !set.insert(relation) {
                return;
            }
        }

        if let Some(callee_id) = callee {
            self.track_reverse_caller(callee_id, caller);
        }

        // Record the caller's file for selective CoW refresh.
        if let Some(file) = self.entity_file_index.get(&caller).map(|v| v.clone()) {
            self.record_affected_files(std::iter::once(file));
        }

        self.bump_version();

        // Build entity-level dependency graph incrementally.
        if let Some(callee_id) = callee {
            let mut graph = self.entity_dependency_graph.write();
            graph.add_dependency(caller, callee_id, rel_type);
        }
    }

    /// Add multiple resolved relations
    pub fn add_resolved_relations(&self, relations: Vec<ResolvedRelation>) {
        for relation in relations {
            self.add_resolved_relation(relation);
        }
    }

    /// Add a resolved file-level relation to the index, keyed by normalized
    /// file path. Deduplicated by edge identity like `add_resolved_relation`.
    ///
    /// File-level edges are attributed to the file itself; they never appear
    /// in `resolved_relation_index` and therefore never pollute entity-scoped
    /// queries or `function_index` counts.
    pub fn add_file_relation(&self, file_path: &str, relation: ResolvedRelation) {
        let mut set = self
            .file_relation_index
            .entry(file_path.to_string())
            .or_default();
        let callee_id = relation.callee_id;
        let inserted = set.insert(relation);
        drop(set);
        if inserted {
            self.record_affected_files(std::iter::once(file_path.to_string()));
            self.bump_version();
            self.track_file_caller(callee_id, file_path);
        }
    }

    /// Record `file_path` as a file-level caller of `callee_id` in the
    /// reverse index. No-op when the relation carries no internal callee.
    pub(super) fn track_file_caller(&self, callee_id: Option<EntityId>, file_path: &str) {
        if let Some(callee_id) = callee_id {
            self.file_callers_by_callee
                .entry(callee_id)
                .or_default()
                .insert(file_path.to_string());
        }
    }

    /// Drop every file-level edge of `file_path` and reconcile the reverse
    /// caller index. Shared by `remove_file`, delta application, and clear.
    pub(super) fn take_file_relations(&self, file_path: &str) -> Option<RelationEdgeSet> {
        let removed = self.file_relation_index.remove(file_path).map(|(_, e)| e);
        if let Some(edges) = &removed {
            for rel in edges.iter() {
                self.untrack_file_caller(rel.callee_id, file_path);
            }
        }
        removed
    }

    /// Remove `file_path` from the caller set of `callee_id`, dropping the
    /// entry when the set becomes empty.
    pub(super) fn untrack_file_caller(&self, callee_id: Option<EntityId>, file_path: &str) {
        if let Some(callee_id) = callee_id
            && let Some(mut callers) = self.file_callers_by_callee.get_mut(&callee_id)
        {
            callers.remove(file_path);
            if callers.is_empty() {
                drop(callers);
                self.file_callers_by_callee.remove(&callee_id);
            }
        }
    }

    /// Insert `caller` into the reverse callee index for `callee_id`, keeping
    /// the per-callee list sorted and deduplicated.
    pub(super) fn track_reverse_caller(&self, callee_id: EntityId, caller: EntityId) {
        let mut entry = self.reverse_callee_index.entry(callee_id).or_default();
        if let Err(pos) = entry.binary_search(&caller) {
            entry.insert(pos, caller);
        }
    }

    /// Remove `caller` from the reverse callee index for `callee_id`,
    /// dropping the entry when the list becomes empty.
    pub(super) fn untrack_reverse_caller(&self, callee_id: EntityId, caller: EntityId) {
        if let Some(mut entry) = self.reverse_callee_index.get_mut(&callee_id) {
            if let Ok(pos) = entry.binary_search(&caller) {
                entry.remove(pos);
                if entry.is_empty() {
                    drop(entry);
                    self.reverse_callee_index.remove(&callee_id);
                }
            }
        }
    }

    /// Conditionally remove `caller` from the reverse index for `callee_id`
    /// only when no remaining forward edge from `caller` to `callee_id` exists.
    pub(super) fn maybe_untrack_reverse_caller(&self, callee_id: EntityId, caller: EntityId) {
        let still_calls = self
            .resolved_relation_index
            .get(&caller)
            .is_some_and(|rels| rels.iter().any(|r| r.callee_id == Some(callee_id)));
        if !still_calls {
            self.untrack_reverse_caller(callee_id, caller);
        }
    }

    /// Rebuild the entire reverse callee index from the forward index.
    /// Used for validation or after bulk mutations that bypass incremental
    /// maintenance.
    pub fn rebuild_reverse_callee_index(&self) {
        self.reverse_callee_index.clear();
        for entry in self.resolved_relation_index.iter() {
            let caller = *entry.key();
            for rel in entry.value().iter() {
                if let Some(callee_id) = rel.callee_id {
                    self.track_reverse_caller(callee_id, caller);
                }
            }
        }
    }

    /// Resolved file-level relations of a normalized file path.
    pub fn file_relations(&self, file_path: &str) -> Option<Vec<ResolvedRelation>> {
        self.file_relation_index
            .get(file_path)
            .map(|v| v.edges.clone())
    }

    /// Register a stable SymbolKey -> EntityId mapping.
    ///
    /// This enables cross-session symbol lookup without relying on
    /// process-local EntityId counters. Also maintains the stable-ID side map
    /// for O(1) `get_entity_id_by_stable_symbol_id` lookups.
    ///
    /// Registration is first-wins: when the key already maps to a *different*
    /// entity, the new mapping is rejected with a warning (the existing
    /// mapping wins) and `false` is returned so callers can record the
    /// collision. Re-registering the same key for the same entity is
    /// idempotent and returns `true`.
    pub fn register_symbol_key(
        &self,
        file_path: &str,
        scoped_name: &str,
        entity: &Entity,
        entity_id: EntityId,
    ) -> bool {
        let key = SymbolKey::new(file_path, scoped_name, entity.kind, &entity.signature);
        // First-wins: write-lock for atomic check-and-insert.
        {
            let mut map = self.symbol_key_to_entity.write();
            match map.get(&key) {
                Some(&existing_id) if existing_id != entity_id => {
                    self.record_symbol_key_conflict(&key, existing_id, entity_id);
                    tracing::warn!(
                        symbol_key = ?key,
                        existing_entity = existing_id.0,
                        new_entity = entity_id.0,
                        "stable symbol key already registered to a different entity; keeping the existing mapping"
                    );
                    return false;
                }
                Some(_) => return true,
                None => {}
            }
            map.insert(key.clone(), entity_id);
        }
        self.bump_version();
        self.entity_to_symbol_key
            .write()
            .insert(entity_id, key.clone());
        self.stable_id_to_entity
            .write()
            .insert(key.stable_id().0, entity_id);
        // Maintain the file reverse index for O(scope) aggregation.
        self.file_symbol_keys
            .write()
            .entry(key.file_path.clone())
            .or_default()
            .push(key.clone());
        true
    }

    /// Record a first-wins symbol key registration collision: increment the
    /// diagnostic counter and push a bounded sample (oldest dropped at capacity).
    fn record_symbol_key_conflict(&self, key: &SymbolKey, kept: EntityId, rejected: EntityId) {
        self.diagnostics
            .symbol_key_conflict_count
            .fetch_add(1, Ordering::Relaxed);
        if let Ok(mut guard) = self.diagnostics.symbol_key_conflict_samples.lock() {
            if guard.len() >= SYMBOL_KEY_CONFLICT_SAMPLE_CAP {
                guard.pop_front();
            }
            guard.push_back(SymbolKeyConflictRecord {
                file_path: key.file_path.clone(),
                scoped_name: key.scoped_name.clone(),
                kind: key.kind,
                kept_entity: kept.0,
                rejected_entity: rejected.0,
            });
        }
    }

    /// Look up EntityId by SymbolKey.
    pub fn get_entity_id_by_symbol_key(&self, key: &SymbolKey) -> Option<EntityId> {
        self.symbol_key_to_entity.read().get(key).copied()
    }

    /// Resolve an opaque stable API identifier into this snapshot's local ID.
    ///
    /// O(1) via the stable-ID side map.
    pub fn get_entity_id_by_stable_symbol_id(&self, stable_id: &str) -> Option<EntityId> {
        self.stable_id_to_entity.read().get(stable_id).copied()
    }

    /// Look up SymbolKey by EntityId.
    pub fn get_symbol_key_by_entity_id(&self, entity_id: EntityId) -> Option<SymbolKey> {
        self.entity_to_symbol_key.read().get(&entity_id).cloned()
    }

    /// Total number of exports skipped during `apply_delta` because their
    /// stable symbol key could not be resolved to an entity.
    pub fn delta_export_unresolved_count(&self) -> u64 {
        self.diagnostics
            .delta_export_unresolved_count
            .load(Ordering::Relaxed)
    }

    /// Snapshot all registered stable symbol keys.
    pub fn stable_symbol_keys(&self) -> Vec<SymbolKey> {
        self.symbol_key_to_entity.read().keys().cloned().collect()
    }

    /// Export the fully resolved graph into the only persistent relationship model.
    ///
    /// The result always contains the complete project graph. Partial graph
    /// serialization is intentionally not an activation input: a relation
    /// epoch must be independently loadable after a restart.
    pub fn to_canonical_snapshot(
        &self,
        config_fingerprint: String,
    ) -> Result<CanonicalRelationSnapshot, String> {
        RelationSnapshotIndex::from_index_shared(self).to_canonical_snapshot(config_fingerprint)
    }

    /// Compute a deterministic fingerprint of the current snapshot.
    ///
    /// The fingerprint is a SHA-256 hash of the canonical components, suitable
    /// for cross-session integrity verification. Byte-identical to
    /// `to_canonical_snapshot().fingerprint()` for the same data.
    pub fn compute_fingerprint(&self) -> String {
        let files = active_file_set(&self.file_records, &self.entity_file_index);
        self.fingerprint_in_files(&files)
    }

    /// Validate index-internal invariants before publication.
    ///
    /// Checks that:
    /// - every relation caller references an existing entity,
    /// - every internal relation target (`callee_id = Some`) references an
    ///   existing entity (no dangling references),
    /// - every entity has a file membership record.
    pub fn validate_snapshot(&self) -> Result<(), String> {
        for entry in self.resolved_relation_index.iter() {
            let caller = *entry.key();
            if !self.function_index.contains_key(&caller) {
                return Err(format!(
                    "relation caller {} has no entity in the snapshot",
                    caller.0
                ));
            }
            for relation in entry.value().iter() {
                if let Some(callee_id) = relation.callee_id
                    && !self.function_index.contains_key(&callee_id)
                {
                    return Err(format!(
                        "relation from {} to {} references a missing internal target",
                        caller.0, callee_id.0
                    ));
                }
            }
        }
        for entry in self.file_relation_index.iter() {
            for relation in entry.value().iter() {
                if let Some(callee_id) = relation.callee_id
                    && !self.function_index.contains_key(&callee_id)
                {
                    return Err(format!(
                        "file-level relation in {} references a missing internal target {}",
                        entry.key(),
                        callee_id.0
                    ));
                }
            }
        }
        for entry in self.function_index.iter() {
            if !self.entity_file_index.contains_key(entry.key()) {
                return Err(format!(
                    "entity {} has no file membership in the snapshot",
                    entry.key().0
                ));
            }
        }
        // The per-file symbol key reverse index must mirror the primary
        // symbol map exactly: every registered key is listed under its own
        // file path, and no stale entries survive removals.
        let ske_guard = self.symbol_key_to_entity.read();
        let fsk_guard = self.file_symbol_keys.read();
        for key in ske_guard.keys() {
            let listed = fsk_guard
                .get(&key.file_path)
                .is_some_and(|vec| vec.iter().any(|k| k == key));
            if !listed {
                return Err(format!(
                    "symbol key {}/{} is missing from the per-file reverse index",
                    key.file_path, key.scoped_name
                ));
            }
        }
        for (path, keys) in fsk_guard.iter() {
            for key in keys.iter() {
                if !ske_guard.contains_key(key) {
                    return Err(format!(
                        "stale symbol key {}/{} in the per-file reverse index",
                        key.file_path, key.scoped_name
                    ));
                }
                if key.file_path != *path {
                    return Err(format!(
                        "symbol key {}/{} is filed under mismatched path {}",
                        key.file_path, key.scoped_name, path
                    ));
                }
            }
        }
        Ok(())
    }

    /// Get the per-file entity ID remap (parsed-local -> index-global) for a
    /// normalized project path.
    ///
    /// `index_file_core` remaps ParsedFile-local entity IDs to globally unique
    /// index IDs. Consumers holding groups built from `ParsedFile.entities`
    /// (e.g. the relationship processor) must translate group entity IDs
    /// through this table before querying the index.
    pub fn entity_id_remap_for(
        &self,
        normalized_path: &str,
    ) -> Option<HashMap<EntityId, EntityId>> {
        self.entity_id_remaps.read().get(normalized_path).cloned()
    }

    /// Clear all indexes
    pub fn clear(&self) {
        self.function_index.clear();
        self.entity_file_index.clear();
        self.resolved_relation_index.clear();
        self.reverse_callee_index.clear();
        self.file_relation_index.clear();
        self.file_callers_by_callee.clear();
        self.file_records.write().clear();
        self.symbol_key_to_entity.write().clear();
        self.entity_to_symbol_key.write().clear();
        self.stable_id_to_entity.write().clear();
        self.entity_id_remaps.write().clear();
        self.entity_id_counter.store(0, Ordering::Relaxed);
        self.diagnostics
            .symbol_key_conflict_count
            .store(0, Ordering::Relaxed);
        if let Ok(mut guard) = self.diagnostics.symbol_key_conflict_samples.lock() {
            guard.clear();
        }
        self.diagnostics
            .entity_derived_key_count
            .store(0, Ordering::Relaxed);
        self.diagnostics
            .relation_derived_key_count
            .store(0, Ordering::Relaxed);
        self.diagnostics
            .delta_export_unresolved_count
            .store(0, Ordering::Relaxed);
        self.file_symbol_keys.write().clear();
        self.file_entities_by_start.write().clear();
        self.entity_dependency_graph.write().clear();
        self.dependency_graph.clear();
    }
}
