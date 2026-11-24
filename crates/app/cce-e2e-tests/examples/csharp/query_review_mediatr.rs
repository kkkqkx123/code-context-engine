//! Full query-result visualization export for the mediatr fixture.
//!
//! Indexes `fixtures/csharp/review/MediatR`, runs every judgment query
//! from `src/judgments/mediatr.rs` through the real query pipeline, and
//! renders per-query complete results plus an aggregated-search demo.
//!
//! Output:
//!   outputs/scenarios/csharp/query_review/mediatr/
//!   ├── run_manifest.txt
//!   ├── index.md
//!   ├── {query_id}.md         — full hits with scores, content, annotation comparison
//!   └── aggregated_demo.md    — two-sub-query merge demo

use cce_e2e_tests::BenchmarkProject;
use cce_e2e_tests::query_review::run_query_review;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    run_query_review(BenchmarkProject::Mediatr.query_review_config()).await
}
