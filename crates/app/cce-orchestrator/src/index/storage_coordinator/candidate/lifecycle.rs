use cce_storage_common::FulltextStorage;
use cce_storage_common::VectorStorage;
use cce_storage_metadb_sqlite::{GenerationOverrideRepository, ProjectIndexManifestRepository};
use std::sync::atomic::Ordering;

use crate::error::OrchestratorError;

use super::super::StorageCoordinator;

impl StorageCoordinator {
    /// Whether a hot-update candidate generation is currently prepared.
    ///
    /// Deletion and preparation paths branch on this: inside a candidate the
    /// work is scoped to the candidate epoch, outside it the operations
    /// address the published generation. A poisoned lock means the candidate
    /// state is unknown, so the error surfaces instead of silently falling
    /// back to the published-generation path.
    pub(crate) fn has_hot_update_candidate(&self) -> Result<bool, OrchestratorError> {
        self.candidate_operation
            .lock()
            .map_err(|_| {
                OrchestratorError::index("hot_update_candidate", "candidate state lock poisoned")
            })
            .map(|state| state.is_some())
    }

    /// Prepare an isolated data generation for a hot-update operation.
    ///
    /// Zero-copy: instead of cloning the active generation, the candidate
    /// manifest is linked to it (`parent_data_epoch`) inside a single
    /// transaction together with the `candidate_ready` marker. Changed files
    /// are later replaced only in the candidate's own rows and hidden in the
    /// parent via overrides, so a failed processor cannot remove data selected
    /// by the active manifest. When `active_epoch == 0` (never published) no
    /// parent link is set and the candidate is a full generation.
    ///
    /// Before registration, an inheritance chain about to exceed depth 2
    /// triggers compaction: the active generation's inherited data is
    /// materialized into itself so it becomes a full generation.
    ///
    /// When `resume` is true the operation is recovering an interrupted run:
    /// an existing, registered building candidate for the same operation is
    /// adopted as-is (skipping registration) so already-completed files are
    /// not reprocessed. A missing or invalid candidate falls back to a fresh
    /// registration.
    pub async fn begin_hot_update_candidate(
        &self,
        operation_id: &str,
        resume: bool,
    ) -> Result<i64, OrchestratorError> {
        {
            let mut current = self.candidate_operation.lock().map_err(|_| {
                OrchestratorError::index("hot_update_candidate", "candidate state lock poisoned")
            })?;
            if current.as_deref() == Some(operation_id) {
                return Ok(self.epoch());
            }
            if current.is_some() {
                return Err(OrchestratorError::index(
                    "hot_update_candidate",
                    "another hot-update candidate is already prepared",
                ));
            }
            *current = Some(operation_id.to_string());
        }

        if resume && let Some(epoch) = self.try_adopt_existing_candidate(operation_id)? {
            tracing::info!(
                operation_id,
                candidate_epoch = epoch,
                "Adopted existing hot-update candidate generation on resume"
            );
            return Ok(epoch);
        }

        let result = async {
            // Resolve the active generation: (epoch, parent link, operation id).
            // Legacy fallback: a manifest-less project falls back to
            // `project_meta.active_epoch`; a missing row is the legitimate
            // default 0 (never published), but DB failures must not be
            // downgraded.
            let (active_epoch, active_parent, active_operation_id) =
                if let Some(client) = self.metadata_store.as_ref().map(|store| store.as_ref()) {
                    let manifest = {
                        let conn = client
                            .read_connection()
                            .map_err(OrchestratorError::Storage)?;
                        ProjectIndexManifestRepository::get_active(&conn, self.project_id)
                            .map_err(OrchestratorError::Storage)?
                    };
                    match manifest {
                        Some(manifest) => (
                            manifest.data_epoch,
                            manifest.parent_data_epoch,
                            Some(manifest.operation_id),
                        ),
                        None => {
                            let epoch = client
                                .project_meta_get_int_optional(self.project_id, "active_epoch")
                                .map(|value| value.unwrap_or(0))
                                .map_err(OrchestratorError::Storage)?;
                            (epoch, None, None)
                        }
                    }
                } else {
                    (0, None, None)
                };
            let candidate_epoch = active_epoch.saturating_add(1).max(1);
            self.epoch.store(candidate_epoch, Ordering::Release);
            self.candidate_relation_epoch.store(0, Ordering::Release);

            // Compaction guard: if the active generation itself inherited its
            // data, adopting on top of it would push the chain to depth 3.
            // Materialize the active generation into a full one first so the
            // new candidate inherits a parent-free generation.
            if let (Some(inherited_epoch), Some(active_operation_id)) =
                (active_parent, active_operation_id.as_deref())
            {
                tracing::info!(
                    operation_id = %operation_id,
                    active_epoch,
                    inherited_epoch,
                    "Compacting inherited active generation before candidate creation"
                );
                self.compact_generation(active_operation_id, active_epoch, inherited_epoch)
                    .await?;
            }

            // External deletions are idempotent: deleting a non-existent epoch
            // is a no-op in both vector and BM25, so a resume that re-executes
            // `begin_hot_update_candidate` after an interrupted delete remains
            // safe (candidate_ready guards adoption).
            if let Some(vector) = &self.vector {
                self.ensure_project_group_id()?;
                vector
                    .delete_by_group_epoch(&self.project_group_id, candidate_epoch)
                    .await?;
                tracing::info!(
                    operation_id = %operation_id,
                    project_id = self.project_id,
                    candidate_epoch = candidate_epoch,
                    group_id = %self.project_group_id,
                    "Vector candidate epoch deleted (idempotent)"
                );
            }
            if let Some(bm25) = &self.fulltext {
                let deleted = bm25
                    .delete_by_project_epoch(
                        &bm25.configured_index_name(),
                        self.project_id,
                        candidate_epoch,
                    )
                    .await?;
                tracing::info!(
                    operation_id = %operation_id,
                    project_id = self.project_id,
                    candidate_epoch = candidate_epoch,
                    deleted = deleted,
                    "BM25 candidate epoch deleted (idempotent)"
                );
            }

            // Single-transaction inheritance registration: begin the building
            // manifest, link it to the active generation, clear any residual
            // candidate rows/overrides from an interrupted run at this epoch
            // number, then mark it ready for resume adoption.
            if let Some(client) = self.metadata_store.as_ref().map(|store| store.as_ref()) {
                let parent_data_epoch = if active_epoch > 0 {
                    Some(active_epoch)
                } else {
                    None
                };
                client
                    .with_transaction(|tx| {
                        ProjectIndexManifestRepository::begin_building(
                            tx,
                            self.project_id,
                            candidate_epoch,
                            operation_id,
                            None,
                        )
                        .map(|_| ())?;
                        ProjectIndexManifestRepository::set_parent_data_epoch(
                            tx,
                            self.project_id,
                            operation_id,
                            parent_data_epoch,
                        )?;
                        Self::clear_candidate_rows_tx(tx, self.project_id, candidate_epoch)?;
                        GenerationOverrideRepository::clear_generation(
                            tx,
                            self.project_id,
                            candidate_epoch,
                        )?;
                        // The inheritance chain is now registered: the
                        // candidate can be safely adopted after an interrupt.
                        ProjectIndexManifestRepository::mark_candidate_ready(
                            tx,
                            self.project_id,
                            operation_id,
                        )
                    })
                    .map_err(OrchestratorError::Storage)?;
            }
            Ok(candidate_epoch)
        }
        .await;

        if result.is_err() {
            if let Err(manifest_error) =
                self.fail_project_manifest(operation_id, "candidate preparation failed")
            {
                tracing::error!(
                    operation_id = %operation_id,
                    error = %manifest_error,
                    "Failed to mark candidate manifest failed after preparation error"
                );
            }
            if let Ok(mut current) = self.candidate_operation.lock() {
                *current = None;
            }
        }
        result
    }

