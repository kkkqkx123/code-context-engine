//! Manifest lifecycle: starting, publishing, failing and reading index
//! generations.
//!
//! Owns the epoch/batch counters and their durable mirrors in `project_meta`,
//! the project index manifest state transitions, and the cheap symbol-table
//! drift check performed after publication.

use std::sync::atomic::Ordering;

use cce_storage_sqlite::ProjectIndexManifestRepository;

use crate::error::OrchestratorError;

use super::StorageCoordinator;

impl StorageCoordinator {
    /// Start a new project index version and make subsequent writes target it.
    pub fn begin_full_index(&self) -> Result<i64, OrchestratorError> {
        let Some(store) = &self.metadata_store else {
            return Ok(self.epoch());
        };
        let client = store.as_ref();
        // Missing meta rows are the legitimate first-run defaults (epoch 0,
        // ready 1); unparseable values and DB failures are propagated so a
        // broken metadata store cannot silently restart from epoch 0.
        let current = client
            .project_meta_get_int_optional(self.project_id, "epoch")
            .map_err(OrchestratorError::Storage)?
            .unwrap_or(0);
        let ready = client
            .project_meta_get_int_optional(self.project_id, "epoch_ready")
            .map_err(OrchestratorError::Storage)?
            .unwrap_or(1);
        // A completed manifest can still have an in-progress checkpoint when
        // the process crashed between publication and checkpoint completion.
        // Reuse that epoch so recovery finalizes the durable publication
        // instead of creating an unrelated generation.
        let has_incomplete_checkpoint = client
            .read_connection()
            .map_err(OrchestratorError::Storage)?
            .query_row(
                "SELECT EXISTS(
                     SELECT 1 FROM checkpoint
                     WHERE project_id = ?1 AND operation_type = 'full_index'
                       AND status = 'in_progress'
                 )",
                rusqlite::params![self.project_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|error| {
                OrchestratorError::Storage(cce_types::StorageError::query(error.to_string()))
            })?
            != 0;
        let epoch = if current > 0 && (ready == 0 || has_incomplete_checkpoint) {
            current
        } else {
            current + 1
        };
        self.epoch.store(epoch, Ordering::Release);
        self.batch_id.store(0, Ordering::Release);
        client
            .project_meta_set_int(self.project_id, "epoch", epoch)
            .and_then(|_| client.project_meta_set_int(self.project_id, "batch_id", 0))
            .and_then(|_| client.project_meta_set_int(self.project_id, "epoch_ready", 0))
            .map_err(OrchestratorError::Storage)?;
        Ok(epoch)
    }

    /// Set the durable batch version used by all storage backends.
    pub fn begin_batch(&self, batch_id: i64) -> Result<(), OrchestratorError> {
        self.batch_id.store(batch_id, Ordering::Release);
        if let Some(client) = self.metadata_store.as_ref().map(|store| store.as_ref()) {
            client
                .project_meta_set_int(self.project_id, "batch_id", batch_id)
                .map_err(OrchestratorError::Storage)?;
        }
        Ok(())
    }

    /// Atomically expose the completed index epoch to project queries.
    pub fn activate_current_epoch(&self) -> Result<(), OrchestratorError> {
        if let Some(client) = self.metadata_store.as_ref().map(|store| store.as_ref()) {
            client
                .project_meta_set_int(self.project_id, "active_epoch", self.epoch())
                .and_then(|_| client.project_meta_set_int(self.project_id, "epoch_ready", 1))
                .map_err(OrchestratorError::Storage)?;
        }
        Ok(())
    }

