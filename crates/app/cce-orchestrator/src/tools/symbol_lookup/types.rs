//! Common types for symbol lookup tools
//!
//! This module defines the data structures used by find_references, get_symbols,
//! and goto_definition tools.

use serde::{Deserialize, Serialize};

use cce_types::{EntityId, EntityKind};

// ============================================================================
// Find References Types
// ============================================================================

/// Request for finding all references to a symbol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindReferencesRequest {
    /// File path containing the symbol
    pub path: String,
    /// Line number (1-based)
    pub line: usize,
    /// Column number (1-based, optional)
    pub column: Option<usize>,
    /// Symbol name (optional, for documentation)
    pub symbol: Option<String>,
    /// Number of context lines to include (optional)
    pub context_lines: Option<usize>,
    /// Whether to include code snippet for each reference
    pub include_snippet: Option<bool>,
    /// Whether to include caller entity information
    pub include_entity_info: Option<bool>,
}

/// A single reference location
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub snippet: Option<String>,
    /// Caller entity information (optional, when include_entity_info is true)
    pub caller_entity: Option<CallerEntityInfo>,
    /// Callee definition file path (if available from symbol snapshot)
    pub callee_file: Option<String>,
    /// Callee definition start line (1-based, if available from symbol snapshot)
    pub callee_line: Option<usize>,
    /// Callee definition end line (1-based, if available from symbol snapshot)
    pub callee_end_line: Option<usize>,
}

/// Information about the caller entity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallerEntityInfo {
    /// Entity name
    pub name: String,
    /// Entity kind
    pub kind: SymbolKind,
    /// Entity ID
    pub entity_id: EntityId,
}

/// References grouped by file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupedReferences {
    /// File path
    pub path: String,
    /// Number of references in this file
    pub count: usize,
    /// List of references
    pub references: Vec<ReferenceLocation>,
}

/// Response for find references operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindReferencesResponse {
    /// Symbol name (if provided)
    pub symbol: Option<String>,
    /// Total number of references
    pub total_count: usize,
    /// Number of files containing references
    pub file_count: usize,
    /// References grouped by file
    pub references: Vec<GroupedReferences>,
}

// ============================================================================
// Get Symbols Types
// ============================================================================

/// Request for getting symbols from files
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetSymbolsRequest {
    /// List of file paths
    pub paths: Vec<String>,
}

/// Symbol kind (maps to LSP SymbolKind)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SymbolKind {
    File,
    Module,
    Namespace,
    Package,
    Class,
    Method,
    Property,
    Field,
    Constructor,
    Enum,
    Interface,
    Function,
    Variable,
    Constant,
    String,
    Number,
    Boolean,
    Array,
    Object,
    Key,
    Null,
    EnumMember,
    Struct,
    Event,
    Operator,
}

impl From<EntityKind> for SymbolKind {
    fn from(kind: EntityKind) -> Self {
        match kind {
            EntityKind::Function => SymbolKind::Function,
            EntityKind::Method => SymbolKind::Method,
            EntityKind::Class => SymbolKind::Class,
            EntityKind::Struct => SymbolKind::Struct,
            EntityKind::Interface => SymbolKind::Interface,
            EntityKind::Enum => SymbolKind::Enum,
            EntityKind::Trait => SymbolKind::Interface,
            EntityKind::TraitImpl => SymbolKind::Interface,
            EntityKind::InherentImpl => SymbolKind::Struct,
            EntityKind::Variable => SymbolKind::Variable,
            EntityKind::Constant => SymbolKind::Constant,
            EntityKind::Field => SymbolKind::Field,
            EntityKind::Property => SymbolKind::Property,
            EntityKind::Constructor => SymbolKind::Constructor,
            EntityKind::Module => SymbolKind::Module,
            EntityKind::Namespace => SymbolKind::Namespace,
            EntityKind::Package => SymbolKind::Package,
            EntityKind::TypeAlias => SymbolKind::Class,
            EntityKind::EnumVariant => SymbolKind::EnumMember,
            EntityKind::Annotation => SymbolKind::Property,
            EntityKind::Macro => SymbolKind::Function,
            _ => SymbolKind::Variable,
        }
    }
}

/// Symbol information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolInfo {
    /// Symbol name
    pub name: String,
    /// Symbol kind
    pub kind: SymbolKind,
    /// Start line number (1-based)
    pub line: usize,
    /// End line number (1-based)
    pub end_line: usize,
    /// Detail information (signature)
    pub detail: Option<String>,
    /// Child symbols
    pub children: Option<Vec<SymbolInfo>>,
}

/// Result for a single file's symbols
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSymbolResult {
    /// File path
    pub path: String,
    /// Whether the operation succeeded
    pub success: bool,
    /// Number of symbols (if successful)
    pub symbol_count: Option<usize>,
    /// Symbol list (if successful)
    pub symbols: Option<Vec<SymbolInfo>>,
    /// Error message (if failed)
    pub error: Option<String>,
}

/// Response for get symbols operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetSymbolsResponse {
    /// Results for each file
    pub results: Vec<FileSymbolResult>,
    /// Number of successful operations
    pub success_count: usize,
    /// Number of failed operations
    pub fail_count: usize,
}

// ============================================================================
// Goto Definition Types
// ============================================================================

/// Request for goto definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GotoDefinitionRequest {
    /// File path
    pub path: String,
    /// Line number (1-based)
    pub line: usize,
    /// Column number (1-based, optional)
    pub column: Option<usize>,
    /// Symbol name (optional, for documentation)
    pub symbol: Option<String>,
    /// Whether to include the full definition body
    pub include_body: bool,
}

/// Definition location
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefinitionLocation {
    /// File path
    pub path: String,
    /// Entity ID
    pub entity_id: EntityId,
    /// Start line number (1-based)
    pub line: usize,
    /// End line number (1-based)
    pub end_line: usize,
}

/// Definition code with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefinitionCode {
    /// Definition location
    pub location: DefinitionLocation,
    /// Symbol name
    pub name: String,
    /// Symbol kind
    pub kind: SymbolKind,
    /// Definition code (signature only or full body)
    pub code: String,
    /// Signature
    pub signature: String,
}

/// Response for goto definition operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GotoDefinitionResponse {
    /// Symbol name (if provided)
    pub symbol: Option<String>,
    /// List of definitions (may be multiple for interface implementations)
    pub definitions: Vec<DefinitionCode>,
}

// ============================================================================
// Error Types
// ============================================================================

/// Error type for symbol lookup operations
#[derive(Debug, Clone, thiserror::Error)]
pub enum SymbolLookupError {
    /// Symbol not found
    #[error("Symbol not found")]
    SymbolNotFound,

    /// File not found
    #[error("File not found: {0}")]
    FileNotFound(String),

    /// File not readable
    #[error("File not readable: {0}")]
    FileNotReadable(String),

    /// Entity not found
    #[error("Entity not found: {0:?}")]
    EntityNotFound(EntityId),

    /// No symbol at position
    #[error("No symbol at position")]
    NoSymbolAtPosition,

    /// Parse error
    #[error("Parse error: {0}")]
    ParseError(String),

    /// Unknown language
    #[error("Unknown language")]
    UnknownLanguage,

    /// Index not available
    #[error("Index not available")]
    IndexNotAvailable,

    /// Internal error
    #[error("Internal error: {0}")]
    Internal(String),
}
