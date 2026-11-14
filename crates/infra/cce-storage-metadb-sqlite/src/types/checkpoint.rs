//! Checkpoint record types for operation tracking.

use serde::{Deserialize, Serialize};

use super::status::ScanStatus;

pub use cce_storage_common::metadb::{
    BatchCheckpointRecord, CheckpointRecord, FileCheckpointRecord, WorkUnitCheckpointRecord,
};

/// Scan phase checkpoint record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanCheckpointRecord {
    pub id: Option<i64>,
    pub operation_id: String,
    pub root_dir: String,
    pub total_files_found: u32,
    pub scan_depth: u32,
    pub last_scanned_path: String,
    pub file_list_hash: String,
    pub status: ScanStatus,
    pub scan_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
