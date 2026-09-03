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

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

use tokio::sync::Mutex;

use std::time::Duration;

use cce_metrics::HotUpdateMetrics;
use cce_storage_sqlite::SqliteClient;
use cce_types::OperationKind;

use super::change::{BatchChangeResult, FileChange, FileChangeType};
use super::change_detector::{CacheUpdateResult, ChangeDetector};
use super::coordinator::coalesce_pending_changes;
use super::error::{HotUpdateError, Result};
use super::processors::UpdateProcessor;
use super::watch_change_queue::WatchChangeQueue;
use crate::operation::checkpoint::CreateCheckpointParams;
use crate::operation::{
    AggregatedMetrics, ModuleFailure, OperationContext, OperationResult, OperationStatus,
    OperationSummary, OperationType,
};

/// Mutable state shared by every hot-update operation.
///
/// Guarded by its own mutex so the background worker can run a long
/// operation while other coordinator APIs (watch status, stop watch, mode
/// checks) keep working under the coordinator lock.
pub struct HotUpdateOperationRuntime {
    /// Project ID: 0 for global project, >0 for user projects
    project_id: i64,
    /// Change detector for scan-based change detection.
    change_detector: ChangeDetector,
    /// Metadata store for reading the previously indexed state.
    metadata_store: Option<Arc<SqliteClient>>,
    /// Checkpoint manager for hot-update checkpoint persistence.
    checkpoint_manager: Option<Arc<crate::operation::CheckpointManager>>,
    /// Storage coordinator shared with the update processors.
    storage_coordinator: Option<Arc<crate::index::StorageCoordinator>>,
    /// Operation coordinator for the hot-update queue.
    operation_coordinator: Option<Arc<crate::operation::OperationCoordinator>>,
    /// Watch root path.
    watch_root: Option<PathBuf>,
    /// Test-only parse counter shared with every `FileProcessor`.
    parse_probe: Option<Arc<AtomicUsize>>,
    /// Pending file changes accumulated from file watch events (bounded with
    /// overflow backpressure).
    pending_watch_changes: Arc<WatchChangeQueue>,
    /// Pending configuration-change paths queued for the operation pipeline.
    ///
    /// The coordinator wires the shared queue owned by `ConfigReloadManager`
    /// so `handle_config_change` enqueues directly into it; `run_operation`
    /// drains one path per `ConfigChange` operation.
    config_change_pending: Arc<Mutex<Vec<PathBuf>>>,
    /// Processors stored for self-execution without an external caller.
    stored_processors: Vec<Arc<dyn UpdateProcessor>>,
    /// Monitoring metrics (optional).
    metrics: Option<Arc<HotUpdateMetrics>>,
    /// Heartbeat interval for long-running operations.
    ///
    /// The runtime periodically refreshes `checkpoint.last_heartbeat` while an
    /// operation is in flight so `cleanup_stale_active` does not clear a live
    /// operation. Defaults to 60s; may be overridden from
    /// `orchestrator.heartbeat_interval_secs`.
    heartbeat_interval: Duration,
}

/// Guard that aborts the periodic heartbeat task on drop.
///
/// The task is spawned when an operation becomes active and is cancelled once
/// the operation reaches a terminal state (`commit`/`abort`). The abort is
/// best-effort: the spawned task handles its own errors with a warn log.
struct HeartbeatGuard {
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl Drop for HeartbeatGuard {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}

impl HotUpdateOperationRuntime {
    /// Create the runtime for a project.
    pub fn new(
        project_id: i64,
        db: Arc<SqliteClient>,
        scan_options: cce_scanner::ScanOptions,
    ) -> Self {
        let mut change_detector = ChangeDetector::new(db, scan_options);
        change_detector.set_project_id(project_id);
        Self {
            project_id,
            change_detector,
            metadata_store: None,
            checkpoint_manager: None,
            storage_coordinator: None,
            operation_coordinator: None,
            watch_root: None,
            parse_probe: None,
            pending_watch_changes: Arc::new(WatchChangeQueue::with_default_capacity()),
            config_change_pending: Arc::new(Mutex::new(Vec::new())),
            stored_processors: Vec::new(),
            metrics: None,
            heartbeat_interval: Duration::from_secs(60),
        }
    }

