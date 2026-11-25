//! Synchronous snapshot-read adapter for codegraph's read port.
//!
//! Relation snapshot loading is intentionally a synchronous, read-only
//! operation. The remote branch bridges its async storage calls onto the
//! metadata runtime that owns the connection pool, so the loader keeps the
//! same semantics on both branches.

use std::collections::HashSet;
use std::future::Future;
use std::sync::mpsc;

use super::PostgresClient;
use cce_storage_common::MetadataStorage;
use cce_types::relation::RelationSnapshotStore;
use cce_types::relation::classification::ExternalCallType;
use cce_types::{
    CanonicalRelationSnapshot, RelationShardComponents, RelationSnapshotManifest, ShardDirectory,
    SnapshotDelta, StorageError,
};
use cce_types::{ShardLocation, StableSymbolKey};

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

    fn read_shard_directory(
        &self,
        project_id: i64,
        relation_epoch: i64,
    ) -> Result<ShardDirectory, StorageError> {
        let client = self.clone();
        block_on(async move {
            client
                .snapshot_shard_directory(project_id, relation_epoch)
                .await
        })
    }

    fn read_shard_components(
        &self,
        project_id: i64,
        relation_epoch: i64,
        shard_id: i64,
    ) -> Result<Option<RelationShardComponents>, StorageError> {
        let client = self.clone();
        block_on(async move {
            client
                .snapshot_shard_components(project_id, relation_epoch, shard_id)
                .await
        })
    }

    fn entity_location_by_id(
        &self,
        project_id: i64,
        epoch: i64,
        entity_id: u64,
    ) -> Result<Option<ShardLocation>, StorageError> {
        let client = self.clone();
        block_on(async move {
            client
                .shard_entity_location_by_id(project_id, epoch, entity_id)
                .await
        })
    }

    fn symbol_key_by_entity_id(
        &self,
        project_id: i64,
        epoch: i64,
        entity_id: u64,
    ) -> Result<Option<StableSymbolKey>, StorageError> {
        let client = self.clone();
        block_on(async move {
            client
                .shard_symbol_key_by_entity_id(project_id, epoch, entity_id)
                .await
        })
    }

    fn entity_location_by_symbol(
        &self,
        project_id: i64,
        epoch: i64,
        key: &StableSymbolKey,
    ) -> Result<Option<ShardLocation>, StorageError> {
        let client = self.clone();
        let key = key.clone();
        block_on(async move {
            client
                .shard_entity_location_by_symbol(project_id, epoch, &key)
                .await
        })
    }

    fn entity_location_by_stable_id(
        &self,
        project_id: i64,
        epoch: i64,
        stable_id: &str,
    ) -> Result<Option<ShardLocation>, StorageError> {
        let client = self.clone();
        let stable_id = stable_id.to_string();
        block_on(async move {
            client
                .shard_entity_location_by_stable_id(project_id, epoch, &stable_id)
                .await
        })
    }

    fn name_shards(
        &self,
        project_id: i64,
        epoch: i64,
        name: &str,
    ) -> Result<Vec<i64>, StorageError> {
        let client = self.clone();
        let name = name.to_string();
        block_on(async move { client.shard_name_shards(project_id, epoch, &name).await })
    }

    fn file_shard(
        &self,
        project_id: i64,
        epoch: i64,
        path: &str,
    ) -> Result<Option<i64>, StorageError> {
        let client = self.clone();
        let path = path.to_string();
        block_on(async move { client.shard_file_shard(project_id, epoch, &path).await })
    }

    fn caller_shards(
        &self,
        project_id: i64,
        epoch: i64,
        callee: u64,
    ) -> Result<Vec<i64>, StorageError> {
        let client = self.clone();
        block_on(async move { client.shard_caller_shards(project_id, epoch, callee).await })
    }

    fn classification_shards(
        &self,
        project_id: i64,
        epoch: i64,
        classification: &ExternalCallType,
    ) -> Result<Vec<i64>, StorageError> {
        let client = self.clone();
        let classification = classification.clone();
        block_on(async move {
            client
                .shard_classification_shards(project_id, epoch, &classification)
                .await
        })
    }

    fn classification_counts(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<std::collections::HashMap<ExternalCallType, usize>, StorageError> {
        let client = self.clone();
        block_on(async move { client.shard_classification_counts(project_id, epoch).await })
    }

    fn directory_files(&self, project_id: i64, epoch: i64) -> Result<Vec<String>, StorageError> {
        let client = self.clone();
        block_on(async move { client.shard_directory_files(project_id, epoch).await })
    }

    fn max_entity_id(&self, project_id: i64, epoch: i64) -> Result<u64, StorageError> {
        let client = self.clone();
        block_on(async move { client.shard_max_entity_id(project_id, epoch).await })
    }

    fn stable_symbol_keys(
        &self,
        project_id: i64,
        epoch: i64,
        files: Option<&HashSet<String>>,
    ) -> Result<Vec<StableSymbolKey>, StorageError> {
        let client = self.clone();
        let files = files.cloned();
        block_on(async move {
            client
                .shard_stable_symbol_keys(project_id, epoch, files.as_ref())
                .await
        })
    }

    fn dependencies_of(
        &self,
        project_id: i64,
        epoch: i64,
        source: &str,
    ) -> Result<Vec<String>, StorageError> {
        let client = self.clone();
        let source = source.to_string();
        block_on(async move {
            client
                .shard_dependencies_of(project_id, epoch, &source)
                .await
        })
    }

    fn dependents_of(
        &self,
        project_id: i64,
        epoch: i64,
        file: &str,
    ) -> Result<Vec<String>, StorageError> {
        let client = self.clone();
        let file = file.to_string();
        block_on(async move { client.shard_dependents_of(project_id, epoch, &file).await })
    }

    fn all_dependencies(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Vec<(String, String)>, StorageError> {
        let client = self.clone();
        block_on(async move { client.shard_all_dependencies(project_id, epoch).await })
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

    rx.recv()
        .map_err(|error| StorageError::connection(format!("snapshot worker: {error}")))?
}
