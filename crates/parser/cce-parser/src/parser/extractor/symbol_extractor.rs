//! Symbol extraction layer
//!
//! Provides language-agnostic traits and language-specific implementations
//! for extracting import/export information from source files.
//!
//! # Architecture
//!
//! - `traits`: Core extraction traits and extractor creation function
//! - `common`: Shared types and utilities
//! - Language-specific modules:
//!   - `c`: C language
//!   - `cpp`: C++ language
//!   - `csharp`: C# language
//!   - `javascript`: JavaScript/JSX language
//!   - `typescript`: TypeScript/TSX language
//!   - `rust`: Rust language
//!   - `go`: Go language
//!   - `python`: Python language
//!   - `php`: PHP language
//!   - `ruby`: Ruby language
//!   - `dart`: Dart language
//!   - `java`: Java language
//!   - `kotlin`: Kotlin language
//!   - `scala`: Scala language

pub mod common;
pub mod plugin_extractor;
pub mod traits;

// Language-specific extractors
pub mod c;
pub mod cpp;
pub mod csharp;
pub mod dart;
pub mod go;
pub mod java;
pub mod javascript;
pub mod kotlin;
pub mod php;
pub mod python;
pub mod ruby;
pub mod rust;
pub mod scala;
pub mod typescript;

// Re-export standardized types from cce_core
pub use cce_types::import::{
    ClassificationMetadata, ExportKind, ExportTarget, ImportClass, ImportClassification,
    ImportKind, ImportTarget, StandardizedExport, StandardizedExportTable, StandardizedImport,
    StandardizedImportTable, TargetKind,
};

// Re-export traits
pub use traits::{
    SymbolExtractor, create_context_with_package, create_extractor, create_extractor_with_registry,
    extract_from_file, extract_package_from_file,
};
