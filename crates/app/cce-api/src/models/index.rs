//! Index management models

use serde::{Deserialize, Serialize};

/// Index request
#[derive(Debug, Serialize, Deserialize)]
pub struct IndexRequest {
    pub project_id: Option<i64>,
    pub path: String,
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default)]
    pub exclude_dirs: Vec<String>,
    #[serde(default = "default_true")]
    pub respect_gitignore: bool,
    #[serde(default)]
    pub custom_gitignore: Option<String>,
}

/// Index response
#[derive(Debug, Serialize, Deserialize)]
pub struct IndexResponse {
    pub success: bool,
    pub files_scanned: usize,
    pub files_indexed: usize,
    pub failed_files: usize,
    pub total_entities: usize,
    pub total_relations: usize,
    pub total_vectors: usize,
    pub elapsed_ms: u64,
    pub message: String,
    #[serde(default)]
    pub errors: Vec<String>,
}

/// Incremental index request
#[derive(Debug, Serialize, Deserialize)]
pub struct IncrementalIndexRequest {
    pub project_id: i64,
    #[serde(default)]
    pub files_to_index: Vec<String>,
    #[serde(default)]
    pub files_to_remove: Vec<String>,
    #[serde(default)]
    pub force_reindex: bool,
}

/// Incremental index response
#[derive(Debug, Serialize, Deserialize)]
pub struct IncrementalIndexResponse {
    pub success: bool,
    pub files_indexed: usize,
    pub files_removed: usize,
    pub total_entities: usize,
    pub total_vectors: usize,
    pub elapsed_ms: u64,
    #[serde(default)]
    pub errors: Vec<String>,
}

/// Parse request
#[derive(Debug, Serialize, Deserialize)]
pub struct ParseRequest {
    pub file_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

/// Parse response
#[derive(Debug, Serialize, Deserialize)]
pub struct ParseResponse {
    pub success: bool,
    pub file_path: String,
    pub language: String,
    pub encoding: String,
    pub entities: Vec<EntityInfo>,
    pub relations: Vec<RelationInfo>,
    pub elapsed_ms: u64,
}

/// Entity info from parse
#[derive(Debug, Serialize, Deserialize)]
pub struct EntityInfo {
    pub id: u64,
    pub kind: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    pub start_line: u32,
    pub end_line: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc_comment: Option<String>,
}

/// Relation info from parse
#[derive(Debug, Serialize, Deserialize)]
pub struct RelationInfo {
    pub caller_id: u64,
    pub callee_id: u64,
    pub relation_type: String,
    pub line: u32,
}

/// Index statistics response
#[derive(Debug, Serialize, Deserialize)]
pub struct IndexStatsResponse {
    pub success: bool,
    pub statistics: IndexStatistics,
    pub elapsed_ms: u64,
}

/// Index statistics
#[derive(Debug, Serialize, Deserialize)]
pub struct IndexStatistics {
    pub total_entities: usize,
    pub total_relations: usize,
    pub total_vectors: usize,
    pub total_bm25_documents: usize,
    pub total_files: usize,
}

/// Clear index request
#[derive(Debug, Serialize, Deserialize)]
pub struct ClearIndexRequest {
    pub project_id: i64,
    #[serde(default)]
    pub vectors: bool,
    #[serde(default)]
    pub bm25: bool,
    #[serde(default)]
    pub relations: bool,
    #[serde(default)]
    pub cache: bool,
}

/// Clear index response
#[derive(Debug, Serialize, Deserialize)]
pub struct ClearIndexResponse {
    pub success: bool,
    pub project_id: i64,
    pub backends: Vec<BackendResultInfo>,
    pub elapsed_ms: u64,
    pub message: String,
}

/// Backend result info
#[derive(Debug, Serialize, Deserialize)]
pub struct BackendResultInfo {
    pub backend: String,
    pub ok: bool,
    pub detail: String,
}

/// Delete file response
#[derive(Debug, Serialize, Deserialize)]
pub struct DeleteFileResponse {
    pub success: bool,
    pub message: String,
    pub vectors_deleted: usize,
    pub bm25_documents_deleted: usize,
    pub relations_deleted: usize,
    pub elapsed_ms: u64,
}

/// Delete entity response
#[derive(Debug, Serialize, Deserialize)]
pub struct DeleteEntityResponse {
    pub success: bool,
    pub message: String,
    pub entity_id: u64,
    pub vectors_deleted: usize,
    pub bm25_documents_deleted: usize,
    pub relations_deleted: usize,
    pub elapsed_ms: u64,
}

/// Batch delete request
#[derive(Debug, Serialize, Deserialize)]
pub struct BatchDeleteRequest {
    #[serde(default)]
    pub file_paths: Vec<String>,
    #[serde(default)]
    pub entity_ids: Vec<u64>,
}

/// Batch delete response
#[derive(Debug, Serialize, Deserialize)]
pub struct BatchDeleteResponse {
    pub success: bool,
    pub files_deleted: usize,
    pub entities_deleted: usize,
    #[serde(default)]
    pub errors: Vec<String>,
    pub elapsed_ms: u64,
}

/// Summary request
#[derive(Debug, Serialize, Deserialize)]
pub struct SummaryRequest {
    #[serde(default)]
    pub file_paths: Vec<String>,
    #[serde(default)]
    pub directory_paths: Vec<String>,
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default)]
    pub exclude_dirs: Vec<String>,
    #[serde(default = "default_true")]
    pub respect_gitignore: bool,
    #[serde(default)]
    pub ignore_patterns: Vec<String>,
    #[serde(default = "default_true")]
    pub recursive: bool,
    #[serde(default = "default_max_files")]
    pub max_files: usize,
}

/// Summary response
#[derive(Debug, Serialize, Deserialize)]
pub struct SummaryResponse {
    pub success: bool,
    pub total_files: usize,
    pub success_count: usize,
    pub failed_count: usize,
    pub summaries: Vec<FileSummaryItem>,
    pub elapsed_ms: u64,
    #[serde(default)]
    pub warnings: Vec<String>,
}

/// File summary item
#[derive(Debug, Serialize, Deserialize)]
pub struct FileSummaryItem {
    pub file_path: String,
    pub language: String,
    pub summary: String,
    pub main_entities: Vec<String>,
    pub imports: Vec<String>,
    pub exports: Vec<String>,
    pub entity_count: u32,
    pub line_count: u32,
    pub tags: Vec<String>,
    pub importance_level: String,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn default_true() -> bool {
    true
}

fn default_max_files() -> usize {
    100
}
