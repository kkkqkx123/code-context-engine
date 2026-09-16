use std::path::Path;

use super::EngineError;
use crate::runtime::{ProjectMeta, RecoveryResult, SnapshotIntegrity, StartupRecoveryCoordinator};
use cce_orchestrator::query::types::{QueryOptions, QueryResult};
use cce_orchestrator::{IndexOptions, IndexResult};
use cce_relation::index::entity_index::EntityIndexOps;
use cce_relation::index::relation_query::RelationQueryOps;
use cce_relation::index::snapshot_loader::RelationSnapshotLoader;
use cce_storage_sqlite::snapshot_store::SqliteSnapshotStore;

impl super::CodeContextEngine {
    // --- Index operations ---

    /// Recover all unfinished operations for a specific project
    ///
    /// This should be called once per project after engine initialization to ensure
    /// any operations that were interrupted are resumed properly.
    /// Returns the count of operations recovered.
    pub async fn recover_unfinished_operations_for_project(
        &self,
        project_id: i64,
    ) -> Result<u32, EngineError> {
        tracing::info!(project_id, "Starting recovery of unfinished operations");

        let operation_coordinator = self.get_operation_coordinator(project_id).await?;
        let recovered_count = operation_coordinator
            .recover_unfinished_operations()
            .await
            .map_err(|e| {
                EngineError::Index(cce_orchestrator::OrchestratorError::index(
                    "recovery",
                    format!("Failed to recover operations: {}", e),
                ))
            })?;

        if recovered_count > 0 {
            tracing::info!(
                project_id,
                recovered_count = recovered_count,
                "Successfully recovered unfinished operations"
            );
        } else {
            tracing::debug!(project_id, "No unfinished operations to recover");
        }

        Ok(recovered_count)
    }

    /// Perform startup recovery for a project
    ///
    /// This triggers the complete recovery sequence:
    /// 1. File classification based on content hash and timestamps
    /// 2. Re-parse queue processing for modified files
    /// 3. Resync queue processing for incomplete files
    /// 4. RelationIndex metadata collection
    ///
    /// Returns recovery statistics for monitoring and logging.
    ///
    /// **Integration**: See [`crate::runtime::StartupCoordinator`] for the
    /// recommended integration pattern in application startup.
    pub async fn recover_project_startup_state(
        &self,
        project_id: i64,
    ) -> Result<RecoveryResult, EngineError> {
        tracing::info!(project_id, "Starting project startup recovery");

        let sqlite_client = self
            .metadata_store
            .as_ref()
            .map(|db| db.for_project(project_id))
            .transpose()
            .map_err(|e| EngineError::Config(format!("Failed to open project database: {e}")))?
            .ok_or_else(|| {
                EngineError::Config("SQLite database not initialized for recovery".to_string())
            })?;

        let coordinator = StartupRecoveryCoordinator::new(sqlite_client.as_ref().clone());

        let orchestrator = self.get_orchestrator(project_id).await.ok();

        let mut result: RecoveryResult = coordinator
            .recover_project(project_id, orchestrator)
            .await
            .map_err(|e| EngineError::Recovery(format!("Startup recovery failed: {}", e)))?;

        // After file recovery, load the relation generation selected by the
        // project manifest. Legacy projects without a manifest retain the
        // previous project-meta fallback.
        let meta = ProjectMeta::load(&sqlite_client, project_id).map_err(|e| {
            EngineError::Recovery(format!("Failed to read project meta after recovery: {}", e))
        })?;
        let relation_epoch = sqlite_client
            .with_transaction(|tx| {
                cce_storage_sqlite::ProjectIndexManifestRepository::get_active(tx, project_id)
            })
            .map_err(|error| EngineError::Recovery(error.to_string()))?
            .map(|manifest| manifest.relation_epoch)
            .unwrap_or(meta.active_relation_epoch);

        if relation_epoch > 0 {
            match RelationSnapshotLoader::load(
                &SqliteSnapshotStore::new((*sqlite_client).clone()),
                project_id,
                relation_epoch,
            ) {
                Ok(index) => {
                    result.entity_count = index.function_count();
                    result.relation_count = index.resolved_relation_count();
                    // The strict loader has already verified manifest state, versions,
                    // counts, references, and canonical fingerprint.
                    let integrity = if result.entity_count == 0 {
                        SnapshotIntegrity::Empty
                    } else {
                        SnapshotIntegrity::Full
                    };
                    let manifest_id = {
                        let conn = sqlite_client.read_connection().map_err(|error| {
                            EngineError::Recovery(format!(
                                "Failed to read relation manifest: {error}"
                            ))
                        })?;
                        cce_storage_sqlite::repo::RelationSnapshotRepository::get_manifest(
                            &conn,
                            project_id,
                            relation_epoch,
                        )
                        .map_err(|error| EngineError::Recovery(error.to_string()))?
                        .map(|manifest| manifest.operation_id)
                    };

                    self.init_relation_runtime(
                        project_id,
                        index,
                        relation_epoch,
                        integrity,
                        manifest_id,
                    )
                    .await?;
                    tracing::info!(
                        project_id,
                        epoch = relation_epoch,
                        entity_count = result.entity_count,
                        relation_count = result.relation_count,
                        ?integrity,
                        "Loaded relation snapshot from SQLite during startup recovery"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        project_id,
                        epoch = relation_epoch,
                        error = %e,
                        "Failed to load relation snapshot from SQLite, runtime will remain Unloaded"
                    );
                }
            }
        }

        Ok(result)
    }

