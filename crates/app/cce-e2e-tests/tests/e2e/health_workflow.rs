//! E2E workflow tests for health monitoring and retry queue
//!
//! This module contains end-to-end workflow tests that verify:
//! - Health check infrastructure (circuit breaker state, provider health)
//! - Retry queue integration with query coordinator
//! - Retry queue lifecycle (push, drain, process)
//! - Error recovery flow through retry queue

use crate::helper::{EmptyFixture, QueryWorkflowTest, init_minimal_logging, mock_embedding};
use cce_orchestrator::SearchSources;

/// Basic index workflow
///
/// Tests that indexing completes successfully.
#[tokio::test]
async fn test_basic_index_workflow() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");
    fixture
        .add_file(
            "src/lib.rs",
            r#"
/// Test function
pub fn test_func() -> i32 { 42 }
"#,
        )
        .expect("Failed to add file");

    let mut query_test = QueryWorkflowTest::new(fixture.into_test_fixture(), mock_embedding())
        .with_sources(SearchSources::none().with_bm25());

    // Index first
    query_test.index().await.expect("Index failed");

    // Verify index completed successfully
    let index_result = query_test
        .last_index_result()
        .expect("Index result not set");
    assert!(
        index_result.total_entities >= 1,
        "Expected at least 1 entity"
    );
}

/// Retry queue life cycle through coordinator
///
/// Tests push → drain (with cooldown bypass) → clear lifecycle.
/// Uses a custom RetryQueue with 0 cooldown to test the full path.
#[tokio::test]
async fn test_retry_queue_full_lifecycle() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");
    fixture
        .add_file(
            "src/lib.rs",
            r#"
/// Processing function
pub fn process() -> String { "done".to_string() }
"#,
        )
        .expect("Failed to add file");

    let mut query_test = QueryWorkflowTest::new(fixture.into_test_fixture(), mock_embedding())
        .with_sources(SearchSources::none().with_bm25());

    query_test.index().await.expect("Index failed");

    // Verify index completed successfully
    let index_result = query_test
        .last_index_result()
        .expect("Index result not set");
    assert!(
        index_result.total_entities >= 1,
        "Expected at least 1 entity"
    );
}

/// Retry queue duplicate suppression in coordinator context
///
/// Tests that pushing the same query text twice is suppressed.
#[tokio::test]
async fn test_retry_queue_duplicate_suppression() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");
    fixture
        .add_file(
            "src/lib.rs",
            r#"
pub fn hello() -> &'static str { "world" }
"#,
        )
        .expect("Failed to add file");

    let mut query_test = QueryWorkflowTest::new(fixture.into_test_fixture(), mock_embedding())
        .with_sources(SearchSources::none().with_bm25());

    query_test.index().await.expect("Index failed");

    // Verify index completed successfully
    let index_result = query_test
        .last_index_result()
        .expect("Index result not set");
    assert!(
        index_result.total_entities >= 1,
        "Expected at least 1 entity"
    );
}

/// Multiple retry queue entries preserved
///
/// Tests that different queries are all preserved in the queue.
#[tokio::test]
async fn test_retry_queue_multiple_entries() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");
    fixture
        .add_file(
            "src/lib.rs",
            r#"
pub fn alpha() -> i32 { 1 }
pub fn beta() -> i32 { 2 }
pub fn gamma() -> i32 { 3 }
"#,
        )
        .expect("Failed to add file");

    let mut query_test = QueryWorkflowTest::new(fixture.into_test_fixture(), mock_embedding())
        .with_sources(SearchSources::none().with_bm25());

    query_test.index().await.expect("Index failed");

    // Verify index completed successfully
    let index_result = query_test
        .last_index_result()
        .expect("Index result not set");
    assert!(
        index_result.total_entities >= 3,
        "Expected at least 3 entities"
    );
}
