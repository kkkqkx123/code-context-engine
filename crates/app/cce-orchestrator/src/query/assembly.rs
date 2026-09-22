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
//! let result = assembler.assemble_single(input).await?;
//! ```
//!
//! # Status (dormant)
//!
//! This module is currently disconnected from the search pipeline: the
//! `ExecutionStrategy::WithAssembly` entry point and the `Searcher`
//! assembly handler were removed because their benefit was never validated
//! (see `docs/plan/tests/assembly-review-export-design.md`). The module is
//! kept for the offline assembly-review example in `cce-e2e-tests`, which
//! generates per-query assembled outputs for manual inspection. Once that
//! review produces a verdict, either re-integrate the module behind a
//! config gate or delete it entirely.
//!
//! The `#[allow(dead_code)]` below suppresses the resulting unused warnings
//! and MUST be removed together with the final resolution.

// Dormant module: kept only for the offline assembly-review example.
// See the "Status (dormant)" section above for the resolution plan.
#![allow(dead_code)]

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
    AssembledResult, AssemblyMetadata, DedupStrategy, ExpandedUnit, FileInfo, SPSRGraphConfig,
    SearchResultInput, SemanticUnitType, TruncationStrategy, UnitDeduplicator,
};
