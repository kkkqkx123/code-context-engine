//! Relation runtime for managing per-project relation index snapshots
//!
//! This module provides the core runtime infrastructure for managing
//! project-scoped relation indexes with atomic snapshot publishing,
//! state tracking, and capability reporting.

use std::sync::Arc;
use std::time::SystemTime;

use cce_relation::index::core::RelationIndex;
use cce_relation::index::snapshot_index::{LayeredSnapshotIndex, RelationSnapshotIndex};
use tokio::sync::{Mutex, RwLock};

/// Relation runtime state
///
/// This enum represents the deterministic state of a project's relation runtime.
/// States are mutually exclusive and transitions follow a strict lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationRuntimeState {
    /// Project configuration has relation capability disabled
    Disabled,
    /// Not yet attempted to load (cold start)
    Unloaded,
    /// First load in progress, no old snapshot available
    Loading,
    /// Complete snapshot available for queries
    Available,
    /// Building new snapshot, old snapshot still served
    Updating,
    /// Load or update failed, serving degraded/old snapshot if available
    Degraded,
}

impl RelationRuntimeState {
    /// Check if this state can serve queries
    pub fn can_serve_queries(&self) -> bool {
        matches!(self, Self::Available | Self::Updating | Self::Degraded)
    }

    /// Check if this state represents a complete snapshot
    pub fn is_complete(&self) -> bool {
        matches!(self, Self::Available)
    }

    /// Check if this is a terminal failure state
    pub fn is_failed(&self) -> bool {
        matches!(self, Self::Degraded)
    }
}

/// Snapshot integrity level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotIntegrity {
    /// Full snapshot with all fields populated
    Full,
    /// Partial snapshot (e.g., from SQLite recovery with missing fields)
    Partial,
    /// Empty but valid snapshot (zero entities/relations)
    Empty,
}

/// Published relation snapshot
///
/// This is an immutable snapshot of a project's relation index at a point in time.
/// Once published, the snapshot cannot be modified - updates create a new snapshot.
/// Supports both plain base snapshots and layered (base + delta) snapshots.
#[derive(Debug, Clone)]
pub struct PublishedSnapshot {
    /// Project ID this snapshot belongs to
    pub project_id: i64,
    /// Relation epoch (monotonically increasing version)
    pub relation_epoch: i64,
    /// The actual relation index (shared via Arc)
    pub index: Arc<LayeredSnapshotIndex>,
    /// Snapshot integrity level
    pub integrity: SnapshotIntegrity,
    /// Publication timestamp
    pub published_at: SystemTime,
    /// Optional manifest identifier for tracking
    pub manifest_id: Option<String>,
}

impl PublishedSnapshot {
    /// Create a new published snapshot from a base snapshot index.
    pub fn new(
        project_id: i64,
        relation_epoch: i64,
        index: Arc<RelationSnapshotIndex>,
        integrity: SnapshotIntegrity,
        manifest_id: Option<String>,
    ) -> Self {
        Self {
            project_id,
            relation_epoch,
            index: Arc::new(LayeredSnapshotIndex::new(index)),
            integrity,
            published_at: SystemTime::now(),
            manifest_id,
        }
    }

    /// Create a published snapshot from a layered (base + delta) index.
    pub fn new_layered(
        project_id: i64,
        relation_epoch: i64,
        layered: Arc<LayeredSnapshotIndex>,
        integrity: SnapshotIntegrity,
        manifest_id: Option<String>,
    ) -> Self {
        Self {
            project_id,
            relation_epoch,
            index: layered,
            integrity,
            published_at: SystemTime::now(),
            manifest_id,
        }
    }

    /// Create an empty snapshot (for valid zero-record projects)
    pub fn empty(project_id: i64, relation_epoch: i64) -> Self {
        // The temporary index is never mutated, so share it zero-copy
        Self::new(
            project_id,
            relation_epoch,
            Arc::new(RelationSnapshotIndex::from_index_shared(
                &RelationIndex::new(),
            )),
            SnapshotIntegrity::Empty,
            None,
        )
    }

    /// Get the number of entities in this snapshot
    pub fn entity_count(&self) -> usize {
        self.index.function_count()
    }