    /// Compact an inherited active generation into a full one.
    ///
    /// Copies the inherited parent data into the active generation's own rows
    /// (skipping overridden files), then clears its parent link and override
    /// set so it becomes parent-free. The CAS-guarded candidate lock ensures
    /// no concurrent hot update runs during compaction.
    async fn compact_generation(
        &self,
        active_operation_id: &str,
        active_epoch: i64,
        parent_epoch: i64,
    ) -> Result<(), OrchestratorError> {
        let Some(client) = self.metadata_store.as_ref().map(|store| store.as_ref()) else {
            return Ok(());
        };
        let excluded_paths: Vec<String> = {
            let conn = client
                .read_connection()
                .map_err(OrchestratorError::Storage)?;
            GenerationOverrideRepository::list_for_generation(&conn, self.project_id, active_epoch)
                .map_err(OrchestratorError::Storage)?
                .into_iter()
                .map(|override_entry| override_entry.file_path)
                .collect()
        };

        self.materialize_sqlite_generation(parent_epoch, active_epoch, &excluded_paths)?;
        self.materialize_external_epochs(parent_epoch, active_epoch, &excluded_paths)
            .await?;

        client
            .with_transaction(|tx| {
                ProjectIndexManifestRepository::set_parent_data_epoch(
                    tx,
                    self.project_id,
                    active_operation_id,
                    None,
                )?;
                GenerationOverrideRepository::clear_generation(tx, self.project_id, active_epoch)
            })
            .map_err(OrchestratorError::Storage)?;
        tracing::info!(
            project_id = self.project_id,
            epoch = active_epoch,
            "Active generation compacted into a full generation"
        );
        Ok(())
    }

