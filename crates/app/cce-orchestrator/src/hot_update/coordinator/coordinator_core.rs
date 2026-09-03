//! Hot update coordinator implementation
//!
//! This module contains the main coordinator for managing hot updates.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use tokio::sync::Mutex;

use crate::hot_update::watcher::{FileEvent, FileEventType, WatchConfig, WatchCoordinator};
use cce_config::HotUpdateConfig;
use cce_metrics::{HotUpdateMetrics, WatchMetrics};
use cce_parser::parser::ParseCoordinator;
use cce_scanner::ScanOptions;
use cce_storage_sqlite::SqliteClient;
use cce_storage_sqlite::project_registry::ProjectRegistry;

use crate::hot_update::change::BatchChangeResult;
use crate::hot_update::change_detector::ChangeDetector;
use crate::hot_update::config::ConfigReloadManager;
use crate::hot_update::debounce::{DebounceConfig, GlobalDebounce};
use crate::hot_update::error::{HotUpdateError, Result};
use crate::hot_update::event_loop::EventLoopManager;
use crate::hot_update::mode_switch::ModeStateMachine;
use crate::hot_update::operation_runtime::HotUpdateOperationRuntime;
use crate::hot_update::periodic_scan::PeriodicScanTask;
use crate::hot_update::processors::UpdateProcessor;
use crate::hot_update::state::HotUpdateMode;
use crate::index::StorageCoordinator;
use crate::operation::{CheckpointManager, OperationCoordinator, OperationType};

/// Capacity of the watcher -> coordinator file event channel.
///
/// Bounded so a slow event loop cannot make the watcher buffer unbounded
/// memory. When full, the watcher drops the event and sets the overflow flag;
/// the event loop then flags the pending change queue for a full rescan, so
/// dropped events are recovered from the filesystem by the next operation.
pub(crate) const FILE_EVENT_CHANNEL_CAPACITY: usize = 4096;

pub struct HotUpdateCoordinator {
    /// Global debounce timer
    pub(crate) debounce: GlobalDebounce,
    /// Parser for file processing
    pub(crate) parser: ParseCoordinator,
    /// Configuration
    pub(crate) config: HotUpdateConfig,

    // ===== File Watch Support =====
    /// File watch coordinator (optional, for real-time mode)
    pub(crate) watch_coordinator: Option<WatchCoordinator>,
    /// Current mode
    pub(crate) mode: HotUpdateMode,
    /// File event receiver (for file watch mode) - bounded to bound memory;
    /// overflow drops events and schedules a full rescan.
    pub(crate) file_event_rx: Option<tokio::sync::mpsc::Receiver<FileEvent>>,

    /// Set when the watcher -> event-loop channel overflows.
    ///
    /// The event loop checks this flag and propagates it to the pending change
    /// queue so the next operation falls back to a full filesystem scan.
    pub(crate) watch_event_overflow: Arc<AtomicBool>,

    // ===== Mode Switch Support =====
    /// Mode switch state machine
    pub(crate) mode_state_machine: Option<Arc<Mutex<ModeStateMachine>>>,
    /// Periodic scan task (when in PeriodicScan mode)
    pub(crate) periodic_scan_task: Option<PeriodicScanTask>,

    // ===== Event Loop Management =====
    /// Event loop manager
    pub(crate) event_loop_manager: EventLoopManager,

    // ===== Statistics =====
    /// Total events processed (for statistics)
    pub(crate) total_events: Arc<AtomicUsize>,

    // ===== Config Reload Support =====
    /// Config reload manager
    pub(crate) config_reload: ConfigReloadManager,

    // ===== Project Context =====
    /// Project registry reference (for loading project config)
    pub(crate) project_registry: Option<Arc<ProjectRegistry>>,

    // ===== Monitoring =====
    /// Watcher metrics (optional)
    pub(crate) watch_metrics: Option<Arc<WatchMetrics>>,

    // ===== Operation Runtime =====
    /// Operation-critical mutable state (change detection, checkpoints,
    /// pending changes, processors, storage references), guarded by its own
    /// mutex so long-running operations do not hold the coordinator lock.
    pub(crate) operation: Arc<Mutex<HotUpdateOperationRuntime>>,

