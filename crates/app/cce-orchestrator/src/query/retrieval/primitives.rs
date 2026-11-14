//! Result-shaping primitives shared by the retrieval strategies.
//!
//! Entity enrichment, hybrid fusion and path glob filtering are used after
//! recall by every strategy; they are pure functions over result sets and
//! hold no query state.

pub mod entity_mapper;
pub mod fusion;
pub mod glob_filter;

pub use entity_mapper::{
    downgrade_empty_bodies, enrich_results, get_chunk_records, get_chunk_records_from_store,
    resolve_project_root_from_store,
};
pub use fusion::{
    HybridFusionConfig, alignment_key, fuse_hybrid_results, fuse_hybrid_results_with_stats,
    minmax_normalize, unified_dedup_key, weak_post_key,
};
pub use glob_filter::GlobFilter;