    // ==================== Accessors (for the coordinator) ====================

    pub fn project_id(&self) -> i64 {
        self.project_id
    }

    pub fn watch_root(&self) -> Option<&Path> {
        self.watch_root.as_deref()
    }

    pub fn set_watch_root(&mut self, root: PathBuf) {
        self.watch_root = Some(root);
    }

    /// Test-only: drop the watch root so ownership checks reject everything.
    #[cfg(test)]
    pub fn clear_watch_root(&mut self) {
        self.watch_root = None;
    }

    pub fn change_detector_mut(&mut self) -> &mut ChangeDetector {
        &mut self.change_detector
    }

    pub fn metadata_store(&self) -> Option<&Arc<SqliteClient>> {
        self.metadata_store.as_ref()
    }

    pub fn set_metadata_store(&mut self, store: Arc<SqliteClient>) {
        self.metadata_store = Some(store.clone());
        self.change_detector =
            ChangeDetector::new(store, self.change_detector.scan_options().clone());
        self.change_detector.set_project_id(self.project_id);
    }

    pub fn set_storage_coordinator(&mut self, coordinator: Arc<crate::index::StorageCoordinator>) {
        self.storage_coordinator = Some(coordinator);
    }

    pub fn set_operation_coordinator(
        &mut self,
        coordinator: Arc<crate::operation::OperationCoordinator>,
    ) {
        self.operation_coordinator = Some(coordinator);
    }

    pub fn set_checkpoint_manager(&mut self, manager: Arc<crate::operation::CheckpointManager>) {
        self.checkpoint_manager = Some(manager);
    }

    pub fn checkpoint_manager(&self) -> Option<Arc<crate::operation::CheckpointManager>> {
        self.checkpoint_manager.clone()
    }

    pub fn set_stored_processors(&mut self, processors: Vec<Arc<dyn UpdateProcessor>>) {
        self.stored_processors = processors;
    }

    pub fn stored_processors(&self) -> &[Arc<dyn UpdateProcessor>] {
        &self.stored_processors
    }

    pub fn set_parse_probe(&mut self, probe: Arc<AtomicUsize>) {
        self.parse_probe = Some(probe);
    }

    pub fn set_metrics(&mut self, metrics: Arc<HotUpdateMetrics>) {
        self.metrics = Some(metrics);
    }

    pub fn metrics(&self) -> Option<&Arc<HotUpdateMetrics>> {
        self.metrics.as_ref()
    }

    pub fn set_heartbeat_interval(&mut self, interval: Duration) {
        self.heartbeat_interval = interval;
    }

    #[allow(dead_code)]
    pub fn heartbeat_interval(&self) -> Duration {
        self.heartbeat_interval
    }