    /// Get the number of relations in this snapshot (delta-aware).
    pub fn relation_count(&self) -> usize {
        self.index.resolved_relation_count()
    }

    /// Get the underlying base snapshot index.
    pub fn base_index(&self) -> &RelationSnapshotIndex {
        &self.index.base
    }
}

/// Relation runtime metadata for error reporting
#[derive(Debug, Clone, Default)]
pub struct RuntimeMetadata {
    /// Last error message (if any)
    pub last_error: Option<String>,
    /// Last successful epoch
    pub last_successful_epoch: i64,
    /// Number of failed attempts
    pub failure_count: u32,
    /// Last update attempt timestamp
    pub last_attempt_at: Option<SystemTime>,
}

/// Runtime-level relation events for upper-layer monitoring.
///
/// These events are emitted on state transitions that matter outside the
/// runtime: publication failures (relation segment down) and staleness
/// (updating while serving the previous snapshot). Subscribers can use them
/// to surface the segment status or to fall back to pure vector results.
#[derive(Debug, Clone)]
pub enum RelationRuntimeEvent {
    /// The relation segment failed to publish/update; the runtime serves the
    /// previous snapshot (if any) and is marked degraded.
    PublishFailed { project_id: i64, error: String },
    /// The runtime is updating (or degraded); the served snapshot is stale
    /// relative to the latest candidate.
    Stale { project_id: i64, reason: String },
}

/// Runtime event listener callback type
#[derive(Clone)]
pub struct RelationRuntimeListener {
    inner: Arc<dyn Fn(&RelationRuntimeEvent) + Send + Sync>,
}

impl std::fmt::Debug for RelationRuntimeListener {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("RelationRuntimeListener")
    }
}

impl RelationRuntimeListener {
    pub fn new<F>(listener: F) -> Self
    where
        F: Fn(&RelationRuntimeEvent) + Send + Sync + 'static,
    {
        Self {
            inner: Arc::new(listener),
        }
    }

    fn call(&self, event: &RelationRuntimeEvent) {
        (self.inner)(event);
    }
}

#[derive(Debug)]
struct RuntimeInner {
    state: RelationRuntimeState,
    published_snapshot: Option<PublishedSnapshot>,
    metadata: RuntimeMetadata,
    listeners: Vec<RelationRuntimeListener>,
}

/// Per-project relation runtime
///
/// Manages the lifecycle of a project's relation index, including:
/// - State tracking (disabled, unloaded, loading, available, updating, degraded)
/// - Atomic snapshot publishing
/// - Error tracking and reporting
/// - Capability reporting for API responses
///
/// # Thread Safety
///
/// This type is designed for concurrent access:
/// - State and metadata are protected by a RwLock
/// - Published snapshots are Arc'd and immutable
/// - Readers can access snapshots without blocking writers
///
/// # Usage
///
/// ```ignore
/// let runtime = RelationRuntime::new(project_id);
///
/// // Publishing a new snapshot
/// runtime.publish_snapshot(new_index, epoch, integrity).await;
///
/// // Getting current snapshot for queries
/// let snapshot = runtime.get_snapshot().await;
/// if let Some(snapshot) = snapshot {
///     // Use snapshot.index for queries
/// }
///
/// // Reporting failure
/// runtime.report_failure(error_message).await;
/// ```
pub struct RelationRuntime {
    /// Project ID
    project_id: i64,
    /// State, snapshot, and metadata share one synchronization boundary.
    inner: Arc<RwLock<RuntimeInner>>,
    /// Serializes complete snapshot publication for this project.
    publication_lock: Arc<Mutex<()>>,
}

impl RelationRuntime {
    /// Create a new relation runtime for a project
    pub fn new(project_id: i64) -> Self {
        Self {
            project_id,
            inner: Arc::new(RwLock::new(RuntimeInner {
                state: RelationRuntimeState::Unloaded,
                published_snapshot: None,
                metadata: RuntimeMetadata::default(),
                listeners: Vec::new(),
            })),
            publication_lock: Arc::new(Mutex::new(())),
        }
    }

