use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use super::EngineError;
use super::project_cache::ProjectCache;
use crate::runtime::{
    RelationRuntime, RelationRuntimeState, ServerRelationSnapshotPublisher, SnapshotIntegrity,
};
use cce_orchestrator::RelationSnapshotPublisher;
use cce_orchestrator::hot_update::processors::RelationUpdateProcessor;
use cce_orchestrator::index::StorageCoordinator;
use cce_orchestrator::query::retry_queue::RetryQueue;
use cce_storage_sqlite::repo::ProjectRepository;
use cce_storage_sqlite::snapshot_store::SqliteSnapshotStore;

impl super::CodeContextEngine {
    /// Get or create project-specific RelationRuntime
    ///
    /// This method implements lazy loading with caching:
    /// 1. Try to get from cache (fast path)
    /// 2. If cache miss, create new runtime (slow path)
    /// 3. Cache for future requests
    pub async fn get_relation_runtime(
        &self,
        project_id: i64,
    ) -> Result<Arc<RelationRuntime>, EngineError> {
        // Fast path: check cache
        if let Some(runtime) = self.relation_runtime_cache.get(project_id).await {
            tracing::debug!(project_id, "RelationRuntime cache hit");
            self.refresh_relation_runtime_if_needed(project_id, &runtime)
                .await;
            return Ok(runtime);
        }

        // Cache miss - create new runtime
        tracing::info!(project_id, "Creating project-specific RelationRuntime");

        let runtime = Arc::new(RelationRuntime::new(project_id));

        // Double-check: another task may have inserted while we were building
        if let Some(existing) = self.relation_runtime_cache.get(project_id).await {
            tracing::debug!(project_id, "RelationRuntime found after double-check");
            return Ok(existing);
        }
        self.relation_runtime_cache
            .insert(project_id, runtime.clone())
            .await;

        tracing::info!(project_id, "Created and cached RelationRuntime");
        self.refresh_relation_runtime_if_needed(project_id, &runtime)
            .await;
        Ok(runtime)
    }

    /// Build the single publisher that owns relation epoch and runtime changes.
    pub async fn get_relation_snapshot_publisher(
        &self,
        project_id: i64,
    ) -> Result<Arc<dyn RelationSnapshotPublisher>, EngineError> {
        let sqlite = self
            .metadata_store
            .as_ref()
            .map(|database| database.for_project(project_id))
            .transpose()
            .map_err(|e| EngineError::Config(format!("Failed to open project database: {e}")))?
            .ok_or_else(|| {
                EngineError::Config(
                    "SQLite database not initialized for relation publishing".to_string(),
                )
            })?;
        let runtime = self.get_relation_runtime(project_id).await?;
        Ok(Arc::new(ServerRelationSnapshotPublisher::new(
            sqlite.as_ref().clone(),
            runtime,
        )))
    }

