//! Full index rebuild tests
//!
//! Tests for complete index rebuild operations.

use crate::helper::{EmptyFixture, init_minimal_logging};

/// Full index rebuild
#[tokio::test]
async fn test_full_index_rebuild() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");
    fixture
        .add_file(
            "src/main.rs",
            r#"
fn main() {
    let x = helper::process();
    println!("{}", x);
}
"#,
        )
        .expect("Failed to add file");
    fixture
        .add_file(
            "src/helper.rs",
            r#"
pub fn process() -> i32 {
    42
}
"#,
        )
        .expect("Failed to add file");

    let mut index_test =
        crate::index::index_workflow::IndexWorkflowTest::new(fixture.into_test_fixture());

    // Initial index
    let result1 = index_test.execute().await.expect("Index failed");
    assert_eq!(result1.total_files, 2);

    // Rebuild
    let result2 = index_test.rebuild().await.expect("Rebuild failed");
    assert_eq!(result2.total_files, 2);
    assert_eq!(result2.total_entities, result1.total_entities);
}

/// Multi-file project rebuild
#[tokio::test]
async fn test_multi_file_project_rebuild() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");

    fixture
        .add_file(
            "src/main.rs",
            r#"
mod lib;
mod utils;

fn main() {
    let result = lib::process();
    let formatted = utils::format(result);
    println!("{}", formatted);
}
"#,
        )
        .expect("Failed to add main.rs");

    fixture
        .add_file(
            "src/lib.rs",
            r#"
pub fn process() -> i32 {
    helper::internal() * 2
}

mod helper {
    pub fn internal() -> i32 {
        21
    }
}
"#,
        )
        .expect("Failed to add lib.rs");

    fixture
        .add_file(
            "src/utils.rs",
            r#"
pub fn format(value: i32) -> String {
    format!("Result: {}", value)
}
"#,
        )
        .expect("Failed to add utils.rs");

    let mut index_test =
        crate::index::index_workflow::IndexWorkflowTest::new(fixture.into_test_fixture());

    // Initial index
    let result1 = index_test.execute().await.expect("Index failed");

    // Rebuild
    let result2 = index_test.rebuild().await.expect("Rebuild failed");
    assert_eq!(result2.total_files, result1.total_files);
    assert_eq!(result2.total_entities, result1.total_entities);
}