    /// Check whether a previously-prepared building candidate for an
    /// operation can be adopted on resume.
    ///
    /// Reuses the exact judgment conditions of
    /// [`Self::try_adopt_existing_candidate`] without mutating any in-memory
    /// state: the candidate is adoptable when a `candidate_ready` building
    /// manifest exists for the operation, its data epoch is still the next
    /// generation after the active one, **and** its registered parent still is
    /// the active generation (an inheritance chain superseded by another
    /// publication must never be adopted). A voided candidate (marked failed,
    /// never registered, or superseded) is not adoptable.
    pub fn is_candidate_adoptable(&self, operation_id: &str) -> Result<bool, OrchestratorError> {
        let Some(client) = self.metadata_store.as_ref().map(|store| store.as_ref()) else {
            return Ok(false);
        };
        let conn = client
            .read_connection()
            .map_err(OrchestratorError::Storage)?;
        let active_epoch = match ProjectIndexManifestRepository::get_active(&conn, self.project_id)
        {
            Ok(Some(manifest)) => manifest.data_epoch,
            Ok(None) => 0,
            Err(error) => return Err(OrchestratorError::Storage(error)),
        };
        let Some(manifest) = ProjectIndexManifestRepository::get_building_by_operation(
            &conn,
            self.project_id,
            operation_id,
        )
        .map_err(OrchestratorError::Storage)?
        else {
            return Ok(false);
        };
        Ok(Self::candidate_matches_active(&manifest, active_epoch))
    }

    /// Adoption judgment: ready + epoch continuity + parent-chain validity.
    fn candidate_matches_active(
        manifest: &cce_storage_metadb_sqlite::ProjectIndexManifest,
        active_epoch: i64,
    ) -> bool {
        let expected_parent = if active_epoch > 0 {
            Some(active_epoch)
        } else {
            None
        };
        manifest.candidate_ready
            && manifest.data_epoch == active_epoch.saturating_add(1)
            && manifest.parent_data_epoch == expected_parent
    }

