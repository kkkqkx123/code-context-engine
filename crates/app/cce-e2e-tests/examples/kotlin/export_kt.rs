//! Export NL documents for Kotlin review fixtures (coroutines).
//!
//! Output:
//!   outputs/scenarios/kotlin/summary/{fixture}/      — markdown NL docs
//!   outputs/scenarios/kotlin/chunks/{fixture}/emb/   — chunk segmentation (Embedding)
//!   outputs/scenarios/kotlin/chunks/{fixture}/bm25/  — chunk segmentation (BM25)
//!   outputs/scenarios/kotlin/structured/{fixture}/   — SUMMARY.md + per-file reports

use cce_e2e_tests::FixtureSpec;
use cce_e2e_tests::init_minimal_logging;
use cce_e2e_tests::review_export::{ReviewExportJob, export_jobs};

#[tokio::main]
async fn main() {
    init_minimal_logging();

    export_jobs(&[ReviewExportJob {
        language: "kotlin",
        fixture_name: "coroutines",
        spec: FixtureSpec::kotlin_review_coroutines(),
        include_patterns: &["*.kt"],
        filter: Default::default(),
    }])
    .await;
}
