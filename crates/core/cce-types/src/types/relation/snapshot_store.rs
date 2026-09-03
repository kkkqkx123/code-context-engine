//! Relation snapshot persistence contract
//!
//! The parser's `RelationSnapshotLoader` reads persisted relationship
//! snapshots through the [`RelationSnapshotStore`] port instead of touching
//! SQLite directly. The concrete SQLite adapter lives in
//! `cce_infrastructure::storage::sqlite::SnapshotStoreAdapter`.

use crate::types::relation::{CanonicalRelationSnapshot, SymbolKeyConflictRecord};
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

    /// Read the full snapshot referenced by a manifest
    fn read_snapshot(
        &self,
        manifest: &RelationSnapshotManifest,
    ) -> Result<CanonicalRelationSnapshot, StorageError>;

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
