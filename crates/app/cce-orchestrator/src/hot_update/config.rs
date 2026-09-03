//! Configuration management for hot update
//!
//! This module provides configuration file handling for the hot update system,
//! including version control and reload management.
//!
//! # Architecture
//!
//! ```text
//! File System Event
//!     ↓
//! ConfigReloadManager (coordinate reload operations)
//!     ↓
//! ConfigVersionRegistry (prevent old config overwriting new)
//!     ↓
//! UpdateProcessor (execute actual reload)
//! ```
//!
//! # Modules
//!
//! - `reload`: Configuration reload management with retry and two-phase commit
//! - `version`: Configuration version control to prevent stale updates

mod reload;
mod version;

pub use reload::ConfigReloadManager;
pub use version::{ConfigVersion, ConfigVersionRegistry};
