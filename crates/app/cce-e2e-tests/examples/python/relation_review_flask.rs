//! Relation-graph visualization export for the flask fixture.
//!
//! Same pipeline as `relation_review_oncecell`, over
//! `fixtures/python/review/flask` with `src/judgments/flask.rs` seeds.
//!
//! Output:
//!   outputs/scenarios/python/relation_review/flask/
//!   ├── run_manifest.txt
//!   ├── index.md
//!   ├── entity/{seed}.md      — callees/callers/chains/ego graph + mermaid
//!   └── graph/{seed}.json     — machine-readable ego graph snapshot

use cce_e2e_tests::BenchmarkProject;
use cce_e2e_tests::relation_review::run_relation_review;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    run_relation_review(BenchmarkProject::Flask.relation_review_config()).await
}