    /// Adopt a previously-prepared building candidate for a resumed operation.
    ///
    /// Returns the candidate epoch when a registered (`candidate_ready`)
    /// building manifest exists for the operation, still targets the next
    /// generation after the active one, and its parent link still points at
    /// the active generation. The in-memory candidate state is rewired to that
    /// epoch so processors continue writing into it. A stale or invalid
    /// candidate returns `None` and forces a fresh registration.
    fn try_adopt_existing_candidate(
        &self,
        operation_id: &str,
    ) -> Result<Option<i64>, OrchestratorError> {
        let Some(client) = self.metadata_store.as_ref().map(|store| store.as_ref()) else {
            return Ok(None);
        };
        let conn = client
            .read_connection()
            .map_err(OrchestratorError::Storage)?;
        let active_epoch = match ProjectIndexManifestRepository::get_active(&conn, self.project_id)
        {
            Ok(Some(manifest)) => manifest.data_epoch,
            Ok(None) => 0,
            Err(error) => return Err(OrchestratorError::Storage(error)),
        };
        let Some(manifest) = ProjectIndexManifestRepository::get_building_by_operation(
            &conn,
            self.project_id,
            operation_id,
        )
        .map_err(OrchestratorError::Storage)?
        else {
            return Ok(None);
        };
        if !Self::candidate_matches_active(&manifest, active_epoch) {
            return Ok(None);
        }
        self.epoch.store(manifest.data_epoch, Ordering::Release);
        self.candidate_relation_epoch.store(0, Ordering::Release);
        Ok(Some(manifest.data_epoch))
    }

    /// Record the relation generation produced by a hot-update processor.
    pub fn set_candidate_relation_epoch(&self, relation_epoch: i64) {
        self.candidate_relation_epoch
            .store(relation_epoch, Ordering::Release);
    }

    /// Activate the data and relation candidates as one project publication.
    pub fn activate_hot_update_candidate(
        &self,
        operation_id: &str,
    ) -> Result<(), OrchestratorError> {
        let relation_epoch = if self.candidate_relation_epoch.load(Ordering::Acquire) > 0 {
            self.candidate_relation_epoch.load(Ordering::Acquire)
        } else {
            self.active_relation_epoch()?
        };
        let result = self.activate_project_manifest(operation_id, relation_epoch);
        if result.is_ok() {
            let mut current = self.candidate_operation.lock().map_err(|_| {
                OrchestratorError::index("hot_update_candidate", "candidate state lock poisoned")
            })?;
            *current = None;
        }
        result
    }

    /// Mark the hot-update candidate failed and retain the active manifest.
    ///
    /// The candidate's own rows and override registrations are cleared so the
    /// epoch number can be safely reused by the next candidate; inherited
    /// parent data was never modified.
    pub fn fail_hot_update_candidate(
        &self,
        operation_id: &str,
        reason: &str,
    ) -> Result<(), OrchestratorError> {
        self.fail_project_manifest(operation_id, reason)?;
        let result = self.cleanup_failed_candidate(self.epoch());
        if let Err(error) = &result {
            tracing::warn!(
                operation_id = %operation_id,
                error = %error,
                "Failed to clear failed candidate rows"
            );
        }
        let mut current = self.candidate_operation.lock().map_err(|_| {
            OrchestratorError::index("hot_update_candidate", "candidate state lock poisoned")
        })?;
        *current = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::StorageCoordinator;
    use cce_storage_metadb_sqlite::{
        NewProjectRecord, ProjectIndexManifestRepository, ProjectRepository, SqliteClient,
    };
    use std::sync::Arc;

    #[tokio::test]
    async fn hot_candidate_registers_inheritance_without_copying_data() {
        let database = Arc::new(SqliteClient::in_memory().expect("in-memory database"));
        let client = database.as_ref().clone();
        client
            .with_transaction(|tx| {
                ProjectRepository::insert(
                    tx,
                    &NewProjectRecord::new("test".to_string(), "/tmp/test".to_string()),
                )?;
                ProjectIndexManifestRepository::activate(tx, 1, 1, 0, "initial", None)?;
                tx.execute(
                    "INSERT INTO files
                        (path, language, last_modified, created_at, project_id, content_hash, epoch, batch_id)
                     VALUES ('src/lib.rs', 'Rust', 1, 1, 1, 'hash', 1, 0)",
                    [],
                )
                .map(|_| ())
                .map_err(|error| cce_types::StorageError::insert("files", error.to_string()))
            })
            .expect("initial generation should be created");

        let storage = StorageCoordinator::new(1)
            .expect("valid project ID")
            .with_metadata_store(database)
            .with_epoch(1);
        let candidate_epoch = storage
            .begin_hot_update_candidate("hot-operation", false)
            .await
            .expect("candidate should be prepared");
        assert_eq!(candidate_epoch, 2);

        let conn = client.write_connection().expect("SQLite connection");
        let active = ProjectIndexManifestRepository::get_active(&conn, 1)
            .expect("active manifest should load")
            .expect("active manifest should exist");
        assert_eq!(active.data_epoch, 1);

        // Zero-copy: the candidate inherits instead of cloning, so the
        // active generation's rows are not duplicated.
        let candidate_files: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM files WHERE project_id = 1 AND epoch = 2",
                [],
                |row| row.get(0),
            )
            .expect("candidate file count should be queryable");
        assert_eq!(candidate_files, 0, "no physical clone may happen");

        // The inheritance chain is registered and adoptable.
        let building =
            ProjectIndexManifestRepository::get_building_by_operation(&conn, 1, "hot-operation")
                .expect("building manifest should load")
                .expect("building manifest should exist");
        assert_eq!(building.parent_data_epoch, Some(1));
        assert!(building.candidate_ready);
        drop(conn);

        storage
            .fail_hot_update_candidate("hot-operation", "test failure")
            .expect("candidate should be marked failed");
        let conn = client.write_connection().expect("SQLite connection");
        let active = ProjectIndexManifestRepository::get_active(&conn, 1)
            .expect("active manifest should load")
            .expect("active manifest should exist");
        assert_eq!(active.data_epoch, 1);

        // Failure path marks the manifest failed (no longer adoptable) and
        // clears the candidate's own rows so the epoch number can be reused.
        let building =
            ProjectIndexManifestRepository::get_building_by_operation(&conn, 1, "hot-operation")
                .expect("building manifest query should succeed");
        assert!(
            building.is_none(),
            "failed candidate must not stay building"
        );
    }

