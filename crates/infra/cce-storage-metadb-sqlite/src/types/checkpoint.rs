//! Checkpoint record types for operation tracking.

use serde::{Deserialize, Serialize};

use super::status::ScanStatus;

pub use cce_storage_common::metadb::{
    CheckpointRecord, FileCheckpointRecord, WorkUnitCheckpointRecord,
};

/// Batch-level checkpoint record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchCheckpointRecord {
    pub id: Option<i64>,
    pub operation_id: String,
    pub batch_index: u32,
    pub first_file: String,
    pub last_file: String,
    pub file_count: u32,
    pub processed_files: u32,
    pub failed_files: u32,
    pub entities_extracted: u32,
    pub relations_found: u32,
    pub chunks_generated: u32,
    pub vectors_stored: u32,
    pub start_time: String,
    pub end_time: Option<String>,
    pub duration_ms: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}

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
