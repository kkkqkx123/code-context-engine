//! Relation annotation module
//!
//! Attaches caller-resolved call-graph neighbours (callees/callers) to search
//! results as structure-preserving text annotations.
//!
//! # Architecture
//!
//! ```text
//! RelationAnnotator (coordinator)
//!     │
//!     ├── SemanticUnitExtractor (extract complete code units)
//!     │       └── Slice code by line range from caller-supplied content
//!     │
//!     ├── SegmentAggregator (aggregate adjacent segments)
//!     │       └── Merge adjacent segments from the same file
//!     │
//!     └── StructureConcatenator (concatenate with relation markers)
//!             └── Add file markers, relation markers, dedup
//! ```
//!
//! # Usage
//!
//! ```ignore
//! use crate::query::annotation::{RelationAnnotator, RelationAnnotationConfig, SearchResultInput};
//!
//! let config = RelationAnnotationConfig::conservative();
//! let annotator = RelationAnnotator::new(config);
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
//! let result = annotator
//!     .annotate_single(input, Vec::new(), Vec::new())
//!     .await?;
//! ```
//!
//! # Relation expansion
//!
//! Relation expansion is caller-driven: `annotate_single` takes pre-resolved
//! forward (callee) and backward (caller) [`ExpandedUnit`]s and only performs
//! structure-preserving concatenation, dedup and budget capping. Graph
//! traversal itself lives on the caller side.
//!
//! The caller owns path/visibility policy: expansion units must be filtered
//! against the query's exclusion rules (path filters, epoch view) *before*
//! reaching `annotate_single`, so excluded neighbours never occupy an
//! expansion budget slot. The annotator additionally applies a defensive
//! second filter (stdlib, external, and non-call edges), orders each
//! direction by score with forward units filling the shared cap first, and
//! downgrades vanished files to references when a workspace root is
//! configured.
//!
//! Every result is capped by the single per-result quota; recall count is
//! owned by `annotation_top_n` and the score threshold, never by a batch token
//! cap.
//!
//! # Integration
//!
//! When `search.annotation.enable_annotation` is set, the searcher annotates the
//! final top-N results after ranking and thresholding (see
//! `searcher::post_processing`), leaving ordering untouched.

pub mod aggregator;
pub mod annotator;
pub mod concatenator;
pub mod error;
pub mod extractor;
pub mod types;

// Re-export main types
pub use aggregator::{AggregatedSegment, SegmentAggregator};
pub use annotator::RelationAnnotator;
pub use concatenator::StructureConcatenator;
pub use error::{AnnotationError, Result};
pub use types::{
    AnnotatedResult, AnnotationMetadata, DedupStrategy, ExpandedUnit, ExpansionOrigin, FileInfo,
    RelationAnnotationConfig, SearchResultInput, SemanticUnitType,
};
