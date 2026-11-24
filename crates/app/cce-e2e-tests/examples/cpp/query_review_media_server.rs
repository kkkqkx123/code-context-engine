//! Full query-result visualization export for the media_server fixture.
//!
//! Indexes `fixtures/cpp/review/media-server`, runs every judgment query
//! from `src/judgments/media_server.rs` through the real query pipeline,
//! and renders per-query complete results plus an aggregated-search demo.
//!
//! Output:
//!   outputs/scenarios/cpp/query_review/media_server/
//!   ├── run_manifest.txt
//!   ├── index.md
//!   ├── {query_id}.md         — full hits with scores, content, annotation comparison
//!   └── aggregated_demo.md    — two-sub-query merge demo

use cce_e2e_tests::BenchmarkProject;
use cce_e2e_tests::query_review::run_query_review;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    run_query_review(BenchmarkProject::MediaServer.query_review_config()).await
}
