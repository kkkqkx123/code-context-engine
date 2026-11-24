//! Retrieval-method benchmark: dense / sparse / hybrid fusion comparison.
//!
//! Reuses `data/benchmark/{baseline}/{fixture}/bge-m3/bench_data.rkyv`
//! (no regeneration, no LLM calls) and compares the recall-method families
//! `emb`, `bm25`, `minmax-*`, `rrf-*`, the single-path policy variants of the
//! balanced fusion (`-vec` / `-bm25` / `-strict`), and the multi-source
//! aggregation strategies (`agg-*`). Single-path `emb`/`bm25` rows measure the
//! raw chunk ranking (each chunk once), identical to the no-aggregation
//! baseline benchmark; the fused methods measure entity-level rankings (one
//! entry per alignment key).
//!
//! Sub-modules:
//! - `fusion`: cross-path alignment key + minmax / RRF fusion
//! - `aggregation`: multi-source merge (sub-query stand-in) score combination
//! - `benchmark`: evaluation orchestration and metrics
//! - `report`: markdown report writers

pub mod aggregation;
pub mod benchmark;
pub mod fusion;
pub mod report;

pub use aggregation::{AggSource, AggregatedEntry, AggregationStat, merge_sources};
pub use benchmark::{
    AGGREGATION_STRATEGIES, AlignmentStat, BenchmarkRun, MINMAX_WEIGHTS, MethodRelevance,
    MethodResult, POLICY_VARIANTS, RRF_K_VALUES, RRF_STRICT_K, RecallRankings,
    RetrievalMethodScore, ScoreSummary, collect_alignment_stat, evaluate_baseline, rank_recall,
    run_retrieval_method_benchmark,
};
pub use fusion::{
    FusedEntry, PathCoverage, PreparedFusion, RankedPath, SinglePathPolicy, alignment_key,
    alignment_keys, chunk_is_alignable, count_unalignable, dedup_ranked, fuse_minmax, fuse_rrf,
};
