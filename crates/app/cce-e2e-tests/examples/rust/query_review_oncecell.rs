//! Full query-result visualization export for the once_cell fixture.
//!
//! Indexes `fixtures/rust/review/once_cell`, runs every judgment query from
//! `src/judgments/once_cell.rs` through the real query pipeline, and renders
//! per-query complete results plus an aggregated-search demo for manual
//! review. Uses the deterministic mock embedder with the embedded local
//! vector engine, so hybrid output needs no external service and always lands
//! in the default `once_cell` directory (recorded in the manifest).
//!
//! Output:
//!   outputs/scenarios/rust/query_review/once_cell/
//!   ├── run_manifest.txt
//!   ├── index.md
//!   ├── {query_id}.md         — full hits with scores, content, annotation comparison
//!   └── aggregated_demo.md    — two-sub-query merge demo

use cce_e2e_tests::BenchmarkProject;
use cce_e2e_tests::query_review::run_query_review;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    run_query_review(BenchmarkProject::OnceCell.query_review_config()).await
}