    /// Get the project ID
    pub fn project_id(&self) -> i64 {
        self.project_id
    }

    /// Subscribe to runtime-level relation events.
    ///
    /// The listener is invoked synchronously from the publication path;
    /// subscribers must not block.
    pub async fn subscribe(&self, listener: RelationRuntimeListener) {
        let mut inner = self.inner.write().await;
        inner.listeners.push(listener);
    }

    /// Emit an event to all subscribers without holding the inner lock.
    async fn emit(&self, event: RelationRuntimeEvent) {
        let listeners = {
            let inner = self.inner.read().await;
            inner.listeners.clone()
        };
        for listener in listeners {
            listener.call(&event);
        }
    }

    /// Get the project-scoped publication lock shared by all publishers.
    pub fn publication_lock(&self) -> Arc<Mutex<()>> {
        Arc::clone(&self.publication_lock)
    }

    /// Get current runtime state (read-only)
    pub async fn get_state(&self) -> RelationRuntimeState {
        self.inner.read().await.state
    }

    /// Set runtime state
    async fn set_state(&self, new_state: RelationRuntimeState) {
        let mut inner = self.inner.write().await;
        tracing::debug!(
            project_id = self.project_id,
            old_state = ?inner.state,
            new_state = ?new_state,
            "RelationRuntime state transition"
        );
        inner.state = new_state;
    }

    /// Get currently published snapshot (read-only)
    pub async fn get_snapshot(&self) -> Option<Arc<PublishedSnapshot>> {
        self.inner
            .read()
            .await
            .published_snapshot
            .clone()
            .map(Arc::new)
    }

    /// Get the current relation epoch
    pub async fn get_relation_epoch(&self) -> i64 {
        self.inner
            .read()
            .await
            .published_snapshot
            .as_ref()
            .map(|snapshot| snapshot.relation_epoch)
            .unwrap_or(0)
    }

    /// Publish a new snapshot atomically
    ///
    /// This method atomically updates both the state and the published snapshot.
    /// Readers will see either the old complete snapshot or the new complete snapshot,
    /// never a partial state.
    ///
    /// # Arguments
    ///
    /// * `index` - The new relation snapshot index (immutable)
    /// * `relation_epoch` - The epoch/version of this snapshot
    /// * `integrity` - The integrity level of the snapshot
    /// * `manifest_id` - Optional manifest identifier
    ///
    /// # Returns
    ///
    /// Returns the previous state before the transition
    pub async fn publish_snapshot(
        &self,
        index: Arc<RelationSnapshotIndex>,
        relation_epoch: i64,
        integrity: SnapshotIntegrity,
        manifest_id: Option<String>,
    ) -> RelationRuntimeState {
        let snapshot = PublishedSnapshot::new(
            self.project_id,
            relation_epoch,
            index,
            integrity,
            manifest_id,
        );

        let entity_count = snapshot.entity_count();
        let relation_count = snapshot.relation_count();

        let mut inner = self.inner.write().await;
        let old_state = inner.state;
        if inner
            .published_snapshot
            .as_ref()
            .is_some_and(|current| current.relation_epoch > relation_epoch)
        {
            tracing::warn!(
                project_id = self.project_id,
                relation_epoch,
                current_epoch = inner
                    .published_snapshot
                    .as_ref()
                    .map(|current| current.relation_epoch)
                    .unwrap_or_default(),
                "Rejected non-monotonic relation snapshot publication"
            );
            return old_state;
        }
        inner.published_snapshot = Some(snapshot);
        inner.state = RelationRuntimeState::Available;
        inner.metadata.last_successful_epoch = relation_epoch;
        inner.metadata.failure_count = 0;
        inner.metadata.last_error = None;
        inner.metadata.last_attempt_at = Some(SystemTime::now());
        drop(inner);

        tracing::info!(
            project_id = self.project_id,
            relation_epoch,
            integrity = ?integrity,
            entity_count,
            relation_count,
            "Published new relation snapshot"
        );

        old_state
    }

