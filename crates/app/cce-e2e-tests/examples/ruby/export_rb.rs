//! Export NL documents for Ruby review fixtures (mixin).
//!
//! Output:
//!   outputs/scenarios/ruby/summary/{fixture}/      — markdown NL docs
//!   outputs/scenarios/ruby/chunks/{fixture}/emb/   — chunk segmentation (Embedding)
//!   outputs/scenarios/ruby/chunks/{fixture}/bm25/  — chunk segmentation (BM25)
//!   outputs/scenarios/ruby/structured/{fixture}/   — SUMMARY.md + per-file reports

use cce_e2e_tests::FixtureSpec;
use cce_e2e_tests::init_minimal_logging;
use cce_e2e_tests::review_export::{ReviewExportJob, export_jobs};

#[tokio::main]
async fn main() {
    init_minimal_logging();

    export_jobs(&[ReviewExportJob {
        language: "ruby",
        fixture_name: "mixin",
        spec: FixtureSpec::ruby_review_mixin(),
        include_patterns: &["*.rb"],
        filter: Default::default(),
    }])
    .await;
}
