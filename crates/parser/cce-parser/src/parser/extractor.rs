//! Extractor module for semantic entity and relation extraction
//!
//! This module provides functionality for:
//! - Extracting semantic entities (functions, classes, etc.) from AST
//! - Extracting raw relations (calls, dependencies) between entities
//! - Managing extraction context (entity stack, depth tracking)
//! - Parsing embedded blocks in SFC files
//! - Extracting import/export symbols (symbol_extractor)
//!
//! # Architecture
//!
//! - **EntityExtractor**: Extracts semantic entities from AST nodes
//! - **RelationExtractor**: Extracts raw (unresolved) relations between entities
//! - **ExtractionContext**: Manages entity stack and depth during extraction
//! - **EmbeddedParser**: Parses embedded blocks in SFC files
//! - **SymbolExtractor**: Extracts import/export symbols from source files
//!
//! # Design Principles
//!
//! - **Semantic Abstraction**: Entity is not AST wrapper, but cross-language semantic concept
//! - **Information Completeness**: Extract all needed info in one pass
//! - **Stateless Output**: Output structures are self-contained, no AST dependency
//! - **Extraction Only**: This module only extracts raw data, resolution is handled by relation module
//!
//! # Note on Cross-Block Relations
//!
//! Cross-block relations in SFC files are resolved by the parser layer.
//! See `crate::parser::coordinator` for details.

pub mod annotation_handler;
mod behavior_extractor;
pub mod capture;
pub mod context;
mod control_flow_extractor;
mod embedded;
mod entity_extractor;
mod macro_body_extractor;
pub mod namespace_policy;
pub mod parent_child_resolver;
pub(crate) mod post_processing;
mod relation_extractor;
mod structural_extractor;
pub(crate) mod symbol_extractor;
pub mod utils;

// Re-export main types from types module
pub use cce_types::{
    Entity, EntityId, EntityKind, ParsedFile, RawRelationData, Relation, RelationTarget,
    RelationType, Span,
};

// Re-export extractor implementations
pub use behavior_extractor::BehaviorExtractor;
pub use context::ExtractionContext;
pub use control_flow_extractor::ControlFlowExtractor;
pub use embedded::EmbeddedParser;
pub use entity_extractor::EntityExtractor;
pub use macro_body_extractor::MacroBodyExtractor;
pub use relation_extractor::RelationExtractor;
pub use structural_extractor::StructuralExtractor;

// Re-export symbol extractor types
pub use symbol_extractor::{
    ClassificationMetadata, ExportKind, ExportTarget, ImportClass, ImportClassification,
    ImportKind, ImportTarget, StandardizedExport, StandardizedExportTable, StandardizedImport,
    StandardizedImportTable, SymbolExtractor, TargetKind, create_context_with_package,
    create_extractor, create_extractor_with_registry, extract_from_file, extract_package_from_file,
};

// Re-export utility functions
pub use utils::{
    capture_name_contains, capture_name_ends_with, create_span_from_capture,
    extract_text_from_source, find_capture_by_name,
};
