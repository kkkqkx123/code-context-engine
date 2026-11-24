//! Rust basic project output report
//!
//! Generates index result snapshots for a basic Rust project fixture
//! into `outputs/scenarios/rust/presentation/basic/`.

use cce_e2e_tests::{
    OutputCategory, OutputManager, OutputReporter, OutputSerializer, ReportMetadata, TestFixture,
    init_minimal_logging,
};
use cce_orchestrator::{IndexOptions, IndexOrchestrator};

#[tokio::main]
async fn main() {
    init_minimal_logging();

    // --- Rust basic complete workflow ---
    let fixture = TestFixture::rust_basic().expect("Failed to load fixture");
    let mut orchestrator = IndexOrchestrator::new(1).expect("failed to create IndexOrchestrator");
    let options = IndexOptions::new(fixture.root()).with_extensions(vec!["rs".to_string()]);
    let result = orchestrator.execute(options).await.expect("Index failed");

    let reporter = OutputReporter::new(
        OutputManager::builder()
            .category(OutputCategory::Scenarios)
            .language("rust")
            .scenario("presentation/basic")
            .build(),
        OutputSerializer::json(),
    )
    .with_metadata(
        ReportMetadata::new()
            .description("Rust basic project complete workflow")
            .category("output")
            .tag("rust")
            .tag("basic"),
    );

    let _output_path = reporter
        .report_index_result("rust_basic_complete", &result)
        .expect("Failed to write report");

    // --- Rust basic BM25 indexing ---
    let fixture = TestFixture::rust_basic().expect("Failed to load fixture");
    let mut orchestrator = IndexOrchestrator::new(1).expect("failed to create IndexOrchestrator");
    let options = IndexOptions::new(fixture.root()).with_extensions(vec!["rs".to_string()]);
    let result = orchestrator.execute(options).await.expect("Index failed");

    let reporter = OutputReporter::new(
        OutputManager::builder()
            .category(OutputCategory::Scenarios)
            .language("rust")
            .scenario("presentation/basic_bm25")
            .build(),
        OutputSerializer::json(),
    )
    .with_metadata(
        ReportMetadata::new()
            .description("Rust basic project with BM25 indexing")
            .category("output")
            .tag("rust")
            .tag("bm25"),
    );

    let _output_path = reporter
        .report_index_result("rust_basic_bm25", &result)
        .expect("Failed to write report");
}
