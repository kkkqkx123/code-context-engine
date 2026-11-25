//! Deterministic backend handle for metadata persistence.

use std::collections::HashSet;

use cce_types::relation::RelationSnapshotStore;
use cce_types::relation::classification::ExternalCallType;
use cce_types::{
    CanonicalRelationSnapshot, RelationShardComponents, RelationSnapshotManifest, ShardDirectory,
    SnapshotDelta, StorageError,
};
use cce_types::{ShardLocation, StableSymbolKey};

/// A metadata persistence handle with exactly one active backend branch.
///
/// The enum stores concrete clients only. All business operations are part
/// of [`super::MetadataStorage`] and are forwarded from one generated
/// implementation, so callers never need branch-specific accessors.
#[derive(Debug, Clone)]
pub enum MetadataBackendStore<L, R> {
    Local(L),
    Remote(R),
}

/// Capability to derive a project-scoped handle from a concrete backend.
///
/// The local branch opens its per-project database; the remote branch keeps
/// the same pooled client because all projects live in one schema.
pub trait MetadataProjectScope: Clone {
    fn scope_to_project(&self, project_id: i64) -> Result<Self, StorageError>;
}

impl<T: MetadataProjectScope> MetadataProjectScope for std::sync::Arc<T> {
    fn scope_to_project(&self, project_id: i64) -> Result<Self, StorageError> {
        Ok(std::sync::Arc::new(T::scope_to_project(
            self.as_ref(),
            project_id,
        )?))
    }
}

impl<L, R> MetadataBackendStore<L, R>
where
    L: MetadataProjectScope,
    R: MetadataProjectScope,
{
    /// Return the backend handle scoped to one project.
    pub fn for_project(&self, project_id: i64) -> Result<Self, StorageError> {
        match self {
            Self::Local(store) => Ok(Self::Local(store.scope_to_project(project_id)?)),
            Self::Remote(store) => Ok(Self::Remote(store.scope_to_project(project_id)?)),
        }
    }
}

impl<L, R> MetadataBackendStore<L, R> {
    /// Whether the embedded local branch is active.
    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local(_))
    }

    /// Whether the remote branch is active.
    pub fn is_remote(&self) -> bool {
        matches!(self, Self::Remote(_))
    }
}