    /// Publish a layered (base + delta) snapshot atomically.
    pub async fn publish_layered_snapshot(
        &self,
        layered: Arc<LayeredSnapshotIndex>,
        relation_epoch: i64,
        integrity: SnapshotIntegrity,
        manifest_id: Option<String>,
    ) -> RelationRuntimeState {
        let snapshot = PublishedSnapshot::new_layered(
            self.project_id,
            relation_epoch,
            layered,
            integrity,
            manifest_id,
        );

        let mut inner = self.inner.write().await;
        let old_state = inner.state;
        if inner
            .published_snapshot
            .as_ref()
            .is_some_and(|current| current.relation_epoch > relation_epoch)
        {
            tracing::warn!(
                project_id = self.project_id,
                relation_epoch,
                current_epoch = inner
                    .published_snapshot
                    .as_ref()
                    .map(|current| current.relation_epoch)
                    .unwrap_or_default(),
                "Rejected non-monotonic relation snapshot publication"
            );
            return old_state;
        }
        inner.published_snapshot = Some(snapshot);
        inner.state = RelationRuntimeState::Available;
        inner.metadata.last_successful_epoch = relation_epoch;
        inner.metadata.failure_count = 0;
        inner.metadata.last_error = None;
        inner.metadata.last_attempt_at = Some(SystemTime::now());
        drop(inner);

        tracing::info!(
            project_id = self.project_id,
            relation_epoch,
            integrity = ?integrity,
            "Published new layered relation snapshot"
        );

        old_state
    }

    /// Mark runtime as loading (first load, no old snapshot)
    pub async fn set_loading(&self) {
        self.set_state(RelationRuntimeState::Loading).await;
    }

    /// Mark runtime as updating (building new snapshot, old still served)
    pub async fn set_updating(&self) -> Option<Arc<PublishedSnapshot>> {
        self.set_state(RelationRuntimeState::Updating).await;
        let snapshot = self.get_snapshot().await;
        self.emit(RelationRuntimeEvent::Stale {
            project_id: self.project_id,
            reason: "relation update in progress".to_string(),
        })
        .await;
        snapshot
    }

    /// Mark runtime as disabled
    pub async fn set_disabled(&self) {
        let mut inner = self.inner.write().await;
        inner.state = RelationRuntimeState::Disabled;
        inner.published_snapshot = None;
    }

    /// Report a failure and transition to Degraded state
    ///
    /// If there's an old snapshot, it continues to be served.
    /// If no old snapshot exists, queries will return 503.
    pub async fn report_failure(&self, error: String) {
        let mut inner = self.inner.write().await;
        let has_snapshot = inner.published_snapshot.is_some();
        inner.metadata.last_error = Some(error.clone());
        inner.metadata.failure_count += 1;
        inner.metadata.last_attempt_at = Some(SystemTime::now());
        inner.state = RelationRuntimeState::Degraded;
        let failure_count = inner.metadata.failure_count;
        drop(inner);
        tracing::warn!(
            project_id = self.project_id,
            error = %error,
            has_snapshot,
            failure_count,
            "Relation runtime failure reported"
        );
        // Explicit failure event: the relation segment is down. Upper layers
        // can use this to surface the status or fall back to vector-only
        // search while the segment stays degraded.
        self.emit(RelationRuntimeEvent::PublishFailed {
            project_id: self.project_id,
            error: error.clone(),
        })
        .await;
        if has_snapshot {
            self.emit(RelationRuntimeEvent::Stale {
                project_id: self.project_id,
                reason: format!("relation publication failed: {error}"),
            })
            .await;
        }
    }

    /// Get runtime metadata (for API responses)
    pub async fn get_metadata(&self) -> RuntimeMetadata {
        self.inner.read().await.metadata.clone()
    }

    /// Check if queries can be served
    pub async fn can_serve_queries(&self) -> bool {
        let inner = self.inner.read().await;
        inner.state.can_serve_queries() && inner.published_snapshot.is_some()
    }

