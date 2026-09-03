//! Hot update coordinator implementation
//!
//! This module contains the main coordinator for managing hot updates.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crate::hot_update::watcher::{WatchConfig, WatchCoordinator};

use crate::hot_update::change::{BatchChangeResult, FileChangeType};
use crate::hot_update::config::ConfigReloadManager;
use crate::hot_update::error::{HotUpdateError, Result};
use crate::hot_update::event_loop::{EventLoopState, EventLoopStats};
use crate::hot_update::periodic_scan::PeriodicScanTask;
use crate::hot_update::processors::UpdateProcessor;
use crate::hot_update::state::HotUpdateMode;
use crate::operation::{OperationContext, OperationResult, OperationType};

use super::coordinator_core::FILE_EVENT_CHANNEL_CAPACITY;
use super::coordinator_core::HotUpdateCoordinator;

impl HotUpdateCoordinator {
    /// Check if there are pending config changes that need reload
    pub async fn has_pending_config_changes(&self) -> bool {
        self.config_reload.has_pending_config_changes().await
    }

    /// Inject a pending config change path for durable replay.
    ///
    /// Unlike `handle_config_change` this bypasses version deduplication
    /// so a crashed `ConfigChange` checkpoint is always replayed even
    /// when the file content hasn't changed since the crash.
    pub async fn enqueue_pending_config_change(&self, path: PathBuf) {
        let queue = self.config_reload.pending_config_changes();
        let mut pending = queue.lock().await;
        pending.push(path);
    }

    /// Process pending config changes through the operation pipeline
    ///
    /// Each pending config path is drained into one
    /// `OperationType::ConfigChange` operation that runs the full
    /// prepare/process/commit/abort protocol on the operation runtime.
    /// Mutual exclusion with normal hot updates comes from the same gates as
    /// the hot path (operation coordinator + storage candidate lock), and the
    /// reload-manager operation lock serializes concurrent reload calls.
    /// Failed changes are re-queued for the next call.
    pub async fn process_pending_config_changes(
        &self,
        processors: &[&dyn UpdateProcessor],
    ) -> Result<()> {
        if self.watch_root().await.is_none() {
            tracing::warn!("No watch root set, cannot reload configs");
            return Ok(());
        }

        // Serialize concurrent reload calls that share the pending queue.
        let _config_guard = self.config_reload.acquire_operation_lock().await;

        let pending = self.config_reload.take_pending_config_changes().await;
        if pending.is_empty() {
            return Ok(());
        }

        tracing::info!(
            count = pending.len(),
            "Processing pending configuration changes through the operation pipeline"
        );

        let mut failed_paths = Vec::new();
        for config_path in pending {
            let runtime = self.operation.clone();
            let mut ctx = match runtime
                .lock()
                .await
                .begin_operation(OperationType::ConfigChange)
                .await
            {
                Ok(ctx) => ctx,
                Err(e) => {
                    tracing::error!(
                        path = %config_path.display(),
                        error = %e,
                        "Failed to begin config-change operation"
                    );
                    failed_paths.push(config_path);
                    continue;
                }
            };
            ctx.config_path = Some(config_path.clone());
            match runtime
                .lock()
                .await
                .run_operation(&mut ctx, processors)
                .await
            {
                Ok(result) if result.is_successful() => {
                    tracing::info!(
                        operation_id = %ctx.operation_id,
                        path = %config_path.display(),
                        "Config-change operation completed"
                    );
                }
                Ok(result) => {
                    tracing::error!(
                        operation_id = %ctx.operation_id,
                        path = %config_path.display(),
                        failed_modules = result.failed_module_count(),
                        "Config-change operation completed with failures; will retry"
                    );
                    failed_paths.push(config_path);
                }
                Err(e) => {
                    tracing::error!(
                        operation_id = %ctx.operation_id,
                        path = %config_path.display(),
                        error = %e,
                        "Config-change operation failed; will retry"
                    );
                    failed_paths.push(config_path);
                }
            }
        }

        if !failed_paths.is_empty() {
            let failed_count = failed_paths.len();
            self.config_reload.requeue_pending(failed_paths).await;
            return Err(HotUpdateError::hot_update(format!(
                "One or more configuration changes failed; {failed_count} change(s) re-queued"
            )));
        }

        Ok(())
    }