    /// Notify signal for background processing task.
    /// Fired when pending_watch_changes is non-empty.
    pub(crate) processing_notify: Arc<tokio::sync::Notify>,

    /// Whether the background processor task has been spawned.
    ///
    /// Guards against double-start: only the first
    /// `start_background_processor_from_arc` call spawns the worker.
    pub(crate) background_processor_started: Arc<AtomicBool>,
}

use super::temp_db::create_temp_db;

impl HotUpdateCoordinator {
    /// Create a new hot update coordinator with required project_id
    pub fn new(config: HotUpdateConfig, project_id: i64) -> Result<Self> {
        if project_id <= 0 {
            return Err(HotUpdateError::config(format!(
                "invalid project_id: {} (must be positive)",
                project_id
            )));
        }
        let scan_options = ScanOptions::from(config.scanner.clone().unwrap_or_default());
        // root_path will be set when initialize_cache is called with actual project root
        let debounce_config = DebounceConfig::new(
            config.debounce.pending_interval_secs,
            config.debounce.max_wait_time_secs,
        );

        // Create a temporary database connection for ChangeDetector
        // This will be replaced when set_metadata_store is called
        let db = create_temp_db()?;

        // The config reload manager owns the pending config-change queue and
        // the operation lock; both are wired into the coordinator so config
        // changes flow through the operation pipeline with mutual exclusion.
        let mut config_reload = ConfigReloadManager::new_default();
        config_reload.set_operation_lock(Arc::new(Mutex::new(())));
        let mut runtime = HotUpdateOperationRuntime::new(project_id, db, scan_options);
        runtime.set_config_change_pending(config_reload.pending_config_changes());

        Ok(Self {
            operation: Arc::new(Mutex::new(runtime)),
            debounce: GlobalDebounce::with_config(debounce_config),
            parser: ParseCoordinator::new(),
            config,
            watch_coordinator: None,
            mode: HotUpdateMode::PeriodicScan,
            file_event_rx: None,
            watch_event_overflow: Arc::new(AtomicBool::new(false)),
            mode_state_machine: None,
            periodic_scan_task: None,
            event_loop_manager: EventLoopManager::new(),
            total_events: Arc::new(AtomicUsize::new(0)),
            config_reload,
            project_registry: None,
            watch_metrics: None,
            processing_notify: Arc::new(tokio::sync::Notify::new()),
            background_processor_started: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Create with file watch support
    ///
    /// This enables real-time file monitoring instead of periodic scanning.
    pub fn with_file_watch(config: HotUpdateConfig, project_id: i64) -> Result<Self> {
        if project_id <= 0 {
            return Err(HotUpdateError::config(format!(
                "invalid project_id: {} (must be positive)",
                project_id
            )));
        }
        let scan_options = ScanOptions::from(config.scanner.clone().unwrap_or_default());
        // root_path will be set when start_watch is called
        let debounce_config = DebounceConfig::new(
            config.debounce.pending_interval_secs,
            config.debounce.max_wait_time_secs,
        );

        // Bounded channel so a slow event loop cannot grow memory
        // without limit. On overflow the watcher drops events and sets the
        // shared flag; the event loop propagates it to the pending change
        // queue for a full rescan fallback.
        let watch_event_overflow = Arc::new(AtomicBool::new(false));
        let (file_event_tx, file_event_rx) =
            tokio::sync::mpsc::channel(FILE_EVENT_CHANNEL_CAPACITY);

        // Create a temporary database connection for ChangeDetector
        let db = create_temp_db()?;

        // Create watch config with unified parameters
        let watch_config = WatchConfig::with_params(
            config.file_watch.event_threshold,
            config.file_watch.fallback_interval_secs,
            config.file_watch.verification_interval_secs,
            vec![], // Will use default extensions
            config
                .scanner
                .as_ref()
                .map(|s| s.exclude_patterns.clone())
                .unwrap_or_default(),
            config.file_watch.watch_config_files,
            1000,
            config.file_watch.storm_duration_secs,
            config.file_watch.recovery_threshold,
            config.file_watch.recovery_duration_secs,
        )
        .map_err(|e| HotUpdateError::hot_update(format!("Invalid watch config: {}", e)))?;

        // Create watch coordinator
        let watch_coordinator = WatchCoordinator::new(
            watch_config.clone(),
            file_event_tx,
            watch_event_overflow.clone(),
        )
        .map_err(|e| {
            HotUpdateError::hot_update(format!("Failed to create watch coordinator: {}", e))
        })?;

        // Create mode switch state machine using unified config
        let mode_switch_config = watch_config.to_mode_switch_config();
        let mode_state_machine = Arc::new(Mutex::new(ModeStateMachine::new(mode_switch_config)));

        let mut change_detector = ChangeDetector::new(db.clone(), scan_options);
        change_detector.set_project_id(project_id);

        // The config reload manager owns the pending config-change queue and
        // the operation lock; both are wired into the coordinator so config
        // changes flow through the operation pipeline with mutual exclusion.
        let mut config_reload = ConfigReloadManager::new_default();
        config_reload.set_operation_lock(Arc::new(Mutex::new(())));
        let mut runtime =
            HotUpdateOperationRuntime::new(project_id, db, change_detector.scan_options().clone());
        runtime.set_config_change_pending(config_reload.pending_config_changes());

        Ok(Self {
            operation: Arc::new(Mutex::new(runtime)),
            debounce: GlobalDebounce::with_config(debounce_config),
            parser: ParseCoordinator::new(),
            config: config.clone(),
            watch_coordinator: Some(watch_coordinator),
            mode: HotUpdateMode::FileWatch,
            file_event_rx: Some(file_event_rx),
            watch_event_overflow,
            mode_state_machine: Some(mode_state_machine),
            periodic_scan_task: None,
            event_loop_manager: EventLoopManager::new(),
            total_events: Arc::new(AtomicUsize::new(0)),
            config_reload,
            project_registry: None,
            watch_metrics: None,
            processing_notify: Arc::new(tokio::sync::Notify::new()),
            background_processor_started: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Set metadata store
    pub fn with_metadata_store(mut self, store: Arc<SqliteClient>) -> Self {
        self.runtime_builder().set_metadata_store(store);
        self
    }

    /// Builder-phase access to the operation runtime.
    ///
    /// The `with_*` builders run while the coordinator is owned (not yet
    /// shared behind an Arc), so the runtime Arc is uniquely referenced and
    /// the mutable borrow cannot fail. The `expect` below is unreachable:
    /// dropping it would turn a construction-order bug into silent
    /// state loss, while error propagation would force every builder method
    /// to handle an impossible failure.
    fn runtime_builder(&mut self) -> &mut HotUpdateOperationRuntime {
        Arc::get_mut(&mut self.operation)
            .map(|mutex| mutex.get_mut())
            .expect("runtime Arc is exclusively owned during coordinator construction")
    }

    /// Set parser
    pub fn with_parser(mut self, parser: ParseCoordinator) -> Self {
        self.parser = parser;
        self
    }

    /// Set configuration
    pub fn with_config(mut self, config: HotUpdateConfig) -> Self {
        let scan_options = ScanOptions::from(config.scanner.clone().unwrap_or_default());
        // root_path will be set by caller
        let debounce_config = DebounceConfig::new(
            config.debounce.pending_interval_secs,
            config.debounce.max_wait_time_secs,
        );
        self.debounce = GlobalDebounce::with_config(debounce_config);

        // Preserve existing database connection when updating config
        let db = self
            .operation
            .try_lock()
            .ok()
            .and_then(|runtime| runtime.metadata_store().cloned())
            .unwrap_or_else(|| {
                // Fallback to a temporary database when the metadata store has
                // not been set yet. `create_temp_db` already falls back from
                // in-memory to a temp file, so a double failure is unreachable
                // in practice; fail loudly instead of panicking blindly.
                create_temp_db().unwrap_or_else(|error| {
                    tracing::error!(error = %error, "Failed to create temp database");
                    // Unreachable: the change detector cannot function without
                    // a database, and both in-memory and temp-file creation
                    // failed, which requires a catastrophic filesystem failure.
                    unreachable!("cannot create any database for the change detector")
                })
            });

        let runtime = self.runtime_builder();
        runtime.set_metadata_store(db);
        runtime.change_detector_mut().set_scan_options(scan_options);
        self.config = config;
        self
    }

    /// Set project registry reference
    pub fn with_project_registry(mut self, registry: Arc<ProjectRegistry>) -> Self {
        self.project_registry = Some(registry);
        self
    }

    /// Reload project configuration from ProjectRegistry
    ///
    /// This method fetches the latest project configuration and applies
    /// hot update related settings to this coordinator.
    ///
    /// # Returns
    ///
    /// `Ok(())` if reload succeeded, or error if project not found
    pub async fn reload_project_config(&mut self) -> Result<()> {
        if let Some(registry) = &self.project_registry {
            let project_id = self.project_id().await;
            let project_entry = registry.get_or_load(project_id).await.map_err(|e| {
                HotUpdateError::hot_update(format!("Failed to load project config: {}", e))
            })?;

            // Apply hot update configuration
            self.apply_hot_update_config(&project_entry.config.orchestrator.hot_update)
                .await;

            tracing::info!(
                project_id = project_id,
                version = project_entry.version,
                "HotUpdateCoordinator reloaded project config"
            );
        }
        Ok(())
    }

    /// Apply new hot update configuration to internal components
    async fn apply_hot_update_config(
        &mut self,
        new_hot_update_config: &cce_config::modules::HotUpdateConfig,
    ) {
        // Update debounce configuration
        let debounce_config = DebounceConfig::new(
            new_hot_update_config.debounce.pending_interval_secs,
            new_hot_update_config.debounce.max_wait_time_secs,
        );
        // Note: set_config is synchronous internally despite returning a Future
        std::mem::drop(self.debounce.set_config(debounce_config));

        // Update scanner configuration (affects file filtering)
        let scan_options =
            ScanOptions::from(new_hot_update_config.scanner.clone().unwrap_or_default());
        // The coordinator may be shared by then; acquire the runtime mutex.
        let mut runtime = self.operation.lock().await;
        runtime.change_detector_mut().set_scan_options(scan_options);

        tracing::debug!("Applied new hot update configuration");
    }

    /// Get current mode
    pub fn mode(&self) -> HotUpdateMode {
        self.mode
    }

    /// Get the configured watch root (None in periodic-scan mode or before
    /// `start_watch`).
    pub async fn watch_root(&self) -> Option<PathBuf> {
        self.operation
            .lock()
            .await
            .watch_root()
            .map(Path::to_path_buf)
    }

    /// Set watch root directly (used by startup durable replay when watch has not been started).
    pub async fn set_watch_root(&self, root: PathBuf) {
        self.operation.lock().await.set_watch_root(root);
    }

    /// Get total events processed
    pub fn total_events(&self) -> usize {
        self.total_events.load(Ordering::Relaxed)
    }

    /// Get the project ID
    pub async fn project_id(&self) -> i64 {
        self.operation.lock().await.project_id()
    }

    /// Set monitoring metrics
    pub fn with_metrics(mut self, metrics: Arc<HotUpdateMetrics>) -> Self {
        self.runtime_builder().set_metrics(metrics);
        self
    }

    /// Set watcher monitoring metrics
    pub fn with_watch_metrics(mut self, metrics: Arc<WatchMetrics>) -> Self {
        if let Some(ref mut watcher) = self.watch_coordinator {
            watcher.set_metrics(metrics.clone());
        }
        self.watch_metrics = Some(metrics);
        self
    }

    /// Set operation coordinator for hot-update queue management
    pub fn with_operation_coordinator(mut self, coordinator: Arc<OperationCoordinator>) -> Self {
        self.runtime_builder()
            .set_operation_coordinator(coordinator);
        self
    }

    /// Set stored processors for self-execution
    ///
    /// These processors are used by the background processor task
    /// and by run_with_stored_processors().
    pub fn with_processors(mut self, processors: Vec<Arc<dyn UpdateProcessor>>) -> Self {
        self.runtime_builder().set_stored_processors(processors);
        self
    }

    /// Set checkpoint manager for hot-update checkpoint persistence
    pub fn with_checkpoint_manager(mut self, manager: Arc<CheckpointManager>) -> Self {
        self.runtime_builder().set_checkpoint_manager(manager);
        self
    }

    /// Set the storage coordinator shared with the update processors.
    ///
    /// On resume the coordinator's candidate adoptability query decides
    /// whether persisted module progress markers stay valid.
    pub fn with_storage_coordinator(mut self, coordinator: Arc<StorageCoordinator>) -> Self {
        self.runtime_builder().set_storage_coordinator(coordinator);
        self
    }

    /// Attach a test-only parse counter shared with every file processor this
    /// coordinator creates. Production wiring never sets it.
    pub fn with_parse_probe(mut self, probe: Arc<AtomicUsize>) -> Self {
        self.runtime_builder().set_parse_probe(probe);
        self
    }

    pub fn with_heartbeat_interval(mut self, interval: std::time::Duration) -> Self {
        self.runtime_builder().set_heartbeat_interval(interval);
        self
    }

    /// Get the checkpoint manager, if configured.
    pub async fn checkpoint_manager(&self) -> Option<Arc<CheckpointManager>> {
        self.operation.lock().await.checkpoint_manager()
    }

    /// Get stored processors
    pub async fn stored_processors(&self) -> Vec<Arc<dyn UpdateProcessor>> {
        self.operation.lock().await.stored_processors().to_vec()
    }

    /// Check if there are pending file changes from watch events.
    ///
    /// Returns true if `pending_watch_changes` is non-empty, meaning
    /// there are accumulated events ready to be processed by a hot-update.
    pub async fn has_pending_changes(&self) -> bool {
        self.operation.lock().await.has_pending_changes().await
    }

    pub async fn pending_changes_len(&self) -> usize {
        self.operation.lock().await.pending_changes_len().await
    }

    /// Get a reference to the metrics (if enabled)
    pub async fn metrics(&self) -> Option<Arc<HotUpdateMetrics>> {
        self.operation.lock().await.metrics().cloned()
    }

    /// Check if storm is detected (delegated to ModeStateMachine)
    pub async fn is_storm(&self) -> bool {
        if let Some(ref state_machine) = self.mode_state_machine {
            let sm = state_machine.lock().await;
            sm.current_event_rate() > sm.config.storm_threshold
        } else {
            false
        }
    }

    /// Start file watching
    ///
    /// Only applicable in file watch mode.
    pub async fn start_watch(&mut self, root: &Path) -> Result<()> {
        if self.mode != HotUpdateMode::FileWatch {
            return Err(HotUpdateError::hot_update(
                "Cannot start watch in periodic scan mode",
            ));
        }

        // Save watch root
        self.operation
            .lock()
            .await
            .set_watch_root(root.to_path_buf());
        self.total_events.store(0, Ordering::Relaxed);

        // Start watch coordinator
        if let Some(ref mut watcher) = self.watch_coordinator {
            watcher.start(root).await.map_err(|e| {
                HotUpdateError::hot_update(format!("Failed to start watcher: {}", e))
            })?;
        }

        tracing::info!("File watch started");
        Ok(())
    }

    /// Stop file watching
    pub async fn stop_watch(&mut self) -> Result<()> {
        // Stop watcher
        if let Some(ref mut watcher) = self.watch_coordinator {
            watcher.stop().await.map_err(|e| {
                HotUpdateError::hot_update(format!("Failed to stop watcher: {}", e))
            })?;
        }

        tracing::info!("File watch stopped");
        Ok(())
    }

    /// Receive file event (for file watch mode)
    ///
    /// Returns None if not in file watch mode or no event available.
    pub async fn recv_file_event(&mut self) -> Option<FileEvent> {
        if let Some(ref mut rx) = self.file_event_rx {
            rx.recv().await
        } else {
            None
        }
    }

    /// Start event processing loop in background
    ///
    /// This spawns a background task that continuously receives file events
    /// and processes them through the hot update pipeline.
    ///
    /// Returns a handle to the event loop task.
    pub async fn start_event_loop(&mut self) -> Result<tokio::task::JoinHandle<()>> {
        if self.mode != HotUpdateMode::FileWatch {
            return Err(HotUpdateError::hot_update(
                "Cannot start event loop in periodic scan mode",
            ));
        }

        let mut rx = self
            .file_event_rx
            .take()
            .ok_or_else(|| HotUpdateError::hot_update("No file event receiver available"))?;

        // Clone necessary components for the async task
        let debounce = self.debounce.clone();
        let mode_state_machine = self.mode_state_machine.clone();
        let total_events = self.total_events.clone();
        let pending_changes = self.operation.lock().await.pending_watch_changes();
        let processing_notify = self.processing_notify.clone();
        let config_reload = self.config_reload.clone();
        let watch_event_overflow = self.watch_event_overflow.clone();
        let watch_metrics = self.watch_metrics.clone();

        // Spawn the event loop task
        let handle = tokio::spawn(async move {
            tracing::info!("Event processing loop started");

            // Accumulated paths for batching
            let mut accumulated_events: Vec<(PathBuf, bool)> = Vec::new();

            // Debounce re-check timer: guarantees a lone file event is
            // eventually forwarded even if no further events arrive, which is
            // the debounce's documented "time-based trigger for idle periods".
            let mut debounce_timer = tokio::time::interval(Duration::from_secs(1));

            // Mode check timer (every 5 seconds)
            let mut mode_check_timer = tokio::time::interval(Duration::from_secs(5));

            loop {
                // Propagate upstream channel overflow to the pending
                // change queue so the next operation does a full rescan
                // (dropped events are recovered from the filesystem).
                if watch_event_overflow.swap(false, Ordering::Relaxed) {
                    pending_changes.mark_full_rescan();
                    if let Some(ref metrics) = watch_metrics {
                        metrics.record_overflow();
                    }
                }

                tokio::select! {
                    // Handle file events
                    event_opt = rx.recv() => {
                        match event_opt {
                            Some(event) => {
                                total_events.fetch_add(1, Ordering::Relaxed);

                                // Record event for storm detection
                                if let Some(ref state_machine) = mode_state_machine {
                                    let mut sm = state_machine.lock().await;
                                    sm.record_event_and_check_storm();
                                }

                                // Route configuration events to the reload
                                // manager instead of the change pipeline.
                                if event.event_type.is_config_modify() {
                                    let content =
                                        match cce_utils::file::read_file_to_utf8_async(
                                            &event.path,
                                        )
                                        .await
                                        {
                                            Ok(content) => content,
                                            Err(e) => {
                                                tracing::warn!(
                                                    error = %e,
                                                    path = %event.path.display(),
                                                    "Failed to read config file; queuing empty content"
                                                );
                                                String::new()
                                            }
                                        };
                                    config_reload
                                        .handle_config_change(&event.path, &content)
                                        .await;
                                    continue;
                                }

                                // Accumulate the event path
                                let is_deletion = matches!(
                                    event.event_type,
                                    FileEventType::Deleted
                                );
                                accumulated_events.push((event.path, is_deletion));

                                // Mark that we have changes
                                debounce.mark_changes().await;

                                // Check if we should process now (debounce)
                                if !debounce.should_process().await {
                                    continue;
                                }

                                // Forward accumulated events to coordinator
                                // instead of calling process_event_internal (which only
                                // parses but never runs the processor chain).
                                if !accumulated_events.is_empty() {
                                    let _batch_count = accumulated_events.len();
                                    let was_full_rescan = pending_changes.needs_full_rescan();
                                    pending_changes
                                        .extend(std::mem::take(&mut accumulated_events))
                                        .await;
                                    if !was_full_rescan && pending_changes.needs_full_rescan() {
                                        if let Some(ref metrics) = watch_metrics {
                                            metrics.record_overflow();
                                        }
                                    }
                                    processing_notify.notify_one();
                                }
                            }
                            None => {
                                // Channel closed, flush remaining events
                                if !accumulated_events.is_empty() {
                                    let was_full_rescan = pending_changes.needs_full_rescan();
                                    pending_changes
                                        .extend(std::mem::take(&mut accumulated_events))
                                        .await;
                                    if !was_full_rescan && pending_changes.needs_full_rescan() {
                                        if let Some(ref metrics) = watch_metrics {
                                            metrics.record_overflow();
                                        }
                                    }
                                    processing_notify.notify_one();
                                }
                                tracing::info!("Event channel closed, stopping event loop");
                                break;
                            }
                        }
                    }

                    // Time-based debounce: re-check so a single event is
                    // forwarded to the background processor within one tick.
                    _ = debounce_timer.tick() => {
                        if !accumulated_events.is_empty() && debounce.should_process().await {
                            let _batch_count = accumulated_events.len();
                            let was_full_rescan = pending_changes.needs_full_rescan();
                            pending_changes
                                .extend(std::mem::take(&mut accumulated_events))
                                .await;
                            if !was_full_rescan && pending_changes.needs_full_rescan() {
                                if let Some(ref metrics) = watch_metrics {
                                    metrics.record_overflow();
                                }
                            }
                            processing_notify.notify_one();
                        }
                    }

                    // Periodic mode check
                    _ = mode_check_timer.tick() => {
                        // Note: Mode switching is handled by the background
                        // processor's check_and_update_mode() call.
                    }
                }
            }

            tracing::info!("Event processing loop stopped");
        });

        Ok(handle)
    }

    /// Start the background processor that drains accumulated watch events and
    /// periodic-scan signals and runs the stored processors.
    ///
    /// The task is idempotent: only the first invocation spawns the worker;
    /// later calls return a finished handle. The worker:
    ///
    /// 1. Wakes on `processing_notify` (file-watch events accumulated by the
    ///    event loop) or on a periodic interval (degraded periodic-scan mode).
    /// 2. Drives mode switching (`check_and_update_mode`), so an event storm
    ///    degrades FileWatch to PeriodicScan and recovery switches back.
    /// 3. Runs the stored processors when changes are pending.
    ///
    /// The coordinator lock is scoped to the mode check and the run decision.
    /// The long-running operation executes on the detached
    /// `HotUpdateOperationRuntime`, so watch events, storm detection and
    /// status/stop-watch APIs stay responsive during the operation.
    ///
    /// The caller must not hold the coordinator lock when calling this (it
    /// takes the notify handle and the single-worker slot under an await).
    pub async fn start_background_processor_from_arc(
        coordinator: Arc<Mutex<Self>>,
    ) -> tokio::task::JoinHandle<()> {
        // Take the notify handle and claim the single-worker slot under the
        // same lock. A second call must not spawn a competing consumer.
        let (notify, check_interval_secs) = {
            let coord = coordinator.lock().await;
            if coord
                .background_processor_started
                .swap(true, Ordering::SeqCst)
            {
                return tokio::spawn(async {});
            }
            (
                coord.processing_notify.clone(),
                coord.config.file_watch.fallback_interval_secs.max(1),
            )
        };

        tokio::spawn(async move {
            tracing::info!("Background processor started");
            let mut interval = tokio::time::interval(Duration::from_secs(check_interval_secs));
            // Skip the immediate first tick so the loop starts in a settled state.
            interval.tick().await;

            loop {
                tokio::select! {
                    _ = notify.notified() => {}
                    _ = interval.tick() => {}
                }

                // Scoped coordinator lock for the decision only.
                let should_run = {
                    let mut coord = coordinator.lock().await;
                    let runtime = coord.operation.lock().await;
                    if runtime.stored_processors().is_empty() {
                        continue;
                    }
                    drop(runtime);

                    // Drive storm degrade / recovery so watch and periodic modes
                    // stay consistent regardless of which path accumulated events.
                    if let Err(error) = coord.check_and_update_mode().await {
                        tracing::warn!(error = %error, "Background processor failed to switch mode");
                    }

                    match coord.mode {
                        HotUpdateMode::FileWatch => {
                            let runtime = coord.operation.lock().await;
                            let changes = runtime.pending_watch_changes();
                            !changes.is_empty().await
                        }
                        HotUpdateMode::PeriodicScan => coord.check_should_update(false).await,
                    }
                };
                if !should_run {
                    continue;
                }

                // Phase 2: run the operation without the coordinator lock.
                let operation = coordinator.lock().await.operation.clone();
                let runtime = operation.lock().await;
                let mut ctx = match runtime.begin_operation(OperationType::HotUpdate).await {
                    Ok(ctx) => ctx,
                    Err(e) => {
                        tracing::error!(error = %e, "Failed to begin operation in background processor");
                        continue;
                    }
                };
                tracing::trace!("Background processor running operation");
                if let Err(e) = runtime.run_with_stored_processors(&mut ctx).await {
                    tracing::error!(error = %e, "Background processor operation failed");
                }
            }
        })
    }

    // Deprecated: Watch events now go through pending_watch_changes and
    // run_operation() for full processor chain execution.
    // process_event_internal has been removed.

    /// Handle file event from watcher
    ///
    /// This is the main entry point for file watch mode.
    /// Verify that a file path belongs to the current project
    ///
    /// Performs security checks to ensure the file is within the watched root directory
    /// and not attempting to access files outside the project boundary.
    ///
    /// # Security Implications
    ///
    /// - Returns false for any path that doesn't start with watch_root
    /// - Returns false for symbolic links pointing outside the project (if configured)
    /// - Returns true only if path is within project boundaries
    pub(crate) async fn verify_file_ownership(&self, file_path: &Path) -> Result<bool> {
        // Check 1: Verify file is within watch root
        if let Some(root_dir) = self.watch_root().await {
            let canonical_file = match file_path.canonicalize() {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!(
                        path = %file_path.display(),
                        error = %e,
                        "Failed to canonicalize file path for ownership check"
                    );
                    return Ok(false);
                }
            };

            let canonical_root = match root_dir.canonicalize() {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!(
                        root = %root_dir.display(),
                        error = %e,
                        "Failed to canonicalize root directory"
                    );
                    return Ok(false);
                }
            };

            if !canonical_file.starts_with(&canonical_root) {
                tracing::warn!(
                    project_id = self.project_id().await,
                    file_path = %canonical_file.display(),
                    root = %canonical_root.display(),
                    "File ownership check failed: path is outside project root"
                );
                return Ok(false);
            }
        } else {
            // No watch root set yet
            return Ok(false);
        }

        Ok(true)
    }

    pub async fn handle_file_event(&mut self, event: FileEvent) -> Result<BatchChangeResult> {
        // Security: Verify file belongs to this project before processing
        if !self.verify_file_ownership(&event.path).await? {
            return Err(HotUpdateError::permission_denied(format!(
                "File {} does not belong to project {}. Access denied.",
                event.path.display(),
                self.project_id().await
            )));
        }

        // Record the event for storm detection so `check_and_update_mode` can
        // degrade to periodic-scan mode under an event storm. Mirrors the
        // event-loop path (`start_event_loop`).
        if let Some(ref state_machine) = self.mode_state_machine {
            let mut sm = state_machine.lock().await;
            sm.record_event_and_check_storm();
        }

        // Check if this is a config modification event
        if event.event_type.is_config_modify() {
            let content = match cce_utils::file::read_file_to_utf8_async(&event.path).await {
                Ok(content) => content,
                Err(e) => {
                    tracing::warn!(
                        "Failed to read config file {}: {}, using legacy method",
                        event.path.display(),
                        e
                    );
                    String::new()
                }
            };
            self.config_reload
                .handle_config_change(&event.path, &content)
                .await;
            return Ok(BatchChangeResult::new());
        }

        // Push event to pending_watch_changes for unified processing
        // instead of direct parsing. The event will be picked up by the next
        // run_operation() call and processed through the full pipeline.
        self.debounce.mark_changes().await;
        let is_deletion = matches!(event.event_type, FileEventType::Deleted);
        {
            let pending = self.operation.lock().await.pending_watch_changes();
            pending.push(event.path.clone(), is_deletion).await;
        }
        self.processing_notify.notify_one();

        // Return empty result - actual processing happens in run_operation()
        Ok(BatchChangeResult::new())
    }
}
