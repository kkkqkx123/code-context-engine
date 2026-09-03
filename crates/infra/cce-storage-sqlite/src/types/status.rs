//! Status enums for SQLite operations.

use rusqlite::Result as SqlResult;
use rusqlite::types::{FromSql, FromSqlError, ToSql, ToSqlOutput};
use serde::{Deserialize, Serialize};

/// Operation-level checkpoint status.
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

impl FromSql for CheckpointStatus {
    fn column_result(value: rusqlite::types::ValueRef) -> Result<Self, FromSqlError> {
        match value.as_str() {
            Ok(s) => s.parse().map_err(|_| FromSqlError::InvalidType),
            Err(e) => Err(FromSqlError::Other(Box::new(e))),
        }
    }
}

impl ToSql for CheckpointStatus {
    fn to_sql(&self) -> SqlResult<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Owned(rusqlite::types::Value::Text(
            self.as_str().to_string(),
        )))
    }
}

/// Module processing status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModuleStatus {
    Pending,
    #[serde(rename = "in_progress")]
    InProgress,
    Success,
    Failed,
    Skipped,
}

impl ModuleStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ModuleStatus::Pending => "pending",
            ModuleStatus::InProgress => "in_progress",
            ModuleStatus::Success => "success",
            ModuleStatus::Failed => "failed",
            ModuleStatus::Skipped => "skipped",
        }
    }
}

impl std::str::FromStr for ModuleStatus {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(ModuleStatus::Pending),
            "in_progress" => Ok(ModuleStatus::InProgress),
            "success" => Ok(ModuleStatus::Success),
            "failed" => Ok(ModuleStatus::Failed),
            "skipped" => Ok(ModuleStatus::Skipped),
            _ => Err(()),
        }
    }
}

impl std::fmt::Display for ModuleStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromSql for ModuleStatus {
    fn column_result(value: rusqlite::types::ValueRef) -> Result<Self, FromSqlError> {
        match value.as_str() {
            Ok(s) => s.parse().map_err(|_| FromSqlError::InvalidType),
            Err(e) => Err(FromSqlError::Other(Box::new(e))),
        }
    }
}

impl ToSql for ModuleStatus {
    fn to_sql(&self) -> SqlResult<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Owned(rusqlite::types::Value::Text(
            self.as_str().to_string(),
        )))
    }
}

/// Overall file processing status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OverallStatus {
    Pending,
    Partial,
    #[serde(rename = "fully_processed")]
    FullyProcessed,
    Failed,
    Skipped,
}

impl OverallStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            OverallStatus::Pending => "pending",
            OverallStatus::Partial => "partial",
            OverallStatus::FullyProcessed => "fully_processed",
            OverallStatus::Failed => "failed",
            OverallStatus::Skipped => "skipped",
        }
    }
}

impl std::str::FromStr for OverallStatus {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(OverallStatus::Pending),
            "partial" => Ok(OverallStatus::Partial),
            "fully_processed" => Ok(OverallStatus::FullyProcessed),
            "failed" => Ok(OverallStatus::Failed),
            "skipped" => Ok(OverallStatus::Skipped),
            _ => Err(()),
        }
    }
}

impl std::fmt::Display for OverallStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromSql for OverallStatus {
    fn column_result(value: rusqlite::types::ValueRef) -> Result<Self, FromSqlError> {
        match value.as_str() {
            Ok(s) => s.parse().map_err(|_| FromSqlError::InvalidType),
            Err(e) => Err(FromSqlError::Other(Box::new(e))),
        }
    }
}

impl ToSql for OverallStatus {
    fn to_sql(&self) -> SqlResult<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Owned(rusqlite::types::Value::Text(
            self.as_str().to_string(),
        )))
    }
}

/// Retry management status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RetryStatus {
    Pending,
    Updating,
    Success,
    Failed,
    #[serde(rename = "dead_letter")]
    DeadLetter,
}

impl RetryStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            RetryStatus::Pending => "pending",
            RetryStatus::Updating => "updating",
            RetryStatus::Success => "success",
            RetryStatus::Failed => "failed",
            RetryStatus::DeadLetter => "dead_letter",
        }
    }
}

impl std::str::FromStr for RetryStatus {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(RetryStatus::Pending),
            "updating" => Ok(RetryStatus::Updating),
            "success" => Ok(RetryStatus::Success),
            "failed" => Ok(RetryStatus::Failed),
            "dead_letter" => Ok(RetryStatus::DeadLetter),
            _ => Err(()),
        }
    }
}

impl std::fmt::Display for RetryStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromSql for RetryStatus {
    fn column_result(value: rusqlite::types::ValueRef) -> Result<Self, FromSqlError> {
        match value.as_str() {
            Ok(s) => s.parse().map_err(|_| FromSqlError::InvalidType),
            Err(e) => Err(FromSqlError::Other(Box::new(e))),
        }
    }
}

impl ToSql for RetryStatus {
    fn to_sql(&self) -> SqlResult<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Owned(rusqlite::types::Value::Text(
            self.as_str().to_string(),
        )))
    }
}

/// Directory scan status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScanStatus {
    Pending,
    #[serde(rename = "in_progress")]
    InProgress,
    Completed,
    Failed,
}

impl ScanStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ScanStatus::Pending => "pending",
            ScanStatus::InProgress => "in_progress",
            ScanStatus::Completed => "completed",
            ScanStatus::Failed => "failed",
        }
    }
}

impl std::str::FromStr for ScanStatus {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(ScanStatus::Pending),
            "in_progress" => Ok(ScanStatus::InProgress),
            "completed" => Ok(ScanStatus::Completed),
            "failed" => Ok(ScanStatus::Failed),
            _ => Err(()),
        }
    }
}

impl std::fmt::Display for ScanStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromSql for ScanStatus {
    fn column_result(value: rusqlite::types::ValueRef) -> Result<Self, FromSqlError> {
        match value.as_str() {
            Ok(s) => s.parse().map_err(|_| FromSqlError::InvalidType),
            Err(e) => Err(FromSqlError::Other(Box::new(e))),
        }
    }
}

impl ToSql for ScanStatus {
    fn to_sql(&self) -> SqlResult<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Owned(rusqlite::types::Value::Text(
            self.as_str().to_string(),
        )))
    }
}

/// Work unit processing status.
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

impl FromSql for WorkUnitStatus {
    fn column_result(value: rusqlite::types::ValueRef) -> Result<Self, FromSqlError> {
        match value.as_str() {
            Ok(s) => s.parse().map_err(|_| FromSqlError::InvalidType),
            Err(e) => Err(FromSqlError::Other(Box::new(e))),
        }
    }
}

impl ToSql for WorkUnitStatus {
    fn to_sql(&self) -> SqlResult<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Owned(rusqlite::types::Value::Text(
            self.as_str().to_string(),
        )))
    }
}
