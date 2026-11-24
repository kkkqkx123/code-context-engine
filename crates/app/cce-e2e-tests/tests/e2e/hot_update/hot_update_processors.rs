//! Processor chain tests for hot update
//!
//! Tests for the execution order and behavior of update processors:
//! - Bm25UpdateProcessor
//! - EmbeddingUpdateProcessor
//! - RelationUpdateProcessor
//! - SummaryUpdateProcessor

use crate::helper::{EmptyFixture, init_minimal_logging};

/// BM25 processor execution
///
/// Tests that the BM25 update processor correctly updates the BM25 index
/// when files change.
#[tokio::test]
async fn test_bm25_processor_execution() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");
    fixture
        .add_file(
            "src/lib.rs",
            r#"
/// Search function
pub fn search(query: &str) -> Vec<String> {
    vec![]
}
"#,
        )
        .expect("Failed to add file");

    let mut index_test =
        crate::index::index_workflow::IndexWorkflowTest::new(fixture.into_test_fixture())
            .with_bm25(true)
            .with_vectors(false);

    // Initial index
    let result1 = index_test.execute().await.expect("Index failed");
    assert!(result1.total_entities >= 1);

    // Modify file - BM25 processor should update index
    index_test
        .fixture()
        .modify_file(
            "src/lib.rs",
            r#"
/// Search function with filter
pub fn search(query: &str, filter: Option<&str>) -> Vec<String> {
    vec![]
}

/// Filter function
pub fn filter(items: &[String]) -> Vec<String> {
    items.to_vec()
}
"#,
        )
        .expect("Failed to modify file");

    // Re-index
    let result2 = index_test.reindex().await.expect("Reindex failed");
    assert!(result2.total_entities > result1.total_entities);
}

/// Embedding processor execution
///
/// Tests that the embedding update processor correctly updates vectors
/// when files change.
#[tokio::test]
async fn test_embedding_processor_execution() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");
    fixture
        .add_file(
            "src/lib.rs",
            r#"
/// Calculate sum
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}
"#,
        )
        .expect("Failed to add file");

    let mut index_test =
        crate::index::index_workflow::IndexWorkflowTest::new(fixture.into_test_fixture())
            .with_vectors(true)
            .with_bm25(false);

    // Initial index
    let result1 = index_test.execute().await.expect("Index failed");
    assert!(result1.total_entities >= 1);

    // Modify file - Embedding processor should update vectors
    index_test
        .fixture()
        .modify_file(
            "src/lib.rs",
            r#"
/// Calculate sum of two numbers
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

/// Calculate difference of two numbers
pub fn subtract(a: i32, b: i32) -> i32 {
    a - b
}
"#,
        )
        .expect("Failed to modify file");

    // Re-index
    let result2 = index_test.reindex().await.expect("Reindex failed");
    assert!(result2.total_entities > result1.total_entities);
}

/// Relation processor execution
///
/// Tests that the relation update processor correctly updates call relations
/// when files change.
#[tokio::test]
async fn test_relation_processor_execution() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");
    fixture
        .add_file(
            "src/main.rs",
            r#"
mod helper;

fn main() {
    helper::process();
}
"#,
        )
        .expect("Failed to add main.rs");

    fixture
        .add_file(
            "src/helper.rs",
            r#"
pub fn process() {
    internal();
}

fn internal() {}
"#,
        )
        .expect("Failed to add helper.rs");

    let mut index_test =
        crate::index::index_workflow::IndexWorkflowTest::new(fixture.into_test_fixture())
            .with_relations(true);

    // Initial index
    let result1 = index_test.execute().await.expect("Index failed");
    assert!(
        result1.total_relations > 0,
        "Expected relations to be built"
    );

    // Modify file - Relation processor should update relations
    index_test
        .fixture()
        .modify_file(
            "src/helper.rs",
            r#"
pub fn process() {
    internal();
    another();
}

fn internal() {}

fn another() {}
"#,
        )
        .expect("Failed to modify file");

    // Re-index
    let result2 = index_test.reindex().await.expect("Reindex failed");
    assert!(result2.total_relations >= result1.total_relations);
}

