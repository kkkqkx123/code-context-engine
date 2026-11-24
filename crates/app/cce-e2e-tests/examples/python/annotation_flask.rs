//! Annotation review export for the flask fixture.
//!
//! Same pipeline as `annotation_oncecell`, over the `full_pipeline`
//! flask benchmark snapshot. Requires
//! `data/benchmark/full_pipeline/flask/bge-m3/bench_data.rkyv`
//! (run `gen_bench_flask` first if missing).
//!
//! Output:
//!   outputs/scenarios/python/annotation/flask/
//!   ├── index.md
//!   └── {query_id}.md          — raw recall hits + annotated content per query

use cce_e2e_tests::BenchmarkProject;
use cce_e2e_tests::annotation_review::run_annotation_review;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    run_annotation_review(BenchmarkProject::Flask.annotation_review_config()).await
}
