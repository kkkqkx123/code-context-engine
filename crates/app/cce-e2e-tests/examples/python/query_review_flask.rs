//! Full query-result visualization export for the flask fixture.
//!
//! Same pipeline as `query_review_oncecell`, over
//! `fixtures/python/review/flask` with `src/judgments/flask.rs` queries.
//! Hybrid output uses the embedded local vector engine, so no external
//! service is needed and the run always lands in the default `flask`
//! directory (recorded in the manifest).
//!
//! Output:
//!   outputs/scenarios/python/query_review/flask/
//!   ├── run_manifest.txt
//!   ├── index.md
//!   ├── {query_id}.md         — full hits with scores, content, annotation comparison
//!   └── aggregated_demo.md    — two-sub-query merge demo

use cce_e2e_tests::BenchmarkProject;
use cce_e2e_tests::query_review::run_query_review;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    run_query_review(BenchmarkProject::Flask.query_review_config()).await
}
