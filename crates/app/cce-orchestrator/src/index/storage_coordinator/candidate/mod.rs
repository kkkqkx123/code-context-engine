//! Hot-update candidate generation.
//!
//! Under the zero-copy inheritance model a hot-update registers the candidate
//! as inheriting from the active generation instead of cloning it: unchanged
//! files stay in the parent epoch and are resolved through the parent chain,
//! while changed/deleted files are hidden there via `generation_overrides`.
//! This module owns candidate preparation, resume, activation and per-file
//! candidate cleanup. Physical reclamation of superseded generations happens
//! exclusively in generation GC; when the inheritance chain would grow past
//! depth 2, the active generation is compacted (materialized) first.

use cce_storage_metadb_sqlite::GenerationOverrideRepository;

use crate::error::OrchestratorError;

use super::StorageCoordinator;

mod lifecycle;
mod prepare;

impl StorageCoordinator {
    /// Transaction-scoped variant: drop every candidate-generation row from
    /// the five data tables.
    ///
    /// Idempotent: used when a fresh candidate reuses an epoch number left
    /// behind by an interrupted run, and by the failure path so the number
    /// becomes safe to reuse.
    pub(crate) fn clear_candidate_rows_tx(
        tx: &rusqlite::Transaction<'_>,
        project_id: i64,
        epoch: i64,
    ) -> Result<(), cce_types::StorageError> {
        for (table, sql) in [
            (
                "file_summaries",
                "DELETE FROM file_summaries WHERE epoch = ?2 AND file_id IN
             (SELECT id FROM files WHERE project_id = ?1 AND epoch = ?2)",
            ),
            (
                "entity_detail_mappings",
                "DELETE FROM entity_detail_mappings WHERE project_id = ?1 AND epoch = ?2",
            ),
            (
                "chunks",
                "DELETE FROM chunks WHERE project_id = ?1 AND epoch = ?2",
            ),
            (
                "entities",
                "DELETE FROM entities WHERE project_id = ?1 AND epoch = ?2",
            ),
            (
                "files",
                "DELETE FROM files WHERE project_id = ?1 AND epoch = ?2",
            ),
        ] {
            tx.execute(sql, rusqlite::params![project_id, epoch])
                .map_err(|error| {
                    cce_types::StorageError::delete(
                        table,
                        format!("failed to clear candidate generation: {error}"),
                    )
                })?;
        }
        Ok(())
    }

    /// Clear every candidate-generation row from the five data tables plus
    /// its override registrations.
    ///
    /// Idempotent: used when a fresh candidate reuses an epoch number left
    /// behind by an interrupted run, and by the failure path so the number
    /// becomes safe to reuse.
    pub(crate) fn cleanup_failed_candidate(&self, epoch: i64) -> Result<(), OrchestratorError> {
        let Some(client) = self.metadata_store.as_ref().map(|store| store.as_ref()) else {
            return Ok(());
        };
        client
            .with_transaction(|tx| {
                Self::clear_candidate_rows_tx(tx, self.project_id, epoch)?;
                GenerationOverrideRepository::clear_generation(tx, self.project_id, epoch)
            })
            .map_err(OrchestratorError::Storage)
    }
}

#[cfg(test)]
mod tests {
    use super::super::StorageCoordinator;
    use cce_storage_metadb_sqlite::{
        NewProjectRecord, ProjectIndexManifestRepository, ProjectRepository, SqliteClient,
    };
    use std::sync::Arc;

    #[tokio::test]
    async fn changed_and_deleted_files_register_generation_overrides() {
        use cce_storage_metadb_sqlite::{
            GenerationOverride, GenerationOverrideRepository, OverrideDisposition,
        };
        use std::path::Path;

        let database = Arc::new(SqliteClient::in_memory().expect("in-memory database"));
        let client = database.as_ref().clone();
        client
            .with_transaction(|tx| {
                ProjectRepository::insert(
                    tx,
                    &NewProjectRecord::new("test".to_string(), "/tmp/test".to_string()),
                )?;
                ProjectIndexManifestRepository::activate(tx, 1, 1, 0, "initial", None)?;
                Ok(())
            })
            .expect("initial generation should be created");

        let storage = StorageCoordinator::new(1)
            .expect("valid project ID")
            .with_metadata_store(database)
            .with_epoch(1);
        storage
            .begin_hot_update_candidate("hot-operation", false)
            .await
            .expect("candidate should be prepared");

        // A replaced file gets a `replaced` override; a later deletion of a
        // prepared file upgrades it to `deleted`.
        storage
            .prepare_hot_update_file(Path::new("src/changed.rs"))
            .await
            .expect("prepare should register replaced");
        storage
            .register_deleted_file(Path::new("src/changed.rs"))
            .await
            .expect("deletion should upgrade to deleted");
        storage
            .register_deleted_file(Path::new("src/gone.rs"))
            .await
            .expect("pure deletion registers deleted");

        let conn = client.read_connection().expect("connection should open");
        let overrides = GenerationOverrideRepository::list_for_generation(&conn, 1, 2)
            .expect("overrides should list");
        assert_eq!(
            overrides,
            vec![
                GenerationOverride {
                    file_path: "src/changed.rs".to_string(),
                    disposition: OverrideDisposition::Deleted,
                },
                GenerationOverride {
                    file_path: "src/gone.rs".to_string(),
                    disposition: OverrideDisposition::Deleted,
                },
            ]
        );

        // The failure path clears the registrations so the epoch number can
        // be reused safely.
        drop(conn);
        storage
            .fail_hot_update_candidate("hot-operation", "test failure")
            .expect("candidate should fail cleanly");
        let conn = client.read_connection().expect("connection should open");
        assert!(
            GenerationOverrideRepository::list_for_generation(&conn, 1, 2)
                .expect("overrides should list")
                .is_empty()
        );
    }
}
