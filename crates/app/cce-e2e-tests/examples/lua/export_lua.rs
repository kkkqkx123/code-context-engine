//! Export NL documents for Lua review fixtures (closure).
//!
//! Output:
//!   outputs/scenarios/lua/summary/{fixture}/      — markdown NL docs
//!   outputs/scenarios/lua/chunks/{fixture}/emb/   — chunk segmentation (Embedding)
//!   outputs/scenarios/lua/chunks/{fixture}/bm25/  — chunk segmentation (BM25)
//!   outputs/scenarios/lua/structured/{fixture}/   — SUMMARY.md + per-file reports

use cce_e2e_tests::FixtureSpec;
use cce_e2e_tests::init_minimal_logging;
use cce_e2e_tests::review_export::{ReviewExportJob, export_jobs};

#[tokio::main]
async fn main() {
    init_minimal_logging();

    export_jobs(&[ReviewExportJob {
        language: "lua",
        fixture_name: "closure",
        spec: FixtureSpec::lua_review_closure(),
        include_patterns: &["*.lua"],
        filter: Default::default(),
    }])
    .await;
}
