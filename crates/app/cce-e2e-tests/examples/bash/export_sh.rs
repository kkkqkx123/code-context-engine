//! Export NL documents for Bash review fixtures (pipeline).
//!
//! Output:
//!   outputs/scenarios/bash/summary/{fixture}/      — markdown NL docs
//!   outputs/scenarios/bash/chunks/{fixture}/emb/   — chunk segmentation (Embedding)
//!   outputs/scenarios/bash/chunks/{fixture}/bm25/  — chunk segmentation (BM25)
//!   outputs/scenarios/bash/structured/{fixture}/   — SUMMARY.md + per-file reports

use cce_e2e_tests::FixtureSpec;
use cce_e2e_tests::init_minimal_logging;
use cce_e2e_tests::review_export::{ReviewExportJob, export_jobs};

#[tokio::main]
async fn main() {
    init_minimal_logging();

    export_jobs(&[ReviewExportJob {
        language: "bash",
        fixture_name: "pipeline",
        spec: FixtureSpec::bash_review_pipeline(),
        include_patterns: &["*.sh"],
        filter: Default::default(),
    }])
    .await;
}