    #[tokio::test]
    async fn resume_adopts_ready_candidate_without_recloning() {
        let database = Arc::new(SqliteClient::in_memory().expect("in-memory database"));
        let client = database.as_ref().clone();
        client
            .with_transaction(|tx| {
                ProjectRepository::insert(
                    tx,
                    &NewProjectRecord::new("test".to_string(), "/tmp/test".to_string()),
                )?;
                ProjectIndexManifestRepository::activate(tx, 1, 1, 0, "initial", None)?;
                tx.execute(
                    "INSERT INTO files
                        (path, language, last_modified, created_at, project_id, content_hash, epoch, batch_id)
                     VALUES ('src/lib.rs', 'Rust', 1, 1, 1, 'hash', 1, 0)",
                    [],
                )
                .map(|_| ())
                .map_err(|error| cce_types::StorageError::insert("files", error.to_string()))
            })
            .expect("initial generation should be created");

        let storage = StorageCoordinator::new(1)
            .expect("valid project ID")
            .with_metadata_store(database.clone())
            .with_epoch(1);
        let candidate_epoch = storage
            .begin_hot_update_candidate("hot-operation", false)
            .await
            .expect("candidate should be prepared");
        assert_eq!(candidate_epoch, 2);

        // A fresh begin sets candidate_ready once the clone completed, so a
        // "restarted" coordinator (fresh in-memory state) can adopt it.
        let restarted = StorageCoordinator::new(1)
            .expect("valid project ID")
            .with_metadata_store(database)
            .with_epoch(1);
        let adopted = restarted
            .begin_hot_update_candidate("hot-operation", true)
            .await
            .expect("resume should succeed");
        assert_eq!(adopted, 2, "resume must reuse the existing candidate epoch");

        // Adopting a second time for the same operation is idempotent.
        let again = restarted
            .begin_hot_update_candidate("hot-operation", true)
            .await
            .expect("resume should be idempotent");
        assert_eq!(again, 2);
    }

