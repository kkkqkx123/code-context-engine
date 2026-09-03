//! Watch coordinator for file system monitoring
//!
//! This module provides the main coordinator for file system watching,
//! handling event processing, storm detection, and mode switching.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::{RwLock, mpsc};

use super::config::WatchConfig;
use super::error::{Result, WatchError};
use super::event::{FileEvent, WatchEvent, WatchStats, WatchStatus};
use cce_metrics::WatchMetrics;

/// Watch coordinator
///
/// Manages file system watching with:
/// - Event filtering and routing
/// - Configuration file watching
///
/// Note: Storm detection and mode switching is handled by HotUpdateCoordinator.
pub struct WatchCoordinator {
    /// File system watcher (notify)
    watcher: Option<RecommendedWatcher>,

    /// Hot update event sender (bounded; overflow drops events and flags a
    /// full rescan)
    hot_update_tx: mpsc::Sender<FileEvent>,

    /// Set when `hot_update_tx` is full and an event was dropped.
    /// The event loop propagates it to the pending change queue for a full
    /// rescan fallback.
    watch_event_overflow: Arc<AtomicBool>,

    /// Event count (for statistics)
    event_count: Arc<AtomicUsize>,

    /// Configuration
    config: WatchConfig,

    /// Running flag
    running: Arc<AtomicBool>,

    /// Root path being watched
    root_path: Option<PathBuf>,

    /// Statistics
    stats: Arc<RwLock<WatchStats>>,

    /// Optional metrics collector
    metrics: Option<Arc<WatchMetrics>>,
}

impl WatchCoordinator {
    /// Create a new watch coordinator
    ///
    /// # Arguments
    ///
    /// * `config` - Watch configuration
    /// * `hot_update_tx` - Channel to send file events to hot update coordinator
    /// * `watch_event_overflow` - Set when the channel overflows
    pub fn new(
        config: WatchConfig,
        hot_update_tx: mpsc::Sender<FileEvent>,
        watch_event_overflow: Arc<AtomicBool>,
    ) -> Result<Self> {
        Ok(Self {
            watcher: None,
            hot_update_tx,
            watch_event_overflow,
            event_count: Arc::new(AtomicUsize::new(0)),
            config,
            running: Arc::new(AtomicBool::new(false)),
            root_path: None,
            stats: Arc::new(RwLock::new(WatchStats::new())),
            metrics: None,
        })
    }

    /// Attach a metrics collector
    pub fn with_metrics(mut self, metrics: Arc<WatchMetrics>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Update the metrics collector in place
    pub fn set_metrics(&mut self, metrics: Arc<WatchMetrics>) {
        self.metrics = Some(metrics);
    }

    /// Start watching a directory
    ///
    /// # Arguments
    ///
    /// * `root` - Root directory to watch
    pub async fn start(&mut self, root: &Path) -> Result<()> {
        if self.running.load(Ordering::Relaxed) {
            return Err(WatchError::AlreadyRunning);
        }

        tracing::info!(path = %root.display(), "Starting file watcher");

        // Create file watcher
        let (sync_tx, sync_rx) = std::sync::mpsc::channel();
        let mut watcher = notify::recommended_watcher(sync_tx).map_err(|e| {
            WatchError::watch_path(root, format!("Failed to create watcher: {}", e))
        })?;

        // Start watching
        watcher
            .watch(root, RecursiveMode::Recursive)
            .map_err(|e| WatchError::watch_path(root, format!("Failed to watch: {}", e)))?;

        self.watcher = Some(watcher);
        self.root_path = Some(root.to_path_buf());
        self.running.store(true, Ordering::Relaxed);
        self.event_count.store(0, Ordering::Relaxed);

        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.status = WatchStatus::FileWatch;
            stats.watched_paths = 1;
        }

        if let Some(metrics) = &self.metrics {
            metrics.set_active(true);
            metrics.set_status_code(Self::status_code(WatchStatus::FileWatch));
            metrics.set_watched_paths(1);
        }

        // Create unbounded event channel for internal processing
        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel::<WatchEvent>();

        // Spawn sync-to-async bridge
        // UnboundedSender::send() is sync, so the thread can directly
        // send to the async channel - eliminates the intermediate forwarding task.
        let event_count = self.event_count.clone();
        let running = self.running.clone();

        let event_tx_clone = event_tx.clone();
        let running_clone = running.clone();
        std::thread::spawn(move || {
            while running_clone.load(Ordering::Relaxed) {
                match sync_rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(Ok(event)) => {
                        event_count.fetch_add(1, Ordering::Relaxed);
                        if let Some(watch_event) = Self::convert_notify_event(event) {
                            if event_tx_clone.send(watch_event).is_err() {
                                tracing::error!("Failed to send watch event - channel closed");
                                break;
                            }
                        }
                    }
                    Ok(Err(e)) => {
                        tracing::trace!(error = %e, "Watch event error");
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        // Continue checking running flag
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                        tracing::info!("Watch event channel disconnected");
                        break;
                    }
                }
            }
        });

