//! Checkpoint record types and their SQLite column mappings.

use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckpointStatus {
    #[serde(rename = "in_progress")]
    InProgress,
    Completed,
    Failed,
}

impl CheckpointStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            CheckpointStatus::InProgress => "in_progress",
            CheckpointStatus::Completed => "completed",
            CheckpointStatus::Failed => "failed",
        }
    }
}

impl std::str::FromStr for CheckpointStatus {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "in_progress" => Ok(CheckpointStatus::InProgress),
            "completed" => Ok(CheckpointStatus::Completed),
            "failed" => Ok(CheckpointStatus::Failed),
            _ => Err(()),
        }
    }
}

impl std::fmt::Display for CheckpointStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkUnitStatus {
    Pending,
    Running,
    Committed,
    Failed,
}

impl WorkUnitStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            WorkUnitStatus::Pending => "pending",
            WorkUnitStatus::Running => "running",
            WorkUnitStatus::Committed => "committed",
            WorkUnitStatus::Failed => "failed",
        }
    }
}

impl std::str::FromStr for WorkUnitStatus {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(WorkUnitStatus::Pending),
            "running" => Ok(WorkUnitStatus::Running),
            "committed" => Ok(WorkUnitStatus::Committed),
            "failed" => Ok(WorkUnitStatus::Failed),
            _ => Err(()),
        }
    }
}

impl std::fmt::Display for WorkUnitStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl rusqlite::types::FromSql for CheckpointStatus {
    fn column_result(
        value: rusqlite::types::ValueRef,
    ) -> Result<Self, rusqlite::types::FromSqlError> {
        match value.as_str() {
            Ok(s) => s
                .parse()
                .map_err(|_| rusqlite::types::FromSqlError::InvalidType),
            Err(e) => Err(rusqlite::types::FromSqlError::Other(Box::new(e))),
        }
    }
}

impl rusqlite::types::ToSql for CheckpointStatus {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        Ok(rusqlite::types::ToSqlOutput::Owned(
            rusqlite::types::Value::Text(self.as_str().to_string()),
        ))
    }
}

impl rusqlite::types::FromSql for WorkUnitStatus {
    fn column_result(
        value: rusqlite::types::ValueRef,
    ) -> Result<Self, rusqlite::types::FromSqlError> {
        match value.as_str() {
            Ok(s) => s
                .parse()
                .map_err(|_| rusqlite::types::FromSqlError::InvalidType),
            Err(e) => Err(rusqlite::types::FromSqlError::Other(Box::new(e))),
        }
    }
}

impl rusqlite::types::ToSql for WorkUnitStatus {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        Ok(rusqlite::types::ToSqlOutput::Owned(
            rusqlite::types::Value::Text(self.as_str().to_string()),
        ))
    }
}