    async fn refresh_relation_runtime_if_needed(
        &self,
        project_id: i64,
        runtime: &Arc<RelationRuntime>,
    ) {
        let Some(database) = &self.metadata_store else {
            return;
        };
        let sqlite = match database.for_project(project_id) {
            Ok(client) => client,
            Err(error) => {
                tracing::warn!(
                    project_id,
                    error = %error,
                    "Failed to open project database for relation runtime refresh"
                );
                return;
            }
        };
        // read-only connection instead of a write transaction — this
        // runs on the query path and must not contend with the write lock.
        let active_epoch = match sqlite.read_connection() {
            Ok(conn) => {
                match cce_storage_sqlite::ProjectIndexManifestRepository::get_active(
                    &conn, project_id,
                ) {
                    Ok(Some(manifest)) => manifest.relation_epoch,
                    Ok(None) => {
                        // Reuse the connection already held by `conn`: the
                        // client-level `project_meta_get_int_optional` helper
                        // would re-acquire the same `read_conn` parking_lot
                        // mutex, which is not reentrant and deadlocks the
                        // caller (the fallback is only reached when no active
                        // manifest exists, e.g. a fresh project).
                        match ProjectRepository::meta_get_int_optional(
                            &conn,
                            project_id,
                            "active_relation_epoch",
                        ) {
                            Ok(Some(epoch)) => epoch,
                            Ok(None) => 0,
                            Err(error) => {
                                tracing::warn!(
                                    project_id,
                                    error = %error,
                                    "Failed to read active_relation_epoch for relation runtime refresh"
                                );
                                return;
                            }
                        }
                    }
                    Err(error) => {
                        tracing::warn!(project_id, error = %error, "Failed to read active project manifest");
                        return;
                    }
                }
            }
            Err(error) => {
                tracing::warn!(project_id, error = %error, "Failed to open read connection for relation runtime refresh");
                return;
            }
        };
        let state = runtime.get_state().await;
        let runtime_epoch = runtime.get_relation_epoch().await;
        if active_epoch <= runtime_epoch {
            // Self-healing: a Degraded runtime retries loading the
            // active epoch even when the epoch has not advanced (a failed
            // publish leaves the active epoch unchanged, but the underlying
            // data may now be loadable — e.g. a transient SQLite error, or a
            // later full index that was activated while the runtime stayed
            // degraded). Retries are throttled with an exponential backoff so
            // a persistent failure does not hammer the query path.
            if !matches!(state, RelationRuntimeState::Degraded) {
                return;
            }
            let metadata = runtime.get_metadata().await;
            let backoff_secs = Self::degraded_retry_backoff(metadata.failure_count);
            if let Some(last_attempt) = metadata.last_attempt_at {
                let elapsed_secs = last_attempt
                    .elapsed()
                    .map(|duration| duration.as_secs())
                    .unwrap_or(0);
                if elapsed_secs < backoff_secs {
                    return;
                }
            }
        }

        runtime.set_updating().await;
        match cce_relation::index::snapshot_loader::RelationSnapshotLoader::load(
            &SqliteSnapshotStore::new((*sqlite).clone()),
            project_id,
            active_epoch,
        ) {
            Ok(index) => {
                // The loaded index is local to this refresh and never mutated
                // afterwards, so share its maps zero-copy into the snapshot
                // instead of deep-copying the whole graph
                let snapshot_index =
                    cce_relation::index::RelationSnapshotIndex::from_index_shared(&index);
                runtime
                    .publish_snapshot(
                        Arc::new(snapshot_index),
                        active_epoch,
                        SnapshotIntegrity::Full,
                        Some(format!("relation-epoch-{active_epoch}")),
                    )
                    .await;
            }
            Err(error) => {
                runtime
                    .report_failure(format!(
                        "failed to load active relation epoch {active_epoch}: {error}"
                    ))
                    .await;
            }
        }
    }

    /// Exponential backoff (seconds) before a Degraded relation runtime
    /// retries loading the active epoch: 1s, 2s, 4s, ... capped at 60s.
    fn degraded_retry_backoff(failure_count: u32) -> u64 {
        let exponent = failure_count.min(6);
        (1u64 << exponent).min(60)
    }

    /// Initialize relation runtime for a project (cold start loading)
    ///
    /// This is called during startup recovery to load a snapshot from SQLite.
    pub async fn init_relation_runtime(
        &self,
        project_id: i64,
        index: cce_relation::index::core::RelationIndex,
        relation_epoch: i64,
        integrity: SnapshotIntegrity,
        manifest_id: Option<String>,
    ) -> Result<(), EngineError> {
        let runtime = self.get_relation_runtime(project_id).await?;
        // The index is moved in by value from the cold-start loader and never
        // mutated afterwards, so share its maps zero-copy
        let snapshot_index = cce_relation::index::RelationSnapshotIndex::from_index_shared(&index);
        runtime
            .publish_snapshot(
                Arc::new(snapshot_index),
                relation_epoch,
                integrity,
                manifest_id,
            )
            .await;
        Ok(())
    }