    #[tokio::test]
    async fn resume_falls_back_to_fresh_candidate_when_not_ready() {
        let database = Arc::new(SqliteClient::in_memory().expect("in-memory database"));
        let client = database.as_ref().clone();
        client
            .with_transaction(|tx| {
                ProjectRepository::insert(
                    tx,
                    &NewProjectRecord::new("test".to_string(), "/tmp/test".to_string()),
                )?;
                ProjectIndexManifestRepository::activate(tx, 1, 1, 0, "initial", None)?;
                // A building manifest that was never marked ready (simulating a
                // crash before the inheritance registration committed).
                ProjectIndexManifestRepository::begin_building(tx, 1, 2, "interrupted", None)?;
                Ok(())
            })
            .expect("initial generation should be created");

        let storage = StorageCoordinator::new(1)
            .expect("valid project ID")
            .with_metadata_store(database)
            .with_epoch(1);
        // candidate_ready is false, so resume must fall back to a fresh
        // registration at the same candidate epoch 2.
        let epoch = storage
            .begin_hot_update_candidate("interrupted", true)
            .await
            .expect("resume should succeed");
        assert_eq!(epoch, 2);

        // After the fresh registration the candidate is ready and linked to
        // the active generation again.
        let conn = client.write_connection().expect("SQLite connection");
        let building =
            ProjectIndexManifestRepository::get_building_by_operation(&conn, 1, "interrupted")
                .expect("building manifest should load")
                .expect("building manifest should exist");
        assert!(building.candidate_ready);
        assert_eq!(building.parent_data_epoch, Some(1));
    }

    #[test]
    fn is_candidate_adoptable_judgment_matrix() {
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

        // No manifest for the operation at all: not adoptable.
        assert!(
            !storage
                .is_candidate_adoptable("unknown-operation")
                .expect("query should succeed")
        );

        // Building but never marked candidate_ready (crash before the
        // inheritance registration committed): not adoptable.
        client
            .with_transaction(|tx| {
                ProjectIndexManifestRepository::begin_building(tx, 1, 2, "not-ready", None)
                    .map(|_| ())
            })
            .expect("building manifest should be created");
        assert!(
            !storage
                .is_candidate_adoptable("not-ready")
                .expect("query should succeed")
        );

        // Building + candidate_ready + epoch == active+1, but without the
        // registered parent link: not adoptable (the inheritance chain is
        // incomplete — adopting would lose all unchanged data).
        client
            .with_transaction(|tx| {
                ProjectIndexManifestRepository::begin_building(tx, 1, 2, "ready", None)?;
                ProjectIndexManifestRepository::mark_candidate_ready(tx, 1, "ready")?;
                Ok(())
            })
            .expect("ready candidate should be created");
        assert!(
            !storage
                .is_candidate_adoptable("ready")
                .expect("query should succeed")
        );

        // Building + candidate_ready + epoch continuity + parent link pointing
        // at the active generation: adoptable.
        client
            .with_transaction(|tx| {
                ProjectIndexManifestRepository::begin_building(tx, 1, 2, "inherited-ready", None)?;
                ProjectIndexManifestRepository::set_parent_data_epoch(
                    tx,
                    1,
                    "inherited-ready",
                    Some(1),
                )?;
                ProjectIndexManifestRepository::mark_candidate_ready(tx, 1, "inherited-ready")?;
                Ok(())
            })
            .expect("inherited ready candidate should be created");
        assert!(
            storage
                .is_candidate_adoptable("inherited-ready")
                .expect("query should succeed")
        );

        // Building + candidate_ready but targeting a stale epoch (the active
        // generation moved on): not adoptable.
        client
            .with_transaction(|tx| {
                ProjectIndexManifestRepository::begin_building(tx, 1, 5, "stale-epoch", None)?;
                ProjectIndexManifestRepository::mark_candidate_ready(tx, 1, "stale-epoch")?;
                Ok(())
            })
            .expect("stale candidate should be created");
        assert!(
            !storage
                .is_candidate_adoptable("stale-epoch")
                .expect("query should succeed")
        );

        // Aborted operation: the manifest was marked failed, so no building
        // manifest remains and the candidate is not adoptable.
        client
            .with_transaction(|tx| {
                ProjectIndexManifestRepository::begin_building(tx, 1, 2, "aborted", None)?;
                ProjectIndexManifestRepository::mark_failed(tx, 1, "aborted", "injected")?;
                Ok(())
            })
            .expect("aborted candidate should be created");
        assert!(
            !storage
                .is_candidate_adoptable("aborted")
                .expect("query should succeed")
        );
    }

