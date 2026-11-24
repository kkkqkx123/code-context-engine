//! Export NL documents for JavaScript review fixtures (express, re_export).
//!
//! Output:
//!   outputs/scenarios/javascript/summary/{fixture}/      — markdown NL docs
//!   outputs/scenarios/javascript/chunks/{fixture}/emb/   — chunk segmentation (Embedding)
//!   outputs/scenarios/javascript/chunks/{fixture}/bm25/  — chunk segmentation (BM25)
//!   outputs/scenarios/javascript/structured/{fixture}/   — SUMMARY.md + per-file reports

use cce_e2e_tests::FixtureSpec;
use cce_e2e_tests::init_minimal_logging;
use cce_e2e_tests::review_export::{ReviewExportJob, export_jobs};

#[tokio::main]
async fn main() {
    init_minimal_logging();

    export_jobs(&[
        ReviewExportJob {
            language: "javascript",
            fixture_name: "express",
            spec: FixtureSpec::javascript_express(),
            include_patterns: &["*.js"],
            filter: Default::default(),
        },
        ReviewExportJob {
            language: "javascript",
            fixture_name: "re_export",
            spec: FixtureSpec::javascript_review_re_export(),
            include_patterns: &["*.js"],
            filter: Default::default(),
        },
    ])
    .await;
}