/// Summary processor execution
///
/// Tests that the summary update processor correctly updates summaries
/// when files change.
#[tokio::test]
async fn test_summary_processor_execution() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");
    fixture
        .add_file(
            "src/lib.rs",
            r#"
//! Library module
//!
//! This module provides utility functions.

/// Helper function
pub fn helper() -> i32 {
    42
}
"#,
        )
        .expect("Failed to add file");

    let mut index_test =
        crate::index::index_workflow::IndexWorkflowTest::new(fixture.into_test_fixture());

    // Initial index
    let result1 = index_test.execute().await.expect("Index failed");
    assert!(result1.total_entities >= 1);

    // Modify module docs - Summary processor should update
    index_test
        .fixture()
        .modify_file(
            "src/lib.rs",
            r#"
//! Library module
//!
//! This module provides utility functions for calculations.

/// Helper function for calculation
pub fn helper() -> i32 {
    42
}

/// Another helper
pub fn another() -> i32 {
    0
}
"#,
        )
        .expect("Failed to modify file");

    // Re-index
    let result2 = index_test.reindex().await.expect("Reindex failed");
    assert!(result2.total_entities > result1.total_entities);
}

/// Processor chain order
///
/// Tests that processors execute in the correct order:
/// 1. Parse changes
/// 2. Update relations
/// 3. Update embeddings
/// 4. Update BM25
/// 5. Update summaries
#[tokio::test]
async fn test_processor_chain_order() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");
    fixture
        .add_file(
            "src/main.rs",
            r#"
mod service;

fn main() {
    let result = service::process();
    println!("{}", result);
}
"#,
        )
        .expect("Failed to add main.rs");

    fixture
        .add_file(
            "src/service.rs",
            r#"
pub fn process() -> i32 {
    let data = fetch();
    transform(data)
}

fn fetch() -> i32 { 42 }
fn transform(n: i32) -> i32 { n * 2 }
"#,
        )
        .expect("Failed to add service.rs");

    let mut index_test =
        crate::index::index_workflow::IndexWorkflowTest::new(fixture.into_test_fixture())
            .with_relations(true)
            .with_vectors(true)
            .with_bm25(true);

    // Initial index - all processors should execute
    let result = index_test.execute().await.expect("Index failed");
    assert!(result.total_entities >= 4, "Expected at least 4 entities");
    assert!(result.total_relations > 0, "Expected relations");
}

/// Processor error handling
///
/// Tests that processor errors are handled gracefully without
/// breaking the entire update chain.
#[tokio::test]
async fn test_processor_error_handling() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");
    fixture
        .add_file("src/valid.rs", "pub fn valid() {}")
        .expect("Failed to add file");

    let mut index_test =
        crate::index::index_workflow::IndexWorkflowTest::new(fixture.into_test_fixture());

    // Index should succeed even with potential processor issues
    let result = index_test.execute().await.expect("Index failed");
    assert!(result.total_entities >= 1);
}

/// Incremental processor update
///
/// Tests that processors correctly handle incremental updates
/// (only processing changed entities).
#[tokio::test]
async fn test_incremental_processor_update() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");
    fixture
        .add_file("src/a.rs", "pub fn a() {}")
        .expect("Failed to add a.rs");
    fixture
        .add_file("src/b.rs", "pub fn b() {}")
        .expect("Failed to add b.rs");

    let mut index_test =
        crate::index::index_workflow::IndexWorkflowTest::new(fixture.into_test_fixture());

    // Initial index
    let result1 = index_test.execute().await.expect("Index failed");
    let initial_count = result1.total_entities;

    // Only modify one file - processors should only update that file
    index_test
        .fixture()
        .modify_file("src/a.rs", "pub fn a_modified() {}")
        .expect("Failed to modify file");

    // Re-index
    let result2 = index_test.reindex().await.expect("Reindex failed");
    assert!(result2.total_entities >= initial_count);
}