    #[tokio::test]
    async fn candidate_creation_compacts_an_inherited_active_generation() {
        let database = Arc::new(SqliteClient::in_memory().expect("in-memory database"));
        let client = database.as_ref().clone();
        client
            .with_transaction(|tx| {
                ProjectRepository::insert(
                    tx,
                    &NewProjectRecord::new("test".to_string(), "/tmp/test".to_string()),
                )?;
                ProjectIndexManifestRepository::activate(tx, 1, 1, 0, "gen-1", None)?;
                for path in ["src/keep.rs", "src/replaced.rs"] {
                    tx.execute(
                        "INSERT INTO files
                            (path, language, last_modified, created_at, project_id, content_hash, epoch, batch_id)
                         VALUES (?1, 'Rust', 1, 1, 1, 'hash', 1, 0)",
                        rusqlite::params![path],
                    )
                    .map(|_| ())
                    .map_err(|error| cce_types::StorageError::insert("files", error.to_string()))?;
                }
                // Generation 2 inherited from generation 1 and was published;
                // one file was replaced inside it (own row + `replaced`).
                ProjectIndexManifestRepository::begin_building(tx, 1, 2, "gen-2", None)?;
                ProjectIndexManifestRepository::set_parent_data_epoch(tx, 1, "gen-2", Some(1))?;
                ProjectIndexManifestRepository::activate(tx, 1, 2, 0, "gen-2", None)?;
                tx.execute(
                    "INSERT INTO files
                        (path, language, last_modified, created_at, project_id, content_hash, epoch, batch_id)
                     VALUES ('src/replaced.rs', 'Rust', 2, 1, 1, 'hash-new', 2, 0)",
                    [],
                )
                .map(|_| ())
                .map_err(|error| cce_types::StorageError::insert("files", error.to_string()))?;
                cce_storage_metadb_sqlite::GenerationOverrideRepository::upsert(
                    tx,
                    1,
                    2,
                    "src/replaced.rs",
                    cce_storage_metadb_sqlite::OverrideDisposition::Replaced,
                )?;
                Ok(())
            })
            .expect("inherited active generation should be created");

        let storage = StorageCoordinator::new(1)
            .expect("valid project ID")
            .with_metadata_store(database)
            .with_epoch(2);
        let candidate_epoch = storage
            .begin_hot_update_candidate("hot-op", false)
            .await
            .expect("candidate should be prepared");
        assert_eq!(candidate_epoch, 3);

        let conn = client.write_connection().expect("SQLite connection");

        // Compaction materialized generation 2: parent link cleared, overrides
        // cleared, and the non-overridden parent rows merged into its own rows.
        let active = ProjectIndexManifestRepository::get_active(&conn, 1)
            .expect("active manifest should load")
            .expect("active manifest should exist");
        assert_eq!(active.data_epoch, 2);
        assert_eq!(active.parent_data_epoch, None);
        assert!(
            cce_storage_metadb_sqlite::GenerationOverrideRepository::list_for_generation(
                &conn, 1, 2
            )
            .expect("overrides should list")
            .is_empty()
        );
        let gen2_paths: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT path FROM files WHERE project_id = 1 AND epoch = 2 ORDER BY path")
                .expect("prepare path query");
            let rows = stmt
                .query_map([], |row| row.get::<_, String>(0))
                .expect("query paths");
            rows.collect::<Result<Vec<_>, _>>().expect("collect paths")
        };
        assert_eq!(gen2_paths, vec!["src/keep.rs", "src/replaced.rs"]);

        // The new candidate inherits from the compacted (parent-free) active
        // generation.
        let building =
            ProjectIndexManifestRepository::get_building_by_operation(&conn, 1, "hot-op")
                .expect("building manifest should load")
                .expect("building manifest should exist");
        assert_eq!(building.parent_data_epoch, Some(2));

        // Sanity: compaction never rewrote the grandparent generation.
        let grandparent_files: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM files WHERE project_id = 1 AND epoch = 1",
                [],
                |row| row.get(0),
            )
            .expect("grandparent count");
        assert_eq!(grandparent_files, 2);
    }
}
