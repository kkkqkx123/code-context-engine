//! Export NL documents for Go review fixtures (gin).
//!
//! Output:
//!   outputs/scenarios/go/summary/{fixture}/      — markdown NL docs
//!   outputs/scenarios/go/chunks/{fixture}/emb/   — chunk segmentation (Embedding)
//!   outputs/scenarios/go/chunks/{fixture}/bm25/  — chunk segmentation (BM25)
//!   outputs/scenarios/go/structured/{fixture}/   — SUMMARY.md + per-file reports

use cce_e2e_tests::FixtureSpec;
use cce_e2e_tests::init_minimal_logging;
use cce_e2e_tests::review_export::{ReviewExportJob, export_jobs};

#[tokio::main]
async fn main() {
    init_minimal_logging();

    export_jobs(&[ReviewExportJob {
        language: "go",
        fixture_name: "gin",
        spec: FixtureSpec::go_gin(),
        include_patterns: &["*.go"],
        filter: Default::default(),
    }])
    .await;
}
