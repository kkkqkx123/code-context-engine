//! Core file and entity record types.

use serde::{Deserialize, Serialize};

pub type DbId = i64;

/// Visible content hash of one file in the active generation view.
///
/// Rows whose stored hash is `None` are still visible paths (they count
/// for change detection) but carry no hash for comparison.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileHashEntry {
    pub path: String,
    pub content_hash: Option<String>,
}

/// File classification row: path identity with its batch and content hash.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileEpochEntry {
    pub path: String,
    pub batch_id: i64,
    pub content_hash: Option<String>,
}

/// File row ensured by an indexing batch without touching content hashes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileEnsureEntry {
    pub path: String,
    pub language: String,
    pub category: u8,
    pub last_modified: i64,
}

/// Entity row together with its owning file path.
///
/// Backs vector-side detail mapping and symbol-tool resolution without
/// exposing a join over two roundtrips.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityWithFile {
    pub entity: EntityRecord,
    pub file_path: String,
}

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
    /// Overload discriminator aligning the data-side natural key with the
    /// relation-side `(scoped_name, kind, overload_discriminator)` identity.
    /// Empty when the entity has no overload siblings.
    #[serde(default)]
    pub overload_discriminator: String,
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
