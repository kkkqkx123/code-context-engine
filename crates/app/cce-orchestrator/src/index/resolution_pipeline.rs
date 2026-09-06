//! Canonical relationship epoch writer.
//!
//! Resolution belongs exclusively to `IndexBuilder`. This service persists the
//! already-resolved canonical graph and never attempts target resolution itself.

use std::collections::HashSet;

use cce_storage_sqlite::SqliteClient;
use cce_storage_sqlite::repo::RelationSnapshotRepository;
use cce_storage_sqlite::repo::RelationSnapshotState;
use cce_types::{
    CanonicalRelationSnapshot, CanonicalRelationTarget, StableSymbolKey, StorageError,
};

pub struct ResolutionPipelineService {
    db_client: SqliteClient,
}

impl ResolutionPipelineService {
    pub fn new(db_client: SqliteClient) -> Self {
        Self { db_client }
    }

    /// Allocate a building epoch and write snapshot data (without activating).
    ///
    /// Returns the epoch number on success. The caller should update memory
    /// (e.g. `RelationRuntime::publish_snapshot`) before calling [`Self::activate`].
    pub fn allocate_and_write(
        &self,
        project_id: i64,
        operation_id: &str,
        snapshot: &CanonicalRelationSnapshot,
    ) -> Result<i64, StorageError> {
        snapshot
            .validate_versions()
            .map_err(StorageError::Validation)?;
        validate_snapshot(snapshot).map_err(StorageError::Validation)?;

        let input_fingerprint = snapshot.input_fingerprint();
        let snapshot_fingerprint = snapshot.fingerprint();

        if let Some(existing) = self.db_client.with_transaction(|tx| {
            RelationSnapshotRepository::get_manifest_by_operation(tx, project_id, operation_id)
        })? {
            match existing.state {
                RelationSnapshotState::Active | RelationSnapshotState::Ready => {
                    if existing.snapshot_fingerprint.as_deref()
                        != Some(snapshot_fingerprint.as_str())
                    {
                        return Err(StorageError::Validation(format!(
                            "existing relation operation {operation_id} has a different snapshot"
                        )));
                    }
                    return Ok(existing.relation_epoch);
                }
                RelationSnapshotState::Failed => {
                    self.db_client.with_transaction(|tx| {
                        RelationSnapshotRepository::retry_failed(
                            tx,
                            project_id,
                            existing.relation_epoch,
                        )
                    })?;
                }
                RelationSnapshotState::Building | RelationSnapshotState::Delta => {}
            }
        }

        let epoch = self.db_client.with_transaction(|tx| {
            RelationSnapshotRepository::allocate_building(
                tx,
                project_id,
                operation_id,
                &snapshot.config_fingerprint,
            )
        })?;

        let write_result = self.db_client.with_transaction(|tx| {
            RelationSnapshotRepository::write_snapshot_and_mark_ready(
                tx,
                project_id,
                epoch,
                snapshot,
                &input_fingerprint,
                &snapshot_fingerprint,
            )
        });
        if let Err(error) = write_result {
            let reason = error.to_string();
            let _ = self.db_client.with_transaction(|tx| {
                RelationSnapshotRepository::mark_failed(tx, project_id, epoch, &reason)
            });
            return Err(error);
        }

        Ok(epoch)
    }

    /// Activate a previously allocated and written epoch.
    ///
    /// This is intentionally a repository operation. Business paths must use
    /// `RelationSnapshotPublisher` so the runtime and SQLite epoch move
    /// together.
    pub fn activate(&self, project_id: i64, epoch: i64) -> Result<(), StorageError> {
        self.db_client
            .with_transaction(|tx| RelationSnapshotRepository::activate(tx, project_id, epoch))?;
        let stale_before = chrono::Utc::now().timestamp() - 24 * 60 * 60;
        if let Err(error) = self.db_client.with_transaction(|tx| {
            RelationSnapshotRepository::collect_garbage(tx, project_id, stale_before).map(|_| ())
        }) {
            tracing::warn!(project_id, error = %error, "Relation epoch garbage collection failed");
        }
        Ok(())
    }

    /// Mark an uncommitted candidate as failed.
    pub fn mark_failed(
        &self,
        project_id: i64,
        epoch: i64,
        reason: &str,
    ) -> Result<(), StorageError> {
        self.db_client.with_transaction(|tx| {
            RelationSnapshotRepository::mark_failed(tx, project_id, epoch, reason)
        })
    }
}

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
        return Err("canonical snapshot contains duplicate stable symbol keys".to_string());
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
    use super::*;

    fn insert_project(client: &SqliteClient) {
        let conn = client
            .write_connection()
            .expect("test connection should open");
        conn.execute(
            "INSERT INTO projects
                (id, name, root_path, config_file_path, created_at, updated_at)
             VALUES (1, 'test', '/tmp/test', '.cce/config.json', 1, 1)",
            [],
        )
        .expect("test project should be inserted");
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
        });
        assert!(validate_snapshot(&snapshot).is_err());
    }

    #[test]
    fn failed_epoch_preserves_previous_active_epoch() {
        let client = SqliteClient::in_memory().expect("in-memory database should open");
        insert_project(&client);
        let snapshot = CanonicalRelationSnapshot::new("config".to_string());
        let writer = ResolutionPipelineService::new(client.clone());

        let first_epoch = writer
            .allocate_and_write(1, "operation-1", &snapshot)
            .expect("initial epoch should be written");
        writer
            .activate(1, first_epoch)
            .expect("initial epoch should activate");

        let failed_epoch = writer
            .allocate_and_write(1, "operation-2", &snapshot)
            .expect("second epoch should be written");
        writer
            .mark_failed(1, failed_epoch, "simulated write failure")
            .expect("second epoch should be marked failed");

        let manifest = client
            .with_transaction(|tx| {
                RelationSnapshotRepository::get_manifest_by_operation(tx, 1, "operation-2")
            })
            .expect("manifest query should succeed")
            .expect("second operation manifest should exist");
        assert_eq!(manifest.state, RelationSnapshotState::Failed);

        assert_eq!(
            client
                .project_meta_get_int(1, "active_relation_epoch")
                .expect("active epoch should remain readable"),
            first_epoch
        );
    }
}
