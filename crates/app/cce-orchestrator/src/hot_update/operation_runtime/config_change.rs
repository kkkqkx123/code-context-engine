//! Hot-update operation runtime.
//!
//! The background worker previously held the entire
//! `Arc<Mutex<HotUpdateCoordinator>>` while running a complete operation
//! (parse + embed + publish), blocking watch-status/stop-watch/mode-switch
//! APIs for the whole operation duration. The operation-critical mutable
//! state lives in this sub-component with its own mutex: the worker locks
//! only the runtime while an operation runs, leaving the coordinator free
//! for event accumulation, storm detection and configuration handling
//! (those paths already run on cloned components and never contend here).

use std::path::Path;

use cce_types::OperationKind;

use crate::hot_update::change::BatchChangeResult;
use crate::hot_update::error::{HotUpdateError, Result};
use crate::hot_update::processors::UpdateProcessor;
use crate::operation::checkpoint::CreateCheckpointParams;
use crate::operation::{ModuleFailure, OperationContext, OperationResult};

/// Mutable state shared by every hot-update operation.
///
/// Guarded by its own mutex so the background worker can run a long
/// operation while other coordinator APIs (watch status, stop watch, mode
/// checks) keep working under the coordinator lock.
use super::HotUpdateOperationRuntime;
impl HotUpdateOperationRuntime {
    /// Run a configuration-change operation through the full candidate
    /// protocol.
    ///
    /// Config changes are scheduled through the operation pipeline instead of
    /// direct `on_config_change` callbacks: the shared candidate generation is
    /// prepared/committed/aborted exactly like a hot update, the operation is
    /// checkpointed (kind `ConfigChange`, which hot-update recovery never
    /// resumes), and the operation coordinator gate prevents concurrent
    /// execution with other operations.
    ///
    /// Only processors with a config-change branch in `process_operation` do
    /// real work (relation rebuild, export config reload); the remaining
    /// processors process the empty change set as a no-op while still
    /// participating in the candidate lifecycle.
    pub(crate) async fn run_config_change_operation(
        &self,
        ctx: &mut OperationContext,
        processors: &[&dyn UpdateProcessor],
        config_path: &Path,
    ) -> Result<OperationResult> {
        tracing::info!(
            operation_id = %ctx.operation_id,
            config_path = %config_path.display(),
            "Running configuration-change operation"
        );

        // Register the operation with the coordinator and dequeue to acquire
        // exclusive execution rights, mirroring the hot-update path. A
        // rejected request (e.g. an active full index) surfaces as an error;
        // the pending config path stays queued for the next retry.
        if let Some(ref coordinator) = self.operation_coordinator {
            let root_dir = self
                .watch_root
                .as_ref()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| ".".to_string());
            coordinator
                .request_hot_update(ctx.operation_id.clone(), root_dir)
                .await
                .map_err(|e| {
                    HotUpdateError::hot_update(format!(
                        "Failed to request config-change operation: {}",
                        e
                    ))
                })?;
            coordinator
                .execute_next_operation()
                .await
                .map_err(|e| {
                    HotUpdateError::hot_update(format!(
                        "Failed to dequeue config-change operation: {}",
                        e
                    ))
                })?
                .ok_or_else(|| {
                    HotUpdateError::hot_update(
                        "Cannot execute config-change: another operation is active".to_string(),
                    )
                })?;
        }

        let _heartbeat_guard = self.spawn_heartbeat_guard(&ctx.operation_id);

        // Create the operation checkpoint before any processor work. The
        // dedicated kind keeps it out of hot-update recovery: a crashed
        // config change is never resumed as a file-change operation.
        // Persist the triggering config file path in `root_dir` so a
        // crash can be replayed from durable state without the volatile
        // in-memory pending queue.
        if let Some(ref cm) = self.checkpoint_manager {
            let config_root = config_path.to_string_lossy().to_string();
            cm.create_checkpoint(CreateCheckpointParams {
                operation_id: &ctx.operation_id,
                operation_type: OperationKind::ConfigChange,
                root_dir: &config_root,
                total_files: 0,
                batch_size: 1,
                file_list_hash: "",
            })
            .await
            .map_err(|e| {
                HotUpdateError::hot_update(format!(
                    "Failed to create config-change checkpoint: {}",
                    e
                ))
            })?;
        }

        let mut all_failures = Vec::new();
        let mut processor_failed = false;
        let enabled_processors: Vec<&dyn UpdateProcessor> = processors
            .iter()
            .copied()
            .filter(|processor| processor.is_enabled())
            .collect();

