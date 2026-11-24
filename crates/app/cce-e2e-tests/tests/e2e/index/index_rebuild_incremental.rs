//! Incremental index update tests
//!
//! Tests for incremental index updates and file rename handling.

use crate::helper::{EmptyFixture, init_minimal_logging};

/// Incremental index update
#[tokio::test]
async fn test_incremental_index_update() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");
    fixture
        .add_file("src/lib.rs", "pub fn old() {}")
        .expect("Failed to add file");

    let mut index_test =
        crate::index::index_workflow::IndexWorkflowTest::new(fixture.into_test_fixture());

    // Initial index
    let result1 = index_test.execute().await.expect("Index failed");
    let initial_entity_count = result1.total_entities;

    // Add new file
    index_test
        .fixture()
        .add_file("src/new.rs", "pub fn new() {}")
        .expect("Failed to add file");

    // Reindex (simulating incremental update)
    let result2 = index_test
        .reindex()
        .await
        .expect("Incremental update failed");
    assert!(result2.total_entities > initial_entity_count);
}

/// Index after file rename
#[tokio::test]
async fn test_index_after_rename() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");
    fixture
        .add_file("src/old_name.rs", "pub fn func() {}")
        .expect("Failed to add file");

    let mut index_test =
        crate::index::index_workflow::IndexWorkflowTest::new(fixture.into_test_fixture());

    // Initial index
    let result1 = index_test.execute().await.expect("Index failed");

    // Delete old file and create new one (simulating rename)
    index_test
        .fixture()
        .delete_file("src/old_name.rs")
        .expect("Failed to delete old file");
    index_test
        .fixture()
        .add_file("src/new_name.rs", "pub fn func() {}")
        .expect("Failed to create new file");

    // Reindex
    let result2 = index_test.reindex().await.expect("Reindex failed");
    assert_eq!(result2.total_files, result1.total_files);
    assert_eq!(result2.total_entities, result1.total_entities);
}
