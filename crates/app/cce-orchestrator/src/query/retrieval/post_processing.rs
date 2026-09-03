//! Post-processing operations for search results
//!
//! Provides utilities for result enrichment, fusion, and filtering after retrieval.
//! These modules operate on retrieved results and are not retrieval strategies themselves.

pub mod entity_mapper;
pub mod fusion;
pub mod glob_filter;

pub(crate) use entity_mapper::{enrich_from_chunk, get_chunk_records};
pub(crate) use fusion::alignment_key;
pub use fusion::{
    FusionAlignmentStats, HybridFusionConfig, compute_alignment_coverage, fuse_hybrid_results,
    fuse_hybrid_results_with_stats, minmax_normalize,
};
pub use glob_filter::GlobFilter;