    /// Publish the current complete relation graph through the unified path.
    pub async fn publish_relation_snapshot_from_orchestrator(
        &self,
        project_id: i64,
    ) -> Result<(), EngineError> {
        let orchestrator = self.get_orchestrator(project_id).await?;
        let graph = {
            let orch = orchestrator.lock().await;
            orch.get_relation_builder()
                .map(|builder| (builder.index().clone(), builder.config_fingerprint()))
        };

        match graph {
            Some((index, config_fingerprint)) => {
                let snapshot = match index.to_canonical_snapshot(config_fingerprint) {
                    Ok(snapshot) => snapshot,
                    Err(error) => {
                        self.report_relation_failure(project_id, error.clone())
                            .await?;
                        return Err(EngineError::Config(error));
                    }
                };
                let publisher = self.get_relation_snapshot_publisher(project_id).await?;
                if let Err(error) = publisher
                    .publish(
                        project_id,
                        &format!(
                            "maintenance-{}",
                            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
                        ),
                        snapshot,
                        &index,
                    )
                    .await
                {
                    self.report_relation_failure(project_id, error.to_string())
                        .await?;
                    return Err(EngineError::Index(error.into()));
                }
                Ok(())
            }
            None => {
                let message = "cannot construct a complete relation candidate without an active relation builder; full rebuild required";
                self.report_relation_failure(project_id, message.to_string())
                    .await?;
                Err(EngineError::Config(message.to_string()))
            }
        }
    }

    /// Publish an HTTP incremental relation operation through the same complete
    /// candidate builder used by file-watch updates.
    pub async fn publish_relation_incremental_candidate(
        &self,
        project_id: i64,
        operation_id: &str,
        batch_result: &cce_orchestrator::hot_update::BatchChangeResult,
    ) -> Result<usize, EngineError> {
        let project = self
            .project_registry
            .get_or_load(project_id)
            .await
            .map_err(|error| EngineError::Config(error.to_string()))?;
        let sqlite = self
            .metadata_store
            .as_ref()
            .map(|database| database.for_project(project_id))
            .transpose()
            .map_err(|error| {
                EngineError::Config(format!("Failed to open project database: {error}"))
            })?
            .ok_or_else(|| {
                EngineError::Config(
                    "SQLite database not initialized for relation publishing".to_string(),
                )
            })?;
        let publisher = self.get_relation_snapshot_publisher(project_id).await?;
        let group_id =
            cce_storage_qdrant::generate_project_group_id(project_id, &project.metadata.root_path);
        let storage = Arc::new(
            StorageCoordinator::new(project_id)
                .map_err(|error| EngineError::Config(error.to_string()))?
                .with_metadata_store(sqlite.clone())
                .with_qdrant(self.qdrant.clone())
                .with_bm25(self.bm25.clone())
                .with_embedder(self.embedder.clone())
                .with_project_group_id(group_id),
        );
        storage
            .begin_hot_update_candidate(operation_id, false)
            .await
            .map_err(|error| EngineError::Config(error.to_string()))?;
        let mut processor = RelationUpdateProcessor::with_persistence_and_config(
            sqlite,
            Path::new(&project.metadata.root_path),
            project_id,
        )
        .with_publisher(publisher.clone());
        if let Some(registry) = self.load_plugin_registry(project_id, &project).await {
            processor = processor.with_plugin_registry(registry);
        }
        processor.set_relation_config(&project.config.relation);
        let result = match processor
            .with_storage(storage.clone())
            .publish_batch(operation_id, batch_result)
            .await
        {
            Ok(result) => result,
            Err(error) => {
                let _ = storage.fail_hot_update_candidate(operation_id, &error.to_string());
                return Err(EngineError::Config(error.to_string()));
            }
        };
        storage
            .activate_hot_update_candidate(operation_id)
            .map_err(|error| EngineError::Config(error.to_string()))?;
        if let Err(error) = publisher.maybe_compact(project_id).await {
            tracing::warn!(
                error = %error,
                "Relation delta-chain compaction after HTTP incremental publish failed; deferred"
            );
        }
        Ok(result)
    }

    /// Publish a verified empty relation graph for project-wide clear actions.
    pub async fn publish_empty_relation_snapshot(
        &self,
        project_id: i64,
    ) -> Result<(), EngineError> {
        let publisher = self.get_relation_snapshot_publisher(project_id).await?;
        let snapshot = cce_types::CanonicalRelationSnapshot::new("maintenance-empty".to_string());
        publisher
            .publish(
                project_id,
                &format!(
                    "maintenance-empty-{}",
                    chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
                ),
                snapshot,
                &cce_relation::index::RelationIndex::new(),
            )
            .await
            .map_err(cce_orchestrator::OrchestratorError::from)?;
        Ok(())
    }

