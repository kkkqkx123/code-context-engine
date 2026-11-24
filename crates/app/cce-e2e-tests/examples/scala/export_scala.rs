//! Export NL documents for Scala review fixtures (case_class).
//!
//! Output:
//!   outputs/scenarios/scala/summary/{fixture}/      — markdown NL docs
//!   outputs/scenarios/scala/chunks/{fixture}/emb/   — chunk segmentation (Embedding)
//!   outputs/scenarios/scala/chunks/{fixture}/bm25/  — chunk segmentation (BM25)
//!   outputs/scenarios/scala/structured/{fixture}/   — SUMMARY.md + per-file reports

use cce_e2e_tests::FixtureSpec;
use cce_e2e_tests::init_minimal_logging;
use cce_e2e_tests::review_export::{ReviewExportJob, export_jobs};

#[tokio::main]
async fn main() {
    init_minimal_logging();

    export_jobs(&[ReviewExportJob {
        language: "scala",
        fixture_name: "case_class",
        spec: FixtureSpec::scala_review_case_class(),
        include_patterns: &["*.scala"],
        filter: Default::default(),
    }])
    .await;
}
