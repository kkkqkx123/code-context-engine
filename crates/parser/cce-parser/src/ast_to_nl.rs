//! AST to Natural Language conversion module
//!
//! This module provides functionality for converting AST structures to natural language
//! descriptions, supporting dual-path output for BM25 and Embedding search.
//!
//! # Architecture
//!
//! The module is organized into three main components:
//!
//! ## Common (Shared Utilities)
//! - `common::normalizer` - Name normalization (snake_case, camelCase -> natural language)
//!
//! ## BM25 (Hybrid Enhanced Text)
//! - `bm25::generator` - BM25 text generator
//! - `bm25::templates` - BM25 templates (preserves key entities)
//! - `bm25::keyword_extractor` - Keyword extraction for BM25 indexing
//!
//! ## Embedding (Pure Semantic Summary)
//! - `embedding::generator` - Embedding semantic summary generator
//! - `embedding::templates` - Embedding templates (pure natural language)
//!
//! # Output Modes
//!
//! - `OutputMode::Bm25` - BM25 hybrid enhanced text (preserves key entities)
//! - `OutputMode::Embedding` - Pure semantic summary (removes all code symbols)
//! - `OutputMode::Both` - Both outputs for dual-path indexing
//!
//! # Example
//!
//! ```no_run
//! use cce_parser::ast_to_nl::{AstToNlConverter, ConversionRequest, OutputMode};
//!
//! let converter = AstToNlConverter::new();
//! // Use convert_entity_groups with parsed entity groups from the parser
//! // let results = converter.convert_entity_groups(&groups, file_path, None);
//! ```

// Core public API modules
pub mod bm25;
pub mod code_form_converter;
pub mod converter;
pub mod embedding;
pub mod noise;

// Internal implementation modules (crate-only access)
pub mod chunker;
pub(crate) mod common;
pub(crate) mod error;
pub(crate) mod options;

// Note: converter is now a directory module (converter/mod.rs)

/// Version tag of the AST-to-NL pipeline implementation (converter, group
/// conversion and chunker behavior).
///
/// Part of both the storage drift-detection fingerprint and the parse
/// artifact cache key: bump this whenever the pipeline output changes in a
/// way not expressed by `AstToNlConfig`, so cached conversion outputs are
/// invalidated together with the code change.
pub const PIPELINE_VERSION: &str = "1";

// Re-export main types
pub use code_form_converter::{CodeFormContext, CodeFormConverter, CodeFormEntity, CodeFormGroup};
pub use converter::AstToNlConverter;
pub use error::{AstToNlError, ConversionContext};

// Re-export internal types
pub use cce_types::ast_to_nl::EntityMetadata;
pub use options::{ConversionOptions, ConversionRequest};
// Re-export from chunker
pub use chunker::{
    ChunkBoundary, ChunkMetadata, ChunkedResult, ChunkingConfig, CodeSpecificMetadata,
    GroupChunker, GroupRelation, GroupRelationType, GroupTracker, OverlapManager, OverlapRegion,
    OverlapType, SplitReason, SplitStrategy, TextSplitter,
};

// Re-export from common for convenience
pub use common::NameNormalizer;
pub use common::TemplateHelpers;
pub use common::clean_comment_content;
pub use common::safe_utf8_boundary;

// Re-export from bm25
pub use bm25::{Bm25Generator, KeywordExtractor};

// Re-export from embedding
pub use embedding::EmbeddingGenerator;

// Re-export from noise
pub use noise::NoiseProfile;

// Re-export output mode
pub use cce_types::OutputMode;
