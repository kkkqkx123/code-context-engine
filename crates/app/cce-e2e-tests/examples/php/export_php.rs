//! Export NL documents for PHP review fixtures (namespace_trait).
//!
//! Output:
//!   outputs/scenarios/php/summary/{fixture}/      — markdown NL docs
//!   outputs/scenarios/php/chunks/{fixture}/emb/   — chunk segmentation (Embedding)
//!   outputs/scenarios/php/chunks/{fixture}/bm25/  — chunk segmentation (BM25)
//!   outputs/scenarios/php/structured/{fixture}/   — SUMMARY.md + per-file reports

use cce_e2e_tests::FixtureSpec;
use cce_e2e_tests::init_minimal_logging;
use cce_e2e_tests::review_export::{ReviewExportJob, export_jobs};

#[tokio::main]
async fn main() {
    init_minimal_logging();

    export_jobs(&[ReviewExportJob {
        language: "php",
        fixture_name: "namespace_trait",
        spec: FixtureSpec::php_review_namespace_trait(),
        include_patterns: &["*.php"],
        filter: Default::default(),
    }])
    .await;
}
