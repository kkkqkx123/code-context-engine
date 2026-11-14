//! Manifest lifecycle: starting, publishing, failing and reading index
//! generations.
//!
//! Owns the epoch/batch counters and their durable mirrors in `project_meta`,
//! the project index manifest state transitions, and the cheap symbol-table
//! drift check performed after publication.

use std::sync::atomic::Ordering;

use cce_storage_common::MetadataStorage;

use crate::error::OrchestratorError;

use super::StorageCoordinator;

impl StorageCoordinator {
    /// Start a new project index version and make subsequent writes target it.
    pub async fn begin_full_index(&self) -> Result<i64, OrchestratorError> {
        let Some(store) = &self.metadata_store else {
            return Ok(self.epoch());
        };
        // Missing meta rows are the legitimate first-run defaults (epoch 0,
        // ready 1); unparseable values and DB failures are propagated so a
        // broken metadata store cannot silently restart from epoch 0.
        let current = store
            .project_meta_get_int_optional(self.project_id, "epoch")
            .await
            .map_err(OrchestratorError::Storage)?
            .unwrap_or(0);
        let ready = store
            .project_meta_get_int_optional(self.project_id, "epoch_ready")
            .await
            .map_err(OrchestratorError::Storage)?
            .unwrap_or(1);
        // A completed manifest can still have an in-progress checkpoint when
        // the process crashed between publication and checkpoint completion.
        // Reuse that epoch so recovery finalizes the durable publication
        // instead of creating an unrelated generation.
        let has_incomplete_checkpoint = store
            .manifest_has_incomplete(self.project_id, "full_index")
            .await
            .map_err(OrchestratorError::Storage)?;
        let epoch = if current > 0 && (ready == 0 || has_incomplete_checkpoint) {
            current
        } else {
            current + 1
        };
        self.epoch.store(epoch, Ordering::Release);
        self.batch_id.store(0, Ordering::Release);
        store
            .project_meta_set_int(self.project_id, "epoch", epoch)
            .await
            .map_err(OrchestratorError::Storage)?;
        store
            .project_meta_set_int(self.project_id, "batch_id", 0)
            .await
            .map_err(OrchestratorError::Storage)?;
        store
            .project_meta_set_int(self.project_id, "epoch_ready", 0)
            .await
            .map_err(OrchestratorError::Storage)?;
        Ok(epoch)
    }

    /// Set the durable batch version used by all storage backends.
    pub async fn begin_batch(&self, batch_id: i64) -> Result<(), OrchestratorError> {
        self.batch_id.store(batch_id, Ordering::Release);
        if let Some(store) = self.metadata_store.as_ref() {
            store
                .project_meta_set_int(self.project_id, "batch_id", batch_id)
                .await
                .map_err(OrchestratorError::Storage)?;
        }
        Ok(())
    }

    /// Atomically expose the completed index epoch to project queries.
    pub async fn activate_current_epoch(&self) -> Result<(), OrchestratorError> {
        if let Some(store) = self.metadata_store.as_ref() {
            store
                .project_meta_set_int(self.project_id, "active_epoch", self.epoch())
                .await
                .map_err(OrchestratorError::Storage)?;
            store
                .project_meta_set_int(self.project_id, "epoch_ready", 1)
                .await
                .map_err(OrchestratorError::Storage)?;
        }
        Ok(())
    }

    /// Atomically publish the data generation together with the relation generation.
    ///
    /// New readers must use this manifest rather than independently sampling
    /// `active_epoch` and `active_relation_epoch`.
    pub async fn activate_project_manifest(
        &self,
        operation_id: &str,
        relation_epoch: i64,
    ) -> Result<(), OrchestratorError> {
        let Some(store) = self.metadata_store.as_ref() else {
            return Ok(());
        };
        store
            .manifest_activate(
                self.project_id,
                self.epoch(),
                relation_epoch,
                operation_id,
                None,
            )
            .await
            .map_err(OrchestratorError::Storage)?;
        self.verify_symbol_table_consistency(self.epoch(), relation_epoch)
            .await;
        Ok(())
    }