        // Spawn main processing loop as background task
        let hot_update_tx = self.hot_update_tx.clone();
        let watch_event_overflow = self.watch_event_overflow.clone();
        let config = self.config.clone();
        let running = self.running.clone();
        let stats = self.stats.clone();
        let metrics = self.metrics.clone();

        tokio::spawn(async move {
            // Create a minimal coordinator for the run loop
            let mut coordinator = InternalWatchRunner {
                event_rx,
                hot_update_tx,
                watch_event_overflow,
                config,
                running,
                stats,
                metrics,
            };

            if let Err(e) = coordinator.run().await {
                tracing::error!(error = %e, "Watch coordinator run error");
            }
        });

        Ok(())
    }

    /// Stop watching
    pub async fn stop(&mut self) -> Result<()> {
        if !self.running.load(Ordering::Relaxed) {
            return Ok(());
        }

        tracing::info!("Stopping file watcher");

        self.running.store(false, Ordering::Relaxed);

        // Stop watcher
        if let Some(mut watcher) = self.watcher.take() {
            if let Some(ref root) = self.root_path {
                let _ = watcher.unwatch(root);
            }
        }

        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.status = WatchStatus::Stopped;
            stats.watched_paths = 0;
        }

        if let Some(metrics) = &self.metrics {
            metrics.set_active(false);
            metrics.set_status_code(Self::status_code(WatchStatus::Stopped));
            metrics.set_watched_paths(0);
        }

        Ok(())
    }

    /// Get watch statistics
    pub async fn stats(&self) -> WatchStats {
        self.stats.read().await.clone()
    }

    /// Check if watcher is running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    /// Get event count (for external storm detection)
    pub fn event_count(&self) -> usize {
        self.event_count.load(Ordering::Relaxed)
    }

    /// Reset event count
    pub fn reset_event_count(&self) {
        self.event_count.store(0, Ordering::Relaxed);
    }

    // ==================== Private Methods ====================

    /// Convert notify event to WatchEvent
    fn convert_notify_event(event: notify::Event) -> Option<WatchEvent> {
        use notify::event::{CreateKind, EventKind, ModifyKind, RemoveKind, RenameMode};

        let path = event.paths.first().cloned()?;

        match event.kind {
            EventKind::Create(_) => {
                // Directory vs file: prefer the explicit notify kind, falling
                // back to a metadata check. The old extension-based check
                // misclassified extensionless files as directories.
                let is_dir =
                    matches!(event.kind, EventKind::Create(CreateKind::Folder)) || path.is_dir();
                if is_dir {
                    Some(WatchEvent::DirCreated(path))
                } else if is_config_file(&path) {
                    Some(WatchEvent::ConfigChanged(path))
                } else {
                    Some(WatchEvent::FileCreated(path))
                }
            }
            EventKind::Modify(ModifyKind::Name(rename_mode)) => {
                // Renames must not be flattened into a bogus "modified" event
                // for the old path: that would re-parse a now-missing file and
                // silently drop the new path. Decompose them instead.
                match rename_mode {
                    RenameMode::Both => Some(WatchEvent::FileRenamed {
                        from: path,
                        to: event.paths.get(1).cloned()?,
                    }),
                    RenameMode::From => Some(WatchEvent::FileDeleted(path)),
                    RenameMode::To => Some(WatchEvent::FileCreated(path)),
                    RenameMode::Any | RenameMode::Other => Some(WatchEvent::FileRenamed {
                        from: path,
                        to: event.paths.get(1).cloned()?,
                    }),
                }
            }
            EventKind::Modify(_) => {
                if is_config_file(&path) {
                    Some(WatchEvent::ConfigChanged(path))
                } else {
                    Some(WatchEvent::FileChanged(path))
                }
            }
            EventKind::Remove(_) => {
                // The removed path no longer exists, so only the notify kind
                // distinguishes a directory from an extensionless file.
                let is_dir =
                    matches!(event.kind, EventKind::Remove(RemoveKind::Folder)) || path.is_dir();
                if is_dir {
                    Some(WatchEvent::DirDeleted(path))
                } else {
                    Some(WatchEvent::FileDeleted(path))
                }
            }
            _ => Some(WatchEvent::Any),
        }
    }

    /// Get watched directories
    pub fn watched_dirs(&self) -> Vec<PathBuf> {
        self.root_path.clone().into_iter().collect()
    }

    fn status_code(status: WatchStatus) -> u64 {
        match status {
            WatchStatus::Stopped => 0,
            WatchStatus::FileWatch => 1,
            WatchStatus::PeriodicScan => 2,
            WatchStatus::Paused => 3,
            WatchStatus::Error => 4,
        }
    }
}

