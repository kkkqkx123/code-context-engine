//! Abstraction over relation snapshot persistence.
//!
//! This trait decouples the orchestrator from the concrete `SqliteClient` /
//! `RelationSnapshotRepository` so hot-update logic can be unit-tested with an
//! in-memory mock and the storage backend can be swapped without touching the
//! processor.

use std::sync::Arc;

use cce_storage_sqlite::SqliteClient;
use cce_storage_sqlite::repo::RelationSnapshotRepository;
use cce_types::{CanonicalRelationSnapshot, SnapshotDelta};

/// Persistent store for relation snapshots and deltas.
///
/// Implementations must provide atomic epoch allocation and publication.
/// The trait is object-safe and `Send + Sync` so it can be held behind `Arc`.
pub trait RelationSnapshotStore: Send + Sync {
    /// Allocate a new `building` epoch for `project_id` and return its epoch number.
    fn allocate_building_epoch(
        &self,
        project_id: i64,
        operation_id: &str,
        config_fingerprint: &str,
    ) -> Result<i64, String>;

    /// Persist a full canonical snapshot and mark it `ready`.
    fn write_snapshot_and_mark_ready(
        &self,
        project_id: i64,
        epoch: i64,
        snapshot: &CanonicalRelationSnapshot,
        input_fingerprint: &str,
        snapshot_fingerprint: &str,
    ) -> Result<(), String>;

    /// Persist an incremental delta.
    fn write_delta(&self, project_id: i64, epoch: i64, delta: &SnapshotDelta)
    -> Result<(), String>;

    /// Activate `epoch` (ready -> active).
    fn activate(&self, project_id: i64, epoch: i64) -> Result<(), String>;

    /// Mark `epoch` as failed with `reason`.
    fn mark_failed(&self, project_id: i64, epoch: i64, reason: &str) -> Result<(), String>;

    /// Read the delta chain for `project_id` after `after_epoch` up to `up_to_epoch`.
    fn get_delta_chain(
        &self,
        project_id: i64,
        after_epoch: i64,
        up_to_epoch: i64,
    ) -> Result<Vec<SnapshotDelta>, String>;
}

/// `SqliteClient` adapter implementing `RelationSnapshotStore`.
///
/// This is the production implementation; tests can inject an in-memory mock
/// via `Arc::new(MockStore)`. The adapter delegates to
/// `RelationSnapshotRepository` inside a transaction so epoch allocation
/// remains atomic.
pub struct SqliteRelationStore {
    client: Arc<SqliteClient>,
}

impl SqliteRelationStore {
    pub fn new(client: Arc<SqliteClient>) -> Self {
        Self { client }
    }

    pub fn client(&self) -> &Arc<SqliteClient> {
        &self.client
    }
}

impl RelationSnapshotStore for SqliteRelationStore {
    fn allocate_building_epoch(
        &self,
        project_id: i64,
        operation_id: &str,
        config_fingerprint: &str,
    ) -> Result<i64, String> {
        self.client
            .with_transaction(|tx| {
                RelationSnapshotRepository::allocate_building(
                    tx,
                    project_id,
                    operation_id,
                    config_fingerprint,
                )
            })
            .map_err(|e| e.to_string())
    }

    fn write_snapshot_and_mark_ready(
        &self,
        project_id: i64,
        epoch: i64,
        snapshot: &CanonicalRelationSnapshot,
        input_fingerprint: &str,
        snapshot_fingerprint: &str,
    ) -> Result<(), String> {
        self.client
            .with_transaction(|tx| {
                RelationSnapshotRepository::write_snapshot_and_mark_ready(
                    tx,
                    project_id,
                    epoch,
                    snapshot,
                    input_fingerprint,
                    snapshot_fingerprint,
                )
            })
            .map_err(|e| e.to_string())
    }

    fn write_delta(
        &self,
        project_id: i64,
        epoch: i64,
        delta: &SnapshotDelta,
    ) -> Result<(), String> {
        self.client
            .with_transaction(|tx| {
                RelationSnapshotRepository::write_delta(tx, project_id, epoch, delta)
            })
            .map_err(|e| e.to_string())
    }

    fn activate(&self, project_id: i64, epoch: i64) -> Result<(), String> {
        self.client
            .with_transaction(|tx| RelationSnapshotRepository::activate(tx, project_id, epoch))
            .map_err(|e| e.to_string())
    }

    fn mark_failed(&self, project_id: i64, epoch: i64, reason: &str) -> Result<(), String> {
        self.client
            .with_transaction(|tx| {
                RelationSnapshotRepository::mark_failed(tx, project_id, epoch, reason)
            })
            .map_err(|e| e.to_string())
    }

    fn get_delta_chain(
        &self,
        project_id: i64,
        after_epoch: i64,
        up_to_epoch: i64,
    ) -> Result<Vec<SnapshotDelta>, String> {
        self.client
            .with_transaction(|tx| {
                RelationSnapshotRepository::get_delta_chain(
                    tx,
                    project_id,
                    after_epoch,
                    up_to_epoch,
                )
            })
            .map_err(|e| e.to_string())
    }
}

/// In-memory mock for unit tests.
#[cfg(test)]
pub mod mock {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    pub struct InMemoryRelationStore {
        snapshots: Mutex<HashMap<(i64, i64), CanonicalRelationSnapshot>>,
        deltas: Mutex<HashMap<(i64, i64), SnapshotDelta>>,
        next_epoch: Mutex<HashMap<i64, i64>>,
    }

    impl RelationSnapshotStore for InMemoryRelationStore {
        fn allocate_building_epoch(
            &self,
            project_id: i64,
            _operation_id: &str,
            _config_fingerprint: &str,
        ) -> Result<i64, String> {
            let mut map = self.next_epoch.lock().expect("lock");
            let e = map.entry(project_id).or_insert(1);
            let epoch = *e;
            *e += 1;
            Ok(epoch)
        }

        fn write_snapshot_and_mark_ready(
            &self,
            project_id: i64,
            epoch: i64,
            snapshot: &CanonicalRelationSnapshot,
            _input_fingerprint: &str,
            _snapshot_fingerprint: &str,
        ) -> Result<(), String> {
            self.snapshots
                .lock()
                .expect("lock")
                .insert((project_id, epoch), snapshot.clone());
            Ok(())
        }

        fn write_delta(
            &self,
            project_id: i64,
            epoch: i64,
            delta: &SnapshotDelta,
        ) -> Result<(), String> {
            self.deltas
                .lock()
                .expect("lock")
                .insert((project_id, epoch), delta.clone());
            Ok(())
        }

        fn activate(&self, _project_id: i64, _epoch: i64) -> Result<(), String> {
            Ok(())
        }

        fn mark_failed(&self, _project_id: i64, _epoch: i64, _reason: &str) -> Result<(), String> {
            Ok(())
        }

        fn get_delta_chain(
            &self,
            project_id: i64,
            after_epoch: i64,
            up_to_epoch: i64,
        ) -> Result<Vec<SnapshotDelta>, String> {
            let deltas = self.deltas.lock().expect("lock");
            let mut out = Vec::new();
            for epoch in (after_epoch + 1)..=up_to_epoch {
                if let Some(d) = deltas.get(&(project_id, epoch)) {
                    out.push(d.clone());
                }
            }
            Ok(out)
        }
    }
}