    /// Get capability information for API responses
    pub async fn get_capability_info(&self) -> RelationCapabilityInfo {
        let inner = self.inner.read().await;
        let state = inner.state;
        let snapshot = &inner.published_snapshot;
        let metadata = inner.metadata.clone();

        RelationCapabilityInfo {
            enabled: !matches!(state, RelationRuntimeState::Disabled),
            available: state.can_serve_queries() && snapshot.is_some(),
            state,
            relation_epoch: snapshot.as_ref().map(|s| s.relation_epoch).unwrap_or(0),
            entity_count: snapshot.as_ref().map(|s| s.entity_count()).unwrap_or(0),
            relation_count: snapshot.as_ref().map(|s| s.relation_count()).unwrap_or(0),
            integrity: snapshot.as_ref().map(|s| s.integrity),
            failure_count: metadata.failure_count,
            // Stale tracking
            stale: matches!(
                state,
                RelationRuntimeState::Updating | RelationRuntimeState::Degraded
            ),
            stale_reason: metadata.last_error.clone(),
            last_error: metadata.last_error,
            last_successful_epoch: metadata.last_successful_epoch,
            pending_operation_id: None,
            active_epoch: snapshot.as_ref().map(|s| s.relation_epoch).unwrap_or(0),
            runtime_epoch: snapshot.as_ref().map(|s| s.relation_epoch).unwrap_or(0),
            snapshot_integrity: snapshot.as_ref().map(|s| s.integrity),
            rebuild_required: matches!(state, RelationRuntimeState::Degraded)
                || snapshot.as_ref().is_some_and(|snapshot| {
                    !matches!(
                        snapshot.integrity,
                        SnapshotIntegrity::Full | SnapshotIntegrity::Empty
                    )
                }),
        }
    }

    /// Clear the runtime (for project deletion)
    pub async fn clear(&self) {
        let mut inner = self.inner.write().await;
        inner.state = RelationRuntimeState::Unloaded;
        inner.published_snapshot = None;
        inner.metadata = RuntimeMetadata::default();

        tracing::info!(project_id = self.project_id, "Relation runtime cleared");
    }
}

/// Capability information for API responses
#[derive(Debug, Clone)]
pub struct RelationCapabilityInfo {
    /// Whether relation capability is enabled for this project
    pub enabled: bool,
    /// Whether queries can be served right now
    pub available: bool,
    /// Current runtime state
    pub state: RelationRuntimeState,
    /// Current relation epoch
    pub relation_epoch: i64,
    /// Number of entities in current snapshot
    pub entity_count: usize,
    /// Number of relations in current snapshot
    pub relation_count: usize,
    /// Snapshot integrity level
    pub integrity: Option<SnapshotIntegrity>,
    /// Last error message (if any)
    pub last_error: Option<String>,
    /// Number of consecutive failures
    pub failure_count: u32,
    /// Whether the current snapshot is stale (hot update pending)
    pub stale: bool,
    /// Reason for staleness (if any)
    pub stale_reason: Option<String>,
    /// Last successful epoch
    pub last_successful_epoch: i64,
    /// Pending operation ID (if any)
    pub pending_operation_id: Option<String>,
    /// Canonical relation epoch active in SQLite.
    pub active_epoch: i64,
    /// Relation epoch currently served by the runtime.
    pub runtime_epoch: i64,
    /// Integrity of the runtime snapshot exposed separately for diagnostics.
    pub snapshot_integrity: Option<SnapshotIntegrity>,
    /// Whether a complete rebuild is required to restore consistency.
    pub rebuild_required: bool,
}