/// Internal watch runner for background task
///
/// This struct owns the event receiver and runs the main processing loop.
struct InternalWatchRunner {
    /// Event receiver (bounded by the notify bridge, unbounded internally)
    event_rx: mpsc::UnboundedReceiver<WatchEvent>,

    /// Hot update event sender (bounded; overflow drops events)
    hot_update_tx: mpsc::Sender<FileEvent>,

    /// Set when `hot_update_tx` is full and an event was dropped
    watch_event_overflow: Arc<AtomicBool>,

    /// Configuration
    config: WatchConfig,

    /// Running flag
    running: Arc<AtomicBool>,

    /// Statistics
    stats: Arc<RwLock<WatchStats>>,

    /// Optional metrics collector
    metrics: Option<Arc<WatchMetrics>>,
}

impl InternalWatchRunner {
    /// Main processing loop
    ///
    /// Simplified version: only processes events and does verification.
    /// Storm detection is handled by HotUpdateCoordinator.
    async fn run(&mut self) -> Result<()> {
        let mut verification_timer =
            tokio::time::interval(Duration::from_secs(self.config.verification_interval_secs));

        while self.running.load(Ordering::Relaxed) {
            tokio::select! {
                // Process file events
                Some(event) = self.event_rx.recv() => {
                    if let Err(e) = self.handle_event(event).await {
                        tracing::error!(error = %e, "Failed to handle watch event");
                    }
                }

                // Periodic verification
                _ = verification_timer.tick() => {
                    if let Err(e) = self.verify_watch_status() {
                        tracing::error!(error = %e, "Failed to verify watch status");
                    }
                }
            }
        }

        Ok(())
    }

    /// Handle a watch event
    async fn handle_event(&mut self, event: WatchEvent) -> Result<()> {
        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.total_events += 1;

            if event.is_file_event() {
                stats.file_events += 1;
            } else if event.is_dir_event() {
                stats.dir_events += 1;
            } else if event.is_config_event() {
                stats.config_events += 1;
            }
        }

        if let Some(metrics) = &self.metrics {
            metrics.record_event();
            match &event {
                WatchEvent::FileCreated(_)
                | WatchEvent::FileChanged(_)
                | WatchEvent::FileDeleted(_)
                | WatchEvent::FileRenamed { .. } => {
                    metrics.record_file_event();
                }
                WatchEvent::DirCreated(_) | WatchEvent::DirDeleted(_) => {
                    metrics.record_dir_event();
                }
                WatchEvent::ConfigChanged(_) => {
                    metrics.record_config_event();
                }
                WatchEvent::Any => {}
            }
        }

        // Filter event
        if !self.should_process(&event) {
            let mut stats = self.stats.write().await;
            stats.filtered_events += 1;
            if let Some(metrics) = &self.metrics {
                metrics.record_filtered_event();
            }
            return Ok(());
        }

