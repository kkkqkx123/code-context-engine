//! Canonical relationship epoch writer.
//!
//! Resolution belongs exclusively to `IndexBuilder`. This service persists the
//! already-resolved canonical graph and never attempts target resolution itself.

use cce_storage_common::MetadataStorage;
use cce_types::relation::RelationSnapshotState;
use cce_types::{CanonicalRelationSnapshot, CanonicalSnapshotArtifact, StorageError};
#[cfg(test)]
use cce_types::{CanonicalRelationTarget, StableSymbolKey};
#[cfg(test)]
use std::collections::HashSet;

use super::vector_store::MetadataStore;

pub struct ResolutionPipelineService {
    store: MetadataStore,
}

impl ResolutionPipelineService {
    pub fn new(store: MetadataStore) -> Self {
        Self { store }
    }

    /// Write an already materialized snapshot.
    ///
    /// Compaction and tests own a snapshot value; the full-index build path
    /// calls [`Self::allocate_and_write_artifact`] directly so it never
    /// materializes a second full graph.
    pub async fn allocate_and_write(
        &self,
        project_id: i64,
        operation_id: &str,
        snapshot: &CanonicalRelationSnapshot,
    ) -> Result<i64, StorageError> {
        let artifact = CanonicalSnapshotArtifact::from_snapshot(snapshot, Default::default())?;
        self.allocate_and_write_artifact(project_id, operation_id, &artifact)
            .await
    }

    /// Allocate a building epoch and persist a canonical artifact.
    ///
    /// The artifact has already computed both fingerprints and carries every
    /// canonical row on disk, so publication does not hold a full snapshot in
    /// memory alongside the relation index.
    pub async fn allocate_and_write_artifact(
        &self,
        project_id: i64,
        operation_id: &str,
        artifact: &CanonicalSnapshotArtifact,
    ) -> Result<i64, StorageError> {
        let header = artifact.header();
        if header.schema_version != cce_types::RELATION_SNAPSHOT_SCHEMA_VERSION
            || header.parser_version != cce_types::RELATION_PARSER_VERSION
            || header.resolver_version != cce_types::RELATION_RESOLVER_VERSION
            || header.path_normalization_version != cce_types::RELATION_PATH_NORMALIZATION_VERSION
        {
            return Err(StorageError::Validation(
                "relationship snapshot version is not supported".to_string(),
            ));
        }

        let snapshot_fingerprint = artifact.snapshot_fingerprint();
        let store = &self.store;

        if let Some(existing) = store
            .snapshot_manifest_by_operation(project_id, operation_id)
            .await?
        {
            match existing.state {
                RelationSnapshotState::Active | RelationSnapshotState::Ready => {
                    if existing.snapshot_fingerprint.as_deref() != Some(snapshot_fingerprint) {
                        return Err(StorageError::Validation(format!(
                            "existing relation operation {operation_id} has a different snapshot"
                        )));
                    }
                    return Ok(existing.relation_epoch);
                }
                RelationSnapshotState::Failed => {
                    store
                        .snapshot_retry_failed(project_id, existing.relation_epoch)
                        .await?;
                }
                RelationSnapshotState::Building | RelationSnapshotState::Delta => {}
            }
        }

        let epoch = store
            .snapshot_allocate(project_id, operation_id, &header.config_fingerprint)
            .await?;

        let write_result = store
            .snapshot_write_artifact(project_id, epoch, artifact)
            .await;
        if let Err(error) = write_result {
            let reason = error.to_string();
            if let Err(mark_error) = store.snapshot_mark_failed(project_id, epoch, &reason).await {
                tracing::error!(
                    project_id,
                    epoch,
                    error = %mark_error,
                    "Failed to mark relation snapshot building as failed after write error"
                );
            }
            return Err(error);
        }

        Ok(epoch)
    }

    /// Activate a previously allocated and written epoch.
    ///
    /// Business paths must use `RelationSnapshotPublisher` so the runtime
    /// and the relation epoch move together.
    pub async fn activate(&self, project_id: i64, epoch: i64) -> Result<(), StorageError> {
        self.store.snapshot_activate(project_id, epoch).await?;
        let stale_before = chrono::Utc::now().timestamp() - 24 * 60 * 60;
        if let Err(error) = self
            .store
            .snapshot_collect_garbage(project_id, stale_before)
            .await
        {
            tracing::warn!(project_id, error = %error, "Relation epoch garbage collection failed");
        }
        Ok(())
    }

    /// Mark an uncommitted candidate as failed.
    pub async fn mark_failed(
        &self,
        project_id: i64,
        epoch: i64,
        reason: &str,
    ) -> Result<(), StorageError> {
        self.store
            .snapshot_mark_failed(project_id, epoch, reason)
            .await
    }
}