    /// Report a relation runtime failure
    pub async fn report_relation_failure(
        &self,
        project_id: i64,
        error: String,
    ) -> Result<(), EngineError> {
        let runtime = self.get_relation_runtime(project_id).await?;
        runtime.report_failure(error).await;
        Ok(())
    }

    /// Get relation runtime capability info for API responses
    pub async fn get_relation_capability_info(
        &self,
        project_id: i64,
    ) -> Result<crate::runtime::RelationCapabilityInfo, EngineError> {
        let runtime = self.get_relation_runtime(project_id).await?;
        let mut info = runtime.get_capability_info().await;
        let active_epoch = match self
            .metadata_store
            .as_ref()
            .map(|database| database.as_ref())
        {
            Some(sqlite) => {
                match sqlite.project_meta_get_int_optional(project_id, "active_relation_epoch") {
                    Ok(value) => value.unwrap_or(0),
                    Err(error) => {
                        tracing::warn!(
                            project_id,
                            error = %error,
                            "Failed to read active_relation_epoch for capability info"
                        );
                        0
                    }
                }
            }
            None => 0,
        };
        info.active_epoch = active_epoch;
        info.runtime_epoch = info.relation_epoch;
        info.rebuild_required = info.rebuild_required || active_epoch != info.runtime_epoch;
        Ok(info)
    }

    /// Remove relation runtime for a project (cleanup on deletion)
    pub async fn remove_relation_runtime(&self, project_id: i64) {
        if let Some(runtime) = self.relation_runtime_cache.remove(project_id).await {
            runtime.clear().await;
            tracing::info!(project_id, "Removed RelationRuntime from cache");
        }
    }

    /// Get all relation runtimes (for debugging/monitoring)
    pub async fn get_all_relation_runtimes(&self) -> HashMap<i64, Arc<RelationRuntime>> {
        let mut result = HashMap::new();
        self.relation_runtime_cache
            .for_each(|pid, runtime| {
                result.insert(pid, runtime.clone());
            })
            .await;
        result
    }

    /// Get the per-project retry queue cache
    pub fn retry_queue(&self) -> &ProjectCache<RetryQueue> {
        &self.retry_queue
    }

    /// Get or create the retry queue for a specific project
    pub async fn get_retry_queue(&self, project_id: i64) -> Arc<RetryQueue> {
        self.retry_queue
            .get_or_insert_with(project_id, || Arc::new(RetryQueue::new()))
            .await
    }

    /// Process the retry queue for a specific project
    ///
    /// Drains all ready queries and re-executes them using the project's searcher.
    /// Returns the number of queries processed.
    pub async fn process_retry_queue(&self, project_id: i64) -> Result<usize, EngineError> {
        let queue = self.get_retry_queue(project_id).await;
        let pending = queue.drain_ready().await;
        if pending.is_empty() {
            return Ok(0);
        }

        let count = pending.len();
        let searcher = self.get_searcher(project_id).await?;
        let searcher = searcher.lock().await;

        for options in pending {
            match searcher.search(&options).await {
                Ok(_) => {
                    tracing::info!(
                        query = %options.query,
                        "Retry queue query succeeded"
                    );
                }
                Err(_) => {
                    tracing::warn!(
                        query = %options.query,
                        "Retry queue query failed, discarding"
                    );
                }
            }
        }

        Ok(count)
    }

    /// Get the total number of queued queries across all projects
    pub async fn retry_queue_total_len(&self) -> usize {
        let queues: Vec<Arc<RetryQueue>> = {
            let mut result = Vec::new();
            self.retry_queue
                .for_each(|_, rq| {
                    result.push(rq.clone());
                })
                .await;
            result
        };
        let mut total = 0;
        for q in queues {
            total += q.len().await;
        }
        total
    }

    /// Clear all retry queues across all projects
    pub async fn clear_all_retry_queues(&self) {
        let queues: Vec<Arc<RetryQueue>> = {
            let mut result = Vec::new();
            self.retry_queue
                .for_each(|_, rq| {
                    result.push(rq.clone());
                })
                .await;
            result
        };
        for q in queues {
            q.clear().await;
        }
    }
}
