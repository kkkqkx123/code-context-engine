//! Multi-language project output report
//!
//! Generates index result snapshots for multi-language, Python, and Java
//! fixtures into `outputs/scenarios/multi_language/`.

use cce_e2e_tests::{
    OutputCategory, OutputManager, OutputReporter, OutputSerializer, ReportMetadata, TestFixture,
    init_minimal_logging,
};
use cce_orchestrator::{IndexOptions, IndexOrchestrator};

#[tokio::main]
async fn main() {
    init_minimal_logging();

    // --- Multi-language project ---
    let fixture = TestFixture::multi_language().expect("Failed to load fixture");
    let mut orchestrator = IndexOrchestrator::new(1).expect("failed to create IndexOrchestrator");
    let options =
        IndexOptions::new(fixture.root()).with_extensions(vec!["rs".to_string(), "py".to_string()]);
    let result = orchestrator.execute(options).await.expect("Index failed");

    let reporter = OutputReporter::new(
        OutputManager::builder()
            .category(OutputCategory::Scenarios)
            .scenario("multi_language")
            .build(),
        OutputSerializer::json(),
    )
    .with_metadata(
        ReportMetadata::new()
            .description("Multi-language project indexing")
            .category("output")
            .tag("multi-language"),
    );

    let _output_path = reporter
        .report_index_result("multi_language_project", &result)
        .expect("Failed to write report");

    // --- Python basic ---
    let fixture = TestFixture::python_basic().expect("Failed to load fixture");
    let mut orchestrator = IndexOrchestrator::new(1).expect("failed to create IndexOrchestrator");
    let options = IndexOptions::new(fixture.root()).with_extensions(vec!["py".to_string()]);
    let result = orchestrator.execute(options).await.expect("Index failed");

    let reporter = OutputReporter::new(
        OutputManager::builder()
            .category(OutputCategory::Scenarios)
            .scenario("multi_language")
            .build(),
        OutputSerializer::json(),
    )
    .with_metadata(
        ReportMetadata::new()
            .description("Python basic project indexing")
            .category("output")
            .tag("python"),
    );

    let _output_path = reporter
        .report_index_result("python_basic", &result)
        .expect("Failed to write report");

    // --- Java basic ---
    let fixture = TestFixture::java_basic().expect("Failed to load fixture");
    let mut orchestrator = IndexOrchestrator::new(1).expect("failed to create IndexOrchestrator");
    let options = IndexOptions::new(fixture.root()).with_extensions(vec!["java".to_string()]);
    let result = orchestrator.execute(options).await.expect("Index failed");

    let reporter = OutputReporter::new(
        OutputManager::builder()
            .category(OutputCategory::Scenarios)
            .scenario("multi_language")
            .build(),
        OutputSerializer::json(),
    )
    .with_metadata(
        ReportMetadata::new()
            .description("Java basic project indexing")
            .category("output")
            .tag("java"),
    );

    let _output_path = reporter
        .report_index_result("java_basic", &result)
        .expect("Failed to write report");
}
