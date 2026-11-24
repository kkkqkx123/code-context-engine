//! Export NL documents for Dart review fixtures (mixin_async).
//!
//! Output:
//!   outputs/scenarios/dart/summary/{fixture}/      — markdown NL docs
//!   outputs/scenarios/dart/chunks/{fixture}/emb/   — chunk segmentation (Embedding)
//!   outputs/scenarios/dart/chunks/{fixture}/bm25/  — chunk segmentation (BM25)
//!   outputs/scenarios/dart/structured/{fixture}/   — SUMMARY.md + per-file reports

use cce_e2e_tests::FixtureSpec;
use cce_e2e_tests::init_minimal_logging;
use cce_e2e_tests::review_export::{ReviewExportJob, export_jobs};

#[tokio::main]
async fn main() {
    init_minimal_logging();

    export_jobs(&[ReviewExportJob {
        language: "dart",
        fixture_name: "mixin_async",
        spec: FixtureSpec::dart_review_mixin_async(),
        include_patterns: &["*.dart"],
        filter: Default::default(),
    }])
    .await;
}
