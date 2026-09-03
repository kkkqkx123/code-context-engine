//! Storage management models

use serde::{Deserialize, Serialize};

use super::qdrant::QdrantProcessStatus;

/// Storage status response
#[derive(Debug, Serialize, Deserialize)]
pub struct StorageStatusResponse {
    pub success: bool,
    pub status: StorageStatus,
}

/// Storage status
#[derive(Debug, Serialize, Deserialize)]
pub struct StorageStatus {
    /// Vector storage status
    pub vector_storage: StorageComponentStatus,
    /// BM25 storage status
    pub bm25_storage: StorageComponentStatus,
    /// Relation storage status
    pub relation_storage: StorageComponentStatus,
    /// Total disk usage in MB
    pub total_disk_usage_mb: f64,
    /// Qdrant process status (only when subprocess management is enabled)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process_status: Option<QdrantProcessInfo>,
}

/// Qdrant subprocess information
#[derive(Debug, Serialize, Deserialize)]
pub struct QdrantProcessInfo {
    /// Whether subprocess management is enabled
    pub managed: bool,
    /// Current process status
    pub status: QdrantProcessStatus,
    /// Whether the process is running
    pub running: bool,
}

/// Storage component status
#[derive(Debug, Serialize, Deserialize)]
pub struct StorageComponentStatus {
    /// Whether the storage is connected
    pub connected: bool,
    /// Number of items stored
    pub item_count: usize,
    /// Disk usage in MB
    pub disk_usage_mb: f64,
    /// Server version (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Last error message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

/// Storage query parameters
#[derive(Debug, Deserialize)]
pub struct StorageQuery {
    /// Project ID for scoped storage operations
    pub project_id: i64,
}
