//! Console-only log endpoints.
//!
//! These routes serve the web console and are intentionally outside the shared
//! contract in `cce-api`. No Rust consumer calls them and their payloads are
//! not modeled as wire types, so keeping them here prevents contract bloat
//! and keeps `cce-client` coverage complete by construction.

pub const LOGS: &str = "/api/logs";
pub const LOGS_FILES: &str = "/api/logs/files";
pub const LOGS_FILE_TEMPLATE: &str = "/api/logs/files/{name}";
pub const LOGS_FILE_DOWNLOAD_TEMPLATE: &str = "/api/logs/files/{name}/download";
pub const LOGS_ROTATE: &str = "/api/logs/rotate";
pub const LOGS_CONFIG: &str = "/api/logs/config";
pub const LOGS_WS: &str = "/ws/logs";