    /// Atomically publish the data generation together with the relation generation.
    ///
    /// New readers must use this manifest rather than independently sampling
    /// `active_epoch` and `active_relation_epoch`.
    pub fn activate_project_manifest(
        &self,
        operation_id: &str,
        relation_epoch: i64,
    ) -> Result<(), OrchestratorError> {
        let Some(client) = self.metadata_store.as_ref().map(|store| store.as_ref()) else {
            return Ok(());
        };
        client
            .with_transaction(|tx| {
                ProjectIndexManifestRepository::activate(
                    tx,
                    self.project_id,
                    self.epoch(),
                    relation_epoch,
                    operation_id,
                    None,
                )
                .map(|_| ())
            })
            .map_err(OrchestratorError::Storage)?;
        self.verify_symbol_table_consistency(self.epoch(), relation_epoch);
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
    fn verify_symbol_table_consistency(&self, data_epoch: i64, relation_epoch: i64) {
        if relation_epoch == 0 {
            return;
        }
        let Some(client) = self.metadata_store.as_ref().map(|store| store.as_ref()) else {
            return;
        };
        let result = client.with_transaction(|tx| {
            let entity_count: i64 = tx
                .query_row(
                    "SELECT COUNT(*) FROM entities WHERE project_id = ?1 AND epoch = ?2",
                    rusqlite::params![self.project_id, data_epoch],
                    |row| row.get(0),
                )
                .map_err(|error| cce_types::StorageError::query(error.to_string()))?;
            let snapshot_count: i64 = tx
                .query_row(
                    "SELECT COUNT(*) FROM relation_snapshot_entities
                     WHERE project_id = ?1 AND relation_epoch = ?2",
                    rusqlite::params![self.project_id, relation_epoch],
                    |row| row.get(0),
                )
                .map_err(|error| cce_types::StorageError::query(error.to_string()))?;
            Ok((entity_count, snapshot_count))
        });
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
    pub fn begin_project_manifest(&self, operation_id: &str) -> Result<(), OrchestratorError> {
        let Some(client) = self.metadata_store.as_ref().map(|store| store.as_ref()) else {
            return Ok(());
        };
        client
            .with_transaction(|tx| {
                ProjectIndexManifestRepository::begin_building(
                    tx,
                    self.project_id,
                    self.epoch(),
                    operation_id,
                    None,
                )
                .map(|_| ())
            })
            .map_err(OrchestratorError::Storage)
    }

    /// Preserve the prior active generation and mark an unfinished candidate failed.
    pub fn fail_project_manifest(
        &self,
        operation_id: &str,
        reason: &str,
    ) -> Result<(), OrchestratorError> {
        let Some(client) = self.metadata_store.as_ref().map(|store| store.as_ref()) else {
            return Ok(());
        };
        client
            .with_transaction(|tx| {
                ProjectIndexManifestRepository::mark_failed(
                    tx,
                    self.project_id,
                    operation_id,
                    reason,
                )
            })
            .map_err(OrchestratorError::Storage)
    }

    /// Read the relation generation currently associated with this project.
    ///
    /// A missing `active_relation_epoch` row (never published) is the
    /// legitimate default 0; unparseable values and DB failures are
    /// propagated instead of being silently downgraded.
    pub fn active_relation_epoch(&self) -> Result<i64, OrchestratorError> {
        let Some(client) = self.metadata_store.as_ref().map(|store| store.as_ref()) else {
            return Ok(0);
        };
        client
            .project_meta_get_int_optional(self.project_id, "active_relation_epoch")
            .map(|value| value.unwrap_or(0))
            .map_err(OrchestratorError::Storage)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use cce_storage_sqlite::{NewProjectRecord, ProjectRepository, SqliteClient};

    use super::super::StorageCoordinator;

    #[test]
    fn active_relation_epoch_missing_row_defaults_to_zero() {
        let database = Arc::new(SqliteClient::in_memory().expect("in-memory database"));
        let client = database.as_ref().clone();
        client
            .with_transaction(|tx| {
                ProjectRepository::insert(
                    tx,
                    &NewProjectRecord::new("test".to_string(), "/tmp/test".to_string()),
                )
                .map(|_| ())
            })
            .expect("project should be inserted");

        let storage = StorageCoordinator::new(1)
            .expect("valid project ID")
            .with_metadata_store(database);
        assert_eq!(
            storage
                .active_relation_epoch()
                .expect("missing row is the legitimate default"),
            0
        );
    }

    #[test]
    fn active_relation_epoch_unparseable_value_is_an_error() {
        let database = Arc::new(SqliteClient::in_memory().expect("in-memory database"));
        let client = database.as_ref().clone();
        client
            .with_transaction(|tx| {
                ProjectRepository::insert(
                    tx,
                    &NewProjectRecord::new("test".to_string(), "/tmp/test".to_string()),
                )?;
                tx.execute(
                    "INSERT INTO project_meta (project_id, key, value, created_at, updated_at)
                     VALUES (1, 'active_relation_epoch', 'corrupt', 1, 1)",
                    [],
                )
                .map(|_| ())
                .map_err(|error| cce_types::StorageError::insert(error.to_string()))
            })
            .expect("meta should be inserted");

        let storage = StorageCoordinator::new(1)
            .expect("valid project ID")
            .with_metadata_store(database);
        let err = storage
            .active_relation_epoch()
            .expect_err("corrupt value must fail instead of downgrading to 0");
        assert!(matches!(err, crate::error::OrchestratorError::Storage(_)));
    }

    /// Shared in-memory buffer capturing formatted tracing events.
    #[derive(Clone, Default)]
    struct LogBuffer(Arc<std::sync::Mutex<Vec<u8>>>);

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
            storage.verify_symbol_table_consistency(data_epoch, relation_epoch);
        });
        let captured = writer.0.lock().expect("log buffer lock").clone();
        String::from_utf8(captured).expect("captured logs are valid UTF-8")
    }

    fn consistency_check_fixture() -> StorageCoordinator {
        let database = Arc::new(SqliteClient::in_memory().expect("in-memory database"));
        let client = database.as_ref().clone();
        client
            .with_transaction(|tx| {
                ProjectRepository::insert(
                    tx,
                    &NewProjectRecord::new("test".to_string(), "/tmp/test".to_string()),
                )
                .map(|_| ())
            })
            .expect("project should be inserted");
        client
            .with_transaction(|tx| {
                tx.execute(
                    "INSERT INTO files
                        (path, language, category, last_modified, created_at, project_id, content_hash, epoch, batch_id)
                     VALUES ('src/lib.rs', 'Rust', 1, 1, 1, 1, NULL, 1, 0)",
                    [],
                )
                .map(|_| ())
                .map_err(|error| cce_types::StorageError::insert(error.to_string()))
            })
            .expect("file row should be inserted");
        client
            .with_transaction(|tx| {
                tx.execute(
                    "INSERT INTO entities (name, kind, file_id, project_id, epoch)
                     VALUES ('Alpha', 'struct', 1, 1, 1)",
                    [],
                )
                .map(|_| ())
                .map_err(|error| cce_types::StorageError::insert(error.to_string()))
            })
            .expect("entity row should be inserted");

        StorageCoordinator::new(1)
            .expect("valid project ID")
            .with_metadata_store(database)
    }

    #[test]
    fn consistency_check_skips_when_relation_generation_never_published() {
        let storage = consistency_check_fixture();

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
        let storage = consistency_check_fixture();

        // With a published relation generation whose snapshot is empty while
        // entities exist, the mismatch is genuine drift and must warn.
        let logs = run_consistency_check_capturing_logs(&storage, 1, 1);
        assert!(
            logs.contains("Symbol table drift detected"),
            "published relation generation with missing snapshots must warn, got: {logs}"
        );
    }
}
