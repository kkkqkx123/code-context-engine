//! Tools module for code analysis utilities
//!
//! This module provides tool APIs for code analysis operations that support
//! programming tasks. These tools offer functionality similar to LSP features
//! but operate on-demand without side effects.
//!
//! # Architecture
//!
//! ```text
//! Tools Module (code analysis tools)
//!     │
//!     ├── SymbolLookup (symbol lookup)
//!     │   ├── find_references() - Find all references to a symbol
//!     │   ├── goto_definition() - Navigate to symbol definition
//!     │   └── get_symbols() - List symbols in a file
//!     │
//!     ├── AstDiagnosis (AST diagnosis)
//!     │   ├── parse_and_diagnose() - Parse code and detect errors
//!     │   ├── collect_errors() - Collect syntax/semantic errors
//!     │   └── Multiple diagnosis strategies
//!     │
//!     ├── Compression (code compression)
//!     │   ├── compress_file() - Compress single file
//!     │   └── batch_compress() - Batch compression
//!     │       └── AST parsing → Grouping → NL conversion
//!     │
//!     ├── FileFold (stateless skeleton fold)
//!     │   └── fold() - Raw text → symbol skeleton
//!     │       └── Language resolve → Parse → Fold → Token accounting
//!     │
//!     └── KeywordSearch (keyword search)
//!         └── search() - BM25 keyword search with highlighted snippets
//!             └── BM25 → SQLite → highlight generation
//! ```
//!
//! # Available Tools
//!
//! - **Compression**: Semantic compression for code files (AST parsing, grouping, NL conversion)
//! - **AST Diagnosis**: Parse code snippets and diagnose syntax errors
//! - **File Fold**: Stateless symbol skeleton extraction from raw text (degrade, never error)
//! - **Symbol Lookup**: LSP-like functionality using internal indexes (find references, get symbols, goto definition)
//!
//! # Usage Example
//!
//! ```ignore
//! use code_context_engine::orchestrator::tools::{GotoDefinitionTool, FindReferencesTool};
//!
//! // Goto definition
//! let tool = GotoDefinitionTool::new(searcher);
//! let result = tool.execute(&request).await?;
//!
//! // Find references
//! let tool = FindReferencesTool::new(searcher);
//! let refs = tool.execute(&request).await?;
//! ```

pub mod ast_diagnosis;
pub mod compression;
pub mod file_fold;
pub mod keyword_search;

pub(crate) mod compression_types;
pub(crate) mod find_references;
pub(crate) mod get_symbols;
pub(crate) mod goto_definition;
pub(crate) mod keyword_search_types;
pub(crate) mod symbol_lookup_types;

pub use ast_diagnosis::{
    AstDiagnosis, DiagnosisError, DiagnosisRequest, DiagnosisResponse, Diagnostic, DiagnosticKind,
    DiagnosticPrecision,
};
pub use compression::{
    BatchCompressionRequest, BatchCompressionResponse, CompressionError, CompressionRequest,
    CompressionResponse, CompressionRetrieval,
};
pub use file_fold::{FileFoldMode, FileFoldRequest, FileFoldResponse, FileFoldTool};
pub use find_references::{FindReferencesConfig, FindReferencesTool};
pub use get_symbols::GetSymbolsTool;
pub use goto_definition::GotoDefinitionTool;
pub use keyword_search::{
    KeywordSearchError, KeywordSearchItem, KeywordSearchRequest, KeywordSearchResponse,
    KeywordSearchTool,
};
pub use symbol_lookup_types::{
    DefinitionCode, DefinitionLocation, FileSymbolResult, FindReferencesRequest,
    FindReferencesResponse, GetSymbolsRequest, GetSymbolsResponse, GotoDefinitionRequest,
    GotoDefinitionResponse, GroupedReferences, ReferenceLocation, SymbolInfo, SymbolKind,
    SymbolLookupError,
};
