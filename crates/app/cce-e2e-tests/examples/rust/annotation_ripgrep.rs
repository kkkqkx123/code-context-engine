//! Annotation review export for the ripgrep fixture.
//!
//! Requires `data/benchmark/full_pipeline/ripgrep/bge-m3/bench_data.rkyv`
//! (run `gen_bench_ripgrep` first if missing).
//!
//! Output:
//!   outputs/scenarios/rust/annotation/ripgrep/
//!   ├── index.md
//!   └── {query_id}.md          — raw recall hits + annotated content per query

use cce_e2e_tests::BenchmarkProject;
use cce_e2e_tests::annotation_review::run_annotation_review;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    run_annotation_review(BenchmarkProject::Ripgrep.annotation_review_config()).await
}
