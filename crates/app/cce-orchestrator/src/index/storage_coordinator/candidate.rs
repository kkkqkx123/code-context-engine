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

use cce_storage_common::MetadataStorage;

use crate::error::OrchestratorError;

use super::StorageCoordinator;

mod lifecycle;
mod prepare;

impl StorageCoordinator {
    /// Clear every candidate-generation row plus its override registrations.
    ///
    /// Idempotent: used when a fresh candidate reuses an epoch number left
    /// behind by an interrupted run, and by the failure path so the number
    /// becomes safe to reuse.
    pub(crate) async fn cleanup_failed_candidate(
        &self,
        epoch: i64,
    ) -> Result<(), OrchestratorError> {
        let Some(store) = self.metadata_store.as_ref() else {
            return Ok(());
        };
        store
            .candidate_clear_epoch(self.project_id, epoch)
            .await
            .map(|_| ())
            .map_err(OrchestratorError::Storage)
    }
}

#[cfg(test)]
mod tests {
    use crate::index::vector_store::{MetadataStore, in_memory_metadata_store};
    use cce_storage_common::MetadataStorage;
    use std::path::Path;

    use super::super::StorageCoordinator;

    #[tokio::test]
    async fn changed_and_deleted_files_register_generation_overrides() {
        let database = in_memory_metadata_store().expect("in-memory store");
        database
            .ensure_project(1, "/tmp/test")
            .await
            .expect("project should be ensured");
        database
            .manifest_begin_building(1, 1, "initial", None)
            .await
            .expect("initial manifest should be created");
        database
            .manifest_activate(1, 1, 0, "initial", None)
            .await
            .expect("initial generation should be activated");

        let storage = StorageCoordinator::new(1)
            .expect("valid project ID")
            .with_metadata_store(database.clone())
            .with_epoch(1);
        storage
            .begin_hot_update_candidate("hot-operation", false)
            .await
            .expect("candidate should be prepared");

        // A deleted file inside the candidate registers a `deleted` override;
        // a repeat registration is idempotent.
        storage
            .register_deleted_file(Path::new("src/changed.rs"))
            .await
            .expect("deletion should register deleted");
        storage
            .register_deleted_file(Path::new("src/changed.rs"))
            .await
            .expect("repeat deletion stays idempotent");
        storage
            .register_deleted_file(Path::new("src/gone.rs"))
            .await
            .expect("pure deletion registers deleted");

        // The batch form registers every file in one pass and must refuse to
        // run without a candidate, where no candidate epoch exists to scope
        // the external delete to.
        storage
            .register_deleted_files(&[Path::new("src/batch_a.rs"), Path::new("src/batch_b.rs")])
            .await
            .expect("batch deletion should register deleted");
        let standalone = in_memory_metadata_store().expect("in-memory store");
        let without_candidate = StorageCoordinator::new(1)
            .expect("valid project ID")
            .with_metadata_store(standalone)
            .with_epoch(1);
        let error = without_candidate
            .register_deleted_files(&[Path::new("src/orphan.rs")])
            .await
            .expect_err("batch deletion without a candidate must fail");
        assert!(
            error
                .to_string()
                .contains("requires a prepared hot-update candidate"),
            "unexpected error: {error}"
        );

        let overrides = database
            .overrides_for_generation(1, 2)
            .await
            .expect("overrides should list");
        assert_eq!(
            overrides
                .into_iter()
                .map(|entry| entry.file_path)
                .collect::<Vec<_>>(),
            vec![
                "src/batch_a.rs".to_string(),
                "src/batch_b.rs".to_string(),
                "src/changed.rs".to_string(),
                "src/gone.rs".to_string(),
            ]
        );

        // The failure path clears the registrations so the epoch number can
        // be reused safely.
        storage
            .fail_hot_update_candidate("hot-operation", "test failure")
            .await
            .expect("candidate should fail cleanly");
        assert!(
            database
                .overrides_for_generation(1, 2)
                .await
                .expect("overrides should list")
                .is_empty()
        );
    }
}
