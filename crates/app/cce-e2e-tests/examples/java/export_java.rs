//! Export NL documents for Java review fixtures (index_sidecar,
//! springboot-minimal-demo, jackson-core).
//!
//! Output:
//!   outputs/scenarios/java/summary/{fixture}/      — markdown NL docs
//!   outputs/scenarios/java/chunks/{fixture}/emb/   — chunk segmentation (Embedding)
//!   outputs/scenarios/java/chunks/{fixture}/bm25/  — chunk segmentation (BM25)
//!   outputs/scenarios/java/structured/{fixture}/   — SUMMARY.md + per-file reports

use cce_e2e_tests::FixtureSpec;
use cce_e2e_tests::init_minimal_logging;
use cce_e2e_tests::review_export::{ReviewExportJob, export_jobs};

#[tokio::main]
async fn main() {
    init_minimal_logging();

    export_jobs(&[
        ReviewExportJob {
            language: "java",
            fixture_name: "index_sidecar",
            spec: FixtureSpec::java_index_sidecar(),
            include_patterns: &["*.java"],
            filter: Default::default(),
        },
        ReviewExportJob {
            language: "java",
            fixture_name: "springboot-minimal-demo",
            spec: FixtureSpec::java_spring_boot(),
            include_patterns: &["*.java"],
            filter: Default::default(),
        },
        ReviewExportJob {
            language: "java",
            fixture_name: "jackson-core",
            spec: FixtureSpec::java_jackson_core(),
            include_patterns: &["*.java"],
            filter: Default::default(),
        },
    ])
    .await;
}
