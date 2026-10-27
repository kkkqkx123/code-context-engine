//! Chunk assembly context types.
//!
//! These structs carry the per-chunk inputs that `ChunkBuilder` consumes. They
//! are kept in their own module so the builder's assembly logic and the
//! orchestration that produces the contexts stay independent.

use crate::grouper::EntityGroup;
use cce_types::entity::EntityId;

use super::result::ChunkPath;

/// Single-path chunk context.
pub struct SingleChunkContext<'a> {
    pub group: &'a EntityGroup,
    pub file_path: &'a str,
    pub path: ChunkPath,
    pub text: &'a str,
    /// Embedding-path keywords. The BM25 path recomputes keywords from
    /// content entity ids and ignores this field.
    pub keywords: &'a [String],
}

/// Unsplit chunk context.
pub struct UnsplitContext<'a> {
    pub group: &'a EntityGroup,
    pub file_path: &'a str,
    pub path: ChunkPath,
    pub chunk_id: String,
    pub chunk_index: usize,
    pub total_chunks: usize,
    pub text: String,
    pub word_count: usize,
    pub end_byte: usize,
    pub content_entity_ids: Vec<EntityId>,
    pub context_entity_ids: Vec<EntityId>,
    /// Embedding-path keywords. The BM25 path recomputes titles and keywords
    /// from content entity ids and ignores this field.
    pub keywords: Vec<String>,
    /// Cross-group parent name for BM25 title and keyword qualification.
    pub parent_qualifier: Option<String>,
    pub split_reason: super::boundary::SplitReason,
    pub related_groups: Vec<super::result::GroupRelation>,
}

/// Header-path parameters for `from_segments`.
pub struct SegmentHeaderContext {
    pub header_entity_id: Option<EntityId>,
    pub include_header_in_first_coverage: bool,
}
