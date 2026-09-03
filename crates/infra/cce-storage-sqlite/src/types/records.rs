//! Core record types for SQLite tables.

use rusqlite::Row;
use serde::{Deserialize, Serialize};

use crate::helpers::FromRow;

/// Database ID type.
pub type DbId = i64;

/// File record.
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

/// Entity record.
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
}

/// Project record.
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

/// New project record (without ID and timestamps).
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

/// Project update record (partial update).
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

/// Entity detail mapping record.
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
        self.qdrant_point_ids =
            serde_json::to_string(point_ids).unwrap_or_else(|_| "[]".to_string());
        self.chunk_count = point_ids.len() as i64;
        self.updated_at = chrono::Utc::now().timestamp();
        self
    }

    pub fn with_bm25_doc_ids(mut self, doc_ids: &[String]) -> Self {
        self.bm25_doc_ids = serde_json::to_string(doc_ids).unwrap_or_else(|_| "[]".to_string());
        self.updated_at = chrono::Utc::now().timestamp();
        self
    }

    pub fn get_qdrant_point_ids(&self) -> Vec<String> {
        serde_json::from_str(&self.qdrant_point_ids).unwrap_or_default()
    }

    pub fn get_bm25_doc_ids(&self) -> Vec<String> {
        serde_json::from_str(&self.bm25_doc_ids).unwrap_or_default()
    }
}

/// Chunk record for storing code chunk content.
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
        }
    }

    pub fn with_bm25_keywords(mut self, keywords: impl Into<String>) -> Self {
        self.bm25_keywords = keywords.into();
        self.updated_at = chrono::Utc::now().timestamp();
        self
    }

    pub fn with_entity_ids(mut self, entity_ids: &[i64]) -> Self {
        self.entity_ids = serde_json::to_string(entity_ids).unwrap_or_else(|_| "[]".to_string());
        self.updated_at = chrono::Utc::now().timestamp();
        self
    }

    pub fn with_entity_ids_json(mut self, entity_ids_json: impl Into<String>) -> Self {
        self.entity_ids = entity_ids_json.into();
        self.updated_at = chrono::Utc::now().timestamp();
        self
    }

    pub fn with_entity_names(mut self, entity_names: &[String]) -> Self {
        self.entity_names =
            serde_json::to_string(entity_names).unwrap_or_else(|_| "[]".to_string());
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
        serde_json::from_str(&self.entity_ids).unwrap_or_default()
    }

    pub fn get_entity_names(&self) -> Vec<String> {
        serde_json::from_str(&self.entity_names).unwrap_or_default()
    }

    pub fn has_entity_id(&self, entity_id: i64) -> bool {
        self.get_entity_ids().contains(&entity_id)
    }
}

/// Statistics for summary generation operations.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SummaryGenerationStats {
    pub total: usize,
    pub completed: usize,
    pub failed: usize,
    pub total_duration_ms: i64,
    pub entry_count: usize,
}

impl SummaryGenerationStats {
    pub fn success_rate(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            (self.completed as f64 / self.total as f64) * 100.0
        }
    }
}

impl FromRow for FileRecord {
    fn from_row(row: &Row) -> Result<Self, rusqlite::Error> {
        Ok(FileRecord {
            id: row.get(0)?,
            path: row.get(1)?,
            language: row.get(2)?,
            category: row.get(3)?,
            last_modified: row.get(4)?,
            created_at: row.get(5)?,
            project_id: row.get(6)?,
            content_hash: row.get(7)?,
        })
    }
}

impl FromRow for EntityRecord {
    fn from_row(row: &Row) -> Result<Self, rusqlite::Error> {
        Ok(EntityRecord {
            id: row.get(0)?,
            name: row.get(1)?,
            kind: row.get(2)?,
            file_id: row.get(3)?,
            signature: row.get(4)?,
            span_start_row: row.get(5)?,
            span_end_row: row.get(6)?,
            span_start_column: row.get(7)?,
            span_end_column: row.get(8)?,
            span_start_byte: row.get(9)?,
            span_end_byte: row.get(10)?,
            scoped_name: row.get(11)?,
            depth: row.get(12)?,
            parent_id: row.get(13)?,
            metadata: row.get(14)?,
            parameters_json: row.get(15)?,
            return_type: row.get(16)?,
            doc_comment: row.get(17)?,
            modifiers_json: row.get(18)?,
            project_id: row.get(19)?,
            epoch: row.get(20)?,
            batch_id: row.get(21)?,
        })
    }
}

impl FromRow for ChunkRecord {
    fn from_row(row: &Row) -> Result<Self, rusqlite::Error> {
        Ok(ChunkRecord {
            chunk_id: row.get(0)?,
            file_path: row.get(1)?,
            content: row.get(2)?,
            start_line: row.get(3)?,
            end_line: row.get(4)?,
            entity_ids: row.get(5)?,
            entity_names: row.get(6)?,
            chunk_type: row.get(7)?,
            test_status: row.get::<_, u8>(8)?,
            test_source: row.get::<_, u8>(9)?,
            created_at: row.get(10)?,
            updated_at: row.get(11)?,
            project_id: row.get(12)?,
            epoch: row.get(13)?,
            batch_id: row.get(14)?,
            path: row.get(15)?,
            bm25_keywords: row.get(16)?,
            segment_id: row.get(17)?,
        })
    }
}

impl FromRow for EntityDetailMapping {
    fn from_row(row: &Row) -> Result<Self, rusqlite::Error> {
        Ok(EntityDetailMapping {
            id: row.get(0)?,
            entity_id: row.get(1)?,
            project_id: row.get(2)?,
            epoch: row.get(3)?,
            qdrant_point_ids: row.get(4)?,
            bm25_doc_ids: row.get(5)?,
            chunk_count: row.get(6)?,
            created_at: row.get(7)?,
            updated_at: row.get(8)?,
        })
    }
}

impl FromRow for ProjectRecord {
    fn from_row(row: &Row) -> Result<Self, rusqlite::Error> {
        Ok(ProjectRecord {
            id: row.get(0)?,
            name: row.get(1)?,
            root_path: row.get(2)?,
            config_file_path: row.get(3)?,
            language: row.get(4)?,
            extensions: row.get(5)?,
            exclude_dirs: row.get(6)?,
            respect_gitignore: row.get::<_, Option<i32>>(7)?.map(|v| v != 0),
            ignore_patterns: row.get(8)?,
            last_indexed: row.get(9)?,
            created_at: row.get(10)?,
            updated_at: row.get(11)?,
        })
    }
}
