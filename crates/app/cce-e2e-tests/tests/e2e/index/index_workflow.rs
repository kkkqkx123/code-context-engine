//! Index workflow tests
//!
//! Tests for the complete index workflow from scanning to storage.

use crate::helper::{EmptyFixture, ExpectedIndexResult, assert_index_result, init_minimal_logging};

pub use crate::helper::IndexWorkflowTest;

/// Basic index workflow
#[tokio::test]
async fn test_basic_index_workflow() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");
    fixture
        .add_file(
            "src/main.rs",
            r#"
fn main() {
    println!("Hello, World!");
}
"#,
        )
        .expect("Failed to add file");

    let mut index_test = IndexWorkflowTest::new(fixture.into_test_fixture());

    let result = index_test.execute().await.expect("Index failed");

    assert_index_result(
        &result,
        ExpectedIndexResult {
            min_files: Some(1),
            no_errors: true,
            ..Default::default()
        },
    );
}

/// Multi-file index workflow
#[tokio::test]
async fn test_multi_file_index_workflow() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");
    fixture
        .add_file(
            "src/main.rs",
            r#"
mod lib;

fn main() {
    let result = lib::process();
    println!("{}", result);
}
"#,
        )
        .expect("Failed to add main.rs");

    fixture
        .add_file(
            "src/lib.rs",
            r#"
pub fn process() -> i32 {
    42
}
"#,
        )
        .expect("Failed to add lib.rs");

    let mut index_test = IndexWorkflowTest::new(fixture.into_test_fixture());

    let result = index_test.execute().await.expect("Index failed");

    assert_index_result(
        &result,
        ExpectedIndexResult {
            min_files: Some(2),
            no_errors: true,
            ..Default::default()
        },
    );
}

/// Index with relations
#[tokio::test]
async fn test_index_with_relations() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");
    fixture
        .add_file(
            "src/main.rs",
            r#"
mod helper;

fn main() {
    let x = helper::calculate();
    println!("{}", x);
}
"#,
        )
        .expect("Failed to add main.rs");

    fixture
        .add_file(
            "src/helper.rs",
            r#"
pub fn calculate() -> i32 {
    inner::compute() * 2
}

mod inner {
    pub fn compute() -> i32 {
        21
    }
}
"#,
        )
        .expect("Failed to add helper.rs");

    let mut index_test = IndexWorkflowTest::new(fixture.into_test_fixture()).with_relations(true);

    let result = index_test.execute().await.expect("Index failed");

    assert_index_result(
        &result,
        ExpectedIndexResult {
            min_files: Some(2),
            no_errors: true,
            ..Default::default()
        },
    );

    // Verify relations were built
    assert!(result.total_relations > 0);
}

/// Index with BM25
#[tokio::test]
async fn test_index_with_bm25() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");
    fixture
        .add_file(
            "src/main.rs",
            r#"
/// Main entry point
fn main() {
    println!("Hello");
}
"#,
        )
        .expect("Failed to add file");

    let mut index_test = IndexWorkflowTest::new(fixture.into_test_fixture())
        .with_bm25(true)
        .with_vectors(false);

    let result = index_test.execute().await.expect("Index failed");

    assert_index_result(
        &result,
        ExpectedIndexResult {
            min_files: Some(1),
            no_errors: true,
            ..Default::default()
        },
    );
}

/// Index with vectors
#[tokio::test]
async fn test_index_with_vectors() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");
    fixture
        .add_file(
            "src/main.rs",
            r#"
fn main() {
    println!("Hello");
}
"#,
        )
        .expect("Failed to add file");

    let mut index_test = IndexWorkflowTest::new(fixture.into_test_fixture())
        .with_vectors(true)
        .with_bm25(false);

    let result = index_test.execute().await.expect("Index failed");

    assert_index_result(
        &result,
        ExpectedIndexResult {
            min_files: Some(1),
            no_errors: true,
            ..Default::default()
        },
    );
}

/// Batch processing index with multiple files
#[tokio::test]
async fn test_batch_processing_index() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");

    // Add multiple files to exercise batch processing
    for i in 0..10 {
        let file_name = format!("src/module_{}.rs", i);
        let content = format!(
            r#"
/// Module {} does some computation
pub fn compute_{}() -> i32 {{
    {}
}}
"#,
            i,
            i,
            i * 10
        );
        fixture
            .add_file(&file_name, &content)
            .expect("Failed to add file");
    }

    let mut index_test = IndexWorkflowTest::new(fixture.into_test_fixture());

    let result = index_test.execute().await.expect("Index failed");

    assert_index_result(
        &result,
        ExpectedIndexResult {
            min_files: Some(10),
            no_errors: true,
            ..Default::default()
        },
    );
}

/// Multi-language index workflow
#[tokio::test]
async fn test_multi_language_index_workflow() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");

    // Rust file
    fixture
        .add_file(
            "src/main.rs",
            r#"
fn main() {
    println!("Hello from Rust");
}
"#,
        )
        .expect("Failed to add Rust file");

    // Python file
    fixture
        .add_file(
            "src/helper.py",
            r#"
def greet(name: str) -> str:
    return f"Hello, {name}!"
"#,
        )
        .expect("Failed to add Python file");

    // JavaScript file
    fixture
        .add_file(
            "src/utils.js",
            r#"
function add(a, b) {
    return a + b;
}
"#,
        )
        .expect("Failed to add JS file");

    // TypeScript file
    fixture
        .add_file(
            "src/types.ts",
            r#"
interface User {
    name: string;
    age: number;
}

function createUser(name: string, age: number): User {
    return { name, age };
}
"#,
        )
        .expect("Failed to add TS file");

    let mut index_test = IndexWorkflowTest::new(fixture.into_test_fixture()).with_extensions(vec![
        "rs".to_string(),
        "py".to_string(),
        "js".to_string(),
        "ts".to_string(),
    ]);

    let result = index_test.execute().await.expect("Index failed");

    assert_index_result(
        &result,
        ExpectedIndexResult {
            min_files: Some(4),
            no_errors: true,
            ..Default::default()
        },
    );
}
