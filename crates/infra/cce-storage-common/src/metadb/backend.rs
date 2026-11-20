//! Deterministic backend handle for metadata persistence.

use cce_types::relation::RelationSnapshotStore;
use cce_types::{CanonicalRelationSnapshot, RelationSnapshotManifest, SnapshotDelta, StorageError};

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

impl<T: MetadataProjectScope + ?Sized> MetadataProjectScope for std::sync::Arc<T> {
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
