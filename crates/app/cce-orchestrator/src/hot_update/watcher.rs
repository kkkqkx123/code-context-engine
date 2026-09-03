//! File watcher module for hot update
//!
//! This module provides file system watching capabilities integrated with hot update.
//! It has been simplified and moved from `src/watch` to eliminate redundancy.

pub mod config;
pub mod coordinator;
pub mod error;
pub mod event;

// Re-export main types
pub use config::{WatchConfig, WatchMode, WatchStrategy};
pub use coordinator::WatchCoordinator;
pub use error::{Result as WatchResult, WatchError};
pub use event::{
    FileEvent, FileEventType, UnifiedWatchStats, WatchEvent, WatchStats, WatchStatus,
    WatchStatusTracker,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_exports() {
        // Test that all main types are exported
        let _config = WatchConfig::default();
    }
}
