//! Event types for file watching
//!
//! This module provides event types used for file system watching and
//! communication between the watcher and hot update coordinator.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// File event type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileEventType {
    /// File was created
    Created,
    /// File was modified
    Modified,
    /// File was deleted
    Deleted,
    /// File was renamed (old name)
    RenamedFrom,
    /// File was renamed (new name)
    RenamedTo,
    /// Configuration file was modified
    ConfigModified,
}

impl FileEventType {
    /// Check if this is a creation event
    pub fn is_create(&self) -> bool {
        matches!(self, Self::Created)
    }

    /// Check if this is a modification event
    pub fn is_modify(&self) -> bool {
        matches!(self, Self::Modified)
    }

    /// Check if this is a deletion event
    pub fn is_delete(&self) -> bool {
        matches!(self, Self::Deleted)
    }

    /// Check if this is a rename event
    pub fn is_rename(&self) -> bool {
        matches!(self, Self::RenamedFrom | Self::RenamedTo)
    }

    /// Check if this is a config modification event
    pub fn is_config_modify(&self) -> bool {
        matches!(self, Self::ConfigModified)
    }
}

/// File event sent to hot update coordinator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEvent {
    /// File path
    pub path: PathBuf,
    /// Event type
    pub event_type: FileEventType,
    /// Event timestamp
    pub timestamp: DateTime<Utc>,
    /// Previous path (for rename events)
    pub previous_path: Option<PathBuf>,
}

impl FileEvent {
    /// Create a new file event
    pub fn new(path: PathBuf, event_type: FileEventType) -> Self {
        Self {
            path,
            event_type,
            timestamp: Utc::now(),
            previous_path: None,
        }
    }

    /// Create a created event
    pub fn created(path: PathBuf) -> Self {
        Self::new(path, FileEventType::Created)
    }

    /// Create a modified event
    pub fn modified(path: PathBuf) -> Self {
        Self::new(path, FileEventType::Modified)
    }

    /// Create a deleted event
    pub fn deleted(path: PathBuf) -> Self {
        Self::new(path, FileEventType::Deleted)
    }

    /// Create a config modified event
    pub fn config_modified(path: PathBuf) -> Self {
        Self::new(path, FileEventType::ConfigModified)
    }

    /// Create a renamed event with previous path tracking
    pub fn renamed(from: PathBuf, to: PathBuf) -> Self {
        Self {
            path: to,
            event_type: FileEventType::RenamedTo,
            timestamp: Utc::now(),
            previous_path: Some(from),
        }
    }
}

/// Watch event type (internal)
///
/// These are the raw events from the file system watcher,
/// before being converted to FileEvent for the hot update coordinator.
#[derive(Debug, Clone)]
pub enum WatchEvent {
    /// File was created
    FileCreated(PathBuf),
    /// File was modified
    FileChanged(PathBuf),
    /// File was deleted
    FileDeleted(PathBuf),
    /// File was renamed
    FileRenamed {
        /// Old path
        from: PathBuf,
        /// New path
        to: PathBuf,
    },
    /// Directory was created
    DirCreated(PathBuf),
    /// Directory was deleted
    DirDeleted(PathBuf),
    /// Configuration file changed
    ConfigChanged(PathBuf),
    /// Any event (for counting)
    Any,
}

impl WatchEvent {
    /// Get the path associated with this event (if any)
    pub fn path(&self) -> Option<&PathBuf> {
        match self {
            Self::FileCreated(path)
            | Self::FileChanged(path)
            | Self::FileDeleted(path)
            | Self::DirCreated(path)
            | Self::DirDeleted(path)
            | Self::ConfigChanged(path) => Some(path),
            Self::FileRenamed { to, .. } => Some(to),
            Self::Any => None,
        }
    }

    /// Check if this is a file event
    pub fn is_file_event(&self) -> bool {
        matches!(
            self,
            Self::FileCreated(_)
                | Self::FileChanged(_)
                | Self::FileDeleted(_)
                | Self::FileRenamed { .. }
        )
    }

