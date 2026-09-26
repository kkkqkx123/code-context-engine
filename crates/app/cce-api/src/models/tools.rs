//! Tools API models
//!
//! These models are the wire contract for the on-demand tool endpoints.
//! Orchestrator result types are converted into these shapes by the
//! server handlers; domain types never leak onto the wire.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

// ============================================================================
// Compression
// ============================================================================

/// Compress request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CompressRequest {
    pub file_path: String,
    #[serde(default)]
    pub include_entities: bool,
    #[serde(default)]
    pub include_groups: bool,
}

/// Semantic compression result
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CompressResult {
    pub file_path: String,
    pub language: String,
    /// File hash (SHA-256)
    pub file_hash: String,
    /// Whether the result came from cache
    pub from_cache: bool,
    /// Entity list (free-form parser entities, present when requested)
    #[serde(default)]
    #[schema(value_type = Option<Object>)]
    pub entities: Option<serde_json::Value>,
    /// Entity group list (free-form grouper groups, present when requested)
    #[serde(default)]
    #[schema(value_type = Option<Object>)]
    pub groups: Option<serde_json::Value>,
    /// Semantic summary for human/LLM consumption
    pub semantic_text: String,
}

/// Single file compression response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CompressApiResponse {
    pub success: bool,
    #[serde(default)]
    pub result: Option<CompressResult>,
    #[serde(default)]
    pub error: Option<String>,
}

/// Batch compress request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BatchCompressRequest {
    pub file_paths: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_entities: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_groups: Option<bool>,
    #[serde(default = "default_max_concurrency")]
    pub max_concurrency: usize,
}

/// One successful entry of a batch compression
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BatchCompressSuccess {
    pub path: String,
    pub result: CompressResult,
}

/// One failed entry of a batch compression
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BatchCompressFailure {
    pub path: String,
    pub error: String,
}

/// Batch compression response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BatchCompressResponse {
    pub successes: Vec<BatchCompressSuccess>,
    pub failures: Vec<BatchCompressFailure>,
}

// ============================================================================
// AST diagnosis
// ============================================================================

/// Diagnose request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DiagnoseRequest {
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
    #[serde(default)]
    pub include_ast: bool,
}

/// AST diagnosis result
#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct DiagnoseResult {
    /// Detected or specified programming language
    pub language: String,
    /// Whether the code is valid (no syntax errors)
    pub is_valid: bool,
    /// AST structure (only when include_ast=true)
    pub ast: Option<AstNodeInfo>,
    /// Diagnostic issues
    pub diagnostics: Vec<DiagnosticEntry>,
}

/// AST diagnosis response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DiagnoseApiResponse {
    pub success: bool,
    #[serde(default)]
    pub result: Option<DiagnoseResult>,
    #[serde(default)]
    pub error: Option<String>,
}

/// AST node in the diagnosis result
#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
#[schema(no_recursion)]
pub struct AstNodeInfo {
    /// Node kind (tree-sitter node type)
    pub kind: String,
    /// Source code text of this node
    pub text: String,
    /// Node span
    pub span: SpanInfo,
    /// Child nodes
    pub children: Vec<AstNodeInfo>,
}

/// AST diagnostic entry
#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct DiagnosticEntry {
    /// Issue type (e.g. "UnclosedString")
    pub kind: String,
    /// Error position (start)
    pub position: PositionInfo,
    /// Error span (for range information)
    pub span: Option<SpanInfo>,
    /// Error message
    pub message: String,
    /// Positioning precision ("High", "Medium", "Low")
    pub precision: String,
}

/// Line/column position (0-indexed)
#[derive(Debug, Serialize, Deserialize, Clone, Copy, ToSchema)]
pub struct PositionInfo {
    pub row: usize,
    pub column: usize,
}

/// Source span with byte and line/column bounds
#[derive(Debug, Serialize, Deserialize, Clone, Copy, ToSchema)]
pub struct SpanInfo {
    pub start_byte: usize,
    pub end_byte: usize,
    pub start_position: PositionInfo,
    pub end_position: PositionInfo,
}

// ============================================================================
// File fold (stateless skeleton extraction)
// ============================================================================

/// File fold request
///
/// Carries raw text plus language hints plus caller token budget.
/// Language resolution is explicit language first, then file-name suffix,
/// then unknown (degraded, never an error).
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct FoldRequest {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
}

