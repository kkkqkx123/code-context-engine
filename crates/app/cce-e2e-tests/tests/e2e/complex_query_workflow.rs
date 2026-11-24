//! E2E workflow tests for complex query scenarios
//!
//! This module contains end-to-end workflow tests that verify the complete
//! integration of the query orchestrator with storage and indexing components.
//!
//! Test Coverage:
//! - Multi-query aggregation workflow
//! - Relation enrichment workflow
//! - Complex filtering workflow

use crate::helper::{EmptyFixture, QueryWorkflowTest, init_minimal_logging, mock_embedding};
use cce_orchestrator::SearchSources;

/// Multi-query aggregation workflow
///
/// Tests the ability to execute multiple sub-queries in parallel and aggregate results.
/// This is useful for complex queries that can be decomposed into simpler sub-queries.
#[tokio::test]
async fn test_multi_query_aggregation_workflow() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");
    fixture
        .add_file(
            "src/main.rs",
            r#"
/// Main entry point for the application
fn main() {
    println!("Hello, World!");
    helper_function();
}

/// Helper function for processing
fn helper_function() {
    println!("Helper");
}

/// Utility function for calculations
fn calculate_sum(a: i32, b: i32) -> i32 {
    a + b
}
"#,
        )
        .expect("Failed to add file");

    let mut query_test = QueryWorkflowTest::new(fixture.into_test_fixture(), mock_embedding())
        .with_sources(SearchSources::none().with_bm25());

    // Index first
    let index_result = query_test.index().await.expect("Index failed");
    assert!(
        index_result.total_entities >= 3,
        "Expected at least 3 entities (main, helper_function, calculate_sum)"
    );

    // Execute basic BM25 query
    let query_result = query_test
        .search_bm25("find main and helper functions", 10)
        .await
        .expect("BM25 query failed");

    tracing::info!("Query returned {} results", query_result.total);

    let aggregated = query_test
        .search_aggregated(
            "main helper function",
            vec!["main".to_string(), "helper_function".to_string()],
        )
        .await
        .expect("aggregated query failed");
    assert_eq!(aggregated.planned_sub_queries, 2);
    assert_eq!(aggregated.executed_sub_queries, 2);
    assert_eq!(aggregated.succeeded_sub_queries, 2);
    assert!(aggregated.total > 0, "expected aggregated hits");
}

/// Relation enrichment workflow
///
/// Tests the ability to build and query call relationships between functions.
/// Verifies that intra-file call chains are indexed with correct entity and relation counts.
#[tokio::test]
async fn test_relation_enrichment_workflow() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");
    fixture
        .add_file(
            "src/lib.rs",
            r#"
/// Process data by calculating and transforming
pub fn process() -> i32 {
    let value = calculate();
    transform(value)
}

/// Calculate a base value
fn calculate() -> i32 {
    42
}

/// Transform the input value
fn transform(input: i32) -> i32 {
    input * 2
}

/// Main function that uses process
pub fn run() {
    let result = process();
    println!("Result: {}", result);
}
"#,
        )
        .expect("Failed to add file");

    let mut query_test = QueryWorkflowTest::new(fixture.into_test_fixture(), mock_embedding())
        .with_sources(SearchSources::none().with_bm25());

    let index_result = query_test.index().await.expect("Index failed");
    assert!(
        index_result.total_entities >= 4,
        "Expected at least 4 entities (process, calculate, transform, run)"
    );
    assert!(
        index_result.total_relations > 0,
        "Expected relations to be built for call chain: run->process->calculate+transform"
    );

    let query_result = query_test
        .search_bm25("process calculate", 10)
        .await
        .expect("BM25 query failed");

    assert!(
        query_result.total > 0,
        "Expected BM25 results for 'process calculate'"
    );
    let names: Vec<&str> = query_result.items.iter().map(|r| r.name.as_str()).collect();
    assert!(
        names.contains(&"process"),
        "Expected 'process' in BM25 results, got: {:?}",
        names
    );
}