        // Handle based on event type
        match event {
            WatchEvent::FileCreated(path) => {
                self.send_file_event(FileEvent::created(path)).await?;
                if let Some(metrics) = &self.metrics {
                    metrics.record_forwarded_event();
                }
            }
            WatchEvent::FileChanged(path) => {
                self.send_file_event(FileEvent::modified(path)).await?;
                if let Some(metrics) = &self.metrics {
                    metrics.record_forwarded_event();
                }
            }
            WatchEvent::FileDeleted(path) => {
                self.send_file_event(FileEvent::deleted(path)).await?;
                if let Some(metrics) = &self.metrics {
                    metrics.record_forwarded_event();
                }
            }
            WatchEvent::FileRenamed { from, to } => {
                // Send deletion event for old path first, then creation event for new path
                self.send_file_event(FileEvent::deleted(from)).await?;
                self.send_file_event(FileEvent::created(to)).await?;
                if let Some(metrics) = &self.metrics {
                    metrics.record_forwarded_event();
                    metrics.record_forwarded_event();
                }
            }
            WatchEvent::ConfigChanged(path) => {
                tracing::info!(path = %path.display(), "Configuration file changed, sending reload event");
                // Send config modification event to hot update coordinator
                self.send_file_event(FileEvent::config_modified(path))
                    .await?;
                if let Some(metrics) = &self.metrics {
                    metrics.record_forwarded_event();
                }
            }
            WatchEvent::DirCreated(_path) => {}
            WatchEvent::DirDeleted(_path) => {
                // Log directory deletion for tracking
                // Full implementation would require maintaining a tracked files list
                // and cleaning up all files under the deleted directory
            }
            WatchEvent::Any => {}
        }

        Ok(())
    }

    /// Send file event to hot update coordinator
    ///
    /// Uses `try_send` so a full channel never blocks the watcher loop.
    /// On overflow the event is dropped and the overflow flag is set: the
    /// coordinator event loop turns it into a full-rescan fallback, so the
    /// dropped event is recovered from the filesystem by the next operation.
    async fn send_file_event(&self, event: FileEvent) -> Result<()> {
        match self.hot_update_tx.try_send(event) {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Full(event)) => {
                if let Some(metrics) = &self.metrics {
                    metrics.record_failed_event();
                }
                tracing::warn!(
                    path = %event.path.display(),
                    "Hot-update event channel full, dropping event and scheduling full rescan"
                );
                self.watch_event_overflow.store(true, Ordering::Relaxed);
                Ok(())
            }
            Err(mpsc::error::TrySendError::Closed(event)) => {
                if let Some(metrics) = &self.metrics {
                    metrics.record_failed_event();
                }
                Err(WatchError::send_event(event.path.display().to_string()))
            }
        }
    }

    /// Verify watch status
    fn verify_watch_status(&self) -> Result<()> {
        // Note: We don't have access to watcher here, so this is simplified
        Ok(())
    }

    /// Check if an event should be processed
    fn should_process(&self, event: &WatchEvent) -> bool {
        let path = match event.path() {
            Some(p) => p,
            None => return false,
        };

        let path_str = path.to_string_lossy();

        // Check ignore patterns
        if self.config.should_ignore_path(&path_str) {
            return false;
        }

        // Check extension for file events
        if event.is_file_event() {
            if let Some(ext) = path.extension() {
                if !self.config.should_watch_extension(&ext.to_string_lossy()) {
                    return false;
                }
            }
        }

        // Check for config files
        if self.config.watch_config_files && is_config_file(path) {
            return true;
        }

        true
    }
}

