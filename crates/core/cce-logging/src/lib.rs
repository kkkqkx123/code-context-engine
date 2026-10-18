//! Unified logging infrastructure for Code Context Engine.
//!
//! Provides initialization, file rotation with size and retention limits,
//! runtime level updates, and (optionally) per-module filtering.

pub mod capture;
pub mod logger;

pub use capture::{CaptureLayer, LogEvent, init as init_capture, recent, subscribe};
pub use logger::config::{get_format_string, get_output_string, parse_level, validate_config};
pub use logger::{LOG_GUARD, Result, cleanup_logs, init, reload_level};