        // Prepare every enabled processor so the shared candidate generation
        // is set up (idempotently) before any writes.
        for processor in &enabled_processors {
            if let Err(error) = processor.prepare_operation(ctx).await {
                processor_failed = true;
                all_failures.push(ModuleFailure {
                    file_path: String::new(),
                    module_name: processor.name().to_string(),
                    error: error.to_string(),
                    retry_count: 0,
                    next_retry_time: None,
                });
                tracing::error!(
                    processor = processor.name(),
                    error = %error,
                    "Failed to prepare config-change candidate"
                );
            }
        }

        // Process with an empty change set: config-aware processors rebuild
        // from the operation context; the rest are no-ops on the empty batch.
        let mut empty_batch = BatchChangeResult::new();
        if !processor_failed {
            for processor in &enabled_processors {
                match processor.process_operation(ctx, &mut empty_batch).await {
                    Ok(result) => {
                        all_failures.extend(result.failed_modules);
                    }
                    Err(e) => {
                        processor_failed = true;
                        all_failures.push(ModuleFailure {
                            file_path: String::new(),
                            module_name: processor.name().to_string(),
                            error: e.to_string(),
                            retry_count: 0,
                            next_retry_time: None,
                        });
                        tracing::error!(
                            processor = processor.name(),
                            error = %e,
                            "Processor failed during config change"
                        );
                    }
                }
            }
        }

        if !processor_failed && all_failures.is_empty() {
            for processor in &enabled_processors {
                if let Err(error) = processor.commit_operation(ctx).await {
                    processor_failed = true;
                    all_failures.push(ModuleFailure {
                        file_path: String::new(),
                        module_name: processor.name().to_string(),
                        error: error.to_string(),
                        retry_count: 0,
                        next_retry_time: None,
                    });
                    tracing::error!(
                        processor = processor.name(),
                        error = %error,
                        "Failed to activate config-change candidate"
                    );
                }
            }
        }

        if processor_failed || !all_failures.is_empty() {
            for processor in &enabled_processors {
                if let Err(error) = processor
                    .abort_operation(ctx, "one or more config-change stages failed")
                    .await
                {
                    all_failures.push(ModuleFailure {
                        file_path: String::new(),
                        module_name: processor.name().to_string(),
                        error: format!("failed to retire candidate: {error}"),
                        retry_count: 0,
                        next_retry_time: None,
                    });
                }
            }
        }

        // No file hashes change during a config change (the change-detection
        // cache is untouched), so no hash commit happens here. The checkpoint
        // must reach a terminal state so a crash cannot leave a recoverable
        // hot-update artifact behind.
        if let Some(ref cm) = self.checkpoint_manager {
            if !processor_failed && all_failures.is_empty() {
                cm.mark_operation_completed(&ctx.operation_id)
                    .await
                    .map_err(|e| {
                        HotUpdateError::hot_update(format!(
                            "Failed to mark config-change checkpoint completed: {}",
                            e
                        ))
                    })?;
            } else {
                let reason = all_failures
                    .first()
                    .map(|failure| failure.error.clone())
                    .unwrap_or_else(|| "unknown config-change failure".to_string());
                if let Err(error) = cm.mark_operation_failed(&ctx.operation_id, &reason).await {
                    tracing::warn!(
                        operation_id = %ctx.operation_id,
                        error = %error,
                        "Failed to mark config-change checkpoint failed"
                    );
                }
            }
        }
        if processor_failed || !all_failures.is_empty() {
            if let Some(ref coordinator) = self.operation_coordinator {
                if let Err(error) = coordinator
                    .clear_active_by_operation(&ctx.operation_id)
                    .await
                {
                    tracing::warn!(
                        operation_id = %ctx.operation_id,
                        error = %error,
                        "Failed to clear active flag after config-change failure"
                    );
                }
            }
        }

        let result = self
            .finalize_operation(ctx, &empty_batch, all_failures)
            .await?;

        // Mark the operation completed in the coordinator (if available).
        if !processor_failed
            && result.failed_modules.is_empty()
            && let Some(ref coordinator) = self.operation_coordinator
        {
            coordinator.complete_operation().await.map_err(|e| {
                HotUpdateError::hot_update(format!(
                    "Failed to complete config-change operation in coordinator: {}",
                    e
                ))
            })?;
        }

        Ok(result)
    }
}
