//! Synchronous snapshot-read adapter for codegraph's read port.
//!
//! Relation snapshot loading is intentionally a synchronous, read-only
//! operation. The remote branch bridges its async storage calls onto the
//! metadata runtime that owns the connection pool, so the loader keeps the
//! same semantics on both branches.

use std::future::Future;
use std::sync::mpsc;

use super::PostgresClient;
use cce_storage_common::MetadataStorage;
use cce_types::relation::RelationSnapshotStore;
use cce_types::{CanonicalRelationSnapshot, RelationSnapshotManifest, SnapshotDelta, StorageError};

impl RelationSnapshotStore for PostgresClient {
    fn get_manifest(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Option<RelationSnapshotManifest>, StorageError> {
        let client = self.clone();
        block_on(
            async move { MetadataStorage::snapshot_manifest(&client, project_id, epoch).await },
        )
    }

    fn read_snapshot(
        &self,
        manifest: &RelationSnapshotManifest,
    ) -> Result<CanonicalRelationSnapshot, StorageError> {
        let client = self.clone();
        let project_id = manifest.project_id;
        let epoch = manifest.relation_epoch;
        block_on(async move { MetadataStorage::snapshot_read(&client, project_id, epoch).await })
    }

    fn find_base_epoch(
        &self,
        project_id: i64,
        delta_epoch: i64,
    ) -> Result<Option<i64>, StorageError> {
        let client = self.clone();
        block_on(async move {
            MetadataStorage::snapshot_find_base(&client, project_id, delta_epoch).await
        })
    }

    fn get_delta_chain(
        &self,
        project_id: i64,
        after_epoch: i64,
        up_to_epoch: i64,
    ) -> Result<Vec<SnapshotDelta>, StorageError> {
        let client = self.clone();
        block_on(async move {
            MetadataStorage::snapshot_delta_chain(&client, project_id, after_epoch, up_to_epoch)
                .await
        })
    }
}

fn block_on<T>(
    future: impl Future<Output = Result<T, StorageError>> + Send + 'static,
) -> Result<T, StorageError>
where
    T: Send + 'static,
{
    // The metadata runtime owns the pool's maintenance tasks. Driving the
    // future from a plain thread avoids nested-runtime panics and works from
    // both async and blocking callers.
    let handle = super::runtime_handle();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(handle.block_on(future));
    });
    let result = rx
        .recv()
        .map_err(|error| StorageError::connection(format!("snapshot worker: {error}")))?;
    result
}
