//! Synchronous snapshot-read adapter for codegraph's read port.
//!
//! Relation snapshot loading is intentionally a synchronous, read-only
//! operation. The remote branch bridges its async storage calls onto the
//! active Tokio runtime so the loader keeps the same semantics on both
//! branches.

use std::future::Future;

use cce_storage_common::MetadataStorage;
use cce_types::relation::RelationSnapshotStore;
use cce_types::{CanonicalRelationSnapshot, RelationSnapshotManifest, SnapshotDelta, StorageError};
use tokio::runtime::Handle;

use super::PostgresClient;

impl RelationSnapshotStore for PostgresClient {
    fn get_manifest(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Option<RelationSnapshotManifest>, StorageError> {
        block_on(MetadataStorage::snapshot_manifest(self, project_id, epoch))
    }

    fn read_snapshot(
        &self,
        manifest: &RelationSnapshotManifest,
    ) -> Result<CanonicalRelationSnapshot, StorageError> {
        block_on(MetadataStorage::snapshot_read(
            self,
            manifest.project_id,
            manifest.relation_epoch,
        ))
    }

    fn find_base_epoch(
        &self,
        project_id: i64,
        delta_epoch: i64,
    ) -> Result<Option<i64>, StorageError> {
        block_on(MetadataStorage::snapshot_find_base(
            self,
            project_id,
            delta_epoch,
        ))
    }

    fn get_delta_chain(
        &self,
        project_id: i64,
        after_epoch: i64,
        up_to_epoch: i64,
    ) -> Result<Vec<SnapshotDelta>, StorageError> {
        block_on(MetadataStorage::snapshot_delta_chain(
            self,
            project_id,
            after_epoch,
            up_to_epoch,
        ))
    }
}

fn block_on<T>(future: impl Future<Output = Result<T, StorageError>>) -> Result<T, StorageError> {
    match Handle::try_current() {
        Ok(handle) => tokio::task::block_in_place(|| handle.block_on(future)),
        Err(_) => tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| StorageError::connection(format!("snapshot runtime: {error}")))?
            .block_on(future),
    }
}
