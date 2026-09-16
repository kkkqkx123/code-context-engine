//! Application startup integration
//!
//! Coordinates startup recovery and initialization tasks that must complete
//! before the application begins accepting requests, and schedules periodic
//! maintenance tasks (checkpoint TTL cleanup).

use std::sync::Arc;
use tracing::{info, warn};

use crate::engine::CodeContextEngine;

/// Startup coordinator for recovery and initialization
pub struct StartupCoordinator {
    engine: Arc<CodeContextEngine>,
}

impl StartupCoordinator {
    /// Create a new startup coordinator
    pub fn new(engine: Arc<CodeContextEngine>) -> Self {
        Self { engine }
    }

    /// Schedule the periodic checkpoint TTL cleanup task.
    ///
    /// The cleanup interval and the default TTL come from the global
    /// orchestrator configuration; each project's effective
    /// `checkpoint_ttl_seconds` (which may be overridden by project config)
    /// is applied per project by the engine task.
    ///
    /// # Responsibility boundary
    ///
    /// This task performs storage hygiene only: it deletes *terminal*
    /// (Completed/Failed) checkpoints older than the TTL and never touches
    /// in_progress operations. Re-arming crashed operations at startup is a
    /// separate concern handled by the operation queue's heartbeat cleanup
    /// (`OperationCoordinator::initialize` →
    /// `OperationQueue::cleanup_stale_operations`), which clears the stale
    /// `active_flag` so the in_progress checkpoint can be recovered.
    pub fn start_periodic_checkpoint_cleanup(&self) {
        let global_config = match cce_config::Settings::global() {
            Ok(config) => config,
            Err(e) => {
                warn!(error = %e, "Failed to load global config for checkpoint cleanup task");
                return;
            }
        };
        self.engine.start_checkpoint_cleanup_task(
            global_config.orchestrator.checkpoint_cleanup_interval_secs,
            global_config.orchestrator.checkpoint_ttl_seconds,
        );
    }

