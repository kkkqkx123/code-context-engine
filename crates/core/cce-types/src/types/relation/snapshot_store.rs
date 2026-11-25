//! Relation snapshot persistence contract
//!
//! The parser's `RelationSnapshotLoader` reads persisted relationship
//! snapshots through the [`RelationSnapshotStore`] port instead of touching
//! SQLite directly. The concrete SQLite adapter lives in
//! `cce_infrastructure::storage::sqlite::SnapshotStoreAdapter`.

use std::collections::HashSet;
use std::sync::Arc;

use crate::types::relation::{
    CanonicalRelationSnapshot, ExternalCallType, RelationShardComponents, ShardDirectory,
    ShardLocation, StableSymbolKey, SymbolKeyConflictRecord,
};
use crate::types::{SnapshotDelta, StorageError};

/// Lifecycle state of a persisted relation snapshot epoch
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationSnapshotState {
    Building,
    Ready,
    Active,
    Failed,
    Delta,
}

impl RelationSnapshotState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Building => "building",
            Self::Ready => "ready",
            Self::Active => "active",
            Self::Failed => "failed",
            Self::Delta => "delta",
        }
    }

    pub fn parse(value: &str) -> Result<Self, StorageError> {
        match value {
            "building" => Ok(Self::Building),
            "ready" => Ok(Self::Ready),
            "active" => Ok(Self::Active),
            "failed" => Ok(Self::Failed),
            "delta" => Ok(Self::Delta),
            _ => Err(StorageError::Query(format!(
                "invalid relation snapshot state: {value}"
            ))),
        }
    }
}

/// Manifest describing a persisted relation snapshot epoch
#[derive(Debug, Clone)]
pub struct RelationSnapshotManifest {
    pub project_id: i64,
    pub relation_epoch: i64,
    pub operation_id: String,
    pub state: RelationSnapshotState,
    pub schema_version: u32,
    pub parser_version: u32,
    pub resolver_version: u32,
    pub path_normalization_version: u32,
    pub config_fingerprint: String,
    pub input_fingerprint: Option<String>,
    pub snapshot_fingerprint: Option<String>,
    pub file_count: Option<usize>,
    pub entity_count: Option<usize>,
    pub relation_count: Option<usize>,
    pub dependency_count: Option<usize>,
    pub failure_reason: Option<String>,
    /// Diagnostic counters for first-wins symbol key registration collisions
    /// during this epoch's build. Persisted for observability; never
    /// participates in integrity verification.
    pub symbol_key_conflict_count: u64,
    pub symbol_key_conflict_samples: Vec<SymbolKeyConflictRecord>,
}

