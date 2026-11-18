//! Console log models.
//!
//! The log endpoints used to be console-only and lived in `cce-server`; the
//! desktop client reads the same console, so the payloads are wire types now.
//! They mirror `cce_logging::LogEvent` and the logger settings, kept separate
//! because `cce-api` must not depend on the logging crate.

use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// One captured log record.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LogEntry {
    /// Event time (RFC 3339).
    pub timestamp: String,
    /// Level name as the logger rendered it (`INFO`, `WARN`, ...).
    pub level: String,
    /// Module or crate target that emitted the record.
    pub target: String,
    /// Rendered message without structured fields.
    pub message: String,
    /// Structured fields attached to the record; shape is per record.
    #[serde(default)]
    pub fields: serde_json::Value,
}

/// Query parameters for the in-memory log buffer.
///
/// Filtering happens server side over the ring buffer, so the page window
/// applies to the filtered set, not to the raw buffer.
#[derive(Debug, Clone, Default, Serialize, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct LogsQuery {
    /// Minimum level (`trace`, `debug`, `info`, `warn`, `error`).
    pub level: Option<String>,
    /// Target/module substring.
    pub module: Option<String>,
    /// Message substring.
    pub query: Option<String>,
    /// Inclusive lower bound (RFC 3339, compared as text).
    pub start_time: Option<String>,
    /// Inclusive upper bound (RFC 3339, compared as text).
    pub end_time: Option<String>,
    /// One-based page number.
    pub page: Option<usize>,
    /// Page size, clamped by the server.
    pub page_size: Option<usize>,
}

/// One page of captured records.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LogsResponse {
    /// Records of this page, newest last.
    pub entries: Vec<LogEntry>,
    /// Records matching the filter before paging.
    pub total: usize,
    /// Echo of the requested page.
    pub page: usize,
    /// Echo of the applied page size.
    pub page_size: usize,
}

/// One file on disk in the log directory.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LogFileInfo {
    pub name: String,
    /// Size in bytes.
    pub size: u64,
    /// Last modification time (RFC 3339).
    pub modified: String,
}

/// Effective logger configuration.
///
/// Every field is the value the server is running with, not the value from
/// the configuration file: a rotation applied at runtime shows here.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LogConfig {
    /// Active level name.
    pub level: String,
    /// Active sink (`stdout`, `file`, both).
    pub output: String,
    /// Active record format (`pretty`, `compact`, `json`).
    pub format: String,
    /// Active log file, absent when file output is off.
    pub file: Option<String>,
    /// Active `tracing` filter directive, when one is set.
    pub filter: Option<String>,
    pub max_file_size_mb: Option<u64>,
    pub max_files: Option<usize>,
    pub max_total_size_mb: Option<u64>,
    /// Whether the HTTP log API is enabled for this process.
    pub enable_http_api: bool,
    /// WebSocket broadcast capacity.
    pub ws_capacity: usize,
    /// In-memory ring buffer capacity.
    pub memory_buffer_size: usize,
}

/// Logger fields the console may change at runtime.
///
/// File sinks and rotation limits are deliberately absent: changing them
/// requires reopening the writer, which is a restart, not a live update.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct UpdateLogConfigRequest {
    /// New level name; unknown names are ignored by the server.
    pub level: Option<String>,
    /// New format name; unknown names are ignored by the server.
    pub format: Option<String>,
    /// New filter directive; takes precedence over `level`.
    pub filter: Option<String>,
}
