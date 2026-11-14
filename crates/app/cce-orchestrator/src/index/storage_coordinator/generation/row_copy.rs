//! Cross-generation copying of single-file rows.
//!
//! Supports chunking-drift sweeps: a re-chunked unchanged file receives own
//! target-generation rows so the vector-store flow can regenerate its detail
//! mappings against fresh point ids.

use cce_storage_common::RelationStorage;

use crate::error::OrchestratorError;

use super::StorageCoordinator;

impl StorageCoordinator {
    /// Copy one file's `files` and `entities` rows from an ancestor
    /// generation into the target generation.
    ///
    /// Used by chunking-drift sweeps: a re-chunked unchanged file gets own
    /// target-epoch entity rows so the vector-store flow can write fresh
    /// `entity_detail_mappings` pointing at the new point ids. Without the
    /// copy, its mappings stay behind in the ancestor generation and are lost
    /// once compaction retires that ancestor.
    ///
    /// No-op when the file already owns a target-generation row (it is being
    /// handled by the normal change flow) or has no source row. Parent
    /// references are remapped to the copied rows. Existing detail mappings
    /// and chunks are deliberately not copied: mappings regenerate against
    /// the freshly stored chunks, and the sweep writes the chunks itself.
    ///
    /// Returns the number of copied entity rows (0 = no-op).
    pub(crate) async fn copy_file_rows_between_epochs(
        &self,
        source_epoch: i64,
        target_epoch: i64,
        path: &str,
    ) -> Result<usize, OrchestratorError> {
        let Some(store) = self.metadata_store.as_ref() else {
            return Ok(0);
        };
        store
            .generation_copy_file(self.project_id, source_epoch, target_epoch, path)
            .await
            .map_err(OrchestratorError::Storage)
    }
}

#[cfg(test)]
mod tests {
    use cce_storage_common::RelationStorage;
    use cce_storage_common::metadb::EntityRecord;
    use cce_storage_metadb::RelationStore;

    use super::super::StorageCoordinator;

    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime")
            .block_on(future)
    }

    async fn seed_sweep_source_file(store: &RelationStore) -> i64 {
        store
            .ensure_project(1, "/tmp/test")
            .await
            .expect("project should be ensured");
        store
            .files_record_hashes(1, 5, &[("src/lib.rs".to_string(), "hash-1".to_string())])
            .await
            .expect("seed source generation");
        let file = store
            .file_by_path(1, "src/lib.rs")
            .await
            .expect("file should load")
            .expect("file should exist");
        let parent = EntityRecord {
            id: 0,
            name: "alpha".to_string(),
            kind: "function".to_string(),
            file_id: file.id,
            signature: Some("fn alpha()".to_string()),
            span_start_row: None,
            span_end_row: None,
            span_start_column: None,
            span_end_column: None,
            span_start_byte: None,
            span_end_byte: None,
            scoped_name: Some("alpha".to_string()),
            overload_discriminator: String::new(),
            depth: None,
            parent_id: None,
            metadata: Some("{\"__source_entity_id\":\"100\"}".to_string()),
            parameters_json: None,
            return_type: None,
            doc_comment: None,
            modifiers_json: None,
            project_id: 1,
            epoch: 5,
            batch_id: 2,
            rank: 0.0,
        };
        let ids = store
            .entities_insert_returning_ids(&[parent])
            .await
            .expect("parent entity should insert");
        let child = EntityRecord {
            id: 0,
            name: "nested".to_string(),
            kind: "function".to_string(),
            file_id: file.id,
            signature: Some("fn nested()".to_string()),
            span_start_row: None,
            span_end_row: None,
            span_start_column: None,
            span_end_column: None,
            span_start_byte: None,
            span_end_byte: None,
            scoped_name: Some("alpha::nested".to_string()),
            overload_discriminator: String::new(),
            depth: None,
            parent_id: Some(ids[0]),
            metadata: Some("{\"__source_entity_id\":\"101\"}".to_string()),
            parameters_json: None,
            return_type: None,
            doc_comment: None,
            modifiers_json: None,
            project_id: 1,
            epoch: 5,
            batch_id: 2,
            rank: 0.0,
        };
        store
            .entities_insert_returning_ids(&[child])
            .await
            .expect("child entity should insert");
        file.id
    }

    #[test]
    fn copy_file_rows_between_epochs_materializes_entities() {
        let database = RelationStore::in_memory().expect("in-memory store");
        block_on(seed_sweep_source_file(&database));

        let storage = StorageCoordinator::new(1)
            .expect("valid project ID")
            .with_metadata_store(database.clone());

        let copied = block_on(storage.copy_file_rows_between_epochs(5, 6, "src/lib.rs"))
            .expect("copy must succeed");
        assert_eq!(copied, 2, "both entities must be copied");

        let target = block_on(database.file_by_path(1, "src/lib.rs"))
            .expect("target file should load")
            .expect("target file should exist");
        assert_eq!(
            target.content_hash.as_deref(),
            Some("hash-1"),
            "content hash must survive the copy"
        );
        let copied_entities =
            block_on(database.entities_with_files(1, 6)).expect("copied entities should load");
        assert_eq!(copied_entities.len(), 2, "both entities must be copied");
        let parent = copied_entities
            .iter()
            .find(|entry| entry.entity.name == "alpha")
            .expect("parent entity should exist");
        let child = copied_entities
            .iter()
            .find(|entry| entry.entity.name == "nested")
            .expect("child entity should exist");
        assert_ne!(parent.entity.id, child.entity.id);
        assert_eq!(
            child.entity.parent_id,
            Some(parent.entity.id),
            "parent references must be remapped to the copied rows"
        );
        assert!(
            parent
                .entity
                .metadata
                .as_deref()
                .unwrap_or_default()
                .contains("__source_entity_id"),
            "source entity ids must survive the copy so detail mappings regenerate"
        );

        // A second copy is a no-op: the target already owns the file row.
        let again = block_on(storage.copy_file_rows_between_epochs(5, 6, "src/lib.rs"))
            .expect("second copy must succeed");
        assert_eq!(again, 0, "existing target rows must not be duplicated");
    }

    #[test]
    fn copy_file_rows_is_a_noop_for_unknown_paths() {
        let database = RelationStore::in_memory().expect("in-memory store");
        block_on(database.ensure_project(1, "/tmp/test")).expect("project should be ensured");

        let storage = StorageCoordinator::new(1)
            .expect("valid project ID")
            .with_metadata_store(database);
        let copied = block_on(storage.copy_file_rows_between_epochs(5, 6, "missing.rs"))
            .expect("unknown path is a legitimate no-op");
        assert_eq!(copied, 0);
    }
}