    fn spawn_heartbeat_guard(&self, operation_id: &str) -> Option<HeartbeatGuard> {
        let coordinator = self.operation_coordinator.clone()?;
        let op_id = operation_id.to_string();
        let interval = self.heartbeat_interval;
        let handle = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.tick().await;
            loop {
                ticker.tick().await;
                if let Err(error) = coordinator.heartbeat(&op_id).await {
                    tracing::warn!(
                        operation_id = %op_id,
                        error = %error,
                        "Periodic heartbeat failed"
                    );
                } else {
                    tracing::trace!(operation_id = %op_id, "Heartbeat refreshed");
                }
            }
        });
        Some(HeartbeatGuard {
            handle: Some(handle),
        })
    }

    pub async fn has_pending_changes(&self) -> bool {
        !self.pending_watch_changes.is_empty().await
    }

    pub async fn pending_changes_len(&self) -> usize {
        self.pending_watch_changes.len().await
    }

    pub fn pending_watch_changes(&self) -> Arc<WatchChangeQueue> {
        self.pending_watch_changes.clone()
    }

    /// Wire the shared config-change queue (owned by `ConfigReloadManager`).
    pub fn set_config_change_pending(&mut self, pending: Arc<Mutex<Vec<PathBuf>>>) {
        self.config_change_pending = pending;
    }

    /// Check if a configuration change is pending for the pipeline.
    pub async fn has_pending_config_changes(&self) -> bool {
        let pending = self.config_change_pending.lock().await;
        !pending.is_empty()
    }

    /// Take one pending configuration path for a `ConfigChange` operation.
    pub async fn take_one_config_change(&self) -> Option<PathBuf> {
        let mut pending = self.config_change_pending.lock().await;
        if pending.is_empty() {
            return None;
        }
        Some(pending.remove(0))
    }

    // ==================== Path helpers ====================

    /// Resolve a possibly project-relative path to its absolute on-disk
    /// location under the scan root.
    fn resolve_scan_path(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            return path.to_path_buf();
        }
        let root = Path::new(&self.change_detector.scan_options().root_path);
        root.join(path)
    }

    /// Express a path relative to the scan root; absolute paths outside the
    /// root are returned unchanged (with a warning, as they break the
    /// project-relative path identity).
    fn relativize_scan_path(&self, path: &Path) -> PathBuf {
        let root = Path::new(&self.change_detector.scan_options().root_path);
        PathBuf::from(cce_types::path::relativize(root, path))
    }

    // ==================== Change detection ====================

    pub async fn initialize_cache(&mut self, project_root: &Path) -> Result<usize> {
        self.change_detector
            .set_root_path(&project_root.to_string_lossy());
        self.change_detector.initialize().await
    }

    /// Set the scan root path without priming the change-detection cache.
    pub fn set_scan_root_path(&mut self, project_root: &Path) {
        self.change_detector
            .set_root_path(&project_root.to_string_lossy());
    }

    /// Check if there are file changes (without updating).
    pub async fn check_changes(&self) -> bool {
        self.change_detector.check_changes().await
    }

    /// Scan files and detect changes
    async fn scan_and_detect_changes(&self) -> Result<CacheUpdateResult> {
        self.change_detector.scan_and_detect().await
    }

    /// Get change-detection statistics for monitoring.
    pub async fn change_detection_stats(&self) -> super::ChangeDetectionStats {
        let stored_files = self.change_detector.count_stored_files().await.unwrap_or(0);
        super::ChangeDetectionStats { stored_files }
    }

    // ==================== Operation lifecycle ====================

    pub async fn begin_operation(&self, operation_type: OperationType) -> Result<OperationContext> {
        let operation_id = uuid::Uuid::new_v4().to_string();
        let file_count = 1000; // Default estimate
        Ok(OperationContext::new(
            self.project_id,
            operation_id,
            operation_type,
            file_count,
        ))
    }

    /// Run an explicitly supplied HTTP/watch change set through the same
    /// candidate publication pipeline as filesystem events.
    pub async fn run_explicit_changes(
        &self,
        changes: Vec<(PathBuf, bool)>,
    ) -> Result<OperationResult> {
        self.pending_watch_changes.extend(changes).await;
        let mut ctx = self.begin_operation(OperationType::HotUpdate).await?;
        self.run_with_stored_processors(&mut ctx).await
    }

    /// Run operation using stored processors
    pub async fn run_with_stored_processors(
        &self,
        ctx: &mut OperationContext,
    ) -> Result<OperationResult> {
        let processor_refs: Vec<Arc<dyn UpdateProcessor>> = self.stored_processors.clone();
        let refs: Vec<&dyn UpdateProcessor> = processor_refs.iter().map(|p| p.as_ref()).collect();
        self.run_operation(ctx, &refs).await
    }

    /// Run a complete operation with processors.
    pub async fn run_operation(
        &self,
        ctx: &mut OperationContext,
        processors: &[&dyn UpdateProcessor],
    ) -> Result<OperationResult> {
        // Configuration changes take priority over file changes and recovery:
        // a pending config change must be applied before any subsequent hot
        // update rebuilds under the old resolution semantics. The coordinator
        // drives one operation per config path; the background processor also
        // drains the queue so watch mode applies pending config changes even
        // without an explicit reload call.
        if ctx.operation_type == OperationType::ConfigChange {
            let config_path = ctx.config_path.clone().ok_or_else(|| {
                HotUpdateError::hot_update(
                    "ConfigChange operation requires a config path".to_string(),
                )
            })?;
            return self
                .run_config_change_operation(ctx, processors, &config_path)
                .await;
        }
        if self.has_pending_config_changes().await {
            let config_path = self
                .take_one_config_change()
                .await
                .expect("pending config changes checked non-empty");
            ctx.operation_type = OperationType::ConfigChange;
            ctx.config_path = Some(config_path.clone());
            return self
                .run_config_change_operation(ctx, processors, &config_path)
                .await;
        }

        // Try to resume an incomplete operation from its checkpoint before
        // starting a fresh one.
        if let Some(recovered_operation_id) = self.try_recover_operation(ctx).await? {
            ctx.operation_id = recovered_operation_id;
            return self.resume_operation(ctx, None, processors).await;
        }

        // Register a new operation only after recovery has been ruled out.
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
                        "Failed to request hot-update operation: {}",
                        e
                    ))
                })?;

            // Dequeue to acquire exclusive execution right.
            // If another operation is active (e.g. full index), this hot-update
            // is rejected; the caller should retry on the next watcher notification.
            coordinator
                .execute_next_operation()
                .await
                .map_err(|e| {
                    HotUpdateError::hot_update(format!(
                        "Failed to dequeue hot-update operation: {}",
                        e
                    ))
                })?
                .ok_or_else(|| {
                    HotUpdateError::hot_update(
                        "Cannot execute hot-update: another operation is active".to_string(),
                    )
                })?;
        }

        let _heartbeat_guard = self.spawn_heartbeat_guard(&ctx.operation_id);

        // Create the operation checkpoint before any parsing so a crash during
        // change detection/parsing can be recovered on restart. The changed
        // files are filled in once parsing completes.
        if let Some(ref cm) = self.checkpoint_manager {
            let root_dir = self
                .watch_root
                .as_ref()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| ".".to_string());
            cm.create_checkpoint(CreateCheckpointParams {
                operation_id: &ctx.operation_id,
                operation_type: OperationKind::HotUpdate,
                root_dir: &root_dir,
                total_files: 0,
                batch_size: 1,
                file_list_hash: "",
            })
            .await
            .map_err(|e| {
                HotUpdateError::hot_update(format!("Failed to create early checkpoint: {}", e))
            })?;
        }

        // Detect the changed files: prefer pending watch events, otherwise fall
        // back to a cache scan. If the bounded watch queue overflowed (or the
        // upstream event channel dropped events), a partial event list would
        // silently miss files, so a full filesystem scan is required instead.
        // The change detector only re-hashes files whose size/mtime
        // differ from the cache, so the fallback stays cheap in practice.
        let needs_full_rescan = self.pending_watch_changes.needs_full_rescan();
        if needs_full_rescan {
            if let Some(ref metrics) = self.metrics {
                metrics.record_full_rescan_fallback();
            }
        }
        let pending_paths = self.pending_watch_changes.take().await;

        // Coalesce duplicate paths: one file write usually yields several
        // watch events (create + modify), and each event would otherwise parse
        // and index the same file multiple times in one operation. The first
        // occurrence keeps its position (deterministic processing order), and
        // the last event's deletion flag wins because it describes the most
        // recent on-disk state.
        let pending_paths = coalesce_pending_changes(pending_paths);

        let mut batch_result = if needs_full_rescan {
            self.update().await?
        } else if !pending_paths.is_empty() {
            self.process_watch_paths(&pending_paths).await?
        } else {
            self.update().await?
        };

        if !batch_result.has_changes() {
            if let Some(ref coordinator) = self.operation_coordinator {
                coordinator.complete_operation().await.map_err(|e| {
                    HotUpdateError::hot_update(format!(
                        "Failed to complete operation when no changes: {}",
                        e
                    ))
                })?;
            }
            // The early checkpoint created before change detection must not
            // linger as in_progress: a later run would otherwise "resume" the
            // empty operation, clone the active generation and publish a
            // pointless new epoch. Marking it completed also lets the TTL
            // cleanup retire it later.
            if let Some(ref cm) = self.checkpoint_manager {
                if let Err(error) = cm.mark_operation_completed(&ctx.operation_id).await {
                    tracing::warn!(
                        operation_id = %ctx.operation_id,
                        error = %error,
                        "Failed to mark no-change checkpoint completed"
                    );
                }
            }
            return Ok(OperationResult {
                operation_id: ctx.operation_id.clone(),
                status: OperationStatus::Completed,
                summary: OperationSummary {
                    total_files_processed: 0,
                    total_files_failed: 0,
                    total_modules_retried: 0,
                    total_duration_ms: ctx.elapsed_ms(),
                    can_resume: false,
                },
                failed_modules: Vec::new(),
                metrics: AggregatedMetrics::default(),
            });
        }

        // Create the batch checkpoint (FK parent of the per-file checkpoints)
        // and merge the changed files into the early operation checkpoint.
        if let Some(ref cm) = self.checkpoint_manager {
            let changed_files: Vec<String> = batch_result
                .file_changes
                .iter()
                .map(|c| c.path.to_string_lossy().to_string())
                .chain(
                    batch_result
                        .parse_results
                        .iter()
                        .map(|pr| pr.file_path.to_string_lossy().to_string()),
                )
                .collect();

            let first_file = changed_files.first().cloned().unwrap_or_default();
            let last_file = changed_files.last().cloned().unwrap_or_default();

            cm.create_batch_checkpoint(
                &ctx.operation_id,
                0,
                &first_file,
                &last_file,
                changed_files.len() as u32,
            )
            .await
            .map_err(|e| {
                HotUpdateError::hot_update(format!("Failed to create batch checkpoint: {}", e))
            })?;

            self.persist_hot_parse_checkpoints(ctx, &batch_result, cm)
                .await?;
        }

        // Execute the configured processors (embedding, bm25, relation,
        // summary, nl_document). Track per-file module status for diagnostics.
        let mut all_file_paths: Vec<String> = batch_result
            .file_changes
            .iter()
            .map(|c| c.path.to_string_lossy().to_string())
            .collect();
        for pr in &batch_result.parse_results {
            let path = pr.file_path.to_string_lossy().to_string();
            if !all_file_paths.contains(&path) {
                all_file_paths.push(path);
            }
        }
        let mut file_module_status: HashMap<String, HashMap<String, bool>> = HashMap::new();
        for path in &all_file_paths {
            let mut modules = HashMap::new();
            modules.insert("relation".to_string(), false);
            modules.insert("summary".to_string(), false);
            modules.insert("embedding".to_string(), false);
            modules.insert("bm25".to_string(), false);
            modules.insert("export".to_string(), false);
            file_module_status.insert(path.clone(), modules);
        }

        // Prepare every enabled processor so the shared candidate generation is
        // set up (idempotently) before any writes. Each processor owns only its
        // own derived state; storage-backed processors share one candidate.
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
                tracing::error!(
                    processor = processor.name(),
                    error = %error,
                    "Failed to prepare hot-update candidate"
                );
            }
        }

        if !processor_failed {
            for processor in processors {
                let processor_name = processor.name().to_string();
                if processor.is_enabled() {
                    match processor.process_operation(ctx, &mut batch_result).await {
                        Ok(result) => {
                            // Mark module success for files without failures
                            for file_path in &all_file_paths {
                                if let Some(modules) = file_module_status.get_mut(file_path) {
                                    if let Some(status) = modules.get_mut(&processor_name) {
                                        let has_failure = result.failed_modules.iter().any(|f| {
                                            f.file_path == *file_path
                                                && f.module_name == processor_name
                                        });
                                        *status = !has_failure;
                                    }
                                }
                            }

                            all_failures.extend(result.failed_modules.clone());
                        }
                        Err(e) => {
                            processor_failed = true;
                            for file_path in &all_file_paths {
                                all_failures.push(ModuleFailure {
                                    file_path: file_path.clone(),
                                    module_name: processor.name().to_string(),
                                    error: e.to_string(),
                                    retry_count: 0,
                                    next_retry_time: None,
                                });
                            }
                            tracing::error!(
                                processor = processor.name(),
                                error = %e,
                                "Processor failed"
                            );
                        }
                    }
                } else {
                    // Mark disabled processor as skipped for diagnostic consistency
                    for file_path in &all_file_paths {
                        if let Some(modules) = file_module_status.get_mut(file_path) {
                            if let Some(status) = modules.get_mut(&processor_name) {
                                *status = true;
                            }
                        }
                    }
                }
            }
        }

        // Persist generated summaries into the parse checkpoints. The early
        // checkpoint persistence ran before the summary processor, so the
        // envelopes carried no `file_summary`; this pass re-persists them so a
        // crash between summary generation and operation completion lets a
        // resumed run reuse the summaries instead of regenerating them.
        if let Some(ref cm) = self.checkpoint_manager {
            if let Err(error) = self
                .persist_summaries_to_checkpoints(ctx, &batch_result, cm)
                .await
            {
                tracing::warn!(
                    operation_id = %ctx.operation_id,
                    error = %error,
                    "Failed to persist summaries into checkpoints"
                );
            }
        }

        if !processor_failed && all_failures.is_empty() {
            // Commit every enabled processor in order. Storage-backed
            // processors share one candidate and `activate` is idempotent for
            // the same operation, so sequential commits are safe.
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
                        "Failed to activate hot-update candidate"
                    );
                }
            }
        }

        if processor_failed || !all_failures.is_empty() {
            for processor in &enabled_processors {
                if let Err(error) = processor
                    .abort_operation(ctx, "one or more hot-update stages failed")
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

        // A file hash is the publication marker. Commit it only after every
        // processor completed, otherwise the next scan must retry the file.
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

            // Mark checkpoint as completed on full success
            if let Some(ref cm) = self.checkpoint_manager {
                cm.mark_operation_completed(&ctx.operation_id)
                    .await
                    .map_err(|e| {
                        HotUpdateError::hot_update(format!(
                            "Failed to mark checkpoint completed: {}",
                            e
                        ))
                    })?;
            }
        } else {
            // Failure path must never leave the queue deadlocked: clear the
            // active flag by operation_id so a subsequent dequeue can proceed
            // without waiting for a restart or heartbeat timeout. The
            // checkpoint stays resumable (InProgress) on purpose: failed
            // modules are retried by the next scan, which reuses the stored
            // parsed envelopes instead of re-reading the sources.
            if let Some(ref coordinator) = self.operation_coordinator {
                if let Err(error) = coordinator
                    .clear_active_by_operation(&ctx.operation_id)
                    .await
                {
                    tracing::warn!(
                        operation_id = %ctx.operation_id,
                        error = %error,
                        "Failed to clear active flag after hot-update failure"
                    );
                }
            }
        }

        // Finalize the operation result (status, failures, metrics).
        let result = self
            .finalize_operation(ctx, &batch_result, all_failures)
            .await?;

        // Mark operation as completed in coordinator (if available)
        if !processor_failed
            && result.failed_modules.is_empty()
            && let Some(ref coordinator) = self.operation_coordinator
        {
            coordinator.complete_operation().await.map_err(|e| {
                HotUpdateError::hot_update(format!(
                    "Failed to complete operation in coordinator: {}",
                    e
                ))
            })?;
        }

        Ok(result)
    }
}

