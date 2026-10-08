//! Core record types for SQLite tables.

use rusqlite::Row;
use serde::{Deserialize, Serialize};

use crate::helpers::FromRow;

pub use cce_storage_common::relation::types::{
    ChunkRecord, DbId, EntityDetailMapping, EntityRecord, FileRecord, NewProjectRecord,
    ProjectRecord, ProjectUpdateRecord,
};

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
            rank: row.get(22)?,
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
            truncated: row.get::<_, u8>(18)?,
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
