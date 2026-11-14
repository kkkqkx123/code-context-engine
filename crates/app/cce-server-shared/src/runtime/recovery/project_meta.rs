//! Per-project metadata management over the relation contract.
//!
//! All reads go through [`RelationStorage`]; the storage branch owns the
//! transaction, callers only see typed keys.

use cce_storage_common::RelationStorage;
use cce_storage_metadb::RelationStore;
use cce_types::StorageError;

const KEY_EPOCH: &str = "epoch";
const KEY_BATCH_ID: &str = "batch_id";
const KEY_ACTIVE_EPOCH: &str = "active_epoch";
const KEY_ACTIVE_RELATION_EPOCH: &str = "active_relation_epoch";
const KEY_EPOCH_READY: &str = "epoch_ready";

/// Per-project metadata management
#[derive(Debug, Clone)]
pub struct ProjectMeta {
    pub project_id: i64,
    pub epoch: i64,
    pub batch_id: i64,
    pub active_epoch: i64,
    pub active_relation_epoch: i64,
}

impl ProjectMeta {
    /// Load metadata for a specific project.
    ///
    /// Missing or unreadable keys fall back to 0, matching the legacy
    /// loader that treated absent rows as the initial generation.
    pub async fn load(store: &RelationStore, project_id: i64) -> Result<Self, StorageError> {
        async fn get(store: &RelationStore, project_id: i64, key: &str) -> i64 {
            store
                .project_meta_get_int_optional(project_id, key)
                .await
                .ok()
                .flatten()
                .unwrap_or(0)
        }

        Ok(ProjectMeta {
            project_id,
            epoch: get(store, project_id, KEY_EPOCH).await,
            batch_id: get(store, project_id, KEY_BATCH_ID).await,
            active_epoch: get(store, project_id, KEY_ACTIVE_EPOCH).await,
            active_relation_epoch: get(store, project_id, KEY_ACTIVE_RELATION_EPOCH).await,
        })
    }

    /// Initialize metadata keys for a new project without overwriting
    /// existing rows.
    pub async fn init_for_project(
        store: &RelationStore,
        project_id: i64,
    ) -> Result<(), StorageError> {
        for key in [
            KEY_EPOCH,
            KEY_BATCH_ID,
            KEY_ACTIVE_EPOCH,
            KEY_ACTIVE_RELATION_EPOCH,
            KEY_EPOCH_READY,
        ] {
            if store
                .project_meta_get_int_optional(project_id, key)
                .await?
                .is_none()
            {
                store.project_meta_set_int(project_id, key, 0).await?;
            }
        }
        Ok(())
    }

    /// Update epoch for project
    pub async fn update_epoch(
        store: &RelationStore,
        project_id: i64,
        epoch: i64,
    ) -> Result<(), StorageError> {
        store
            .project_meta_set_int(project_id, KEY_EPOCH, epoch)
            .await
    }

    /// Update batch_id for project
    pub async fn update_batch_id(
        store: &RelationStore,
        project_id: i64,
        batch_id: i64,
    ) -> Result<(), StorageError> {
        store
            .project_meta_set_int(project_id, KEY_BATCH_ID, batch_id)
            .await
    }

    /// Update active_epoch for project, keeping monotonicity.
    ///
    /// Older epochs never overwrite newer ones. The check-then-set runs in
    /// the caller because the contract exposes no compare-and-swap; every
    /// production writer (manifest activation) already serializes through
    /// the operation queue, so concurrent racing cannot regress the epoch.
    pub async fn update_active_epoch(
        store: &RelationStore,
        project_id: i64,
        active_epoch: i64,
    ) -> Result<(), StorageError> {
        let current = store
            .project_meta_get_int_optional(project_id, KEY_ACTIVE_EPOCH)
            .await?
            .unwrap_or(0);
        if active_epoch <= current {
            tracing::warn!(
                project_id,
                active_epoch,
                "Failed to update active_epoch: current epoch is equal or newer"
            );
            return Ok(());
        }
        store
            .project_meta_set_int(project_id, KEY_ACTIVE_EPOCH, active_epoch)
            .await
    }
}