    /// Execute all startup recovery and initialization tasks
    ///
    /// This method should be called during application initialization before
    /// the server starts accepting requests. It performs:
    /// 1. Project startup recovery for all active projects
    /// 2. Version state validation
    /// 3. Incomplete operation cleanup
    /// 4. Explicit ConfigChange replay
    ///
    /// In a typical setup, projects are discovered through the orchestrator
    /// or CLI arguments. This coordinator iterates through them and recovers each.
    ///
    /// # Returns
    ///
    /// Returns the number of projects successfully recovered. Non-critical
    /// failures are logged but don't block startup.
    pub async fn execute_startup(
        &self,
        project_ids: &[i64],
    ) -> Result<usize, Box<dyn std::error::Error>> {
        if project_ids.is_empty() {
            info!("No projects to recover");
            return Ok(0);
        }

        info!(
            project_count = project_ids.len(),
            "Starting application startup recovery"
        );

        let mut recovered_count = 0;
        let mut failed_count = 0;

        for project_id in project_ids {
            if let Err(error) = self
                .engine
                .recover_unfinished_operations_for_project(*project_id)
                .await
            {
                warn!(
                    project_id,
                    %error,
                    "Failed to enqueue unfinished operations during startup"
                );
            }

            match self.engine.recover_project_startup_state(*project_id).await {
                Ok(result) => {
                    info!(
                        project_id = project_id,
                        files_classified = result.files_classified,
                        files_reparsed = result.files_reparsed,
                        files_resynced = result.files_resynced,
                        entities = result.entity_count,
                        relations = result.relation_count,
                        "Project startup recovery completed"
                    );
                    recovered_count += 1;
                }
                Err(e) => {
                    warn!(
                        project_id = project_id,
                        error = %e,
                        "Project startup recovery failed (non-critical)"
                    );
                    failed_count += 1;
                    // Continue with other projects - recovery failures are non-critical
                }
            }
        }

        // Durable ConfigChange replay: the in-memory pending queue is
        // volatile, so a crash loses it. Scan the checkpoint table for
        // unfinished ConfigChange entries within the freshness window and
        // re-inject their config paths into the hot-update coordinator's
        // pending queue before the memory-only replay below.
        for project_id in project_ids {
            let operation_coordinator = match self
                .engine
                .get_operation_coordinator(*project_id)
                .await
            {
                Ok(coord) => coord,
                Err(error) => {
                    warn!(
                        project_id = *project_id,
                        error = %error,
                        "Failed to get operation coordinator for durable config replay (non-critical)"
                    );
                    continue;
                }
            };
            let hot_coordinator = match self.engine.get_hot_update_coordinator(*project_id).await {
                Ok(coord) => coord,
                Err(error) => {
                    warn!(
                        project_id = *project_id,
                        error = %error,
                        "Failed to get hot-update coordinator for durable config replay (non-critical)"
                    );
                    continue;
                }
            };
            if hot_coordinator.lock().await.watch_root().await.is_none() {
                if let Ok(entry) = self
                    .engine
                    .project_registry()
                    .get_or_load(*project_id)
                    .await
                {
                    hot_coordinator
                        .lock()
                        .await
                        .set_watch_root(std::path::PathBuf::from(&entry.metadata.root_path))
                        .await;
                }
            }
            let unfinished = match operation_coordinator
                .checkpoint_manager()
                .get_unfinished_operations()
                .await
            {
                Ok(ops) => ops,
                Err(error) => {
                    warn!(
                        project_id = *project_id,
                        error = %error,
                        "Failed to list unfinished checkpoints for durable config replay (non-critical)"
                    );
                    continue;
                }
            };
            // Freshness window mirrors `OperationCoordinator::is_stale_for_recovery`.
            let ttl_secs = match self
                .engine
                .project_registry()
                .get_or_load(*project_id)
                .await
            {
                Ok(entry) => entry.config.orchestrator.checkpoint_ttl_seconds,
                Err(_) => 86400,
            };
            for checkpoint in unfinished
                .into_iter()
                .filter(|cp| cp.operation_type == "config_change")
            {
                let is_stale = match chrono::DateTime::parse_from_rfc3339(&checkpoint.updated_at) {
                    Ok(updated_at) => {
                        (chrono::Utc::now() - updated_at.with_timezone(&chrono::Utc)).num_seconds()
                            > ttl_secs as i64
                    }
                    Err(_) => true,
                };
                if is_stale {
                    warn!(
                        project_id = *project_id,
                        operation_id = %checkpoint.operation_id,
                        "Stale ConfigChange checkpoint skipped during durable replay"
                    );
                    if let Err(error) = operation_coordinator
                        .checkpoint_manager()
                        .mark_operation_failed(&checkpoint.operation_id, "stale recovery skipped")
                        .await
                    {
                        warn!(
                            project_id = *project_id,
                            operation_id = %checkpoint.operation_id,
                            error = %error,
                            "Failed to mark stale ConfigChange checkpoint failed"
                        );
                    }
                    continue;
                }
                let config_path = {
                    let raw = std::path::PathBuf::from(&checkpoint.root_dir);
                    if raw.extension().is_some_and(|ext| ext == "toml")
                        || (raw.is_absolute() && raw.exists())
                    {
                        raw
                    } else {
                        // Legacy checkpoint stored watch_root: fall back to project's .cce/config.toml
                        match self
                            .engine
                            .project_registry()
                            .get_or_load(*project_id)
                            .await
                        {
                            Ok(entry) => std::path::PathBuf::from(&entry.metadata.root_path)
                                .join(".cce")
                                .join("config.toml"),
                            Err(_) => raw.join("config.toml"),
                        }
                    }
                };
                hot_coordinator
                    .lock()
                    .await
                    .enqueue_pending_config_change(config_path.clone())
                    .await;
                info!(
                    project_id = *project_id,
                    operation_id = %checkpoint.operation_id,
                    config_path = %config_path.display(),
                    "Re-injected durable ConfigChange checkpoint into pending queue"
                );
            }
        }

        // ConfigChange operations are deliberately isolated from hot-update resume:
        // `HotUpdateOperationRuntime::try_recover_operation` filters by
        // `OperationKind::HotUpdate`, so a crashed ConfigChange checkpoint is
        // never resumed as a file change operation. It relies on the operation
        // queue replay (`recover_unfinished_operations`) and on the pending
        // config-change queue. Startup explicitly drives one
        // `process_pending_config_changes` cycle so a pending change does not
        // linger silently until the next file-watch event. Failures are
        // re-queued and do not block normal hot updates (active-operation
        // gate).
        for project_id in project_ids {
            match self.engine.get_hot_update_coordinator(*project_id).await {
                Ok(coordinator) => {
                    let has_pending = {
                        let guard = coordinator.lock().await;
                        guard.has_pending_config_changes().await
                    };
                    if !has_pending {
                        continue;
                    }
                    let processors = {
                        let guard = coordinator.lock().await;
                        guard.stored_processors().await
                    };
                    let processor_refs: Vec<&dyn cce_orchestrator::hot_update::UpdateProcessor> =
                        processors
                            .iter()
                            .map(|p| {
                                p.as_ref() as &dyn cce_orchestrator::hot_update::UpdateProcessor
                            })
                            .collect();
                    let guard = coordinator.lock().await;
                    match guard.process_pending_config_changes(&processor_refs).await {
                        Ok(()) => {
                            info!(
                                project_id = *project_id,
                                "Pending config changes replayed at startup"
                            );
                        }
                        Err(error) => {
                            warn!(
                                project_id = *project_id,
                                error = %error,
                                "Pending config changes failed at startup, re-queued for retry (non-blocking)"
                            );
                        }
                    }
                }
                Err(error) => {
                    warn!(
                        project_id = *project_id,
                        error = %error,
                        "Failed to get hot-update coordinator for config-change replay (non-critical)"
                    );
                }
            }
        }

        info!(
            recovered = recovered_count,
            failed = failed_count,
            "Application startup recovery completed"
        );

        Ok(recovered_count)
    }
}