    /// Check if this is a directory event
    pub fn is_dir_event(&self) -> bool {
        matches!(self, Self::DirCreated(_) | Self::DirDeleted(_))
    }

    /// Check if this is a config event
    pub fn is_config_event(&self) -> bool {
        matches!(self, Self::ConfigChanged(_))
    }

    /// Convert to FileEvent (if applicable)
    ///
    /// This is now the primary conversion method, eliminating the need for
    /// manual matching in multiple places.
    pub fn to_file_event(&self) -> Option<FileEvent> {
        match self {
            Self::FileCreated(path) => Some(FileEvent::created(path.clone())),
            Self::FileChanged(path) => Some(FileEvent::modified(path.clone())),
            Self::FileDeleted(path) => Some(FileEvent::deleted(path.clone())),
            Self::FileRenamed { from, to } => Some(FileEvent::renamed(from.clone(), to.clone())),
            _ => None,
        }
    }

    /// Convert to FileEvent with timestamp override
    pub fn to_file_event_with_timestamp(
        &self,
        timestamp: chrono::DateTime<chrono::Utc>,
    ) -> Option<FileEvent> {
        let mut event = self.to_file_event()?;
        event.timestamp = timestamp;
        Some(event)
    }
}

/// Watch status - unified with HotUpdateMode
///
/// This enum is now an alias for HotUpdateMode to eliminate duplication.
/// Additional states (Paused, Error) are handled separately in WatchStatusTracker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WatchStatus {
    /// Watcher is not started
    Stopped,
    /// Watcher is running in file watch mode
    FileWatch,
    /// Watcher is running in periodic scan mode (degraded)
    PeriodicScan,
    /// Watcher is paused
    Paused,
    /// Watcher encountered an error
    Error,
}

impl WatchStatus {
    /// Check if watcher is active
    pub fn is_active(&self) -> bool {
        matches!(self, Self::FileWatch | Self::PeriodicScan)
    }

    /// Check if watcher is in degraded mode
    pub fn is_degraded(&self) -> bool {
        matches!(self, Self::PeriodicScan)
    }

    /// Convert to HotUpdateMode
    pub fn to_hot_update_mode(&self) -> Option<super::super::HotUpdateMode> {
        match self {
            Self::FileWatch => Some(super::super::HotUpdateMode::FileWatch),
            Self::PeriodicScan => Some(super::super::HotUpdateMode::PeriodicScan),
            _ => None,
        }
    }

    /// Convert from HotUpdateMode
    pub fn from_hot_update_mode(mode: super::super::HotUpdateMode) -> Self {
        match mode {
            super::super::HotUpdateMode::FileWatch => Self::FileWatch,
            super::super::HotUpdateMode::PeriodicScan => Self::PeriodicScan,
        }
    }
}

/// Watch status tracker for API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchStatusTracker {
    /// Whether watch is active
    pub active: bool,
    /// Watched directories
    pub watched_dirs: Vec<String>,
    /// Number of events processed
    pub events_processed: u64,
    /// Started at timestamp
    pub started_at: Option<DateTime<Utc>>,
    /// Current mode
    pub mode: WatchStatus,
}

impl Default for WatchStatusTracker {
    fn default() -> Self {
        Self {
            active: false,
            watched_dirs: Vec::new(),
            events_processed: 0,
            started_at: None,
            mode: WatchStatus::Stopped,
        }
    }
}

impl WatchStatusTracker {
    /// Create a new status tracker
    pub fn new() -> Self {
        Self::default()
    }

    /// Mark as started
    pub fn start(&mut self, path: &std::path::Path) {
        self.active = true;
        self.mode = WatchStatus::FileWatch;
        self.started_at = Some(Utc::now());
        self.events_processed = 0;
        let path_str = path.to_string_lossy().to_string();
        if !self.watched_dirs.contains(&path_str) {
            self.watched_dirs.push(path_str);
        }
    }

    /// Mark as stopped
    pub fn stop(&mut self) {
        self.active = false;
        self.mode = WatchStatus::Stopped;
        self.started_at = None;
    }

    /// Increment events processed
    pub fn increment_events(&mut self) {
        self.events_processed += 1;
    }

