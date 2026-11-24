//! Export NL documents for C review fixtures (macros).
//!
//! Output:
//!   outputs/scenarios/c/summary/{fixture}/      — markdown NL docs
//!   outputs/scenarios/c/chunks/{fixture}/emb/   — chunk segmentation (Embedding)
//!   outputs/scenarios/c/chunks/{fixture}/bm25/  — chunk segmentation (BM25)
//!   outputs/scenarios/c/structured/{fixture}/   — SUMMARY.md + per-file reports

use cce_e2e_tests::FixtureSpec;
use cce_e2e_tests::init_minimal_logging;
use cce_e2e_tests::review_export::{ReviewExportJob, export_jobs};

#[tokio::main]
async fn main() {
    init_minimal_logging();

    export_jobs(&[ReviewExportJob {
        language: "c",
        fixture_name: "macros",
        spec: FixtureSpec::c_review_macros(),
        include_patterns: &["*.c", "*.h"],
        filter: Default::default(),
    }])
    .await;
}