    /// Cross-check the two symbol-table copies after a publication.
    ///
    /// `entities` (data-epoch scoped) and `relation_snapshot_entities`
    /// (relation-epoch scoped) intentionally live in separate version domains,
    /// but they describe the same parsed symbols, so their row counts must
    /// agree. The two tables are written by independent pipelines; this cheap
    /// count comparison surfaces silent drift early instead of letting it
    /// accumulate until queries return inconsistent symbol views. A mismatch is
    /// reported as a warning only: the publication itself stays valid and the
    /// next full rebuild converges both copies.
    ///
    /// A `relation_epoch` of 0 means no relation generation has ever been
    /// published for this project (deployments that never run the relation
    /// pipeline stay at zero). Comparing against an empty snapshot domain is
    /// then meaningless — it would flag every data-only publication as drift —
    /// so the check is skipped until the first relation publication exists.
    async fn verify_symbol_table_consistency(&self, data_epoch: i64, relation_epoch: i64) {
        if relation_epoch == 0 {
            return;
        }
        let Some(store) = self.metadata_store.as_ref() else {
            return;
        };
        let result = async {
            let entity_count = store.entities_count(self.project_id, data_epoch).await?;
            let snapshot_count = store
                .snapshot_entities_count(self.project_id, relation_epoch)
                .await?;
            Ok::<_, cce_types::StorageError>((entity_count, snapshot_count))
        }
        .await;
        match result {
            Ok((entity_count, snapshot_count)) => {
                if entity_count != snapshot_count {
                    tracing::warn!(
                        project_id = self.project_id,
                        data_epoch,
                        relation_epoch,
                        entity_count,
                        snapshot_count,
                        "Symbol table drift detected between entities and relation_snapshot_entities"
                    );
                }
            }
            Err(error) => {
                // Observability only; never fail a publication because of it.
                tracing::warn!(
                    project_id = self.project_id,
                    error = %error,
                    "Symbol table consistency check failed"
                );
            }
        }
    }

    /// Record the candidate generation before any data writes begin.
    pub async fn begin_project_manifest(
        &self,
        operation_id: &str,
    ) -> Result<(), OrchestratorError> {
        let Some(store) = self.metadata_store.as_ref() else {
            return Ok(());
        };
        store
            .manifest_begin_building(self.project_id, self.epoch(), operation_id, None)
            .await
            .map(|_| ())
            .map_err(OrchestratorError::Storage)
    }

    /// Preserve the prior active generation and mark an unfinished candidate failed.
    pub async fn fail_project_manifest(
        &self,
        operation_id: &str,
        reason: &str,
    ) -> Result<(), OrchestratorError> {
        let Some(store) = self.metadata_store.as_ref() else {
            return Ok(());
        };
        store
            .manifest_mark_failed(self.project_id, operation_id, reason)
            .await
            .map_err(OrchestratorError::Storage)
    }

    /// Read the relation generation currently associated with this project.
    ///
    /// A missing `active_relation_epoch` row (never published) is the
    /// legitimate default 0; unparseable values and DB failures are
    /// propagated instead of being silently downgraded.
    pub async fn active_relation_epoch(&self) -> Result<i64, OrchestratorError> {
        let Some(store) = self.metadata_store.as_ref() else {
            return Ok(0);
        };
        store
            .project_meta_get_int_optional(self.project_id, "active_relation_epoch")
            .await
            .map(|value| value.unwrap_or(0))
            .map_err(OrchestratorError::Storage)
    }
}

#[cfg(test)]
mod tests {
    use crate::index::vector_store::{MetadataStore, in_memory_metadata_store};
    use cce_storage_common::MetadataStorage;

