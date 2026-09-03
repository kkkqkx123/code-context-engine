//! Parser module for code parsing and analysis
//!
//! This module provides functionality for:
//! - Language detection
//! - AST parsing using tree-sitter
//! - Semantic entity extraction
//! - Raw relation extraction (unresolved)
//! - Import/export symbol extraction
//!
//! Note: Code splitting/chunking is NOT handled here.
//! That responsibility belongs to the processor/embedder modules.
//!
//! # Architecture
//!
//! The module uses a pipeline-based architecture with clear separation of concerns:
//!
//! ```text
//! ParseCoordinator (Top-level entry point)
//!   ├── Components (shared components)
//!   └── ParseContext (execution context)
//! ```
//!
//! # Responsibility Boundaries
//!
//! This module only handles single-file parsing:
//! - Cross-file reference resolution is handled by the `relation` module
//! - Project-level indexing is handled by the `relation` module
//! - Call classification is handled by the `relation` module
//! - Raw relations contain only target names, resolution happens in relation::IndexBuilder

// Core public API modules
pub mod ast_parser;
pub mod components;
pub mod context;
pub mod coordinator;
pub mod extractor;
pub mod pipeline;
pub mod stages;

// Internal implementation modules (crate-only access)
pub(crate) mod comment_processor;
pub(crate) mod embedded_types;
pub(crate) mod helpers;
pub(crate) mod language_detector;
pub(crate) mod stdlib;

// Re-export main types
pub use coordinator::ParseCoordinator;

// Re-export component types
pub use ast_parser::AstParser;
pub use embedded_types::{
    BlockRelation, BlockRelationType, BlockType, CssInJsBlock, CssInJsCollection, CssInJsLibrary,
    EmbeddedBlock, EmbeddedParseConfig,
};
pub use language_detector::LanguageDetector;

// Re-export extractor types
pub use extractor::{
    ClassificationMetadata, EmbeddedParser, EntityExtractor, ExportKind, ExportTarget,
    ExtractionContext, ImportClass, ImportClassification, ImportKind, ImportTarget,
    RelationExtractor, StandardizedExport, StandardizedExportTable, StandardizedImport,
    StandardizedImportTable, SymbolExtractor, TargetKind, create_context_with_package,
    create_extractor, create_extractor_with_registry, extract_from_file, extract_package_from_file,
};
