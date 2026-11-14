use cce_storage_common::FulltextStorage;
use cce_storage_common::RelationStorage;
use cce_storage_common::VectorStorage;
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

        if resume && let Some(epoch) = self.try_adopt_existing_candidate(operation_id).await? {
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
                if let Some(store) = self.metadata_store.as_ref() {
                    let manifest = store
                        .manifest_active(self.project_id)
                        .await
                        .map_err(OrchestratorError::Storage)?;
                    match manifest {
                        Some(manifest) => (
                            manifest.data_epoch,
                            manifest.parent_data_epoch,
                            Some(manifest.operation_id),
                        ),
                        None => {
                            let epoch = store
                                .project_meta_get_int_optional(self.project_id, "active_epoch")
                                .await
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
            if let Some(store) = self.metadata_store.as_ref() {
                let parent_data_epoch = if active_epoch > 0 {
                    Some(active_epoch)
                } else {
                    None
                };
                // The inheritance chain is now registered: the candidate
                // can be safely adopted after an interrupt.
                store
                    .candidate_begin(
                        self.project_id,
                        candidate_epoch,
                        operation_id,
                        parent_data_epoch,
                    )
                    .await
                    .map_err(OrchestratorError::Storage)?;
            }
            Ok(candidate_epoch)
        }
        .await;

        if result.is_err() {
            if let Err(manifest_error) = self
                .fail_project_manifest(operation_id, "candidate preparation failed")
                .await
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
        let Some(store) = self.metadata_store.as_ref() else {
            return Ok(());
        };
        let excluded_paths: Vec<String> = store
            .overrides_for_generation(self.project_id, active_epoch)
            .await
            .map_err(OrchestratorError::Storage)?
            .into_iter()
            .map(|override_entry| override_entry.file_path)
            .collect();

        self.materialize_sqlite_generation(parent_epoch, active_epoch, &excluded_paths)
            .await?;
        self.materialize_external_epochs(parent_epoch, active_epoch, &excluded_paths)
            .await?;

        store
            .manifest_set_parent(self.project_id, active_operation_id, None)
            .await
            .map_err(OrchestratorError::Storage)?;
        store
            .overrides_clear(self.project_id, active_epoch)
            .await
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
    pub async fn is_candidate_adoptable(
        &self,
        operation_id: &str,
    ) -> Result<bool, OrchestratorError> {
        let Some(store) = self.metadata_store.as_ref() else {
            return Ok(false);
        };
        let active_epoch = match store
            .manifest_active(self.project_id)
            .await
            .map_err(OrchestratorError::Storage)?
        {
            Some(manifest) => manifest.data_epoch,
            None => 0,
        };
        let Some(manifest) = store
            .manifest_by_operation(self.project_id, operation_id)
            .await
            .map_err(OrchestratorError::Storage)?
        else {
            return Ok(false);
        };
        Ok(Self::candidate_matches_active(&manifest, active_epoch))
    }

    /// Adoption judgment: ready + epoch continuity + parent-chain validity.
    fn candidate_matches_active(
        manifest: &cce_storage_common::metadb::ProjectIndexManifest,
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
    async fn try_adopt_existing_candidate(
        &self,
        operation_id: &str,
    ) -> Result<Option<i64>, OrchestratorError> {
        let Some(store) = self.metadata_store.as_ref() else {
            return Ok(None);
        };
        let active_epoch = match store
            .manifest_active(self.project_id)
            .await
            .map_err(OrchestratorError::Storage)?
        {
            Some(manifest) => manifest.data_epoch,
            None => 0,
        };
        let Some(manifest) = store
            .manifest_by_operation(self.project_id, operation_id)
            .await
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
    pub async fn activate_hot_update_candidate(
        &self,
        operation_id: &str,
    ) -> Result<(), OrchestratorError> {
        let relation_epoch = if self.candidate_relation_epoch.load(Ordering::Acquire) > 0 {
            self.candidate_relation_epoch.load(Ordering::Acquire)
        } else {
            self.active_relation_epoch().await?
        };
        let result = self
            .activate_project_manifest(operation_id, relation_epoch)
            .await;
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
    pub async fn fail_hot_update_candidate(
        &self,
        operation_id: &str,
        reason: &str,
    ) -> Result<(), OrchestratorError> {
        self.fail_project_manifest(operation_id, reason).await?;
        let result = self.cleanup_failed_candidate(self.epoch()).await;
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
    use cce_storage_common::RelationStorage;
    use cce_storage_metadb::RelationStore;

    async fn seed_active_generation(store: &RelationStore, operation: &str, epoch: i64) {
        store
            .ensure_project(1, "/tmp/test")
            .await
            .expect("project should be ensured");
        store
            .manifest_begin_building(1, epoch, operation, None)
            .await
            .expect("building manifest should be created");
        store
            .manifest_activate(1, epoch, 0, operation, None)
            .await
            .expect("generation should be activated");
    }

    #[tokio::test]
    async fn hot_candidate_registers_inheritance_without_copying_data() {
        let database = RelationStore::in_memory().expect("in-memory store");
        seed_active_generation(&database, "initial", 1).await;
        database
            .files_record_hashes(1, 1, &[("src/lib.rs".to_string(), "hash".to_string())])
            .await
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

        let active = database
            .manifest_active(1)
            .await
            .expect("active manifest should load")
            .expect("active manifest should exist");
        assert_eq!(active.data_epoch, 1);

        // Zero-copy: the candidate inherits instead of cloning, so the
        // active generation's rows are not duplicated.
        let candidate_files = database
            .files_list(1, &[2])
            .await
            .expect("candidate files should be queryable");
        assert!(candidate_files.is_empty(), "no physical clone may happen");

        // The inheritance chain is registered and adoptable.
        let building = database
            .manifest_by_operation(1, "hot-operation")
            .await
            .expect("building manifest should load")
            .expect("building manifest should exist");
        assert_eq!(building.parent_data_epoch, Some(1));
        assert!(building.candidate_ready);

        storage
            .fail_hot_update_candidate("hot-operation", "test failure")
            .await
            .expect("candidate should be marked failed");
        let active = database
            .manifest_active(1)
            .await
            .expect("active manifest should load")
            .expect("active manifest should exist");
        assert_eq!(active.data_epoch, 1);

        // Failure path marks the manifest failed (no longer adoptable) and
        // clears the candidate's own rows so the epoch number can be reused.
        let building = database
            .manifest_by_operation(1, "hot-operation")
            .await
            .expect("building manifest query should succeed");
        assert!(
            building.is_none(),
            "failed candidate must not stay building"
        );
    }

    #[tokio::test]
    async fn resume_adopts_ready_candidate_without_recloning() {
        let database = RelationStore::in_memory().expect("in-memory store");
        seed_active_generation(&database, "initial", 1).await;
        database
            .files_record_hashes(1, 1, &[("src/lib.rs".to_string(), "hash".to_string())])
            .await
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
        let database = RelationStore::in_memory().expect("in-memory store");
        seed_active_generation(&database, "initial", 1).await;
        // A building manifest that was never marked ready (simulating a
        // crash before the inheritance registration committed).
        database
            .manifest_begin_building(1, 2, "interrupted", None)
            .await
            .expect("interrupted manifest should be created");

        let storage = StorageCoordinator::new(1)
            .expect("valid project ID")
            .with_metadata_store(database.clone())
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
        let building = database
            .manifest_by_operation(1, "interrupted")
            .await
            .expect("building manifest should load")
            .expect("building manifest should exist");
        assert!(building.candidate_ready);
        assert_eq!(building.parent_data_epoch, Some(1));
    }

    #[tokio::test]
    async fn is_candidate_adoptable_judgment_matrix() {
        let database = RelationStore::in_memory().expect("in-memory store");
        seed_active_generation(&database, "initial", 1).await;
        let storage = StorageCoordinator::new(1)
            .expect("valid project ID")
            .with_metadata_store(database.clone())
            .with_epoch(1);

        // No manifest for the operation at all: not adoptable.
        assert!(
            !storage
                .is_candidate_adoptable("unknown-operation")
                .await
                .expect("query should succeed")
        );

        // Building but never marked candidate_ready (crash before the
        // inheritance registration committed): not adoptable.
        database
            .manifest_begin_building(1, 2, "not-ready", None)
            .await
            .expect("building manifest should be created");
        assert!(
            !storage
                .is_candidate_adoptable("not-ready")
                .await
                .expect("query should succeed")
        );

        // Building + candidate_ready + epoch == active+1, but without the
        // registered parent link: not adoptable (the inheritance chain is
        // incomplete — adopting would lose all unchanged data).
        database
            .manifest_begin_building(1, 2, "ready", None)
            .await
            .expect("ready candidate should be created");
        database
            .manifest_mark_candidate_ready(1, "ready")
            .await
            .expect("candidate should be marked ready");
        assert!(
            !storage
                .is_candidate_adoptable("ready")
                .await
                .expect("query should succeed")
        );

        // Building + candidate_ready + epoch continuity + parent link pointing
        // at the active generation: adoptable.
        database
            .manifest_begin_building(1, 2, "inherited-ready", None)
            .await
            .expect("inherited ready candidate should be created");
        database
            .manifest_set_parent(1, "inherited-ready", Some(1))
            .await
            .expect("parent link should be set");
        database
            .manifest_mark_candidate_ready(1, "inherited-ready")
            .await
            .expect("candidate should be marked ready");
        assert!(
            storage
                .is_candidate_adoptable("inherited-ready")
                .await
                .expect("query should succeed")
        );

        // Building + candidate_ready but targeting a stale epoch (the active
        // generation moved on): not adoptable.
        database
            .manifest_begin_building(1, 5, "stale-epoch", None)
            .await
            .expect("stale candidate should be created");
        database
            .manifest_mark_candidate_ready(1, "stale-epoch")
            .await
            .expect("candidate should be marked ready");
        assert!(
            !storage
                .is_candidate_adoptable("stale-epoch")
                .await
                .expect("query should succeed")
        );

        // Aborted operation: the manifest was marked failed, so no building
        // manifest remains and the candidate is not adoptable.
        database
            .manifest_begin_building(1, 2, "aborted", None)
            .await
            .expect("aborted candidate should be created");
        database
            .manifest_mark_failed(1, "aborted", "injected")
            .await
            .expect("candidate should be marked failed");
        assert!(
            !storage
                .is_candidate_adoptable("aborted")
                .await
                .expect("query should succeed")
        );
    }

    #[tokio::test]
    async fn candidate_creation_compacts_an_inherited_active_generation() {
        use cce_storage_common::metadb::OverrideDisposition;

        let database = RelationStore::in_memory().expect("in-memory store");
        database
            .ensure_project(1, "/tmp/test")
            .await
            .expect("project should be ensured");
        database
            .manifest_begin_building(1, 1, "gen-1", None)
            .await
            .expect("gen-1 should be created");
        database
            .manifest_activate(1, 1, 0, "gen-1", None)
            .await
            .expect("gen-1 should be activated");
        database
            .files_record_hashes(
                1,
                1,
                &[
                    ("src/keep.rs".to_string(), "hash".to_string()),
                    ("src/replaced.rs".to_string(), "hash".to_string()),
                ],
            )
            .await
            .expect("gen-1 files should seed");
        // Generation 2 inherited from generation 1 and was published;
        // one file was replaced inside it (own row + `replaced`).
        database
            .manifest_begin_building(1, 2, "gen-2", None)
            .await
            .expect("gen-2 should be created");
        database
            .manifest_set_parent(1, "gen-2", Some(1))
            .await
            .expect("gen-2 parent should be linked");
        database
            .manifest_activate(1, 2, 0, "gen-2", None)
            .await
            .expect("gen-2 should be activated");
        database
            .files_record_hashes(
                1,
                2,
                &[("src/replaced.rs".to_string(), "hash-new".to_string())],
            )
            .await
            .expect("gen-2 own row should seed");
        database
            .overrides_upsert_batch(
                1,
                2,
                &[("src/replaced.rs".to_string(), OverrideDisposition::Replaced)],
            )
            .await
            .expect("replaced override should seed");

        let storage = StorageCoordinator::new(1)
            .expect("valid project ID")
            .with_metadata_store(database.clone())
            .with_epoch(2);
        let candidate_epoch = storage
            .begin_hot_update_candidate("hot-op", false)
            .await
            .expect("candidate should be prepared");
        assert_eq!(candidate_epoch, 3);

        // Compaction materialized generation 2: parent link cleared, overrides
        // cleared, and the non-overridden parent rows merged into its own rows.
        let active = database
            .manifest_active(1)
            .await
            .expect("active manifest should load")
            .expect("active manifest should exist");
        assert_eq!(active.data_epoch, 2);
        assert_eq!(active.parent_data_epoch, None);
        assert!(
            database
                .overrides_for_generation(1, 2)
                .await
                .expect("overrides should list")
                .is_empty()
        );
        let mut gen2_paths: Vec<String> = database
            .files_list(1, &[2])
            .await
            .expect("gen-2 files should load")
            .into_iter()
            .map(|file| file.path)
            .collect();
        gen2_paths.sort();
        assert_eq!(gen2_paths, vec!["src/keep.rs", "src/replaced.rs"]);

        // The new candidate inherits from the compacted (parent-free) active
        // generation.
        let building = database
            .manifest_by_operation(1, "hot-op")
            .await
            .expect("building manifest should load")
            .expect("building manifest should exist");
        assert_eq!(building.parent_data_epoch, Some(2));

        // Sanity: compaction never rewrote the grandparent generation.
        let grandparent_files = database
            .files_list(1, &[1])
            .await
            .expect("grandparent files should load");
        assert_eq!(grandparent_files.len(), 2);
    }
}