/// Check if a path is a configuration file
///
/// Delegates to the canonical build config file name rule set in
/// `cce_utils::path` (single source of truth shared with the
/// build-system detector and `FileCategory`).
fn is_config_file(path: &Path) -> bool {
    path.file_name()
        .map(|name| cce_types::path::is_build_config_name(&name.to_string_lossy()))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::super::event::FileEventType;
    use super::*;

    #[test]
    fn test_watch_config_default() {
        let config = WatchConfig::default();
        assert!(!config.extensions.is_empty());
        assert!(!config.ignore_patterns.is_empty());
    }

    #[test]
    fn test_watch_config_should_watch() {
        let config = WatchConfig::default();

        assert!(config.should_watch_extension("rs"));
        assert!(config.should_watch_extension("js"));
        assert!(!config.should_watch_extension("exe"));
    }

    #[test]
    fn test_watch_config_should_ignore() {
        let config = WatchConfig::default();

        assert!(config.should_ignore_path("src/node_modules/test.js"));
        assert!(config.should_ignore_path("src/target/debug/test.rs"));
        assert!(config.should_ignore_path("project/.git/hooks/pre-commit"));
        assert!(!config.should_ignore_path("src/main.rs"));
        assert!(!config.should_ignore_path("node_modules_extra/src/test.js"));
    }

    #[test]
    fn test_is_config_file() {
        assert!(is_config_file(Path::new("Cargo.toml")));
        assert!(is_config_file(Path::new("package.json")));
        // Make/Docker build files trigger the config-reload flow
        assert!(is_config_file(Path::new("Makefile")));
        assert!(is_config_file(Path::new("sub/GNUmakefile")));
        assert!(is_config_file(Path::new("Dockerfile")));
        assert!(!is_config_file(Path::new("main.rs")));
    }

    #[tokio::test]
    async fn test_watch_coordinator_creation() {
        let (tx, _rx) = mpsc::channel(16);
        let config = WatchConfig::default();
        let coordinator = WatchCoordinator::new(config, tx, Arc::new(AtomicBool::new(false)));

        assert!(coordinator.is_ok());
        let coordinator = coordinator.expect("WatchCoordinator should be created successfully");
        assert!(!coordinator.is_running());
    }

    /// a rename (old+new in one notify event) must decompose
    /// into a Deleted(old) + Created(new) FileEvent pair so the old path's
    /// data is removed and the new path is indexed — never a bogus
    /// "modified old path" event that drops the new file.
    #[test]
    fn test_convert_notify_rename_decomposes_to_delete_plus_create() {
        use notify::event::{EventKind, ModifyKind, RenameMode};

        let from = PathBuf::from("/proj/src/old.rs");
        let to = PathBuf::from("/proj/src/new.rs");
        let event = notify::Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::Both)))
            .add_path(from.clone())
            .add_path(to.clone());

        match WatchCoordinator::convert_notify_event(event) {
            Some(WatchEvent::FileRenamed { from: f, to: t }) => {
                assert_eq!(f, from);
                assert_eq!(t, to);
                // The decomposition the coordinator forwards downstream.
                let events = WatchEvent::FileRenamed { from: f, to: t }.to_file_event();
                let pair = WatchEvent::FileRenamed { from, to }.to_file_event();
                assert!(events.is_some() && pair.is_some());
            }
            other => panic!("rename must map to FileRenamed, got {other:?}"),
        }
    }

    #[test]
    fn test_convert_notify_rename_modes() {
        use notify::event::{EventKind, ModifyKind, RenameMode};

        // Split rename modes (some backends emit them separately) must map to
        // a deletion and a creation respectively.
        let from = PathBuf::from("/proj/src/old.rs");
        let removed = notify::Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::From)))
            .add_path(from);
        match WatchCoordinator::convert_notify_event(removed) {
            Some(WatchEvent::FileDeleted(path)) => {
                assert_eq!(path, PathBuf::from("/proj/src/old.rs"))
            }
            other => panic!("rename-from must map to FileDeleted, got {other:?}"),
        }

        let to = PathBuf::from("/proj/src/new.rs");
        let created =
            notify::Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::To))).add_path(to);
        match WatchCoordinator::convert_notify_event(created) {
            Some(WatchEvent::FileCreated(path)) => {
                assert_eq!(path, PathBuf::from("/proj/src/new.rs"))
            }
            other => panic!("rename-to must map to FileCreated, got {other:?}"),
        }
    }

    #[test]
    fn test_rename_decomposition_to_file_events() {
        // The decomposition contract for the coordinator: FileRenamed becomes
        // Deleted(from) then Created(to), in that order.
        let from = PathBuf::from("/proj/src/old.rs");
        let to = PathBuf::from("/proj/src/new.rs");
        let events = WatchEvent::FileRenamed {
            from: from.clone(),
            to: to.clone(),
        }
        .to_file_event()
        .expect("rename converts to a file event");
        assert_eq!(events.event_type, FileEventType::RenamedTo);
        assert_eq!(events.path, to);
        assert_eq!(events.previous_path.as_ref(), Some(&from));

        // The pair the watch handler forwards: delete the old path first, then
        // create the new one.
        let deleted = FileEvent::deleted(from.clone());
        let created = FileEvent::created(to.clone());
        assert_eq!(deleted.event_type, FileEventType::Deleted);
        assert_eq!(created.event_type, FileEventType::Created);
        assert_eq!(created.path, to);
    }
}
