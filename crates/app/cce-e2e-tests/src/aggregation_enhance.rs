//! Aggregation-enhance benchmark: relation and file-level summary boosts.
//!
//! Reuses `data/benchmark/full_pipeline/{fixture}/bge-m3/bench_data.rkyv`
//! (no regeneration, no LLM calls) and compares the plain base rankings
//! (`emb`, `bm25`, `minmax-0.5`) against the same rankings with offline
//! relation boosts (`+rel`), summary boosts (`+sum`), or both (`+both`).
//! Boost parameters come from the production config defaults.
//!
//! Sub-modules:
//! - `enhance`: offline boost simulation (file-cohort graph, pooled file
//!   vectors, production-capped aggregation)
//! - `benchmark`: evaluation orchestration and metrics
//! - `report`: markdown report writers

pub mod benchmark;
pub mod enhance;
pub mod report;

pub use benchmark::{
    BASELINE, BenchmarkRun, MINMAX_WEIGHT, SignalStats, evaluate_baseline, is_production_parity,
    ordered_methods, output_dir, run_aggregation_enhance_benchmark,
};
pub use enhance::{EnhanceParams, FileCohortGraph, FileVectors};