impl HotUpdateOperationRuntime {
    /// Perform a hot update (scan-based fallback when no watch events pending).
    pub async fn update(&self) -> Result<BatchChangeResult> {
        let start = std::time::Instant::now();
        let mut result = BatchChangeResult::new();

        // Scan the cache to detect added/modified/removed files.
        let cache_result = self.scan_and_detect_changes().await?;

        if !cache_result.has_changes() {
            tracing::trace!("No file changes detected");

            // Record metrics even when no changes
            if let Some(metrics) = &self.metrics {
                let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
                metrics.record_update(latency_ms, 0, 0, 0, 0);
            }

            return Ok(result);
        }

        tracing::trace!(
            added = cache_result.added.len(),
            modified = cache_result.modified.len(),
            removed = cache_result.removed.len(),
            "Detected file changes"
        );

        // Record removed files as deletions.
        for path in &cache_result.removed {
            result.add_file_change(FileChange::new(
                path.clone(),
                FileChangeType::Deleted,
                String::new(),
                0,
                chrono::Utc::now(),
            ));
        }

        // Parse the added and modified files. Sequential processing is used
        // because process_file_change requires &mut self and cannot be shared
        // across concurrent tasks.
        let changed_paths: Vec<_> = cache_result
            .added
            .iter()
            .chain(cache_result.modified.iter())
            .cloned()
            .collect();

        // Process files sequentially. One shared FileProcessor keeps the raw
        // entity-id counter monotonically increasing across the batch, so
        // groups produced for different files never collide (a per-file
        // processor would re-seed the counter at the same value and generate
        // duplicate group/chunk ids).
        let mut file_processor = self.new_file_processor();
        for path in changed_paths {
            // Skip non-text files (e.g., binary, images) silently
            if !cce_utils::file::is_text_file(&path) {
                continue;
            }

            let change_type = if cache_result.added.contains(&path) {
                FileChangeType::Added
            } else {
                FileChangeType::Modified
            };

            let parse_path = path.to_string_lossy().into_owned();
            let read_path = self.resolve_scan_path(&path);
            match file_processor
                .process_file_change_at(
                    &read_path,
                    &parse_path,
                    change_type,
                    &self.metadata_store,
                    self.project_id,
                )
                .await
            {
                Ok(parse_result) => {
                    result.add_parse_result(parse_result);
                }
                Err(e) => {
                    tracing::error!(path = %path.display(), error = %e, "Failed to process file");
                    result.add_failed(path, e.to_string());
                }
            }
        }

        let processed_count = result.processed_count();
        let failed_count = result.failed_count();
        let files_changed =
            cache_result.added.len() + cache_result.modified.len() + cache_result.removed.len();

        tracing::trace!(
            processed = processed_count,
            failed = failed_count,
            "Hot update completed"
        );

        // Record metrics if enabled
        if let Some(metrics) = &self.metrics {
            let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
            metrics.record_update(latency_ms, files_changed, processed_count, failed_count, 0);
        }

        Ok(result)
    }

