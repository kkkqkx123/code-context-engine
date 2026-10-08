//! Post-processing modules for retrieval results.

pub mod entity_mapper;
pub mod fusion;
pub mod glob_filter;

pub use entity_mapper::{
    enrich_results, get_chunk_records, get_chunk_records_from_store,
    resolve_project_root_from_store,
};
pub use fusion::{
    HybridFusionConfig, alignment_key, fuse_hybrid_results, fuse_hybrid_results_with_stats,
    minmax_normalize,
};
pub use glob_filter::GlobFilter;
