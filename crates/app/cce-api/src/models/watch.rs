//! Watch (hot reload) models

use serde::{Deserialize, Serialize};

/// Watch status
#[derive(Debug, Serialize, Deserialize)]
pub struct WatchStatus {
    /// Whether watch is active
    pub active: bool,
    /// Watched directories
    pub watched_dirs: Vec<String>,
    /// Number of events processed
    pub events_processed: usize,
    /// Started at timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
}

/// Watch status response
#[derive(Debug, Serialize, Deserialize)]
pub struct WatchStatusResponse {
    pub success: bool,
    pub status: WatchStatus,
}

/// Start watch request
#[derive(Debug, Serialize, Deserialize)]
pub struct StartWatchRequest {
    /// Directory to watch
    pub path: String,
    /// File extensions to watch
    #[serde(default)]
    pub extensions: Vec<String>,
    /// Debounce interval in milliseconds
    #[serde(default = "default_debounce_ms")]
    pub debounce_ms: u64,
}

fn default_debounce_ms() -> u64 {
    500
}
