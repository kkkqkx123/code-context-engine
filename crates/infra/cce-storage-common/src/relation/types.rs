//! Backend-neutral record types for relation storage.

use serde::{Deserialize, Serialize};

pub type DbId = i64;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRecord {
    pub id: i64,
    pub path: String,
    pub language: String,
    pub category: u8,
    pub last_modified: i64,
    pub created_at: i64,
    pub project_id: i64,
    pub content_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityRecord {
    pub id: i64,
    pub name: String,
    pub kind: String,
    pub file_id: i64,
    pub signature: Option<String>,
    pub span_start_row: Option<i64>,
    pub span_end_row: Option<i64>,
    pub span_start_column: Option<i64>,
    pub span_end_column: Option<i64>,
    pub span_start_byte: Option<i64>,
    pub span_end_byte: Option<i64>,
    pub scoped_name: Option<String>,
    pub depth: Option<i64>,
    pub parent_id: Option<i64>,
    pub metadata: Option<String>,
    pub parameters_json: Option<String>,
    pub return_type: Option<String>,
    pub doc_comment: Option<String>,
    pub modifiers_json: Option<String>,
    pub project_id: i64,
    pub epoch: i64,
    pub batch_id: i64,
    pub rank: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectRecord {
    pub id: i64,
    pub name: String,
    pub root_path: String,
    pub config_file_path: String,
    pub language: Option<String>,
    pub extensions: Option<String>,
    pub exclude_dirs: Option<String>,
    pub respect_gitignore: Option<bool>,
    pub ignore_patterns: Option<String>,
    pub last_indexed: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewProjectRecord {
    pub name: String,
    pub root_path: String,
    pub config_file_path: Option<String>,
    pub language: Option<String>,
    pub extensions: Option<String>,
    pub exclude_dirs: Option<String>,
    pub respect_gitignore: Option<bool>,
    pub ignore_patterns: Option<String>,
}

impl NewProjectRecord {
    pub fn new(name: String, root_path: String) -> Self {
        Self {
            name,
            root_path,
            config_file_path: Some(".cce/config.json".to_string()),
            language: None,
            extensions: None,
            exclude_dirs: None,
            respect_gitignore: None,
            ignore_patterns: None,
        }
    }

    pub fn build(self) -> ProjectRecord {
        use chrono::Utc;
        let now = Utc::now().timestamp();

        ProjectRecord {
            id: 0,
            name: self.name,
            root_path: self.root_path,
            config_file_path: self
                .config_file_path
                .unwrap_or_else(|| ".cce/config.json".to_string()),
            language: self.language,
            extensions: self.extensions,
            exclude_dirs: self.exclude_dirs,
            respect_gitignore: self.respect_gitignore,
            ignore_patterns: self.ignore_patterns,
            last_indexed: None,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectUpdateRecord {
    pub name: Option<String>,
    pub root_path: Option<String>,
    pub config_file_path: Option<String>,
    pub language: Option<String>,
    pub extensions: Option<String>,
    pub exclude_dirs: Option<String>,
    pub respect_gitignore: Option<bool>,
    pub ignore_patterns: Option<String>,
    pub last_indexed: Option<String>,
}

impl ProjectUpdateRecord {
    pub fn with_name(mut self, name: String) -> Self {
        self.name = Some(name);
        self
    }

    pub fn with_root_path(mut self, root_path: String) -> Self {
        self.root_path = Some(root_path);
        self
    }

    pub fn with_config_file_path(mut self, config_file_path: String) -> Self {
        self.config_file_path = Some(config_file_path);
        self
    }

    pub fn with_language(mut self, language: String) -> Self {
        self.language = Some(language);
        self
    }

    pub fn with_extensions(mut self, extensions: String) -> Self {
        self.extensions = Some(extensions);
        self
    }

    pub fn with_exclude_dirs(mut self, exclude_dirs: String) -> Self {
        self.exclude_dirs = Some(exclude_dirs);
        self
    }

    pub fn with_respect_gitignore(mut self, respect_gitignore: bool) -> Self {
        self.respect_gitignore = Some(respect_gitignore);
        self
    }

    pub fn with_ignore_patterns(mut self, ignore_patterns: String) -> Self {
        self.ignore_patterns = Some(ignore_patterns);
        self
    }

    pub fn with_last_indexed(mut self, last_indexed: String) -> Self {
        self.last_indexed = Some(last_indexed);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityDetailMapping {
    pub id: i64,
    pub entity_id: i64,
    pub project_id: Option<i64>,
    pub epoch: i64,
    pub qdrant_point_ids: String,
    pub bm25_doc_ids: String,
    pub chunk_count: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

impl EntityDetailMapping {
    pub fn new(entity_id: i64) -> Self {
        use chrono::Utc;
        let now = Utc::now().timestamp();
        Self {
            id: 0,
            entity_id,
            project_id: None,
            epoch: 0,
            qdrant_point_ids: "[]".to_string(),
            bm25_doc_ids: "[]".to_string(),
            chunk_count: 0,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn with_project_id(mut self, project_id: i64) -> Self {
        self.project_id = Some(project_id);
        self
    }

    pub fn with_epoch(mut self, epoch: i64) -> Self {
        self.epoch = epoch;
        self
    }

    pub fn with_qdrant_point_ids(mut self, point_ids: &[String]) -> Self {
        self.qdrant_point_ids = serde_json::to_string(point_ids).unwrap_or_else(|error| {
            tracing::warn!(error = %error, "Failed to serialize qdrant point ids; storing empty list");
            "[]".to_string()
        });
        self.chunk_count = point_ids.len() as i64;
        self.updated_at = chrono::Utc::now().timestamp();
        self
    }

    pub fn with_bm25_doc_ids(mut self, doc_ids: &[String]) -> Self {
        self.bm25_doc_ids = serde_json::to_string(doc_ids).unwrap_or_else(|error| {
            tracing::warn!(error = %error, "Failed to serialize bm25 doc ids; storing empty list");
            "[]".to_string()
        });
        self.updated_at = chrono::Utc::now().timestamp();
        self
    }

    pub fn get_qdrant_point_ids(&self) -> Vec<String> {
        match serde_json::from_str(&self.qdrant_point_ids) {
            Ok(ids) => ids,
            Err(error) => {
                tracing::warn!(error = %error, "Corrupt qdrant point ids; returning empty list");
                Vec::new()
            }
        }
    }

    pub fn try_get_qdrant_point_ids(&self) -> Result<Vec<String>, serde_json::Error> {
        serde_json::from_str(&self.qdrant_point_ids)
    }

    pub fn get_bm25_doc_ids(&self) -> Vec<String> {
        match serde_json::from_str(&self.bm25_doc_ids) {
            Ok(ids) => ids,
            Err(error) => {
                tracing::warn!(error = %error, "Corrupt bm25 doc ids; returning empty list");
                Vec::new()
            }
        }
    }

    pub fn try_get_bm25_doc_ids(&self) -> Result<Vec<String>, serde_json::Error> {
        serde_json::from_str(&self.bm25_doc_ids)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkRecord {
    pub chunk_id: String,
    pub file_path: String,
    pub content: String,
    pub start_line: i64,
    pub end_line: i64,
    pub entity_ids: String,
    pub entity_names: String,
    pub chunk_type: String,
    pub test_status: u8,
    pub test_source: u8,
    pub created_at: i64,
    pub updated_at: i64,
    pub project_id: Option<i64>,
    pub epoch: i64,
    pub batch_id: i64,
    pub path: String,
    pub bm25_keywords: String,
    pub segment_id: String,
    pub truncated: u8,
}

impl ChunkRecord {
    pub fn new(
        chunk_id: String,
        file_path: String,
        content: String,
        start_line: i64,
        end_line: i64,
    ) -> Self {
        use chrono::Utc;
        let now = Utc::now().timestamp();
        Self {
            chunk_id,
            file_path,
            content,
            start_line,
            end_line,
            entity_ids: "[]".to_string(),
            entity_names: "[]".to_string(),
            chunk_type: "unknown".to_string(),
            test_status: 0,
            test_source: 0,
            created_at: now,
            updated_at: now,
            project_id: None,
            epoch: 0,
            batch_id: 0,
            path: "emb".to_string(),
            bm25_keywords: String::new(),
            segment_id: String::new(),
            truncated: 0,
        }
    }

    pub fn with_bm25_keywords(mut self, keywords: impl Into<String>) -> Self {
        self.bm25_keywords = keywords.into();
        self.updated_at = chrono::Utc::now().timestamp();
        self
    }

    pub fn with_truncated(mut self, truncated: bool) -> Self {
        self.truncated = truncated as u8;
        self.updated_at = chrono::Utc::now().timestamp();
        self
    }

    pub fn with_entity_ids(mut self, entity_ids: &[i64]) -> Self {
        self.entity_ids = serde_json::to_string(entity_ids).unwrap_or_else(|error| {
            tracing::warn!(error = %error, "Failed to serialize entity ids; storing empty list");
            "[]".to_string()
        });
        self.updated_at = chrono::Utc::now().timestamp();
        self
    }

    pub fn with_entity_ids_json(mut self, entity_ids_json: impl Into<String>) -> Self {
        self.entity_ids = entity_ids_json.into();
        self.updated_at = chrono::Utc::now().timestamp();
        self
    }

    pub fn with_entity_names(mut self, entity_names: &[String]) -> Self {
        self.entity_names = serde_json::to_string(entity_names).unwrap_or_else(|error| {
            tracing::warn!(error = %error, "Failed to serialize entity names; storing empty list");
            "[]".to_string()
        });
        self.updated_at = chrono::Utc::now().timestamp();
        self
    }

    pub fn with_chunk_type(mut self, chunk_type: String) -> Self {
        self.chunk_type = chunk_type;
        self.updated_at = chrono::Utc::now().timestamp();
        self
    }

    pub fn with_test_status(mut self, test_status: u8) -> Self {
        self.test_status = test_status;
        self.updated_at = chrono::Utc::now().timestamp();
        self
    }

    pub fn with_test_source(mut self, test_source: u8) -> Self {
        self.test_source = test_source;
        self.updated_at = chrono::Utc::now().timestamp();
        self
    }

    pub fn with_project_id(mut self, project_id: i64) -> Self {
        self.project_id = Some(project_id);
        self.updated_at = chrono::Utc::now().timestamp();
        self
    }

    pub fn with_epoch(mut self, epoch: i64) -> Self {
        self.epoch = epoch;
        self.updated_at = chrono::Utc::now().timestamp();
        self
    }

    pub fn with_batch_id(mut self, batch_id: i64) -> Self {
        self.batch_id = batch_id;
        self.updated_at = chrono::Utc::now().timestamp();
        self
    }

    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = path.into();
        self.updated_at = chrono::Utc::now().timestamp();
        self
    }

    pub fn with_segment_id(mut self, segment_id: impl Into<String>) -> Self {
        self.segment_id = segment_id.into();
        self.updated_at = chrono::Utc::now().timestamp();
        self
    }

    pub fn get_entity_ids(&self) -> Vec<i64> {
        match serde_json::from_str(&self.entity_ids) {
            Ok(ids) => ids,
            Err(error) => {
                tracing::warn!(error = %error, "Corrupt entity ids; returning empty list");
                Vec::new()
            }
        }
    }

    pub fn try_get_entity_ids(&self) -> Result<Vec<i64>, serde_json::Error> {
        serde_json::from_str(&self.entity_ids)
    }

    pub fn get_entity_names(&self) -> Vec<String> {
        match serde_json::from_str(&self.entity_names) {
            Ok(names) => names,
            Err(error) => {
                tracing::warn!(error = %error, "Corrupt entity names; returning empty list");
                Vec::new()
            }
        }
    }

    pub fn try_get_entity_names(&self) -> Result<Vec<String>, serde_json::Error> {
        serde_json::from_str(&self.entity_names)
    }

    pub fn has_entity_id(&self, entity_id: i64) -> bool {
        self.get_entity_ids().contains(&entity_id)
    }
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverrideDisposition {
    Replaced,
    Deleted,
}

impl OverrideDisposition {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Replaced => "replaced",
            Self::Deleted => "deleted",
        }
    }

    pub fn parse(value: &str) -> Result<Self, cce_types::StorageError> {
        match value {
            "replaced" => Ok(Self::Replaced),
            "deleted" => Ok(Self::Deleted),
            other => Err(cce_types::StorageError::Query(format!(
                "invalid generation override disposition: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerationOverride {
    pub file_path: String,
    pub disposition: OverrideDisposition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectIndexManifestState {
    Building,
    Active,
    Failed,
}

impl ProjectIndexManifestState {
    pub fn parse(value: &str) -> Result<Self, cce_types::StorageError> {
        match value {
            "building" => Ok(Self::Building),
            "active" => Ok(Self::Active),
            "failed" => Ok(Self::Failed),
            _ => Err(cce_types::StorageError::Query(format!(
                "invalid project index manifest state: {value}"
            ))),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProjectIndexManifest {
    pub project_id: i64,
    pub publication_epoch: i64,
    pub data_epoch: i64,
    pub relation_epoch: i64,
    pub operation_id: String,
    pub state: ProjectIndexManifestState,
    pub input_fingerprint: Option<String>,
    pub candidate_ready: bool,
    pub parent_data_epoch: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GenerationGcPlan {
    pub stale_publication_epochs: Vec<i64>,
    pub stale_data_epochs: Vec<i64>,
    pub stale_relation_epochs: Vec<i64>,
    pub protected_data_epochs: Vec<i64>,
    pub protected_relation_epochs: Vec<i64>,
}

#[derive(Debug, Clone)]
pub struct AdmissionAuditRecord {
    pub token_fingerprint: String,
    pub projects: String,
    pub quota_bytes: Option<i64>,
    pub bytes_used: i64,
    pub admitted: i64,
    pub auth_rejections: i64,
    pub scope_rejections: i64,
    pub rate_rejections: i64,
    pub body_rejections: i64,
    pub quota_rejections: i64,
    pub last_used: Option<i64>,
    pub last_reject_reason: Option<String>,
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
