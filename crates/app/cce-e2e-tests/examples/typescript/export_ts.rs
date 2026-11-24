//! Export NL documents for TypeScript review fixtures (index_sidecar,
//! re_export, wildcard).
//!
//! Output:
//!   outputs/scenarios/typescript/summary/{fixture}/      — markdown NL docs
//!   outputs/scenarios/typescript/chunks/{fixture}/emb/   — chunk segmentation (Embedding)
//!   outputs/scenarios/typescript/chunks/{fixture}/bm25/  — chunk segmentation (BM25)
//!   outputs/scenarios/typescript/structured/{fixture}/   — SUMMARY.md + per-file reports

use cce_e2e_tests::FixtureSpec;
use cce_e2e_tests::init_minimal_logging;
use cce_e2e_tests::review_export::{ReviewExportJob, export_jobs};

#[tokio::main]
async fn main() {
    init_minimal_logging();

    export_jobs(&[
        ReviewExportJob {
            language: "typescript",
            fixture_name: "index_sidecar",
            spec: FixtureSpec::typescript_index_sidecar(),
            include_patterns: &["*.ts"],
            filter: Default::default(),
        },
        ReviewExportJob {
            language: "typescript",
            fixture_name: "re_export",
            spec: FixtureSpec::typescript_review_re_export(),
            include_patterns: &["*.ts"],
            filter: Default::default(),
        },
        ReviewExportJob {
            language: "typescript",
            fixture_name: "wildcard",
            spec: FixtureSpec::typescript_review_wildcard(),
            include_patterns: &["*.ts"],
            filter: Default::default(),
        },
    ])
    .await;
}
