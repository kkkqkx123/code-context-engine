//! Symbol lookup tools module
//!
//! This module provides LSP-like functionality using the internal relation index
//! instead of external language servers. It offers three main tools:
//!
//! - **FindReferences**: Find all references to a symbol
//! - **GetSymbols**: Get all symbols in a file
//! - **GotoDefinition**: Jump to the definition of a symbol
//!
//! # Architecture
//!
//! These tools leverage the existing RelationIndex infrastructure:
//! - `callee_index` for O(1) reverse lookups (find references)
//! - `function_index` for entity storage (get symbols)
//! - `entity_file_index` for file-entity mapping
//! - `resolved_relation_index` for call relationships
//!
//! # Performance
//!
//! All operations are O(1) or O(log n) thanks to the DashMap-based indexes:
//! - Find references: O(1) lookup in callee_index
//! - Get symbols: O(n) where n is entities in file
//! - Goto definition: O(1) lookup in function_index

mod find_references;
mod get_symbols;
mod goto_definition;
mod types;
mod utils;

// Re-export main types
pub use find_references::{FindReferencesConfig, FindReferencesTool};
pub use get_symbols::GetSymbolsTool;
pub use goto_definition::GotoDefinitionTool;

// Re-export all types from types module
pub use types::{
    DefinitionCode, DefinitionLocation, FileSymbolResult, FindReferencesRequest,
    FindReferencesResponse, GetSymbolsRequest, GetSymbolsResponse, GotoDefinitionRequest,
    GotoDefinitionResponse, GroupedReferences, ReferenceLocation, SymbolInfo, SymbolKind,
    SymbolLookupError,
};

// Re-export utility functions
pub use utils::{
    contains_position, extract_context_lines, find_entity_at_position, format_position, format_span,
};
