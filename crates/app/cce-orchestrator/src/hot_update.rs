//! Hot update coordinator
//!
//! This module provides a simplified hot update mechanism based on the design:
//! - Global debounce + batch processing
//! - SQLite-based change detection (single source of truth)
//! - Parser-level change detection
//! - Unified downstream processing
//!
//! # Architecture
//!
//! ```text
//! File System
//!     ↓
//! WatchCoordinator (file watching) OR Periodic Scan
//!     ↓
//! GlobalDebounce (batch control)
//!     ↓
//! Scanner (scan changed files)
//!     ↓
//! ChangeDetector (detect changes via SQLite hash comparison)
//!     ↓
//! Parser (parse + detect entity changes)
//!     ↓
//! Downstream (relation, summary, embedding)
//! ```

// Module declarations
mod change;
mod change_detector;
pub mod config;
mod coordinator;
mod debounce;
mod error;
mod event_loop;
mod exclude_rules;
mod file_processor;
mod mode_switch;
mod operation_runtime;
mod periodic_scan;
pub mod processors;
pub mod progress;
mod state;
mod watch_change_queue;
pub mod watcher;

pub use self::watch_change_queue::WatchChangeQueue;

// Re-export public types
pub use self::change::{
    BatchChangeResult, EntityChange, EntityChangeType, FileChange, FileChangeType,
    ParseResultWithChanges,
};
pub use self::change_detector::CacheUpdateResult;
pub use self::config::{ConfigReloadManager, ConfigVersion, ConfigVersionRegistry};
pub use self::coordinator::HotUpdateCoordinator;
pub use self::debounce::{DebounceConfig, DebounceConfigBuilder, GlobalDebounce};
pub use self::error::{HotUpdateError, Result};
pub use self::event_loop::{EventLoopManager, EventLoopState, EventLoopStats};
pub use self::exclude_rules::{ExcludeRule, ExcludeRules, ExcludeRulesConfig};
pub use self::file_processor::FileProcessor;
pub use self::mode_switch::{ModeStateMachine, ModeSwitchConfig};
pub use self::periodic_scan::PeriodicScanTask;
pub use self::processors::{
    BoxedUpdateProcessor, EmbeddingUpdateProcessor, ExternalPackageData, ProcessorCollection,
    ProcessorConfig, RelationUpdateProcessor, SummaryUpdateProcessor, UpdateProcessor,
};
pub use self::state::{ChangeDetectionStats, DebounceInfo, HotUpdateMode, HotUpdateState};
pub use crate::export_processor::NlDocumentUpdateProcessor;