/// File fold response
///
/// Degrade-not-error: unknown language, parse failure, over-limit and empty
/// input all return this shape with a truncated text and
/// `structure_known=false`.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct FoldResponse {
    pub success: bool,
    pub folded_text: String,
    pub language: String,
    pub structure_known: bool,
    pub original_tokens: usize,
    pub folded_tokens: usize,
    pub kept_sections: usize,
    pub dropped_sections: usize,
}

// ============================================================================
// Symbol lookup (LSP-like, project-scoped)
// ============================================================================

/// Get symbols request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GetSymbolsRequest {
    pub project_id: i64,
    pub paths: Vec<String>,
}

/// Symbol information
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(no_recursion)]
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
#[derive(Debug, Serialize, Deserialize, ToSchema)]
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

/// Get symbols result payload
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GetSymbolsResult {
    /// Results for each file
    pub results: Vec<FileSymbolResult>,
    /// Number of successful operations
    pub success_count: usize,
    /// Number of failed operations
    pub fail_count: usize,
}

/// Get symbols API response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GetSymbolsResponse {
    pub success: bool,
    #[serde(default)]
    pub result: Option<GetSymbolsResult>,
    #[serde(default)]
    pub error: Option<String>,
    /// Relation capability state when the index is degraded
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<Object>)]
    pub relation_info: Option<serde_json::Value>,
}

/// Find references request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
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
#[derive(Debug, Serialize, Deserialize, ToSchema)]
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
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CallerEntityInfo {
    /// Entity name
    pub name: String,
    /// Entity kind (LSP SymbolKind name)
    pub kind: String,
    /// Entity ID
    pub entity_id: u64,
}

/// References grouped by file
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GroupedReferences {
    /// File path
    pub path: String,
    /// Number of references in this file
    pub count: usize,
    /// List of references
    pub references: Vec<ReferenceLocation>,
}

/// Find references result payload
#[derive(Debug, Serialize, Deserialize, ToSchema)]
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
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct FindReferencesResponse {
    pub success: bool,
    #[serde(default)]
    pub result: Option<FindReferencesResult>,
    #[serde(default)]
    pub error: Option<String>,
    /// Relation capability state when the index is degraded
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<Object>)]
    pub relation_info: Option<serde_json::Value>,
}

/// Goto definition request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
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
#[derive(Debug, Serialize, Deserialize, ToSchema)]
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
#[derive(Debug, Serialize, Deserialize, ToSchema)]
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

/// Goto definition result payload
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GotoDefinitionResult {
    /// Symbol name (if provided)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// List of definitions (may be multiple for interface implementations)
    pub definitions: Vec<DefinitionCode>,
}

/// Goto definition API response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GotoDefinitionResponse {
    pub success: bool,
    #[serde(default)]
    pub result: Option<GotoDefinitionResult>,
    #[serde(default)]
    pub error: Option<String>,
    /// Relation capability state when the index is degraded
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<Object>)]
    pub relation_info: Option<serde_json::Value>,
}

// ============================================================================
// Keyword search (BM25)
// ============================================================================

/// Operator for combining multiple query terms
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum KeywordTermOperator {
    /// Match any term (OR semantics)
    #[default]
    Or,
    /// Match all terms (AND semantics)
    And,
}

/// Keyword search request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct KeywordSearchRequest {
    /// Search query text (must be non-empty)
    pub query: String,
    /// Maximum number of results to return (must be > 0)
    pub top_n: usize,
    /// Project ID for scoped search
    pub project_id: i64,
    /// Optional epoch for version-aware filtering
    #[serde(default)]
    pub epoch: Option<i64>,
    /// Operator for combining multiple query terms
    #[serde(default)]
    pub term_operator: KeywordTermOperator,
}

/// A single keyword search result with highlighted snippet
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct KeywordSearchItem {
    /// Chunk/document ID
    pub chunk_id: String,
    /// BM25 relevance score
    pub score: f32,
    /// File path containing the match
    pub file_path: String,
    /// Entity/function title
    pub title: String,
    /// Highlighted code snippet (HTML with <mark> tags)
    pub highlighted_snippet: String,
    /// Start line in the file
    pub start_line: u32,
    /// End line in the file
    pub end_line: u32,
}

/// Keyword search result payload
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct KeywordSearchResult {
    /// The original query
    pub query: String,
    /// Total number of results returned
    pub total: usize,
    /// Search results with highlighted snippets
    pub results: Vec<KeywordSearchItem>,
}

/// Keyword search API response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct KeywordSearchApiResponse {
    pub success: bool,
    #[serde(default)]
    pub result: Option<KeywordSearchResult>,
    #[serde(default)]
    pub error: Option<String>,
}

fn default_max_concurrency() -> usize {
    4
}
