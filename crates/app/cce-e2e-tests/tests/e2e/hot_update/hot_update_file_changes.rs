//! File changes hot update tests
//!
//! Tests for file create, modify, and delete operations during hot update.

use crate::helper::{EmptyFixture, init_minimal_logging};

/// File modification hot update
#[tokio::test]
async fn test_file_modify_hot_update() {
    init_minimal_logging();

    // Create test scenario inline
    let fixture = EmptyFixture::new().expect("Failed to create fixture");
    fixture
        .add_file(
            "src/lib.rs",
            r#"
pub fn original_function() -> i32 {
    42
}
"#,
        )
        .expect("Failed to add file");

    // Initialize index
    let mut index_test =
        crate::index::index_workflow::IndexWorkflowTest::new(fixture.into_test_fixture());

    let result = index_test.execute().await.expect("Initial index failed");
    assert!(
        result.total_entities >= 1,
        "Expected at least 1 entity, got {}",
        result.total_entities
    );

    // Simulate file modification
    let modified_content = r#"
pub fn modified_function() -> i32 {
    100
}

pub fn new_function() -> String {
    "new".to_string()
}
"#;

    index_test
        .fixture()
        .modify_file("src/lib.rs", modified_content)
        .expect("Failed to modify file");

    // Re-index to verify changes
    let result2 = index_test.reindex().await.expect("Reindex failed");
    assert!(result2.total_entities >= 2);
}

/// File creation hot update
#[tokio::test]
async fn test_file_create_hot_update() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");
    fixture
        .add_file("src/main.rs", "fn main() {}")
        .expect("Failed to add file");

    let mut index_test =
        crate::index::index_workflow::IndexWorkflowTest::new(fixture.into_test_fixture());

    let result1 = index_test.execute().await.expect("Initial index failed");
    let initial_count = result1.total_entities;

    // Create new file
    index_test
        .fixture()
        .add_file(
            "src/lib.rs",
            r#"
pub fn new_function() -> i32 {
    42
}
"#,
        )
        .expect("Failed to create file");

    // Re-index
    let result2 = index_test.reindex().await.expect("Reindex failed");
    assert!(result2.total_entities > initial_count);
}

/// File deletion hot update
#[tokio::test]
async fn test_file_delete_hot_update() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");
    fixture
        .add_file("src/lib.rs", "pub fn to_delete() {}")
        .expect("Failed to add file");

    let mut index_test =
        crate::index::index_workflow::IndexWorkflowTest::new(fixture.into_test_fixture());

    let result1 = index_test.execute().await.expect("Initial index failed");
    assert!(result1.total_entities >= 1);

    // Delete file
    index_test
        .fixture()
        .delete_file("src/lib.rs")
        .expect("Failed to delete file");

    // Re-index
    let result2 = index_test.reindex().await.expect("Reindex failed");
    assert!(result2.total_entities < result1.total_entities);
}

/// Multiple file changes
#[tokio::test]
async fn test_multiple_file_changes() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");
    fixture
        .add_file(
            "src/main.rs",
            r#"
mod lib;
fn main() {
    lib::process();
}
"#,
        )
        .expect("Failed to add file");

    fixture
        .add_file(
            "src/lib.rs",
            r#"
pub fn process() -> i32 {
    42
}
"#,
        )
        .expect("Failed to add file");

    let mut index_test =
        crate::index::index_workflow::IndexWorkflowTest::new(fixture.into_test_fixture());

    let result1 = index_test.execute().await.expect("Initial index failed");

    // Modify both files
    index_test
        .fixture()
        .modify_file(
            "src/main.rs",
            r#"
mod lib;
fn main() {
    let x = lib::process();
    println!("{}", x);
}
"#,
        )
        .expect("Failed to modify main.rs");

    index_test
        .fixture()
        .modify_file(
            "src/lib.rs",
            r#"
pub fn process() -> i32 {
    100
}

pub fn helper() -> i32 {
    1
}
"#,
        )
        .expect("Failed to modify lib.rs");

    // Re-index
    let result2 = index_test.reindex().await.expect("Reindex failed");
    assert!(result2.total_entities > result1.total_entities);
}
