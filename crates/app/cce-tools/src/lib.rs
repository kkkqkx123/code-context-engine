//! Code analysis tools over the storage contracts.
//!
//! These tools offer LSP-like functionality operating on demand without side
//! effects: symbol lookup (find references, goto definition, get symbols),
//! BM25 keyword search with raw source snippets, AST diagnosis, semantic
//! compression, and stateless file folding.
//!
//! All reads go through the backend-neutral storage contracts; no tool
//! touches a concrete connection or transaction. Display-quality rendering
//! stays in the export crate behind the [`render::SemanticRenderer`] port,
//! and project-state checks arrive as injected ports instead of being
//! created here.

pub mod ast_diagnosis;
pub mod common;
pub mod compression;
pub mod epoch_view;
pub mod file_fold;
pub mod find_references;
pub mod get_symbols;
pub mod goto_definition;
pub mod keyword_search;
pub mod render;
pub mod store;
pub mod symbol_lookup;
pub mod symbol_lookup_types;

pub use ast_diagnosis::{
    AstDiagnosis, DiagnosisError, DiagnosisRequest, DiagnosisResponse, Diagnostic, DiagnosticKind,
    DiagnosticPrecision,
};
pub use compression::{
    BatchCompressionRequest, BatchCompressionResponse, CompressionError, CompressionRequest,
    CompressionResponse, CompressionRetrieval,
};
pub use epoch_view::{EpochView, load_epoch_view};
pub use file_fold::{
    FileFoldBatchError, FileFoldBatchItem, FileFoldBatchItemResponse, FileFoldBatchRequest,
    FileFoldBatchResponse, FileFoldBatchResult, FileFoldBatchStats, FileFoldMode, FileFoldRequest,
    FileFoldResponse, FileFoldTool, MAX_FOLD_BATCH_BYTES, MAX_FOLD_BATCH_ITEMS,
};
pub use find_references::{FindReferencesConfig, FindReferencesTool};
pub use get_symbols::GetSymbolsTool;
pub use goto_definition::GotoDefinitionTool;
pub use keyword_search::{
    KeywordSearchError, KeywordSearchItem, KeywordSearchRequest, KeywordSearchResponse,
    KeywordSearchTool,
};
pub use render::{PlainSemanticRenderer, SemanticRenderer, SharedRenderer};
pub use store::{get_chunk_records, read_snippets_batch, resolve_project_root};
pub use symbol_lookup_types::{
    DefinitionCode, DefinitionLocation, FileSymbolResult, FindReferencesRequest,
    FindReferencesResponse, GetSymbolsRequest, GetSymbolsResponse, GotoDefinitionRequest,
    GotoDefinitionResponse, GroupedReferences, ReferenceLocation, SymbolInfo, SymbolKind,
    SymbolLookupError,
};

/// Project-state check injected into tools that adapt to indexed projects.
///
/// Tools never construct the tracker; the orchestrator injects an
/// implementation bound to its own state.
#[async_trait::async_trait]
pub trait IndexStateCheck: Send + Sync {
    /// Whether the file at `path` is fully updated in the index.
    async fn is_fully_updated(&self, path: &std::path::Path) -> bool;
}
