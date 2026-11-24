//! Export NL documents for C# review fixtures (MediatR).
//!
//! Output:
//!   outputs/scenarios/csharp/summary/{fixture}/      — markdown NL docs
//!   outputs/scenarios/csharp/chunks/{fixture}/emb/   — chunk segmentation (Embedding)
//!   outputs/scenarios/csharp/chunks/{fixture}/bm25/  — chunk segmentation (BM25)
//!   outputs/scenarios/csharp/structured/{fixture}/   — SUMMARY.md + per-file reports

use cce_e2e_tests::FixtureSpec;
use cce_e2e_tests::init_minimal_logging;
use cce_e2e_tests::review_export::{ReviewExportJob, export_jobs};

#[tokio::main]
async fn main() {
    init_minimal_logging();

    export_jobs(&[ReviewExportJob {
        language: "csharp",
        fixture_name: "MediatR",
        spec: FixtureSpec::csharp_mediatr(),
        include_patterns: &["*.cs"],
        filter: Default::default(),
    }])
    .await;
}