#[cfg(test)]
fn validate_snapshot(snapshot: &CanonicalRelationSnapshot) -> Result<(), String> {
    let files: HashSet<&str> = snapshot
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    if files.len() != snapshot.files.len() {
        return Err("canonical snapshot contains duplicate file paths".to_string());
    }

    let entities: HashSet<&StableSymbolKey> =
        snapshot.entities.iter().map(|entity| &entity.key).collect();
    if entities.len() != snapshot.entities.len() {
        let mut seen = HashSet::new();
        let mut dupes: Vec<String> = Vec::new();
        for entity in &snapshot.entities {
            if !seen.insert(&entity.key) && dupes.len() < 5 {
                dupes.push(entity.key.sort_key());
            }
        }
        return Err(format!(
            "canonical snapshot contains duplicate stable symbol keys: {}",
            dupes.join(", ")
        ));
    }
    for entity in &snapshot.entities {
        if !files.contains(entity.key.file_path.as_str()) {
            return Err(format!(
                "entity {} references a missing file",
                entity.key.scoped_name
            ));
        }
        if let Some(parent) = &entity.parent
            && !entities.contains(parent)
        {
            return Err(format!(
                "entity {} references a missing parent",
                entity.key.scoped_name
            ));
        }
    }
    for relation in &snapshot.relations {
        if !entities.contains(&relation.caller) {
            // File-scoped edges are canonicalized under the per-file
            // placeholder caller `(path, "<file>", Module)`; it is valid when
            // the referenced file exists in the snapshot.
            if !(relation.caller.is_file_placeholder()
                && files.contains(relation.caller.file_path.as_str()))
            {
                return Err(format!(
                    "relation to '{}' references a missing caller {}",
                    relation.raw_target,
                    relation.caller.sort_key()
                ));
            }
        }
        if let CanonicalRelationTarget::Internal { key } = &relation.target
            && !entities.contains(key)
        {
            return Err(format!(
                "relation {} references a missing internal target",
                relation.raw_target
            ));
        }
    }
    for dependency in &snapshot.dependencies {
        if !files.contains(dependency.source_file.as_str()) {
            return Err(format!(
                "dependency source {} is not in the snapshot",
                dependency.source_file
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::vector_store::in_memory_metadata_store;
    use super::*;

    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime")
            .block_on(future)
    }

    fn insert_project(store: &MetadataStore) {
        block_on(store.ensure_project(1, "/tmp/test")).expect("test project should be inserted");
    }

    #[test]
    fn rejects_missing_internal_target() {
        let mut snapshot = CanonicalRelationSnapshot::new("config".to_string());
        let key = StableSymbolKey::new(
            "src/lib.rs",
            "missing",
            cce_types::EntityKind::Function,
            "fn missing()",
        );
        snapshot.relations.push(cce_types::CanonicalRelation {
            caller: key.clone(),
            target: CanonicalRelationTarget::Internal { key },
            raw_target: "missing".to_string(),
            relation_type: cce_types::RelationType::DirectCall,
            span: cce_types::Span::default(),
            stdlib_category: None,
            overload_signature: None,
            callee_symbol: None,
            owner_type: None,
            call_context: Default::default(),
            call_frequency: 1,
            cfg_condition: None,
        });
        assert!(validate_snapshot(&snapshot).is_err());
    }

    #[test]
    fn failed_epoch_preserves_previous_active_epoch() {
        let store = in_memory_metadata_store().expect("in-memory store should open");
        insert_project(&store);
        let snapshot = CanonicalRelationSnapshot::new("config".to_string());
        let writer = ResolutionPipelineService::new(store.clone());

        let first_epoch = block_on(writer.allocate_and_write(1, "operation-1", &snapshot))
            .expect("initial epoch should be written");
        block_on(writer.activate(1, first_epoch)).expect("initial epoch should activate");

        let failed_epoch = block_on(writer.allocate_and_write(1, "operation-2", &snapshot))
            .expect("second epoch should be written");
        block_on(writer.mark_failed(1, failed_epoch, "simulated write failure"))
            .expect("second epoch should be marked failed");

        let manifest = block_on(store.snapshot_manifest_by_operation(1, "operation-2"))
            .expect("manifest query should succeed")
            .expect("second operation manifest should exist");
        assert_eq!(manifest.state, RelationSnapshotState::Failed);

        assert_eq!(
            block_on(store.project_meta_get_int(1, "active_relation_epoch"))
                .expect("active epoch should remain readable"),
            first_epoch
        );
    }
}
