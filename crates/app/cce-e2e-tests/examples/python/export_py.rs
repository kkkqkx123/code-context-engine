//! Export NL documents for Python review fixtures (flask, index_sidecar,
//! re_export, wildcard).
//!
//! Output:
//!   outputs/scenarios/python/summary/{fixture}/      — markdown NL docs
//!   outputs/scenarios/python/chunks/{fixture}/emb/   — chunk segmentation (Embedding)
//!   outputs/scenarios/python/chunks/{fixture}/bm25/  — chunk segmentation (BM25)
//!   outputs/scenarios/python/structured/{fixture}/   — SUMMARY.md + per-file reports

use cce_e2e_tests::FixtureSpec;
use cce_e2e_tests::init_minimal_logging;
use cce_e2e_tests::review_export::{ReviewExportJob, export_jobs};
use cce_e2e_tests::review_filter::ReviewFilterOptions;

#[tokio::main]
async fn main() {
    init_minimal_logging();

    export_jobs(&[
        ReviewExportJob {
            language: "python",
            fixture_name: "flask",
            spec: FixtureSpec::python_flask(),
            include_patterns: &["*.py"],
            // Demo the opt-in output filter: keep test files out of the
            // written reports while indexing stays exhaustive.
            filter: ReviewFilterOptions::default().with_exclude_tests(true),
        },
        ReviewExportJob {
            language: "python",
            fixture_name: "index_sidecar",
            spec: FixtureSpec::python_index_sidecar(),
            include_patterns: &["*.py"],
            filter: Default::default(),
        },
        ReviewExportJob {
            language: "python",
            fixture_name: "re_export",
            spec: FixtureSpec::python_review_re_export(),
            include_patterns: &["*.py"],
            filter: Default::default(),
        },
        ReviewExportJob {
            language: "python",
            fixture_name: "wildcard",
            spec: FixtureSpec::python_review_wildcard(),
            include_patterns: &["*.py"],
            filter: Default::default(),
        },
    ])
    .await;
}
