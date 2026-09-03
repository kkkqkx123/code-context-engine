//! Tools API models

use serde::{Deserialize, Serialize};

/// Compress request
#[derive(Debug, Serialize, Deserialize)]
pub struct CompressRequest {
    pub file_path: String,
    #[serde(default)]
    pub include_entities: bool,
    #[serde(default)]
    pub include_groups: bool,
}

/// Compress response
#[derive(Debug, Serialize, Deserialize)]
pub struct CompressResponse {
    pub success: bool,
    pub compressed: String,
    pub original_size: usize,
    pub compressed_size: usize,
    pub ratio: f32,
}

/// Batch compress request
#[derive(Debug, Serialize, Deserialize)]
pub struct BatchCompressRequest {
    pub file_paths: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_entities: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_groups: Option<bool>,
    pub max_concurrency: usize,
}

/// Batch compress response
#[derive(Debug, Serialize, Deserialize)]
pub struct BatchCompressResponse {
    pub successes: Vec<(String, CompressResponse)>,
    pub failures: Vec<(String, String)>,
}

/// Diagnose request
#[derive(Debug, Serialize, Deserialize)]
pub struct DiagnoseRequest {
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
    #[serde(default)]
    pub include_ast: bool,
}

/// Diagnose API response
#[derive(Debug, Serialize, Deserialize)]
pub struct DiagnoseApiResponse {
    pub success: bool,
    pub issues: Vec<DiagnoseIssue>,
    #[serde(default)]
    pub error: Option<String>,
}

