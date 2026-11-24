//! Annotation review export for the mediatr fixture.
//!
//! Requires `data/benchmark/full_pipeline/mediatr/bge-m3/bench_data.rkyv`
//! (run `gen_bench_mediatr` first if missing).
//!
//! Output:
//!   outputs/scenarios/csharp/annotation/mediatr/
//!   ├── index.md
//!   └── {query_id}.md          — raw recall hits + annotated content per query

use cce_e2e_tests::BenchmarkProject;
use cce_e2e_tests::annotation_review::run_annotation_review;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    run_annotation_review(BenchmarkProject::Mediatr.annotation_review_config()).await
}