    /// Reload configuration for all processors
    ///
    /// This method should be called when a configuration file changes.
    /// It queues the config change and processes it through the locked path
    /// to ensure mutual exclusion with hot-update operations.
    pub async fn reload_processor_configs(
        &self,
        config_path: &Path,
        processors: &[&dyn UpdateProcessor],
    ) -> Result<()> {
        // Queue the config change
        self.config_reload
            .handle_config_change(config_path, "")
            .await;

        // Process through the locked path for mutual exclusion
        self.process_pending_config_changes(processors).await
    }

    /// Reload all configurations for all processors
    ///
    /// Acquires the operation lock to ensure mutual exclusion with hot updates.
    pub async fn reload_all_processor_configs(
        &self,
        processors: &[&dyn UpdateProcessor],
    ) -> Result<()> {
        let project_root = match self.watch_root().await {
            Some(root) => root,
            None => {
                return Err(HotUpdateError::hot_update(
                    "No watch root set, cannot reload configs",
                ));
            }
        };

        // Acquire the operation lock for mutual exclusion with hot updates.
        let _operation_guard = self.operation.lock().await;

        // For full reload, we just call on_config_change for each processor
        // with the project root as the config path
        for processor in processors {
            if processor.is_enabled() && processor.supports_config_reload() {
                if let Err(e) = processor
                    .on_config_change(&project_root, &project_root)
                    .await
                {
                    tracing::error!(
                        processor = processor.name(),
                        error = %e,
                        "Failed to handle full config reload"
                    );
                }
            }
        }

        tracing::info!("Full configuration reload completed");
        Ok(())
    }

    /// Get change-detection statistics for monitoring.
    pub async fn change_detection_stats(&self) -> crate::hot_update::ChangeDetectionStats {
        self.operation.lock().await.change_detection_stats().await
    }

    /// Get debounce state information for monitoring.
    pub async fn debounce_info(&self) -> crate::hot_update::DebounceInfo {
        let config = self.debounce.config().await;
        let has_pending = self.debounce.has_pending_changes().await;
        let time_until = self.debounce.time_until_next().await;
        crate::hot_update::DebounceInfo {
            has_pending_changes: has_pending,
            time_until_next: time_until,
            config,
        }
    }

    /// Check if an update should be triggered
    ///
    /// This method should be called periodically or on file system events.
    /// It uses the global debounce mechanism to batch changes.
    ///
    /// # Arguments
    ///
    /// * `force` - Force update regardless of debounce timing
    ///
    /// # Returns
    ///
    /// `true` if update should be triggered now
    pub async fn check_should_update(&self, force: bool) -> bool {
        // Check file changes from SQLite
        let has_changes = self.operation.lock().await.check_changes().await;

        // Check debounce
        self.debounce.should_update(has_changes, force).await
    }

    /// Start a new operation
    ///
    /// Initializes a new operation context with unique operation_id and registers it
    /// with the progress manager for fault tolerance.
    ///
    /// # Arguments
    ///
    /// * `operation_type` - Type of operation (HotUpdate, FullIndexing, etc.)
    ///
    /// # Returns
    ///
    /// `OperationContext` with initialized operation_id and configuration
    pub async fn begin_operation(&self, operation_type: OperationType) -> Result<OperationContext> {
        self.operation
            .lock()
            .await
            .begin_operation(operation_type)
            .await
    }

    /// Run a complete operation with processors.
    ///
    /// The operation runs on the detached `HotUpdateOperationRuntime`;
    /// callers hold only the runtime mutex, never the coordinator lock, so
    /// watch events and mode handling stay responsive during long operations.
    pub async fn run_operation(
        &self,
        ctx: &mut OperationContext,
        processors: &[&dyn UpdateProcessor],
    ) -> Result<OperationResult> {
        self.operation
            .lock()
            .await
            .run_operation(ctx, processors)
            .await
    }

