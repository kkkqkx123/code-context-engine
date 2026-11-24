//! Annotation review export for the spring_boot fixture.
//!
//! Requires `data/benchmark/full_pipeline/spring_boot/bge-m3/bench_data.rkyv`
//! (run `gen_bench_spring_boot` first if missing).
//!
//! Output:
//!   outputs/scenarios/java/annotation/spring_boot/
//!   ├── index.md
//!   └── {query_id}.md          — raw recall hits + annotated content per query

use cce_e2e_tests::BenchmarkProject;
use cce_e2e_tests::annotation_review::run_annotation_review;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    run_annotation_review(BenchmarkProject::SpringBoot.annotation_review_config()).await
}