    use super::super::StorageCoordinator;

    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime")
            .block_on(future)
    }

    #[test]
    fn active_relation_epoch_missing_row_defaults_to_zero() {
        let database = in_memory_metadata_store().expect("in-memory store");
        block_on(database.ensure_project(1, "/tmp/test")).expect("project should be ensured");

        let storage = StorageCoordinator::new(1)
            .expect("valid project ID")
            .with_metadata_store(database);
        assert_eq!(
            block_on(storage.active_relation_epoch())
                .expect("missing row is the legitimate default"),
            0
        );
    }

    #[test]
    fn active_relation_epoch_unparseable_value_is_an_error() {
        let database = in_memory_metadata_store().expect("in-memory store");
        block_on(database.ensure_project(1, "/tmp/test")).expect("project should be ensured");
        block_on(database.project_meta_set_string(1, "active_relation_epoch", "corrupt"))
            .expect("meta should be written");

        let storage = StorageCoordinator::new(1)
            .expect("valid project ID")
            .with_metadata_store(database);
        let err = block_on(storage.active_relation_epoch())
            .expect_err("corrupt value must fail instead of downgrading to 0");
        assert!(matches!(err, crate::error::OrchestratorError::Storage(_)));
    }

    /// Shared in-memory buffer capturing formatted tracing events.
    #[derive(Clone, Default)]
    struct LogBuffer(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

    impl std::io::Write for LogBuffer {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0
                .lock()
                .expect("log buffer lock")
                .extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn run_consistency_check_capturing_logs(
        storage: &StorageCoordinator,
        data_epoch: i64,
        relation_epoch: i64,
    ) -> String {
        use tracing_subscriber::Layer;
        use tracing_subscriber::layer::SubscriberExt;

        let writer = LogBuffer::default();
        let subscriber_writer = writer.clone();
        let subscriber = tracing_subscriber::registry().with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(move || subscriber_writer.clone())
                .with_filter(tracing_subscriber::filter::LevelFilter::WARN),
        );
        tracing::subscriber::with_default(subscriber, || {
            block_on(storage.verify_symbol_table_consistency(data_epoch, relation_epoch));
        });
        let captured = writer.0.lock().expect("log buffer lock").clone();
        String::from_utf8(captured).expect("captured logs are valid UTF-8")
    }

    async fn consistency_check_fixture() -> StorageCoordinator {
        let database = in_memory_metadata_store().expect("in-memory store");
        database
            .ensure_project(1, "/tmp/test")
            .await
            .expect("project should be ensured");
        database
            .files_ensure(
                1,
                1,
                0,
                &[cce_storage_common::metadb::FileEnsureEntry {
                    path: "src/lib.rs".to_string(),
                    language: "Rust".to_string(),
                    category: 1,
                    last_modified: 1,
                }],
            )
            .await
            .expect("file row should be ensured");
        let file = database
            .file_by_path(1, "src/lib.rs")
            .await
            .expect("file should load")
            .expect("file should exist");
        let record = cce_storage_common::metadb::EntityRecord {
            id: 0,
            name: "Alpha".to_string(),
            kind: "struct".to_string(),
            file_id: file.id,
            signature: Some("struct Alpha".to_string()),
            span_start_row: None,
            span_end_row: None,
            span_start_column: None,
            span_end_column: None,
            span_start_byte: None,
            span_end_byte: None,
            scoped_name: None,
            overload_discriminator: String::new(),
            depth: None,
            parent_id: None,
            metadata: Some("{\"__source_entity_id\":\"1\"}".to_string()),
            parameters_json: None,
            return_type: None,
            doc_comment: None,
            modifiers_json: None,
            project_id: 1,
            epoch: 1,
            batch_id: 0,
            rank: 0.0,
        };
        database
            .entities_upsert(std::slice::from_ref(&record))
            .await
            .expect("entity row should be inserted");

        StorageCoordinator::new(1)
            .expect("valid project ID")
            .with_metadata_store(database)
    }

    #[test]
    fn consistency_check_skips_when_relation_generation_never_published() {
        let storage = block_on(consistency_check_fixture());

        // relation_epoch 0 means the relation pipeline never published a
        // generation: the snapshot domain is empty by design and must not be
        // reported as drift.
        let logs = run_consistency_check_capturing_logs(&storage, 1, 0);
        assert!(
            !logs.contains("Symbol table drift"),
            "data-only publication must not warn, got: {logs}"
        );
    }

    #[test]
    fn consistency_check_warns_on_real_drift_between_published_generations() {
        let storage = block_on(consistency_check_fixture());

        // With a published relation generation whose snapshot is empty while
        // entities exist, the mismatch is genuine drift and must warn.
        let logs = run_consistency_check_capturing_logs(&storage, 1, 1);
        assert!(
            logs.contains("Symbol table drift detected"),
            "published relation generation with missing snapshots must warn, got: {logs}"
        );
    }
}
