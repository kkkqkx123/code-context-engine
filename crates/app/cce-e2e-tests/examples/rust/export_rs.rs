//! Export NL documents for Rust review fixtures (once_cell, ripgrep,
//! index_sidecar, re_export, relation_demo, relation_diamond).
//!
//! Each fixture is extracted, scanned and processed exactly once; the results
//! feed markdown NL-doc summary, chunk segmentation, and structured symbol /
//! relation outputs. Tree-sitter parse products (`ParsedFile`) are reused
//! between the NL export path and the relation / type-inference path so that
//! both pipelines operate on identical snapshots.
//!
//! Output:
//!   outputs/scenarios/rust/summary/{fixture}/      — markdown NL docs
//!   outputs/scenarios/rust/chunks/{fixture}/emb/   — chunk segmentation (Embedding)
//!   outputs/scenarios/rust/chunks/{fixture}/bm25/  — chunk segmentation (BM25)
//!   outputs/scenarios/rust/structured/{fixture}/   — SUMMARY.md + per-file <path>.txt
//!                                                    + per-directory <dir>.dir.txt

use cce_e2e_tests::FixtureSpec;
use cce_e2e_tests::init_minimal_logging;
use cce_e2e_tests::review_export::{ReviewExportJob, export_jobs};

#[tokio::main]
async fn main() {
    init_minimal_logging();

    export_jobs(&[
        ReviewExportJob {
            language: "rust",
            fixture_name: "once_cell",
            spec: FixtureSpec::rust_once_cell(),
            include_patterns: &["*.rs"],
            filter: Default::default(),
        },
        ReviewExportJob {
            language: "rust",
            fixture_name: "ripgrep",
            spec: FixtureSpec::rust_ripgrep(),
            include_patterns: &["*.rs"],
            filter: Default::default(),
        },
        ReviewExportJob {
            language: "rust",
            fixture_name: "index_sidecar",
            spec: FixtureSpec::rust_index_sidecar(),
            include_patterns: &["*.rs"],
            filter: Default::default(),
        },
        ReviewExportJob {
            language: "rust",
            fixture_name: "re_export",
            spec: FixtureSpec::rust_review_re_export(),
            include_patterns: &["*.rs"],
            filter: Default::default(),
        },
        ReviewExportJob {
            language: "rust",
            fixture_name: "relation_demo",
            spec: FixtureSpec::rust_relation_demo(),
            include_patterns: &["*.rs"],
            filter: Default::default(),
        },
        ReviewExportJob {
            language: "rust",
            fixture_name: "relation_diamond",
            spec: FixtureSpec::rust_relation_diamond(),
            include_patterns: &["*.rs"],
            filter: Default::default(),
        },
    ])
    .await;
}
