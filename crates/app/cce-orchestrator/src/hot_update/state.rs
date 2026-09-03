//! Hot update state and type definitions
//!
//! This module provides runtime state types and core type definitions
//! for hot update operations.

use std::time::Duration;

// ============================================================================
// Core Type Definitions
// ============================================================================

/// Hot update mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotUpdateMode {
    /// Periodic scan mode (traditional)
    PeriodicScan,
    /// File watch mode (real-time)
    FileWatch,
}

/// Change-detection statistics of a hot update coordinator
///
/// Reports the number of files tracked by the SQLite-backed hash cache
/// (the single source of truth for change detection).
#[derive(Debug, Clone, Default)]
pub struct ChangeDetectionStats {
    /// Number of files tracked in the persisted hash cache
    pub stored_files: usize,
}

// ============================================================================
// Runtime State Types
// ============================================================================

/// Debounce state information (runtime state)
#[derive(Debug, Clone, Copy)]
pub struct DebounceInfo {
    /// Whether there are pending changes
    pub has_pending_changes: bool,
    /// Time until next potential update
    pub time_until_next: Duration,
    /// Current configuration (from debounce module)
    pub config: crate::hot_update::debounce::DebounceConfig,
}

impl DebounceInfo {
    /// Create a new debounce info
    pub fn new(config: crate::hot_update::debounce::DebounceConfig) -> Self {
        Self {
            has_pending_changes: false,
            time_until_next: config.pending_interval,
            config,
        }
    }

    /// Create with pending changes
    pub fn with_pending(config: crate::hot_update::debounce::DebounceConfig) -> Self {
        Self {
            has_pending_changes: true,
            time_until_next: config.pending_interval,
            config,
        }
    }
}

/// Hot update runtime state
#[derive(Debug, Clone, Default)]
pub struct HotUpdateState {
    /// Whether hot update is currently active
    pub is_active: bool,
    /// Number of files processed in current batch
    pub files_processed: usize,
    /// Number of changes detected
    pub changes_detected: usize,
    /// Last update timestamp
    pub last_update: Option<std::time::Instant>,
}

impl HotUpdateState {
    /// Create a new state
    pub fn new() -> Self {
        Self::default()
    }

    /// Mark as active
    pub fn activate(&mut self) {
        self.is_active = true;
    }

    /// Mark as inactive
    pub fn deactivate(&mut self) {
        self.is_active = false;
        self.last_update = Some(std::time::Instant::now());
    }

    /// Record a file processed
    pub fn record_file(&mut self) {
        self.files_processed += 1;
    }

    /// Record a change detected
    pub fn record_change(&mut self) {
        self.changes_detected += 1;
    }

    /// Reset counters
    pub fn reset_counters(&mut self) {
        self.files_processed = 0;
        self.changes_detected = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hot_update::debounce::DebounceConfig;

    #[test]
    fn test_change_detection_stats_default() {
        let stats = ChangeDetectionStats::default();
        assert_eq!(stats.stored_files, 0);
    }

    #[test]
    fn test_hot_update_mode() {
        let mode = HotUpdateMode::PeriodicScan;
        assert_ne!(mode, HotUpdateMode::FileWatch);
    }

    #[test]
    fn test_debounce_info() {
        let config = DebounceConfig::default();
        let info = DebounceInfo::new(config);

        assert!(!info.has_pending_changes);
        assert_eq!(info.time_until_next, Duration::from_secs(30));
    }

    #[test]
    fn test_debounce_info_with_pending() {
        let config = DebounceConfig::default();
        let info = DebounceInfo::with_pending(config);

        assert!(info.has_pending_changes);
        assert_eq!(info.time_until_next, Duration::from_secs(30));
    }

    #[test]
    fn test_hot_update_state() {
        let mut state = HotUpdateState::new();

        assert!(!state.is_active);
        assert_eq!(state.files_processed, 0);

        state.activate();
        assert!(state.is_active);

        state.record_file();
        state.record_change();
        assert_eq!(state.files_processed, 1);
        assert_eq!(state.changes_detected, 1);

        state.deactivate();
        assert!(!state.is_active);
        assert!(state.last_update.is_some());
    }

    #[test]
    fn test_hot_update_state_reset_counters() {
        let mut state = HotUpdateState::new();

        // Activate and record some operations
        state.activate();
        state.record_file();
        state.record_file();
        state.record_change();
        state.record_change();
        state.record_change();

        assert_eq!(state.files_processed, 2);
        assert_eq!(state.changes_detected, 3);

        // Deactivate
        state.deactivate();
        assert!(!state.is_active);
        assert!(state.last_update.is_some());

        // Reset counters
        state.reset_counters();
        assert_eq!(state.files_processed, 0);
        assert_eq!(state.changes_detected, 0);
    }
}
