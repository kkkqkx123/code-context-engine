//! CoW-aware snapshot types for automatic staleness detection.
//!
//! `CoWRelationSnapshot` wraps a `RelationSnapshotIndex` with generation-based
//! staleness detection. `CowLayeredSnapshot` extends this to layered snapshots
//! with auto-refresh semantics.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Weak};

use super::core::RelationIndex;
use super::snapshot_generation::{CoWSnapshotGuard, SnapshotGeneration};
use super::snapshot_index::{LayeredSnapshotIndex, RelationSnapshotIndex};

/// A snapshot that automatically detects staleness via generation comparison.
///
/// Composed of:
/// - The inner `RelationSnapshotIndex` (initially zero-copy from source)
/// - An `Arc<SnapshotGeneration>` shared with the source for staleness checking
/// - The creation-time generation value
///
/// On any read operation, if `generation.current() > creation_generation`,
/// the snapshot is stale. Callers can use `refresh()` to create a fresh
/// snapshot, or use `snapshot_if_fresh()` to get the inner snapshot only
/// when it is still current.
pub struct CoWRelationSnapshot {
    /// The current snapshot data.
    inner: Arc<RelationSnapshotIndex>,
    /// Shared generation counter from the source.
    generation: Arc<SnapshotGeneration>,
    /// Generation value when this snapshot was created.
    creation_generation: u64,
    /// RAII guard that decrements the reader count when dropped.
    _guard: CoWSnapshotGuard,
}

impl CoWRelationSnapshot {
    /// Create a new CoW-aware snapshot from a source index.
    ///
    /// The snapshot shares the source's maps (zero-copy) and holds a guard
    /// that increments the source's reader count.
    pub fn new(source: &RelationIndex) -> Self {
        let generation = Arc::clone(&source.generation);
        let creation_generation = generation.current();
        let inner = Arc::new(RelationSnapshotIndex::from_index_shared(source));
        let _guard = CoWSnapshotGuard::new(Arc::clone(&generation));

        Self {
            inner,
            generation,
            creation_generation,
            _guard,
        }
    }

    /// Check whether this snapshot is stale relative to the source.
    pub fn is_stale(&self) -> bool {
        self.generation.current() > self.creation_generation
    }

    /// Get a reference to the inner snapshot, regardless of staleness.
    ///
    /// Callers should check `is_stale()` before using this to ensure
    /// they're not working with outdated data.
    pub fn inner(&self) -> &Arc<RelationSnapshotIndex> {
        &self.inner
    }

    /// Get the generation value when this snapshot was created.
    pub fn creation_generation(&self) -> u64 {
        self.creation_generation
    }

    /// Get the current generation of the source.
    pub fn current_generation(&self) -> u64 {
        self.generation.current()
    }
}

/// A layered snapshot that transparently refreshes its base when stale.
///
/// The base `RelationSnapshotIndex` is created via `from_index_shared`
/// (zero-copy). This struct additionally holds:
/// - The `Arc<SnapshotGeneration>` from the source
/// - A `Weak<RelationIndex>` to the source for deep-copy refresh
/// - The creation-time generation
///
/// Queries first check freshness; if stale, the base is refreshed via
/// a deep copy of the source (or via delta materialization if available),
/// and subsequent queries use the fresh base.
pub struct CowLayeredSnapshot {
    /// Current base snapshot (may be refreshed).
    base: parking_lot::RwLock<Arc<RelationSnapshotIndex>>,
    /// Deltas layered on top of the base.
    deltas: Vec<Arc<cce_types::SnapshotDelta>>,
    /// Shared generation counter.
    generation: Arc<SnapshotGeneration>,
    /// Generation when the base was last refreshed.
    base_generation: AtomicU64,
    /// Weak reference to the source for refresh.
    source: Weak<RelationIndex>,
    /// Guard tracking this snapshot's reference to the source maps.
    _guard: CoWSnapshotGuard,
}

impl CowLayeredSnapshot {
    /// Create from a source index: zero-copy base, empty delta chain.
    ///
    /// The source's generation counter is shared for staleness detection.
    pub fn from_source(source: &RelationIndex) -> Self {
        let generation = Arc::clone(&source.generation);
        let creation_generation = generation.current();
        let base = Arc::new(RelationSnapshotIndex::from_index_shared(source));
        let guard = CoWSnapshotGuard::new(Arc::clone(&generation));

        Self {
            base: parking_lot::RwLock::new(base),
            deltas: Vec::new(),
            generation,
            base_generation: AtomicU64::new(creation_generation),
            source: Weak::new(),
            _guard: guard,
        }
    }

    /// Create from a source index with a weak reference for refresh.
    pub fn from_source_with_refresh(source: Arc<RelationIndex>) -> Self {
        let generation = Arc::clone(&source.generation);
        let creation_generation = generation.current();
        let base = Arc::new(RelationSnapshotIndex::from_index_shared(&source));
        let guard = CoWSnapshotGuard::new(Arc::clone(&generation));
        let source_weak = Arc::downgrade(&source);

        Self {
            base: parking_lot::RwLock::new(base),
            deltas: Vec::new(),
            generation,
            base_generation: AtomicU64::new(creation_generation),
            source: source_weak,
            _guard: guard,
        }
    }

    /// Check whether the base snapshot is stale relative to the source.
    pub fn is_stale(&self) -> bool {
        self.generation.current() > self.base_generation.load(Ordering::Acquire)
    }

    /// Refresh the base snapshot, using selective CoW copy when possible.
    ///
    /// If the source recorded which files were affected by mutations, only
    /// those files' entries are deep-copied; the rest are shared via
    /// `Arc::clone` (O(1)). Falls back to a full deep copy when no
    /// affected-file information is available (e.g., first refresh or
    /// entity-only mutations).
    ///
    /// Only succeeds if the source is still alive. Returns the new
    /// base generation, or `None` if the source was dropped.
    pub fn refresh(&self) -> Option<u64> {
        let source = self.source.upgrade()?;
        let new_base = if let Some(affected) = source.take_affected_files() {
            if affected.is_empty() {
                // Empty set means no file-level mutations recorded; full copy.
                let cow = source.detached_clone();
                Arc::new(RelationSnapshotIndex::from_index_shared(&cow))
            } else {
                let cow = source.selective_cow_copy(&affected);
                Arc::new(RelationSnapshotIndex::from_index_shared(&cow))
            }
        } else {
            // No affected-file information recorded; full copy.
            Arc::new(RelationSnapshotIndex::from_index(&source))
        };
        let new_gen = self.generation.current();
        *self.base.write() = new_base;
        self.base_generation.store(new_gen, Ordering::Release);
        Some(new_gen)
    }

    /// Ensure the base is fresh, refreshing if needed. Returns a
    /// reference to the (possibly refreshed) base.
    pub fn ensure_fresh(&self) -> Arc<RelationSnapshotIndex> {
        if self.is_stale() {
            self.refresh();
        }
        Arc::clone(&self.base.read())
    }

    /// Add a delta to the layered snapshot.
    pub fn push_delta(&mut self, delta: Arc<cce_types::SnapshotDelta>) {
        self.deltas.push(delta);
    }

    /// Delegate query to the `LayeredSnapshotIndex` view.
    /// Lazily refreshes the base if stale before answering.
    pub fn as_layered(&self) -> LayeredSnapshotIndex {
        let base = self.ensure_fresh();
        LayeredSnapshotIndex::with_deltas(base, self.deltas.clone())
    }
}
