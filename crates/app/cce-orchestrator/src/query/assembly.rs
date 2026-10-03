//! SPSR-Graph assembly module
//!
//! This module provides SPSR-Graph (Structure-Preserving and Semantically-Reordered
//! Code Graph) assembly functionality for search results.
//!
//! # Architecture
//!
//! ```text
//! SPSRGraphAssembler (coordinator)
//!     │
//!     ├── SemanticUnitExtractor (extract complete code units)
//!     │       └── Read source files, extract by line range
//!     │
//!     ├── SegmentAggregator (aggregate adjacent segments)
//!     │       └── Merge adjacent segments, check file coverage
//!     │
//!     └── StructureConcatenator (assemble with structure)
//!             └── Add file markers, dedup
//! ```
//!
//! # Usage
//!
//! ```ignore
//! use crate::query::assembly::{SPSRGraphAssembler, SPSRGraphConfig, SearchResultInput};
//!
//! let config = SPSRGraphConfig::conservative();
//! let assembler = SPSRGraphAssembler::new(config);
//!
//! let input = SearchResultInput {
//!     id: "id".to_string(),
//!     entity_id: Some(entity_id),
//!     name: "function_name".to_string(),
//!     kind: "function".to_string(),
//!     file_path: "src/main.rs".to_string(),
//!     start_line: 10,
//!     end_line: 20,
//!     content: "fn foo() { ... }".to_string(),
//!     score: 0.95,
//! };
//! let result = assembler
//!     .assemble_single(input, Vec::new(), Vec::new())
//!     .await?;
//! ```
//!
//! # Relation expansion
//!
//! Relation expansion is caller-driven: `assemble_single` takes pre-resolved
//! forward (callee) and backward (caller) [`ExpandedUnit`]s and only performs
//! structure-preserving concatenation, dedup and budget capping. Graph
//! traversal itself lives on the caller side.
//!
//! The caller owns path/visibility policy: expansion units must be filtered
//! against the query's exclusion rules (path filters, epoch view) *before*
//! reaching `assemble_single`, so excluded neighbours never occupy an
//! expansion budget slot. The assembler additionally applies a defensive
//! second filter (stdlib, external, and non-call edges), orders each
//! direction by score with forward units filling the shared cap first, and
//! downgrades vanished files to references when a workspace root is
//! configured.
//!
//! Every result is capped by the single per-result quota; recall count is
//! owned by `assembly_top_n` and the score threshold, never by a batch token
//! cap.
//!
//! # Integration
//!
//! When `search.assembly.enable_assembly` is set, the searcher assembles the
//! final top-N results after ranking and thresholding (see
//! `searcher::post_processing`), leaving ordering untouched.

pub mod aggregator;
pub mod assembler;
pub mod concatenator;
pub mod error;
pub mod extractor;
pub mod types;

// Re-export main types
pub use aggregator::{AggregatedSegment, SegmentAggregator};
pub use assembler::SPSRGraphAssembler;
pub use concatenator::StructureConcatenator;
pub use error::{AssemblyError, Result};
pub use types::{
    AssembledResult, AssemblyMetadata, DedupStrategy, ExpandedUnit, ExpansionOrigin, FileInfo,
    SPSRGraphConfig, SearchResultInput, SemanticUnitType,
};