/// Diagnose issue
#[derive(Debug, Serialize, Deserialize)]
pub struct DiagnoseIssue {
    pub severity: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

/// Get symbols request
#[derive(Debug, Serialize, Deserialize)]
pub struct GetSymbolsRequest {
    pub project_id: i64,
    pub paths: Vec<String>,
}

/// Symbol information
#[derive(Debug, Serialize, Deserialize)]
pub struct SymbolInfo {
    /// Symbol name
    pub name: String,
    /// Symbol kind (LSP SymbolKind name)
    pub kind: String,
    /// Start line number (1-based)
    pub line: usize,
    /// End line number (1-based)
    pub end_line: usize,
    /// Detail information (signature)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Child symbols
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<SymbolInfo>>,
}

/// Result for a single file's symbols
#[derive(Debug, Serialize, Deserialize)]
pub struct FileSymbolResult {
    /// File path
    pub path: String,
    /// Whether the operation succeeded
    pub success: bool,
    /// Number of symbols (if successful)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol_count: Option<usize>,
    /// Symbol list (if successful)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbols: Option<Vec<SymbolInfo>>,
    /// Error message (if failed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Get symbols result payload (matches the orchestrator response)
#[derive(Debug, Serialize, Deserialize)]
pub struct GetSymbolsResult {
    /// Results for each file
    pub results: Vec<FileSymbolResult>,
    /// Number of successful operations
    pub success_count: usize,
    /// Number of failed operations
    pub fail_count: usize,
}

/// Get symbols API response
#[derive(Debug, Serialize, Deserialize)]
pub struct GetSymbolsResponse {
    pub success: bool,
    #[serde(default)]
    pub result: Option<GetSymbolsResult>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relation_info: Option<serde_json::Value>,
}

/// Find references request
#[derive(Debug, Serialize, Deserialize)]
pub struct FindReferencesRequest {
    pub project_id: i64,
    pub path: String,
    pub line: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_lines: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_snippet: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_entity_info: Option<bool>,
}

/// A single reference location
#[derive(Debug, Serialize, Deserialize)]
pub struct ReferenceLocation {
    /// File path
    pub path: String,
    /// Start line number (1-based)
    pub line: usize,
    /// Start column number (1-based)
    pub column: usize,
    /// End line number (1-based)
    pub end_line: usize,
    /// End column number (1-based)
    pub end_column: usize,
    /// Code snippet (optional, when include_snippet is true)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
    /// Caller entity information (optional, when include_entity_info is true)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller_entity: Option<CallerEntityInfo>,
    /// Callee definition file path (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callee_file: Option<String>,
    /// Callee definition start line (1-based, if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callee_line: Option<usize>,
    /// Callee definition end line (1-based, if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callee_end_line: Option<usize>,
}

/// Information about the caller entity
#[derive(Debug, Serialize, Deserialize)]
pub struct CallerEntityInfo {
    /// Entity name
    pub name: String,
    /// Entity kind (LSP SymbolKind name)
    pub kind: String,
    /// Entity ID
    pub entity_id: u64,
}

/// References grouped by file
#[derive(Debug, Serialize, Deserialize)]
pub struct GroupedReferences {
    /// File path
    pub path: String,
    /// Number of references in this file
    pub count: usize,
    /// List of references
    pub references: Vec<ReferenceLocation>,
}

/// Find references result payload (matches the orchestrator response)
#[derive(Debug, Serialize, Deserialize)]
pub struct FindReferencesResult {
    /// Symbol name (if provided)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Total number of references
    pub total_count: usize,
    /// Number of files containing references
    pub file_count: usize,
    /// References grouped by file
    pub references: Vec<GroupedReferences>,
}

/// Find references API response
#[derive(Debug, Serialize, Deserialize)]
pub struct FindReferencesResponse {
    pub success: bool,
    #[serde(default)]
    pub result: Option<FindReferencesResult>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relation_info: Option<serde_json::Value>,
}

/// Goto definition request
#[derive(Debug, Serialize, Deserialize)]
pub struct GotoDefinitionRequest {
    pub project_id: i64,
    pub path: String,
    pub line: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    #[serde(default)]
    pub include_body: bool,
}

/// Definition location
#[derive(Debug, Serialize, Deserialize)]
pub struct DefinitionLocation {
    /// File path
    pub path: String,
    /// Entity ID
    pub entity_id: u64,
    /// Start line number (1-based)
    pub line: usize,
    /// End line number (1-based)
    pub end_line: usize,
}

/// Definition code with metadata
#[derive(Debug, Serialize, Deserialize)]
pub struct DefinitionCode {
    /// Definition location
    pub location: DefinitionLocation,
    /// Symbol name
    pub name: String,
    /// Symbol kind (LSP SymbolKind name)
    pub kind: String,
    /// Definition code (signature only or full body)
    pub code: String,
    /// Signature
    pub signature: String,
}

/// Goto definition result payload (matches the orchestrator response)
#[derive(Debug, Serialize, Deserialize)]
pub struct GotoDefinitionResult {
    /// Symbol name (if provided)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// List of definitions (may be multiple for interface implementations)
    pub definitions: Vec<DefinitionCode>,
}

/// Goto definition API response
#[derive(Debug, Serialize, Deserialize)]
pub struct GotoDefinitionResponse {
    pub success: bool,
    #[serde(default)]
    pub result: Option<GotoDefinitionResult>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relation_info: Option<serde_json::Value>,
}

/// Keyword search request
#[derive(Debug, Serialize, Deserialize)]
pub struct KeywordSearchRequest {
    pub query: String,
    pub top_n: usize,
    pub project_id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub epoch: Option<String>,
}

/// Keyword search response
#[derive(Debug, Serialize, Deserialize)]
pub struct KeywordSearchResponse {
    pub success: bool,
    #[serde(default)]
    pub data: Option<KeywordSearchData>,
    #[serde(default)]
    pub error: Option<String>,
}

/// Keyword search data
#[derive(Debug, Serialize, Deserialize)]
pub struct KeywordSearchData {
    pub total: usize,
    pub results: Vec<KeywordSearchItem>,
}

/// Keyword search result item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeywordSearchItem {
    pub chunk_id: String,
    pub score: f32,
    pub file_path: String,
    pub title: String,
    pub highlighted_snippet: String,
    pub start_line: u32,
    pub end_line: u32,
}
