//! Annotation review export for the express fixture.
//!
//! Requires `data/benchmark/full_pipeline/express/bge-m3/bench_data.rkyv`
//! (run `gen_bench_express` first if missing).
//!
//! Output:
//!   outputs/scenarios/javascript/annotation/express/
//!   ├── index.md
//!   └── {query_id}.md          — raw recall hits + annotated content per query

use cce_e2e_tests::BenchmarkProject;
use cce_e2e_tests::annotation_review::run_annotation_review;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    run_annotation_review(BenchmarkProject::Express.annotation_review_config()).await
}
