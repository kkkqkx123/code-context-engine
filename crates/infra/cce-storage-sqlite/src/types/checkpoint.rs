//! Checkpoint record types for operation tracking.

use serde::{Deserialize, Serialize};

use super::status::{CheckpointStatus, ScanStatus, WorkUnitStatus};

/// Operation-level checkpoint record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointRecord {
    pub id: Option<i64>,
    pub project_id: i64,
    pub operation_id: String,
    pub operation_type: String,
    pub root_dir: String,
    pub total_files: u32,
    pub batch_size: u32,
    pub current_batch_index: u32,
    pub current_phase: String,
    pub file_list_hash: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub last_error: Option<String>,
    pub failure_count: u32,
    pub status: CheckpointStatus,
    pub active_flag: bool,
    pub priority: i32,
    pub last_heartbeat: Option<String>,
    pub failed_at: Option<String>,
}

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

/// File-level checkpoint record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileCheckpointRecord {
    pub id: Option<i64>,
    pub operation_id: String,
    pub batch_index: u32,
    pub file_path: String,
    pub file_id: Option<i64>,
    pub language: Option<String>,
    pub file_size: Option<i64>,
    pub content_hash: Option<String>,
    pub parsed_data: Option<Vec<u8>>,
    pub parse_error: Option<String>,
    pub summary_data: Option<Vec<u8>>,
    pub embedding_count: u32,
    pub bm25_doc_id: Option<String>,
    pub export_path: Option<String>,
    pub render_fingerprint: Option<String>,
    pub module_progress: Option<String>,
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

/// Work unit checkpoint record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkUnitCheckpointRecord {
    pub id: Option<i64>,
    pub project_id: i64,
    pub operation_id: String,
    pub stage: String,
    pub target_epoch: i64,
    pub work_unit_hash: String,
    pub status: WorkUnitStatus,
    pub item_count: u32,
    pub created_at: String,
    pub updated_at: String,
}