    /// Process accumulated watch event paths into a BatchChangeResult.
    pub(crate) async fn process_watch_paths(
        &self,
        pending: &[(PathBuf, bool)],
    ) -> Result<BatchChangeResult> {
        let mut result = BatchChangeResult::new();
        let mut file_processor = self.new_file_processor();

        for (path, is_deletion) in pending {
            if *is_deletion {
                // Deletions are recorded with the project-relative path, the
                // same keying used by parse results and the `files` table.
                // Storage removal (`prepare_hot_update_file`,
                // `commit_file_hashes`) matches rows on this exact string; an
                // absolute path would silently miss and leave the deleted
                // file behind in the next generation.
                let delete_path = self.relativize_scan_path(path);
                result.add_file_change(FileChange::new(
                    delete_path,
                    FileChangeType::Deleted,
                    String::new(),
                    0,
                    chrono::Utc::now(),
                ));
            } else {
                if !cce_utils::file::is_text_file(path) {
                    continue;
                }
                let parse_path = self.relativize_scan_path(path);
                match file_processor
                    .process_file_change_at(
                        path,
                        &parse_path.to_string_lossy(),
                        FileChangeType::Modified,
                        &self.metadata_store,
                        self.project_id,
                    )
                    .await
                {
                    Ok(parse_result) => result.add_parse_result(parse_result),
                    Err(e) => {
                        tracing::error!(path = %path.display(), error = %e, "Failed to process watch event file");
                        result.add_failed(path.clone(), e.to_string());
                    }
                }
            }
        }

        Ok(result)
    }
}

mod config_change;
mod persistence;
mod resume;