    /// Resume an interrupted operation.
    pub async fn resume_operation(
        &self,
        ctx: &mut OperationContext,
        recovery_info: Option<String>,
        processors: &[&dyn UpdateProcessor],
    ) -> Result<OperationResult> {
        self.operation
            .lock()
            .await
            .resume_operation(ctx, recovery_info, processors)
            .await
    }

    /// Run operation using stored processors
    pub async fn run_with_stored_processors(
        &self,
        ctx: &mut OperationContext,
    ) -> Result<OperationResult> {
        self.operation
            .lock()
            .await
            .run_with_stored_processors(ctx)
            .await
    }

    /// Run an explicitly supplied HTTP/watch change set through the same
    /// candidate publication pipeline as filesystem events.
    pub async fn run_explicit_changes(
        &self,
        changes: Vec<(PathBuf, bool)>,
    ) -> Result<OperationResult> {
        self.operation
            .lock()
            .await
            .run_explicit_changes(changes)
            .await
    }

    /// Execute forced hot update through the full processor chain.
    ///
    /// Now goes through run_operation() instead of just update(),
    /// ensuring all downstream processors (relation, embedding, BM25, summary)
    /// are executed. This eliminates the bypass that previously parsed files
    /// without processing them through the storage pipeline.
    ///
    /// Uses stored processors if available, otherwise falls back to
    /// scan-only mode.
    pub async fn force_update(&mut self) -> Result<BatchChangeResult> {
        self.debounce.reset().await;
        let runtime = self.operation.lock().await;
        if !runtime.stored_processors().is_empty() {
            let mut ctx = runtime.begin_operation(OperationType::HotUpdate).await?;
            match runtime.run_with_stored_processors(&mut ctx).await {
                Ok(_result) => {
                    // BatchChangeResult is consumed inside run_operation;
                    // return a minimal indication
                    Ok(BatchChangeResult::new())
                }
                Err(e) => Err(e),
            }
        } else {
            // Fallback: scan only (no downstream processing)
            runtime.update().await
        }
    }

    /// Perform a hot update (scan-based change detection + parse).
    ///
    /// Delegates to the operation runtime; like every operation entry point
    /// it holds only the runtime mutex.
    pub async fn update(&self) -> Result<BatchChangeResult> {
        self.operation.lock().await.update().await
    }

    pub async fn initialize_cache(&mut self, project_root: &Path) -> Result<usize> {
        self.operation
            .lock()
            .await
            .initialize_cache(project_root)
            .await
    }

    /// Set the scan root path without priming the change-detection cache.
    pub async fn set_scan_root_path(&mut self, project_root: &Path) {
        self.operation.lock().await.set_scan_root_path(project_root);
    }

    // ==================== Mode Switch Methods ====================
    // ==================== Mode Switch Methods ====================

    /// Get watched directories
    pub fn watched_dirs(&self) -> Vec<PathBuf> {
        if let Some(ref watcher) = self.watch_coordinator {
            watcher.watched_dirs()
        } else {
            vec![]
        }
    }

    /// Reload all configurations for all processors (public wrapper)
    ///
    /// This method reloads configurations for all registered processors.
    /// It should be called when configuration files change or during initialization.
    ///
    /// # Arguments
    ///
    /// * `project_root` - Project root directory where config files are located
    /// * `processors` - List of processors to reload configurations for
    ///
    /// # Returns
    ///
    /// `Ok(())` if all reloads succeeded, or error with details
    pub async fn reload_all_configs(
        &self,
        project_root: &Path,
        processors: &[&dyn UpdateProcessor],
    ) -> Result<()> {
        tracing::info!(
            processor_count = processors.len(),
            "Reloading configurations for all processors"
        );

        // Acquire the operation lock for mutual exclusion with hot updates.
        let _operation_guard = self.operation.lock().await;

        // For full reload, we just call on_config_change for each processor
        for processor in processors {
            if processor.is_enabled() && processor.supports_config_reload() {
                if let Err(e) = processor.on_config_change(project_root, project_root).await {
                    tracing::error!(
                        processor = processor.name(),
                        error = %e,
                        "Failed to handle full config reload"
                    );
                }
            }
        }

        tracing::info!("Full configuration reload completed");
        Ok(())
    }

