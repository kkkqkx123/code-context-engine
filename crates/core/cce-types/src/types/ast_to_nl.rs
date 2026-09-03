//! AST to Natural Language conversion types (cross-layer)
//!
//! This module provides types that are shared across multiple layers:
//! - **OutputMode**: Used by config, ast_to_nl, and types layers
//! - **ConversionResult**: Core data exchange format between processing and storage layers
//!
//! Types that are internal to the `ast_to_nl` module are defined in `src/ast_to_nl`:
//! - `ConversionOptions`: Configuration for conversion (in `ast_to_nl::options`)
//! - `EntityMetadata`: Lightweight entity summary (in `ast_to_nl::metadata`)
//!
//! # Cross-Layer Design
//!
//! These types are kept in the `types` layer because they are exchanged between
//! different architectural layers:
//!
//! ```text
//! Config Layer ──────> Core Layer ──────> Storage Layer
//!     │                    │                    │
//!     │   OutputMode       │  ConversionResult  │
//!     └────────────────────┴────────────────────┘
//!              Shared in types layer
//! ```

pub mod chunk_path;
pub mod chunked;
pub mod file_category;
pub mod group_conversions;
pub mod metadata;
pub mod options;
pub mod query_type;
pub mod rerank;
pub mod result;
pub mod split_reason;

pub use chunk_path::ChunkPath;
pub use chunked::{
    ChunkContentType, ChunkMetadata, ChunkedResult, CodeSpecificMetadata, DocumentSpecificMetadata,
    GroupRelation, GroupRelationType, OverlapRegion, OverlapType, SourceSpanKind,
};
pub use file_category::FileCategory;
pub use group_conversions::GroupConversions;
pub use metadata::EntityMetadata;
pub use options::OutputMode;
pub use query_type::QueryType;
pub use rerank::{RerankCandidate, RerankResult, RerankedCandidate};
pub use result::ConversionResult;
pub use split_reason::SplitReason;
