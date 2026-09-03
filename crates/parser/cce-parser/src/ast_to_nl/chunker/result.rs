//! Chunking result types
//!
//! Re-exports the cross-layer chunk contract from `cce_core` (moved in the
//! plugin-extension milestone so the plugin `Chunk` capability can reference
//! [`ChunkedResult`] without depending on the parser crate).

pub use cce_types::ChunkPath;
pub use cce_types::ast_to_nl::split_reason::SplitReason;
pub use cce_types::ast_to_nl::{
    ChunkContentType, ChunkMetadata, ChunkedResult, CodeSpecificMetadata, DocumentSpecificMetadata,
    GroupRelation, GroupRelationType, OverlapRegion, OverlapType, SourceSpanKind,
};