    /// Get config reload manager reference
    pub fn config_reload_manager(&self) -> &ConfigReloadManager {
        &self.config_reload
    }

    /// Check and update mode based on event rate
    ///
    /// This should be called periodically to check if mode switching is needed.
    pub async fn check_and_update_mode(&mut self) -> Result<()> {
        if let Some(ref state_machine) = self.mode_state_machine {
            let mut sm = state_machine.lock().await;
            match sm.current_mode {
                HotUpdateMode::FileWatch => {
                    // Check if should degrade to periodic scan
                    if sm.should_degrade() {
                        drop(sm); // Release lock before calling switch
                        self.switch_to_periodic_scan().await?;
                    }
                }
                HotUpdateMode::PeriodicScan => {
                    // Check if should recover to file watch
                    let event_rate = sm.current_event_rate();
                    if sm.should_recover(event_rate) {
                        drop(sm); // Release lock before calling switch
                        self.switch_to_file_watch().await?;
                    }
                }
            }
        }

        Ok(())
    }

    /// Switch to periodic scan mode (degraded mode)
    async fn switch_to_periodic_scan(&mut self) -> Result<()> {
        tracing::warn!("Event storm detected, switching to periodic scan mode");

        // 1. Stop file watch
        self.stop_watch().await?;

        // 2. Update mode
        if let Some(ref state_machine) = self.mode_state_machine {
            let mut sm = state_machine.lock().await;
            sm.switch_to_periodic_scan();
        }
        self.mode = HotUpdateMode::PeriodicScan;

        // 3. Start periodic scan task
        let interval = if let Some(ref state_machine) = self.mode_state_machine {
            let sm = state_machine.lock().await;
            sm.config.degraded_scan_interval_secs
        } else {
            self.config.file_watch.fallback_interval_secs
        };

        let debounce = self.debounce.clone();
        self.periodic_scan_task = Some(PeriodicScanTask::start(debounce, interval));

        tracing::info!("Switched to periodic scan mode");
        Ok(())
    }

    /// Switch to file watch mode (recovery)
    async fn switch_to_file_watch(&mut self) -> Result<()> {
        tracing::info!("Event rate normalized, switching back to file watch mode");

        // 1. Stop periodic scan task
        if let Some(task) = self.periodic_scan_task.take() {
            task.stop();
        }

        // 2. Update mode
        if let Some(ref state_machine) = self.mode_state_machine {
            let mut sm = state_machine.lock().await;
            sm.switch_to_file_watch();
        }
        self.mode = HotUpdateMode::FileWatch;

        // 3. Restart file watch
        if let Some(ref root) = self.watch_root().await {
            // Bounded channel with shared overflow flag (dropped events
            // trigger a full rescan instead of being lost silently).
            let (file_event_tx, file_event_rx) =
                tokio::sync::mpsc::channel(FILE_EVENT_CHANNEL_CAPACITY);
            self.file_event_rx = Some(file_event_rx);
            self.watch_event_overflow = Arc::new(AtomicBool::new(false));

            // Recreate watch coordinator with unified config
            let watch_config = WatchConfig::with_params(
                self.config.file_watch.event_threshold,
                self.config.file_watch.fallback_interval_secs,
                self.config.file_watch.verification_interval_secs,
                vec![],
                self.config
                    .scanner
                    .as_ref()
                    .map(|s| s.exclude_patterns.clone())
                    .unwrap_or_default(),
                self.config.file_watch.watch_config_files,
                1000,
                self.config.file_watch.storm_duration_secs,
                self.config.file_watch.recovery_threshold,
                self.config.file_watch.recovery_duration_secs,
            )
            .map_err(|e| HotUpdateError::hot_update(format!("Invalid watch config: {}", e)))?;

            let watch_coordinator = WatchCoordinator::new(
                watch_config,
                file_event_tx,
                self.watch_event_overflow.clone(),
            )
            .map_err(|e| {
                HotUpdateError::hot_update(format!("Failed to recreate watch coordinator: {}", e))
            })?;
            let watch_coordinator = if let Some(metrics) = &self.watch_metrics {
                watch_coordinator.with_metrics(metrics.clone())
            } else {
                watch_coordinator
            };
            self.watch_coordinator = Some(watch_coordinator);

            // Start watching
            if let Some(ref mut watcher) = self.watch_coordinator {
                watcher.start(root).await.map_err(|e| {
                    HotUpdateError::hot_update(format!("Failed to restart watcher: {}", e))
                })?;
            }

            // Restart the event loop so events arriving on the fresh channel
            // (recreated above) are consumed. The previous loop exits when its
            // old channel closes; without this restart, watch events would be
            // silently dropped after a storm recovery.
            self.start_event_loop().await?;
        }

        tracing::info!("Switched back to file watch mode");
        Ok(())
    }