    /// Execute full indexing with project-specific configuration
    pub async fn index(
        &self,
        project_id: i64,
        mut options: IndexOptions,
    ) -> Result<IndexResult, EngineError> {
        // Generate unique operation ID using timestamp
        let operation_id = format!("full_index_{}", chrono::Utc::now().timestamp_millis());

        // Get project configuration (with cache)
        let project_entry = self
            .project_registry
            .get_or_load(project_id)
            .await
            .map_err(|e| EngineError::Config(format!("Failed to load project config: {}", e)))?;
        options.root_dir = Path::new(&project_entry.metadata.root_path).to_path_buf();

        // Request full-index operation in coordinator
        let operation_coordinator = self.get_operation_coordinator(project_id).await?;
        operation_coordinator
            .request_full_index(
                operation_id.clone(),
                options.root_dir.to_string_lossy().to_string(),
            )
            .await
            .map_err(|e| {
                EngineError::Index(cce_orchestrator::OrchestratorError::index(
                    "full_index",
                    format!("Failed to request operation: {}", e),
                ))
            })?;

        // Dequeue to acquire exclusive execution right (marks operation as active).
        // Without this gate, multiple index() calls could run concurrently.
        // The active state is persisted to the database for crash recovery.
        operation_coordinator
            .execute_next_operation()
            .await
            .map_err(|e| {
                EngineError::Index(cce_orchestrator::OrchestratorError::index(
                    "full_index",
                    format!("Failed to dequeue operation: {}", e),
                ))
            })?
            .ok_or_else(|| {
                EngineError::Index(cce_orchestrator::OrchestratorError::index(
                    "full_index",
                    "Another operation is already active, cannot execute concurrently".to_string(),
                ))
            })?;

        // Apply project-specific scanner configuration to index options
        tracing::debug!(
            project_id,
            project_name = project_entry.metadata.name,
            operation_id = %operation_id,
            "Applying project-specific configuration for indexing"
        );

        // Override with project-specific settings if configured
        let project_config = &project_entry.config;

        // Apply project orchestrator config
        let orchestrator_config = &project_config.orchestrator;

        // Apply project indexer config
        let indexer_config = &orchestrator_config.indexer;
        if !indexer_config.extensions.is_empty() {
            options.extensions = indexer_config.extensions.clone();
            tracing::debug!(
                project_id,
                extensions_count = options.extensions.len(),
                "Applied project-specific file extensions"
            );
        }
        if !indexer_config.exclude_dirs.is_empty() {
            options.exclude_dirs = indexer_config.exclude_dirs.clone();
            tracing::debug!(
                project_id,
                exclude_dirs_count = options.exclude_dirs.len(),
                "Applied project-specific exclude directories"
            );
        }
        options.store_vectors = indexer_config.store_vectors;
        options.store_bm25 = indexer_config.store_bm25;
        options.store_summaries = indexer_config.store_summaries;
        options.build_relations =
            indexer_config.build_relations && project_config.relation.index.enabled;

        // Get project-specific orchestrator (with project batch config)
        let orchestrator = self.get_orchestrator(project_id).await?;
        let mut orchestrator = orchestrator.lock().await;

        // Save build_relations flag before moving options
        let build_relations = options.build_relations;

        // Execute indexing operation
        let result: IndexResult = orchestrator
            .execute(options)
            .await
            .map_err(EngineError::Index)?;

        // `IndexOrchestrator` publishes complete relation snapshots through
        // the injected publisher before returning a successful result.
        let published = !build_relations || result.is_success();
        if build_relations && !result.is_success() {
            // Block publication on partial/incomplete result
            tracing::warn!(
                project_id,
                "Blocking relation snapshot publication due to incomplete indexing outcome"
            );
        }

        // Complete operation ONLY after successful publish
        if published {
            operation_coordinator
                .complete_operation()
                .await
                .map_err(|e| {
                    EngineError::Index(cce_orchestrator::OrchestratorError::index(
                        "full_index",
                        format!("Failed to complete operation: {}", e),
                    ))
                })?;

            // 4.B.2: Trigger catch-up hot update for accumulated watch events.
            // During full-index, watcher events accumulate in pending_watch_changes
            // but run_operation() is blocked by the dequeue gate. After completion,
            // we drain them immediately so the watcher is back in sync.
            if let Ok(hot_coordinator) = self.get_hot_update_coordinator(project_id).await {
                let mut hot = hot_coordinator.lock().await;
                if hot.has_pending_changes().await {
                    tracing::info!(
                        project_id,
                        "Triggering catch-up hot update after full index"
                    );
                    if let Err(e) = hot.force_update().await {
                        // Non-fatal: watcher will retry on next notification
                        tracing::warn!(
                            project_id,
                            error = %e,
                            "Catch-up hot update failed (non-fatal)"
                        );
                    }
                } else {
                    tracing::debug!(project_id, "No pending watch changes after full index");
                }
            }
        } else {
            let reason = if result.errors().is_empty() {
                "full_index publish failed or incomplete result".to_string()
            } else {
                result.errors().join("; ")
            };
            if let Err(error) = operation_coordinator
                .checkpoint_manager()
                .mark_operation_failed(&operation_id, &reason)
                .await
            {
                tracing::warn!(
                    operation_id = %operation_id,
                    error = %error,
                    "Failed to mark full-index checkpoint failed"
                );
            }
            if let Err(error) = operation_coordinator
                .clear_active_by_operation(&operation_id)
                .await
            {
                tracing::warn!(
                    operation_id = %operation_id,
                    error = %error,
                    "Failed to clear active flag after full-index failure"
                );
            }
            tracing::warn!(
                project_id,
                operation_id = %operation_id,
                reason = %reason,
                "Operation marked failed and active flag cleared after indexing failure"
            );
        }

        Ok(result)
    }

    // --- Query operations ---

    /// Search for code with project-specific configuration
    pub async fn search(
        &self,
        project_id: i64,
        options: &QueryOptions,
    ) -> Result<QueryResult, EngineError> {
        if options.project_id <= 0 {
            return Err(EngineError::Config(
                "QueryOptions.project_id must be positive".to_string(),
            ));
        }
        if options.project_id != project_id {
            return Err(EngineError::Config(format!(
                "QueryOptions.project_id ({}) does not match the engine project_id ({})",
                options.project_id, project_id
            )));
        }

        // Get project-specific searcher
        let searcher = self.get_searcher(project_id).await?;
        let searcher = searcher.lock().await;
        searcher.search(options).await.map_err(EngineError::Query)
    }
}
