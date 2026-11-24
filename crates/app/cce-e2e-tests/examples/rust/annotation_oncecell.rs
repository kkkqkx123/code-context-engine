//! Annotation review export for the once_cell fixture (primary example).
//!
//! Replays embedding recall offline over the `full_pipeline` benchmark
//! snapshot and renders per-query raw vs annotated markdown pairs for manual
//! review of the relation annotator.
//!
//! Requires `data/benchmark/full_pipeline/once_cell/bge-m3/bench_data.rkyv`
//! (run `gen_bench_oncecell` first if missing).
//!
//! Output:
//!   outputs/scenarios/rust/annotation/once_cell/
//!   ├── index.md
//!   └── {query_id}.md          — raw recall hits + annotated content per query

use cce_e2e_tests::BenchmarkProject;
use cce_e2e_tests::annotation_review::run_annotation_review;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    run_annotation_review(BenchmarkProject::OnceCell.annotation_review_config()).await
}
