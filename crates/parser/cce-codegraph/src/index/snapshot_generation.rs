//! Shared generation counter for snapshot CoW tracking.
//!
//! `SnapshotGeneration` replaces the bare `Arc<AtomicU64>` version counter
//! in `RelationIndex`, adding reader-count tracking so that the source
//! can detect when zero-copy snapshots are alive and need selective
//! copy-on-write treatment during mutations.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Shared generation counter for a relation index.
///
/// Lives inside the source `RelationIndex` as `Arc<SnapshotGeneration>`.
/// Every zero-copy snapshot derived from the source holds an `Arc` clone
/// of this same generation, plus the generation value at snapshot creation.
///
/// The source increments `current` on every mutation. A snapshot compares
/// its creation-time `generation` against the current value to detect
/// staleness.
#[derive(Debug)]
pub struct SnapshotGeneration {
    /// Monotonically increasing counter. Incremented on every mutation.
    current: AtomicU64,
    /// Number of live zero-copy snapshot references sharing the source maps.
    /// When > 0, mutations must copy the affected maps before writing.
    active_readers: AtomicU64,
}

impl SnapshotGeneration {
    /// Create a new generation counter starting at 0.
    pub fn new() -> Self {
        Self {
            current: AtomicU64::new(0),
            active_readers: AtomicU64::new(0),
        }
    }

    /// Current generation (read by the source to check for snapshot readers).
    pub fn current(&self) -> u64 {
        self.current.load(Ordering::Acquire)
    }

    /// Advance the generation by 1. Called from every mutation entry point.
    pub fn advance(&self) {
        self.current.fetch_add(1, Ordering::Release);
    }

    /// Number of active zero-copy snapshot references.
    pub fn active_readers(&self) -> u64 {
        self.active_readers.load(Ordering::Acquire)
    }

    /// Increment the reader count. Called when a zero-copy snapshot is created.
    pub fn advance_reader_count(&self) {
        self.active_readers.fetch_add(1, Ordering::Release);
    }

    /// Decrement the reader count. Called when a zero-copy snapshot is dropped.
    pub fn retire_reader_count(&self) {
        self.active_readers.fetch_sub(1, Ordering::Release);
    }
}

impl Default for SnapshotGeneration {
    fn default() -> Self {
        Self::new()
    }
}

/// RAII guard tracking one live snapshot reference to the source.
///
/// Created when a zero-copy snapshot is derived from a `RelationIndex`.
/// Dropped when the snapshot (or anything derived from it) is dropped.
/// The source checks `active_readers()` on the generation to decide
/// whether a mutation needs a copy-on-write.
pub struct CoWSnapshotGuard {
    generation: Arc<SnapshotGeneration>,
}

impl CoWSnapshotGuard {
    /// Create a new guard that increments the reader count.
    pub fn new(generation: Arc<SnapshotGeneration>) -> Self {
        generation.advance_reader_count();
        Self { generation }
    }
}

impl Drop for CoWSnapshotGuard {
    fn drop(&mut self) {
        self.generation.retire_reader_count();
    }
}