    /// Get current mode
    pub fn current_mode(&self) -> HotUpdateMode {
        self.mode
    }

    /// Get current event rate from mode state machine
    pub async fn current_event_rate(&self) -> Option<usize> {
        if let Some(ref state_machine) = self.mode_state_machine {
            let sm = state_machine.lock().await;
            Some(sm.current_event_rate())
        } else {
            None
        }
    }

    // ==================== Event Loop Management Methods ====================

    /// Get event loop state
    pub fn event_loop_state(&self) -> EventLoopState {
        self.event_loop_manager.state()
    }

    /// Get event loop statistics
    pub fn event_loop_stats(&self) -> EventLoopStats {
        self.event_loop_manager.stats().clone()
    }

    /// Check if event loop is running
    pub fn is_event_loop_running(&self) -> bool {
        self.event_loop_manager.is_running()
    }

    /// Generate and attach file summaries to batch change result
    ///
    /// This method processes all parse results in the batch and generates
    /// file summaries for each, attaching them to the ParseResultWithChanges.
    /// This should be called before running downstream processors.
    ///
    /// # Arguments
    ///
    /// * `batch_result` - Mutable batch change result to update with summaries
    /// * `summary_generator` - Summary generator to use for generating summaries
    ///
    /// # Returns
    ///
    /// Number of summaries generated successfully
    pub async fn generate_summaries_for_batch(
        batch_result: &mut BatchChangeResult,
        summary_generator: &dyn cce_parser::summary::SummaryGenerator,
    ) -> usize {
        let mut count = 0;

        for parse_result in &mut batch_result.parse_results {
            // Only generate summary for added/modified files, not deleted ones
            if parse_result.file_change_type == FileChangeType::Deleted {
                continue;
            }

            let summary = summary_generator.generate(&parse_result.parsed_file).await;
            parse_result.file_summary = Some(summary);
            count += 1;
        }

        count
    }

    // Summary generation is now performed by the SummaryUpdateProcessor
    // during the processor phase, with persistence handled by the unified
    // checkpoint system (`persist_summaries_to_checkpoints`).
}

pub(crate) fn coalesce_pending_changes(pending: Vec<(PathBuf, bool)>) -> Vec<(PathBuf, bool)> {
    let mut order: Vec<PathBuf> = Vec::new();
    let mut flags: std::collections::HashMap<PathBuf, bool> = std::collections::HashMap::new();
    for (path, is_deletion) in pending {
        if !flags.contains_key(&path) {
            order.push(path.clone());
        }
        flags.insert(path, is_deletion);
    }
    order
        .into_iter()
        .map(|path| {
            let is_deletion = flags.remove(&path).unwrap_or(false);
            (path, is_deletion)
        })
        .collect()
}