/// Read-side port for persisted relation snapshots
///
/// Implemented by the SQLite storage adapter in the infrastructure layer so
/// the parser can load epochs without depending on concrete storage.
pub trait RelationSnapshotStore: Send + Sync {
    /// Look up the manifest for an epoch
    fn get_manifest(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Option<RelationSnapshotManifest>, StorageError>;

    /// Read the full snapshot referenced by a manifest.
    ///
    /// Verification and compaction only; the query runtime uses the shard
    /// directory and per-shard components instead.
    fn read_snapshot(
        &self,
        manifest: &RelationSnapshotManifest,
    ) -> Result<CanonicalRelationSnapshot, StorageError>;

    /// Read the lightweight routing directory of one canonical epoch.
    fn read_shard_directory(
        &self,
        project_id: i64,
        relation_epoch: i64,
    ) -> Result<ShardDirectory, StorageError>;

    /// Read the canonical rows of one shard. `None` means the shard does not
    /// belong to this epoch; empty components are a valid shard.
    fn read_shard_components(
        &self,
        project_id: i64,
        relation_epoch: i64,
        shard_id: i64,
    ) -> Result<Option<RelationShardComponents>, StorageError>;

    /// Locate one entity by its runtime ID. Implementations backed by an
    /// indexed store answer this without loading the shard.
    fn entity_location_by_id(
        &self,
        project_id: i64,
        epoch: i64,
        entity_id: u64,
    ) -> Result<Option<ShardLocation>, StorageError> {
        Ok(self
            .read_shard_directory(project_id, epoch)?
            .entity_locations
            .get(&entity_id)
            .cloned())
    }

    /// Stable symbol key of one entity, without loading the whole shard.
    fn symbol_key_by_entity_id(
        &self,
        project_id: i64,
        epoch: i64,
        entity_id: u64,
    ) -> Result<Option<StableSymbolKey>, StorageError> {
        Ok(self
            .read_shard_directory(project_id, epoch)?
            .entity_locations
            .get(&entity_id)
            .and_then(|_location| {
                self.read_shard_directory(project_id, epoch)
                    .ok()?
                    .symbol_locations
                    .iter()
                    .find(|(_, candidate)| candidate.entity_id == entity_id)
                    .map(|(key, _)| key.clone())
            }))
    }

    /// Locate one entity by its stable symbol key.
    fn entity_location_by_symbol(
        &self,
        project_id: i64,
        epoch: i64,
        key: &StableSymbolKey,
    ) -> Result<Option<ShardLocation>, StorageError> {
        Ok(self
            .read_shard_directory(project_id, epoch)?
            .symbol_locations
            .get(key)
            .cloned())
    }

    /// Locate one entity by its stable ID.
    fn entity_location_by_stable_id(
        &self,
        project_id: i64,
        epoch: i64,
        stable_id: &str,
    ) -> Result<Option<ShardLocation>, StorageError> {
        Ok(self
            .read_shard_directory(project_id, epoch)?
            .stable_id_locations
            .get(stable_id)
            .cloned())
    }

    /// Shard IDs containing entities with this simple name.
    fn name_shards(
        &self,
        project_id: i64,
        epoch: i64,
        name: &str,
    ) -> Result<Vec<i64>, StorageError> {
        Ok(self
            .read_shard_directory(project_id, epoch)?
            .name_locations
            .get(name)
            .cloned()
            .unwrap_or_default())
    }

    /// Shard owning a normalized file path.
    fn file_shard(
        &self,
        project_id: i64,
        epoch: i64,
        path: &str,
    ) -> Result<Option<i64>, StorageError> {
        Ok(self
            .read_shard_directory(project_id, epoch)?
            .file_shards
            .get(path)
            .copied())
    }

    /// Shards that contain callers of `callee`.
    fn caller_shards(
        &self,
        project_id: i64,
        epoch: i64,
        callee: u64,
    ) -> Result<Vec<i64>, StorageError> {
        Ok(self
            .read_shard_directory(project_id, epoch)?
            .target_callers
            .get(&callee)
            .cloned()
            .unwrap_or_default())
    }

    /// Shards containing edges of an external classification.
    fn classification_shards(
        &self,
        project_id: i64,
        epoch: i64,
        classification: &ExternalCallType,
    ) -> Result<Vec<i64>, StorageError> {
        Ok(self
            .read_shard_directory(project_id, epoch)?
            .classification_shards
            .get(classification)
            .cloned()
            .unwrap_or_default())
    }

    /// Aggregated counts by external classification.
    fn classification_counts(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<std::collections::HashMap<ExternalCallType, usize>, StorageError> {
        Ok(self
            .read_shard_directory(project_id, epoch)?
            .classification_counts)
    }

    /// All normalized file paths of an epoch.
    fn directory_files(&self, project_id: i64, epoch: i64) -> Result<Vec<String>, StorageError> {
        Ok(self
            .read_shard_directory(project_id, epoch)?
            .file_shards
            .into_keys()
            .collect())
    }

    /// Highest entity ID of an epoch.
    fn max_entity_id(&self, project_id: i64, epoch: i64) -> Result<u64, StorageError> {
        Ok(self
            .read_shard_directory(project_id, epoch)?
            .entity_locations
            .keys()
            .copied()
            .max()
            .unwrap_or(0))
    }

    /// Stable symbol keys, optionally restricted to a set of files.
    fn stable_symbol_keys(
        &self,
        project_id: i64,
        epoch: i64,
        files: Option<&HashSet<String>>,
    ) -> Result<Vec<StableSymbolKey>, StorageError> {
        Ok(self
            .read_shard_directory(project_id, epoch)?
            .stable_symbol_keys(files))
    }

    fn dependencies_of(
        &self,
        project_id: i64,
        epoch: i64,
        source: &str,
    ) -> Result<Vec<String>, StorageError> {
        Ok(self
            .read_shard_directory(project_id, epoch)?
            .dependencies
            .get(source)
            .cloned()
            .unwrap_or_default())
    }

    fn dependents_of(
        &self,
        project_id: i64,
        epoch: i64,
        file: &str,
    ) -> Result<Vec<String>, StorageError> {
        Ok(self
            .read_shard_directory(project_id, epoch)?
            .dependents
            .get(file)
            .cloned()
            .unwrap_or_default())
    }

    fn all_dependencies(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Vec<(String, String)>, StorageError> {
        Ok(self
            .read_shard_directory(project_id, epoch)?
            .dependencies
            .into_iter()
            .flat_map(|(source, targets)| {
                targets
                    .into_iter()
                    .map(move |target| (source.clone(), target))
            })
            .collect())
    }

    /// Walk the delta chain backwards to find the nearest base epoch
    fn find_base_epoch(
        &self,
        project_id: i64,
        delta_epoch: i64,
    ) -> Result<Option<i64>, StorageError>;

    /// Read all deltas between `after_epoch` (exclusive) and `up_to_epoch`
    /// (inclusive), ordered by epoch ascending
    fn get_delta_chain(
        &self,
        project_id: i64,
        after_epoch: i64,
        up_to_epoch: i64,
    ) -> Result<Vec<SnapshotDelta>, StorageError>;
}

impl<T: RelationSnapshotStore + ?Sized> RelationSnapshotStore for Arc<T> {
    fn get_manifest(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Option<RelationSnapshotManifest>, StorageError> {
        T::get_manifest(self.as_ref(), project_id, epoch)
    }

    fn read_snapshot(
        &self,
        manifest: &RelationSnapshotManifest,
    ) -> Result<CanonicalRelationSnapshot, StorageError> {
        T::read_snapshot(self.as_ref(), manifest)
    }

    fn read_shard_directory(
        &self,
        project_id: i64,
        relation_epoch: i64,
    ) -> Result<ShardDirectory, StorageError> {
        T::read_shard_directory(self.as_ref(), project_id, relation_epoch)
    }

    fn read_shard_components(
        &self,
        project_id: i64,
        relation_epoch: i64,
        shard_id: i64,
    ) -> Result<Option<RelationShardComponents>, StorageError> {
        T::read_shard_components(self.as_ref(), project_id, relation_epoch, shard_id)
    }

    fn entity_location_by_id(
        &self,
        project_id: i64,
        epoch: i64,
        entity_id: u64,
    ) -> Result<Option<ShardLocation>, StorageError> {
        T::entity_location_by_id(self.as_ref(), project_id, epoch, entity_id)
    }

    fn symbol_key_by_entity_id(
        &self,
        project_id: i64,
        epoch: i64,
        entity_id: u64,
    ) -> Result<Option<StableSymbolKey>, StorageError> {
        T::symbol_key_by_entity_id(self.as_ref(), project_id, epoch, entity_id)
    }

    fn entity_location_by_symbol(
        &self,
        project_id: i64,
        epoch: i64,
        key: &StableSymbolKey,
    ) -> Result<Option<ShardLocation>, StorageError> {
        T::entity_location_by_symbol(self.as_ref(), project_id, epoch, key)
    }

    fn entity_location_by_stable_id(
        &self,
        project_id: i64,
        epoch: i64,
        stable_id: &str,
    ) -> Result<Option<ShardLocation>, StorageError> {
        T::entity_location_by_stable_id(self.as_ref(), project_id, epoch, stable_id)
    }

    fn name_shards(
        &self,
        project_id: i64,
        epoch: i64,
        name: &str,
    ) -> Result<Vec<i64>, StorageError> {
        T::name_shards(self.as_ref(), project_id, epoch, name)
    }

    fn file_shard(
        &self,
        project_id: i64,
        epoch: i64,
        path: &str,
    ) -> Result<Option<i64>, StorageError> {
        T::file_shard(self.as_ref(), project_id, epoch, path)
    }

    fn caller_shards(
        &self,
        project_id: i64,
        epoch: i64,
        callee: u64,
    ) -> Result<Vec<i64>, StorageError> {
        T::caller_shards(self.as_ref(), project_id, epoch, callee)
    }

    fn classification_shards(
        &self,
        project_id: i64,
        epoch: i64,
        classification: &ExternalCallType,
    ) -> Result<Vec<i64>, StorageError> {
        T::classification_shards(self.as_ref(), project_id, epoch, classification)
    }

    fn classification_counts(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<std::collections::HashMap<ExternalCallType, usize>, StorageError> {
        T::classification_counts(self.as_ref(), project_id, epoch)
    }

    fn directory_files(&self, project_id: i64, epoch: i64) -> Result<Vec<String>, StorageError> {
        T::directory_files(self.as_ref(), project_id, epoch)
    }

    fn max_entity_id(&self, project_id: i64, epoch: i64) -> Result<u64, StorageError> {
        T::max_entity_id(self.as_ref(), project_id, epoch)
    }

    fn stable_symbol_keys(
        &self,
        project_id: i64,
        epoch: i64,
        files: Option<&HashSet<String>>,
    ) -> Result<Vec<StableSymbolKey>, StorageError> {
        T::stable_symbol_keys(self.as_ref(), project_id, epoch, files)
    }

    fn dependencies_of(
        &self,
        project_id: i64,
        epoch: i64,
        source: &str,
    ) -> Result<Vec<String>, StorageError> {
        T::dependencies_of(self.as_ref(), project_id, epoch, source)
    }

    fn dependents_of(
        &self,
        project_id: i64,
        epoch: i64,
        file: &str,
    ) -> Result<Vec<String>, StorageError> {
        T::dependents_of(self.as_ref(), project_id, epoch, file)
    }

    fn all_dependencies(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Vec<(String, String)>, StorageError> {
        T::all_dependencies(self.as_ref(), project_id, epoch)
    }

    fn find_base_epoch(
        &self,
        project_id: i64,
        delta_epoch: i64,
    ) -> Result<Option<i64>, StorageError> {
        T::find_base_epoch(self.as_ref(), project_id, delta_epoch)
    }

    fn get_delta_chain(
        &self,
        project_id: i64,
        after_epoch: i64,
        up_to_epoch: i64,
    ) -> Result<Vec<SnapshotDelta>, StorageError> {
        T::get_delta_chain(self.as_ref(), project_id, after_epoch, up_to_epoch)
    }
}
