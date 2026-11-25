//! Resume of an interrupted hot update from its checkpoint.
//!
//! Rebuilds the change set a previous run had already parsed and continues
//! from the first processor stage that did not finish, instead of reparsing
//! the whole batch.
//!
//! The operation-critical mutable state of a hot update lives here behind its
//! own mutex. The background worker locks only this runtime while an
//! operation runs, so the coordinator stays available for event
//! accumulation, storm detection and configuration handling.

use crate::hot_update::change::FileChangeType;
use crate::hot_update::error::{HotUpdateError, Result};
use crate::hot_update::processors::UpdateProcessor;
use crate::operation::{ModuleFailure, OperationContext, OperationResult};

/// Mutable state shared by every hot-update operation.
///
/// Guarded by its own mutex so the background worker can run a long
/// operation while other coordinator APIs (watch status, stop watch, mode
/// checks) keep working under the coordinator lock.
use super::HotUpdateOperationRuntime;
impl HotUpdateOperationRuntime {
    /// Resume an interrupted operation.
    pub async fn resume_operation(
        &self,
        ctx: &mut OperationContext,
        _recovery_info: Option<String>,
        processors: &[&dyn UpdateProcessor],
    ) -> Result<OperationResult> {
        tracing::info!(
            operation_id = %ctx.operation_id,
            "Resuming interrupted operation"
        );
        // Storage-backed processors adopt the previously-prepared candidate
        // generation and skip modules whose work already completed.
        ctx.resume = true;
        let _heartbeat_guard = self.spawn_heartbeat_guard(&ctx.operation_id);

        // Invalidate module progress markers when the operation's candidate
        // generation can no longer be adopted. A module marker is only valid
        // while the candidate it was written against survives: a crash keeps
        // the building candidate (adoptable → markers stay valid), while an
        // abort voids it (manifest failed → markers must be cleared so every
        // module re-executes against the fresh clone).
        //
        // Adoptability can only be verified with the storage coordinator. When
        // it is absent the markers are treated as void: a marker whose
        // candidate was never confirmed alive would make every module skip a
        // file it never durably wrote, which loses data, whereas clearing them
        // only costs a redundant module pass.
        let adoptable = match self.storage_coordinator {
            Some(ref storage) => storage
                .is_candidate_adoptable(&ctx.operation_id)
                .await
                .map_err(|e| HotUpdateError::hot_update(e.to_string()))?,
            None => false,
        };
        if !adoptable {
            let cleared = if let Some(ref cm) = self.checkpoint_manager {
                cm.clear_module_progress(&ctx.operation_id)
                    .await
                    .map_err(|error| HotUpdateError::hot_update(error.to_string()))?
            } else {
                0
            };
            tracing::warn!(
                operation_id = %ctx.operation_id,
                cleared_file_checkpoints = cleared,
                "Candidate generation is not adoptable on resume; cleared module progress markers"
            );
        } else {
            tracing::debug!(
                operation_id = %ctx.operation_id,
                "Candidate generation is adoptable on resume; keeping module progress markers"
            );
        }

        // Reconstruct the exact hot-update input from the persisted
        // checkpoints. Version or content mismatches fall back to a fresh
        // parse; deleted files keep their tombstone.
        let mut batch_result = self
            .reconstruct_batch_from_checkpoints(&ctx.operation_id)
            .await?;

        // A crash can happen after manifest activation but before the
        // checkpoint/hash projection is committed. The manifest is the durable
        // publication record in that case; never prepare a new candidate or
        // write into the already-active generation.
        if self.is_manifest_active(&ctx.operation_id).await? {
            let published_paths: Vec<_> = batch_result
                .parse_results
                .iter()
                .map(|result| result.file_path.clone())
                .collect();
            let deleted_paths: Vec<_> = batch_result
                .file_changes
                .iter()
                .filter(|change| matches!(change.change_type, FileChangeType::Deleted))
                .map(|change| change.path.clone())
                .collect();
            self.commit_file_hashes(&published_paths, &deleted_paths)
                .await?;
            if let Some(ref cm) = self.checkpoint_manager {
                cm.mark_operation_completed(&ctx.operation_id)
                    .await
                    .map_err(|error| HotUpdateError::hot_update(error.to_string()))?;
            }
            if let Some(ref coordinator) = self.operation_coordinator {
                coordinator
                    .complete_operation()
                    .await
                    .map_err(|error| HotUpdateError::hot_update(error.to_string()))?;
            }
            return self
                .finalize_operation(ctx, &batch_result, Vec::new())
                .await;
        }

        let mut all_failures = Vec::new();
        let mut processor_failed = false;
        let enabled_processors: Vec<&dyn UpdateProcessor> = processors
            .iter()
            .copied()
            .filter(|processor| processor.is_enabled())
            .collect();
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
            }
        }

        if !processor_failed {
            for processor in processors {
                if processor.is_enabled() {
                    match processor.process_operation(ctx, &mut batch_result).await {
                        Ok(result) => {
                            all_failures.extend(result.failed_modules);
                        }
                        Err(e) => {
                            processor_failed = true;
                            for path in batch_result
                                .parse_results
                                .iter()
                                .map(|result| result.file_path.to_string_lossy().to_string())
                                .chain(
                                    batch_result
                                        .file_changes
                                        .iter()
                                        .map(|change| change.path.to_string_lossy().to_string()),
                                )
                            {
                                all_failures.push(ModuleFailure {
                                    file_path: path,
                                    module_name: processor.name().to_string(),
                                    error: e.to_string(),
                                    retry_count: 0,
                                    next_retry_time: None,
                                });
                            }
                            tracing::error!(
                                processor = processor.name(),
                                error = %e,
                                "Processor failed during resume"
                            );
                        }
                    }
                }
            }
        }

        // Persist summaries generated during resume so a subsequent crash is
        // also recoverable without regenerating summaries.
        if let Some(ref cm) = self.checkpoint_manager
            && let Err(error) = self
                .persist_summaries_to_checkpoints(ctx, &batch_result, cm)
                .await
        {
            tracing::warn!(
                operation_id = %ctx.operation_id,
                error = %error,
                "Failed to persist summaries into checkpoints during resume"
            );
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
                }
            }
        }
        if processor_failed || !all_failures.is_empty() {
            for processor in &enabled_processors {
                if let Err(error) = processor
                    .abort_operation(ctx, "one or more resumed stages failed")
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

        if !processor_failed && all_failures.is_empty() {
            let published_paths: Vec<_> = batch_result
                .parse_results
                .iter()
                .map(|result| result.file_path.clone())
                .collect();
            let deleted_paths: Vec<_> = batch_result
                .file_changes
                .iter()
                .filter(|c| matches!(c.change_type, FileChangeType::Deleted))
                .map(|c| c.path.clone())
                .collect();
            self.commit_file_hashes(&published_paths, &deleted_paths)
                .await?;
            if let Some(ref cm) = self.checkpoint_manager {
                cm.mark_operation_completed(&ctx.operation_id)
                    .await
                    .map_err(|error| HotUpdateError::hot_update(error.to_string()))?;
            }
            if let Some(ref coordinator) = self.operation_coordinator {
                coordinator
                    .complete_operation()
                    .await
                    .map_err(|error| HotUpdateError::hot_update(error.to_string()))?;
            }
        } else {
            // Stay resumable: a failed resume keeps its checkpoint InProgress
            // so the next scan retries with envelope reuse; only the queue
            // activity is released here.
            if let Some(ref coordinator) = self.operation_coordinator
                && let Err(error) = coordinator
                    .clear_active_by_operation(&ctx.operation_id)
                    .await
            {
                tracing::warn!(
                    operation_id = %ctx.operation_id,
                    error = %error,
                    "Failed to clear active flag after resume failure"
                );
            }
        }

        // Every module failure recorded here is work a later pass must redo;
        // the count feeds the hot-update retry-rate metric.
        if !all_failures.is_empty()
            && let Some(metrics) = self.metrics()
        {
            metrics.module_retry_total.add(all_failures.len() as u64);
        }

        let result = self
            .finalize_operation(ctx, &batch_result, all_failures)
            .await?;

        Ok(result)
    }
    /// Helper: Try to recover incomplete operation
    pub(crate) async fn try_recover_operation(
        &self,
        ctx: &OperationContext,
    ) -> Result<Option<String>> {
        if let Some(ref cm) = self.checkpoint_manager {
            let root_dir = self
                .watch_root
                .as_ref()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| ".".to_string());
            match cm
                .validate_and_recover_checkpoint(
                    &ctx.operation_id,
                    cce_types::OperationKind::HotUpdate,
                    &root_dir,
                )
                .await
            {
                Ok(Some(checkpoint)) => {
                    tracing::info!(
                        operation_id = %ctx.operation_id,
                        status = %checkpoint.status,
                        "Found incomplete checkpoint for recovery"
                    );
                    return Ok(Some(checkpoint.operation_id));
                }
                Ok(None) => {
                    tracing::trace!(
                        operation_id = %ctx.operation_id,
                        "No incomplete checkpoint found"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        operation_id = %ctx.operation_id,
                        error = %e,
                        "Failed to check checkpoint for recovery"
                    );
                }
            }
        }
        Ok(None)
    }
}