impl RelationCapabilityInfo {
    /// Convert to JSON-serializable map for API responses
    pub fn to_json_map(&self) -> serde_json::Value {
        serde_json::json!({
            "enabled": self.enabled,
            "available": self.available,
            "state": format!("{:?}", self.state),
            "relation_epoch": self.relation_epoch,
            "entity_count": self.entity_count,
            "relation_count": self.relation_count,
            "integrity": self.integrity.map(|i| format!("{:?}", i)),
            "last_error": self.last_error,
            "failure_count": self.failure_count,
            "stale": self.stale,
            "stale_reason": self.stale_reason,
            "last_successful_epoch": self.last_successful_epoch,
            "pending_operation_id": self.pending_operation_id,
            "active_epoch": self.active_epoch,
            "runtime_epoch": self.runtime_epoch,
            "snapshot_integrity": self.snapshot_integrity.map(|i| format!("{:?}", i)),
            "rebuild_required": self.rebuild_required,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_runtime_state_transitions() {
        let runtime = RelationRuntime::new(1);

        // Initial state
        assert_eq!(runtime.get_state().await, RelationRuntimeState::Unloaded);

        // Set loading
        runtime.set_loading().await;
        assert_eq!(runtime.get_state().await, RelationRuntimeState::Loading);

        // Publish snapshot
        let index = RelationIndex::new();
        let snap_index = Arc::new(RelationSnapshotIndex::from_index(&index));
        runtime
            .publish_snapshot(snap_index, 1, SnapshotIntegrity::Full, None)
            .await;
        assert_eq!(runtime.get_state().await, RelationRuntimeState::Available);

        // Set updating
        runtime.set_updating().await;
        assert_eq!(runtime.get_state().await, RelationRuntimeState::Updating);

        // Report failure
        runtime.report_failure("test error".to_string()).await;
        assert_eq!(runtime.get_state().await, RelationRuntimeState::Degraded);

        // Clear
        runtime.clear().await;
        assert_eq!(runtime.get_state().await, RelationRuntimeState::Unloaded);
    }

    #[tokio::test]
    async fn test_snapshot_publishing() {
        let runtime = RelationRuntime::new(1);

        let index = RelationIndex::new();
        let snap_index = Arc::new(RelationSnapshotIndex::from_index(&index));
        runtime
            .publish_snapshot(
                snap_index.clone(),
                1,
                SnapshotIntegrity::Full,
                Some("manifest-1".to_string()),
            )
            .await;

        let snapshot = runtime.get_snapshot().await;
        assert!(snapshot.is_some());
        let snapshot = snapshot.unwrap();
        assert_eq!(snapshot.project_id, 1);
        assert_eq!(snapshot.relation_epoch, 1);
        assert_eq!(snapshot.integrity, SnapshotIntegrity::Full);
        assert_eq!(snapshot.manifest_id, Some("manifest-1".to_string()));
    }

    #[tokio::test]
    async fn test_capability_info() {
        let runtime = RelationRuntime::new(1);

        // Initially unavailable
        let info = runtime.get_capability_info().await;
        assert!(info.enabled);
        assert!(!info.available);

        // After publishing
        let index = RelationIndex::new();
        let snap_index = Arc::new(RelationSnapshotIndex::from_index(&index));
        runtime
            .publish_snapshot(snap_index, 1, SnapshotIntegrity::Full, None)
            .await;

        let info = runtime.get_capability_info().await;
        assert!(info.enabled);
        assert!(info.available);
        assert_eq!(info.relation_epoch, 1);
    }

    #[tokio::test]
    async fn failure_emits_explicit_relation_events() {
        let runtime = RelationRuntime::new(1);
        let events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = Arc::clone(&events);
        runtime
            .subscribe(RelationRuntimeListener::new(move |event| {
                captured.lock().unwrap().push(event.clone());
            }))
            .await;

        // Set updating emits a Stale event.
        runtime.set_updating().await;
        // Failure emits PublishFailed; with a served snapshot it also emits Stale.
        let index = RelationIndex::new();
        let snap_index = Arc::new(RelationSnapshotIndex::from_index(&index));
        runtime
            .publish_snapshot(snap_index, 1, SnapshotIntegrity::Full, None)
            .await;
        runtime
            .report_failure("delta validation failed".to_string())
            .await;

        let events = events.lock().unwrap().clone();
        let publish_failed: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, RelationRuntimeEvent::PublishFailed { .. }))
            .collect();
        let stale: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, RelationRuntimeEvent::Stale { .. }))
            .collect();
        assert_eq!(publish_failed.len(), 1, "failure must emit PublishFailed");
        assert_eq!(
            stale.len(),
            2,
            "updating + degraded-with-snapshot must emit Stale"
        );
        if let RelationRuntimeEvent::PublishFailed { project_id, error } = &publish_failed[0] {
            assert_eq!(*project_id, 1);
            assert!(error.contains("delta validation failed"));
        }
    }
}
