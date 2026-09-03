//! Boundary for publishing complete relation snapshots.
//!
//! The orchestrator owns candidate construction while the server owns the
//! process-local runtime. This trait keeps that dependency direction intact.

use async_trait::async_trait;
use cce_relation::index::{LayeredSnapshotIndex, RelationIndex};
use cce_types::{CanonicalRelationSnapshot, SnapshotDelta, StorageError};

/// Result returned after a complete relation snapshot is made active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelationPublication {
    /// Newly active canonical relation epoch.
    pub relation_epoch: i64,
}

/// Publishes a complete canonical relation snapshot.
///
/// Implementations must not activate partial snapshots. The server-side
/// implementation also synchronizes its immutable query runtime.
#[async_trait]
pub trait RelationSnapshotPublisher: Send + Sync {
    /// Validate, persist, activate, and expose a complete snapshot.
    ///
    /// `index` is the in-memory relation index the snapshot was exported
    /// from; implementations build the query-time runtime projection from it
    /// directly  instead of re-reading what was just persisted.
    async fn publish(
        &self,
        project_id: i64,
        operation_id: &str,
        snapshot: CanonicalRelationSnapshot,
        index: &RelationIndex,
    ) -> Result<RelationPublication, StorageError>;

    /// Publish an incremental delta on top of an existing base snapshot.
    ///
    /// This method persists only the delta (not a full snapshot) and updates
    /// the runtime with a layered index (base + delta). The `base_epoch`
    /// identifies which full snapshot this delta builds upon.
    ///
    /// `base` supplies the process-internal layered state of `delta.base_epoch`
    /// (the relation base cache: a materialized base plus the already published
    /// delta chain). When provided, implementations must use it directly
    /// instead of re-reading the base from durable storage, and they may share
    /// its maps into the runtime projection (zero copy). The caller guarantees
    /// the view is used read-only. When `None` (cold path: no cache, e.g. after
    /// a full index or process restart), implementations fall back to loading
    /// the base from the store.
    async fn publish_delta(
        &self,
        project_id: i64,
        operation_id: &str,
        delta: SnapshotDelta,
        base: Option<LayeredSnapshotIndex>,
    ) -> Result<RelationPublication, StorageError>;

    /// Compact the project's delta chain when it crosses the implementation's
    /// thresholds: merge the chain into a fresh full base snapshot, activate
    /// it, and retire the old delta manifests.
    ///
    /// Called after an operation's candidate has been activated (never while
    /// an operation is in flight), so implementations may freely advance the
    /// active relation epoch. Implementations must be safe to call repeatedly
    /// and must not fail the operation when the chain is below the threshold
    /// or another publication candidate is still in flight. The default is a
    /// no-op for implementations without durable delta chains.
    async fn maybe_compact(&self, project_id: i64) -> Result<(), StorageError> {
        let _ = project_id;
        Ok(())
    }
}
