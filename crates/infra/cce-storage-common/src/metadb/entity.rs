//! Core file and entity record types.

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
