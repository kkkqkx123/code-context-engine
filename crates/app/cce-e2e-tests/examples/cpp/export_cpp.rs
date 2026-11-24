//! Export NL documents for C++ review fixtures (templates, media-server).
//!
//! Output:
//!   outputs/scenarios/cpp/summary/{fixture}/      — markdown NL docs
//!   outputs/scenarios/cpp/chunks/{fixture}/emb/   — chunk segmentation (Embedding)
//!   outputs/scenarios/cpp/chunks/{fixture}/bm25/  — chunk segmentation (BM25)
//!   outputs/scenarios/cpp/structured/{fixture}/   — SUMMARY.md + per-file reports

use cce_e2e_tests::FixtureSpec;
use cce_e2e_tests::init_minimal_logging;
use cce_e2e_tests::review_export::{ReviewExportJob, export_jobs};

#[tokio::main]
async fn main() {
    init_minimal_logging();

    export_jobs(&[
        ReviewExportJob {
            language: "cpp",
            fixture_name: "templates",
            spec: FixtureSpec::cpp_review_templates(),
            include_patterns: &["*.cpp", "*.h", "*.hpp"],
            filter: Default::default(),
        },
        ReviewExportJob {
            language: "cpp",
            fixture_name: "media-server",
            spec: FixtureSpec::cpp_media_server(),
            include_patterns: &["*.cpp", "*.h", "*.hpp", "*.c"],
            filter: Default::default(),
        },
    ])
    .await;
}
