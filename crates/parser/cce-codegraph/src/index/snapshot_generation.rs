//! Shared generation counter for snapshot versioning.
//!
//! `SnapshotGeneration` is a monotonically increasing mutation counter in
//! `RelationIndex`. Snapshots record the version at creation time.

use std::sync::atomic::{AtomicU64, Ordering};

/// Shared generation counter for a relation index.
///
/// Lives inside the source `RelationIndex` as `Arc<SnapshotGeneration>`.
/// The source increments `current` on every mutation.
#[derive(Debug)]
pub struct SnapshotGeneration {
    /// Monotonically increasing counter. Incremented on every mutation.
    current: AtomicU64,
}

impl SnapshotGeneration {
    /// Create a new generation counter starting at 0.
    pub fn new() -> Self {
        Self {
            current: AtomicU64::new(0),
        }
    }

    /// Current generation.
    pub fn current(&self) -> u64 {
        self.current.load(Ordering::Acquire)
    }

    /// Advance the generation by 1. Called from every mutation entry point.
    pub fn advance(&self) {
        self.current.fetch_add(1, Ordering::Release);
    }
}

impl Default for SnapshotGeneration {
    fn default() -> Self {
        Self::new()
    }
}