    /// Set mode
    pub fn set_mode(&mut self, mode: WatchStatus) {
        self.mode = mode;
        self.active = mode.is_active();
    }
}

/// Unified statistics for file watching and event processing
///
/// This structure consolidates statistics from both WatchStats and EventLoopStats
/// to provide a single source of truth for monitoring.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedWatchStats {
    // === Event counts ===
    /// Total events received from file system
    pub total_events: u64,
    /// Events successfully processed
    pub events_processed: u64,
    /// Events that failed processing
    pub events_failed: u64,
    /// Events filtered out (ignored patterns)
    pub filtered_events: u64,

    // === Event breakdown ===
    /// File events count
    pub file_events: u64,
    /// Directory events count
    pub dir_events: u64,
    /// Config events count
    pub config_events: u64,

    // === Performance metrics ===
    /// Events per second (current rate)
    pub events_per_sec: usize,

    // === Status ===
    /// Current watch status
    pub status: WatchStatus,
    /// Number of watched paths
    pub watched_paths: usize,

    // === Timing ===
    /// When watching started
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    /// When watching stopped
    pub stopped_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl Default for UnifiedWatchStats {
    fn default() -> Self {
        Self {
            total_events: 0,
            events_processed: 0,
            events_failed: 0,
            filtered_events: 0,
            file_events: 0,
            dir_events: 0,
            config_events: 0,
            events_per_sec: 0,
            status: WatchStatus::Stopped,
            watched_paths: 0,
            started_at: None,
            stopped_at: None,
        }
    }
}

impl UnifiedWatchStats {
    /// Create new empty stats
    pub fn new() -> Self {
        Self::default()
    }

    /// Record an event received
    pub fn record_event(&mut self) {
        self.total_events += 1;
    }

    /// Record an event processed successfully
    pub fn record_processed(&mut self) {
        self.events_processed += 1;
    }

    /// Record an event failed
    pub fn record_failed(&mut self) {
        self.events_failed += 1;
    }

    /// Record an event filtered
    pub fn record_filtered(&mut self) {
        self.filtered_events += 1;
    }

    /// Calculate success rate
    pub fn success_rate(&self) -> f64 {
        if self.events_processed + self.events_failed == 0 {
            0.0
        } else {
            self.events_processed as f64 / (self.events_processed + self.events_failed) as f64
        }
    }
}

/// Watch statistics (legacy, kept for backward compatibility)
///
/// Note: This is now a subset of UnifiedWatchStats.
/// Consider migrating to UnifiedWatchStats in future versions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchStats {
    /// Total events received
    pub total_events: u64,
    /// Events per second (current rate)
    pub events_per_sec: usize,
    /// File events count
    pub file_events: u64,
    /// Directory events count
    pub dir_events: u64,
    /// Config events count
    pub config_events: u64,
    /// Events filtered out
    pub filtered_events: u64,
    /// Current watch status
    pub status: WatchStatus,
    /// Number of watched paths
    pub watched_paths: usize,
}

impl Default for WatchStats {
    fn default() -> Self {
        Self {
            total_events: 0,
            events_per_sec: 0,
            file_events: 0,
            dir_events: 0,
            config_events: 0,
            filtered_events: 0,
            status: WatchStatus::Stopped,
            watched_paths: 0,
        }
    }
}

impl WatchStats {
    /// Create new empty stats
    pub fn new() -> Self {
        Self {
            status: WatchStatus::Stopped,
            ..Default::default()
        }
    }

    /// Convert to UnifiedWatchStats
    pub fn to_unified(&self) -> UnifiedWatchStats {
        UnifiedWatchStats {
            total_events: self.total_events,
            events_processed: self.file_events + self.dir_events + self.config_events,
            events_failed: 0,
            filtered_events: self.filtered_events,
            file_events: self.file_events,
            dir_events: self.dir_events,
            config_events: self.config_events,
            events_per_sec: self.events_per_sec,
            status: self.status,
            watched_paths: self.watched_paths,
            started_at: None,
            stopped_at: None,
        }
    }
}
