//! Relation-graph visualization export for the jackson_core fixture.
//!
//! Indexes `fixtures/java/review/jackson-core`, resolves seed entities from
//! the judgment expectation ranges plus hub names, and renders per-seed call
//! relations, call chains, and ego graphs for manual review. Fully offline.
//!
//! Output:
//!   outputs/scenarios/java/relation_review/jackson_core/
//!   ├── run_manifest.txt
//!   ├── index.md
//!   ├── entity/{seed}.md      — callees/callers/chains/ego graph + mermaid
//!   └── graph/{seed}.json     — machine-readable ego graph snapshot

use cce_e2e_tests::BenchmarkProject;
use cce_e2e_tests::relation_review::run_relation_review;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    run_relation_review(BenchmarkProject::JacksonCore.relation_review_config()).await
}