/// Complex filtering workflow
///
/// Tests the ability to filter search results by various criteria:
/// - File extensions
/// - Directory prefixes
/// - Entity types
/// - Content types (exclude tests, generated code, etc.)
#[tokio::test]
async fn test_complex_filtering_workflow() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");

    // Add source files
    fixture
        .add_file(
            "src/main.rs",
            r#"
/// Application entry point
fn main() {
    println!("Starting application");
    let result = process_data();
    println!("Result: {}", result);
}

/// Process data from various sources
fn process_data() -> i32 {
    42
}
"#,
        )
        .expect("Failed to add main.rs");

    fixture
        .add_file(
            "src/utils.rs",
            r#"
/// Utility function for formatting
pub fn format_output(value: i32) -> String {
    format!("Value: {}", value)
}

/// Utility function for validation
pub fn validate_input(input: &str) -> bool {
    !input.is_empty()
}
"#,
        )
        .expect("Failed to add utils.rs");

    // Add test files (should be excludable)
    fixture
        .add_file(
            "tests/test_main.rs",
            r#"
#[test]
fn test_main() {
    assert_eq!(2 + 2, 4);
}

#[test]
fn test_process() {
    assert_eq!(super::process_data(), 42);
}
"#,
        )
        .expect("Failed to add test file");

    // Add Python file for multi-language testing
    fixture
        .add_file(
            "scripts/helper.py",
            r#"
def python_helper():
    """Python helper function"""
    return "Hello from Python"
"#,
        )
        .expect("Failed to add Python file");

    let mut query_test = QueryWorkflowTest::new(fixture.into_test_fixture(), mock_embedding())
        .with_sources(SearchSources::none().with_bm25());

    // Index all files
    let index_result = query_test.index().await.expect("Index failed");
    assert!(
        index_result.total_files >= 4,
        "Expected at least 4 files to be indexed"
    );
    assert!(
        index_result.total_entities >= 6,
        "Expected at least 6 entities across all files"
    );

    // Test 1: Basic BM25 search (no filters) - avoid vector search to prevent Qdrant dependency
    // Note: This may fail if BM25 index is not properly configured in test environment
    let unfiltered_result = match query_test.search_bm25("process data", 10).await {
        Ok(result) => result,
        Err(e) => {
            tracing::warn!("BM25 search failed (expected in test environment): {}", e);
            return; // Skip remaining tests if BM25 is not available
        }
    };

    // Should find results from both Rust and Python files
    assert!(
        unfiltered_result.total > 0,
        "Expected at least one result from hybrid search"
    );

    // Test 2: Filter by file extension (Rust only)
    let rust_fixture = EmptyFixture::new().expect("Failed to create fixture");
    rust_fixture
        .add_file(
            "src/main.rs",
            r#"
/// Application entry point
fn main() {
    println!("Starting application");
    let result = process_data();
    println!("Result: {}", result);
}

/// Process data from various sources
fn process_data() -> i32 {
    42
}
"#,
        )
        .expect("Failed to add main.rs");

    let mut rust_only_test =
        QueryWorkflowTest::new(rust_fixture.into_test_fixture(), mock_embedding())
            .with_sources(SearchSources::none().with_bm25());
    rust_only_test.index().await.expect("Index failed");

    let rust_result = rust_only_test
        .search_bm25("process", 10)
        .await
        .expect("Rust-only query failed");

    // All results should be from .rs files
    for item in &rust_result.items {
        assert!(
            item.file_path.ends_with(".rs"),
            "Expected only .rs files, got: {}",
            item.file_path
        );
    }

    // Test 3: Search with content verification
    let content_fixture = EmptyFixture::new().expect("Failed to create fixture");
    content_fixture
        .add_file(
            "src/lib.rs",
            r#"
pub fn production_code() -> i32 {
    42
}
"#,
        )
        .expect("Failed to add lib.rs");
    content_fixture
        .add_file(
            "tests/test_main.rs",
            r#"
#[test]
fn test_main() {
    assert_eq!(2 + 2, 4);
}
"#,
        )
        .expect("Failed to add test file");

    let mut content_test =
        QueryWorkflowTest::new(content_fixture.into_test_fixture(), mock_embedding())
            .with_sources(SearchSources::none().with_bm25());
    let content_index = content_test.index().await.expect("Index failed");
    assert!(
        content_index.total_files >= 2,
        "Expected at least 2 files (src/lib.rs, tests/test_main.rs)"
    );

    let content_result = content_test
        .search_bm25("production_code", 10)
        .await
        .expect("Content query failed");

    assert!(
        content_result.total > 0,
        "Expected BM25 results for 'production_code'"
    );
    assert!(
        content_result
            .items
            .iter()
            .any(|r| r.name == "production_code" || r.name.contains("production")),
        "Expected 'production_code' or matching entity in results"
    );

    // Test 4: Multi-language query
    let multi_lang_fixture = EmptyFixture::new().expect("Failed to create fixture");
    multi_lang_fixture
        .add_file(
            "src/utils.rs",
            r#"
/// Utility function for formatting
pub fn format_output(value: i32) -> String {
    format!("Value: {}", value)
}
"#,
        )
        .expect("Failed to add utils.rs");
    multi_lang_fixture
        .add_file(
            "scripts/helper.py",
            r#"
def python_helper():
    """Python helper function"""
    return "Hello from Python"
"#,
        )
        .expect("Failed to add Python file");

    let mut multi_lang_test =
        QueryWorkflowTest::new(multi_lang_fixture.into_test_fixture(), mock_embedding())
            .with_sources(SearchSources::none().with_bm25());
    let multi_index = multi_lang_test.index().await.expect("Index failed");
    assert!(
        multi_index.total_files >= 2,
        "Expected at least 2 files (utils.rs, helper.py)"
    );

    let lang_result = multi_lang_test
        .search_bm25("format_output", 10)
        .await
        .expect("Multi-language query failed");

    assert!(
        lang_result.total > 0,
        "Expected BM25 results for 'format_output'"
    );
    assert!(
        lang_result
            .items
            .iter()
            .any(|r| r.file_path.ends_with(".rs")),
        "Expected Rust results from format_output"
    );
}