/// The snapshot loader is synchronous and reads only published rows; both
/// branches provide that read port so callers keep one handle.
impl<L, R> RelationSnapshotStore for MetadataBackendStore<L, R>
where
    L: RelationSnapshotStore,
    R: RelationSnapshotStore,
{
    fn get_manifest(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Option<RelationSnapshotManifest>, StorageError> {
        match self {
            Self::Local(store) => store.get_manifest(project_id, epoch),
            Self::Remote(store) => store.get_manifest(project_id, epoch),
        }
    }

    fn read_snapshot(
        &self,
        manifest: &RelationSnapshotManifest,
    ) -> Result<CanonicalRelationSnapshot, StorageError> {
        match self {
            Self::Local(store) => store.read_snapshot(manifest),
            Self::Remote(store) => store.read_snapshot(manifest),
        }
    }

    fn read_shard_directory(
        &self,
        project_id: i64,
        relation_epoch: i64,
    ) -> Result<ShardDirectory, StorageError> {
        match self {
            Self::Local(store) => store.read_shard_directory(project_id, relation_epoch),
            Self::Remote(store) => store.read_shard_directory(project_id, relation_epoch),
        }
    }

    fn read_shard_components(
        &self,
        project_id: i64,
        relation_epoch: i64,
        shard_id: i64,
    ) -> Result<Option<RelationShardComponents>, StorageError> {
        match self {
            Self::Local(store) => store.read_shard_components(project_id, relation_epoch, shard_id),
            Self::Remote(store) => {
                store.read_shard_components(project_id, relation_epoch, shard_id)
            }
        }
    }

    fn entity_location_by_id(
        &self,
        project_id: i64,
        epoch: i64,
        entity_id: u64,
    ) -> Result<Option<ShardLocation>, StorageError> {
        match self {
            Self::Local(store) => store.entity_location_by_id(project_id, epoch, entity_id),
            Self::Remote(store) => store.entity_location_by_id(project_id, epoch, entity_id),
        }
    }

    fn symbol_key_by_entity_id(
        &self,
        project_id: i64,
        epoch: i64,
        entity_id: u64,
    ) -> Result<Option<StableSymbolKey>, StorageError> {
        match self {
            Self::Local(store) => store.symbol_key_by_entity_id(project_id, epoch, entity_id),
            Self::Remote(store) => store.symbol_key_by_entity_id(project_id, epoch, entity_id),
        }
    }

    fn entity_location_by_symbol(
        &self,
        project_id: i64,
        epoch: i64,
        key: &StableSymbolKey,
    ) -> Result<Option<ShardLocation>, StorageError> {
        match self {
            Self::Local(store) => store.entity_location_by_symbol(project_id, epoch, key),
            Self::Remote(store) => store.entity_location_by_symbol(project_id, epoch, key),
        }
    }

    fn entity_location_by_stable_id(
        &self,
        project_id: i64,
        epoch: i64,
        stable_id: &str,
    ) -> Result<Option<ShardLocation>, StorageError> {
        match self {
            Self::Local(store) => store.entity_location_by_stable_id(project_id, epoch, stable_id),
            Self::Remote(store) => store.entity_location_by_stable_id(project_id, epoch, stable_id),
        }
    }

    fn name_shards(
        &self,
        project_id: i64,
        epoch: i64,
        name: &str,
    ) -> Result<Vec<i64>, StorageError> {
        match self {
            Self::Local(store) => store.name_shards(project_id, epoch, name),
            Self::Remote(store) => store.name_shards(project_id, epoch, name),
        }
    }

    fn file_shard(
        &self,
        project_id: i64,
        epoch: i64,
        path: &str,
    ) -> Result<Option<i64>, StorageError> {
        match self {
            Self::Local(store) => store.file_shard(project_id, epoch, path),
            Self::Remote(store) => store.file_shard(project_id, epoch, path),
        }
    }

    fn caller_shards(
        &self,
        project_id: i64,
        epoch: i64,
        callee: u64,
    ) -> Result<Vec<i64>, StorageError> {
        match self {
            Self::Local(store) => store.caller_shards(project_id, epoch, callee),
            Self::Remote(store) => store.caller_shards(project_id, epoch, callee),
        }
    }

    fn classification_shards(
        &self,
        project_id: i64,
        epoch: i64,
        classification: &ExternalCallType,
    ) -> Result<Vec<i64>, StorageError> {
        match self {
            Self::Local(store) => store.classification_shards(project_id, epoch, classification),
            Self::Remote(store) => store.classification_shards(project_id, epoch, classification),
        }
    }

    fn classification_counts(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<std::collections::HashMap<ExternalCallType, usize>, StorageError> {
        match self {
            Self::Local(store) => store.classification_counts(project_id, epoch),
            Self::Remote(store) => store.classification_counts(project_id, epoch),
        }
    }

    fn directory_files(&self, project_id: i64, epoch: i64) -> Result<Vec<String>, StorageError> {
        match self {
            Self::Local(store) => store.directory_files(project_id, epoch),
            Self::Remote(store) => store.directory_files(project_id, epoch),
        }
    }

    fn max_entity_id(&self, project_id: i64, epoch: i64) -> Result<u64, StorageError> {
        match self {
            Self::Local(store) => store.max_entity_id(project_id, epoch),
            Self::Remote(store) => store.max_entity_id(project_id, epoch),
        }
    }

    fn stable_symbol_keys(
        &self,
        project_id: i64,
        epoch: i64,
        files: Option<&HashSet<String>>,
    ) -> Result<Vec<StableSymbolKey>, StorageError> {
        match self {
            Self::Local(store) => store.stable_symbol_keys(project_id, epoch, files),
            Self::Remote(store) => store.stable_symbol_keys(project_id, epoch, files),
        }
    }

    fn dependencies_of(
        &self,
        project_id: i64,
        epoch: i64,
        source: &str,
    ) -> Result<Vec<String>, StorageError> {
        match self {
            Self::Local(store) => store.dependencies_of(project_id, epoch, source),
            Self::Remote(store) => store.dependencies_of(project_id, epoch, source),
        }
    }

    fn dependents_of(
        &self,
        project_id: i64,
        epoch: i64,
        file: &str,
    ) -> Result<Vec<String>, StorageError> {
        match self {
            Self::Local(store) => store.dependents_of(project_id, epoch, file),
            Self::Remote(store) => store.dependents_of(project_id, epoch, file),
        }
    }

    fn all_dependencies(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Vec<(String, String)>, StorageError> {
        match self {
            Self::Local(store) => store.all_dependencies(project_id, epoch),
            Self::Remote(store) => store.all_dependencies(project_id, epoch),
        }
    }

    fn find_base_epoch(
        &self,
        project_id: i64,
        delta_epoch: i64,
    ) -> Result<Option<i64>, StorageError> {
        match self {
            Self::Local(store) => store.find_base_epoch(project_id, delta_epoch),
            Self::Remote(store) => store.find_base_epoch(project_id, delta_epoch),
        }
    }

    fn get_delta_chain(
        &self,
        project_id: i64,
        after_epoch: i64,
        up_to_epoch: i64,
    ) -> Result<Vec<SnapshotDelta>, StorageError> {
        match self {
            Self::Local(store) => store.get_delta_chain(project_id, after_epoch, up_to_epoch),
            Self::Remote(store) => store.get_delta_chain(project_id, after_epoch, up_to_epoch),
        }
    }
}
