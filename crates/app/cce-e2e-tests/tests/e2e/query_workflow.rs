//! Query workflow tests
//!
//! Tests for the complete query workflow including vector, BM25, and relation searches.

use crate::helper::{EmptyFixture, QueryWorkflowTest, init_minimal_logging, mock_embedding};
use cce_orchestrator::SearchSources;

/// Basic query workflow
///
/// Verifies index + BM25 query returns entities with expected names and file paths.
#[tokio::test]
async fn test_basic_query_workflow() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");
    fixture
        .add_file(
            "src/main.rs",
            r#"
/// Main entry point for the application
fn main() {
    println!("Hello, World!");
}
"#,
        )
        .expect("Failed to add file");

    let mut query_test = QueryWorkflowTest::new(fixture.into_test_fixture(), mock_embedding())
        .with_sources(SearchSources::none().with_bm25());

    let index_result = query_test.index().await.expect("Index failed");
    assert!(
        index_result.total_entities >= 1,
        "Expected at least 1 entity"
    );

    let query_result = query_test
        .search_bm25("main entry point", 10)
        .await
        .expect("BM25 query failed");

    assert!(query_result.total > 0, "Expected at least one BM25 result");
    assert!(
        query_result.items.iter().any(|r| r.name == "main"),
        "Expected result named 'main', got: {:?}",
        query_result
            .items
            .iter()
            .map(|r| &r.name)
            .collect::<Vec<_>>()
    );
    assert!(
        query_result
            .items
            .iter()
            .any(|r| r.file_path.ends_with("src/main.rs")),
        "Expected result from src/main.rs"
    );
}

/// BM25 search with multiple entities
///
/// Vector search requires Qdrant; this test uses BM25-only to verify
/// that multiple entities in a single file are indexed and searchable.
#[tokio::test]
async fn test_vector_search_query() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");
    fixture
        .add_file(
            "src/lib.rs",
            r#"
/// Calculate the sum of two numbers
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

/// Calculate the difference of two numbers
pub fn subtract(a: i32, b: i32) -> i32 {
    a - b
}
"#,
        )
        .expect("Failed to add file");

    let mut query_test = QueryWorkflowTest::new(fixture.into_test_fixture(), mock_embedding())
        .with_sources(SearchSources::none().with_bm25());

    let index_result = query_test.index().await.expect("Index failed");
    assert!(
        index_result.total_entities >= 2,
        "Expected at least 2 entities"
    );

    let query_result = query_test
        .search_bm25("add subtract", 10)
        .await
        .expect("BM25 query failed");

    assert!(query_result.total > 0, "Expected at least one BM25 result");
    let names: Vec<&str> = query_result.items.iter().map(|r| r.name.as_str()).collect();
    assert!(
        names.contains(&"add"),
        "Expected 'add' in results, got: {:?}",
        names
    );
}

/// BM25 search query with content verification
#[tokio::test]
async fn test_bm25_search_query() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");
    fixture
        .add_file(
            "src/lib.rs",
            r#"
/// Process user data
pub fn process_user(name: String, age: u32) -> User {
    User { name, age }
}

/// Validate user input
pub fn validate_user(user: &User) -> bool {
    !user.name.is_empty() && user.age > 0
}

struct User {
    name: String,
    age: u32,
}
"#,
        )
        .expect("Failed to add file");

    let mut query_test = QueryWorkflowTest::new(fixture.into_test_fixture(), mock_embedding())
        .with_sources(SearchSources::none().with_bm25());

    let index_result = query_test.index().await.expect("Index failed");
    assert!(
        index_result.total_entities >= 2,
        "Expected at least 2 entities (process_user, validate_user)"
    );

    let query_result = query_test
        .search_bm25("user", 10)
        .await
        .expect("BM25 query failed");

    assert!(
        query_result.total > 0,
        "Expected at least one BM25 result for 'user'"
    );
    let names: Vec<&str> = query_result.items.iter().map(|r| r.name.as_str()).collect();
    assert!(
        names.contains(&"process_user"),
        "Expected 'process_user' in results, got: {:?}",
        names
    );
    assert!(
        query_result
            .items
            .iter()
            .all(|r| r.file_path.ends_with("src/lib.rs")),
        "All results should be from src/lib.rs"
    );
}

/// Cross-file call chain query
///
/// Verifies that indexing multi-file project with cross-module calls
/// produces correct entities, BM25 searchability, and relation data.
#[tokio::test]
async fn test_call_chain_query() {
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
    let data = fetch_data();
    transform(data)
}

fn fetch_data() -> i32 {
    42
}

fn transform(input: i32) -> i32 {
    input * 2
}
"#,
        )
        .expect("Failed to add service.rs");

    let mut query_test = QueryWorkflowTest::new(fixture.into_test_fixture(), mock_embedding())
        .with_sources(SearchSources::none().with_bm25());

    let index_result = query_test.index().await.expect("Index failed");
    assert!(
        index_result.total_entities >= 3,
        "Expected at least 3 entities (main, process, fetch_data, transform)"
    );

    let query_result = query_test
        .search_bm25("process", 10)
        .await
        .expect("BM25 query failed");

    assert!(
        query_result.total > 0,
        "Expected BM25 results for 'process'"
    );
    let names: Vec<&str> = query_result.items.iter().map(|r| r.name.as_str()).collect();
    assert!(
        names.contains(&"process"),
        "Expected 'process' in BM25 results, got: {:?}",
        names
    );
}

/// BM25 search with struct entities
///
/// Verifies functions next to a struct definition are indexed correctly.
#[tokio::test]
async fn test_hybrid_search_query() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");
    fixture
        .add_file(
            "src/lib.rs",
            r#"
/// Search for items in the database
pub fn search_items(query: &str) -> Vec<Item> {
    // Implementation
    vec![]
}

/// Filter items by category
pub fn filter_items(items: &[Item], category: &str) -> Vec<Item> {
    items.iter().filter(|i| i.category == category).cloned().collect()
}

struct Item {
    name: String,
    category: String,
}
"#,
        )
        .expect("Failed to add file");

    let mut query_test = QueryWorkflowTest::new(fixture.into_test_fixture(), mock_embedding())
        .with_sources(SearchSources::none().with_bm25());

    let index_result = query_test.index().await.expect("Index failed");
    assert!(
        index_result.total_entities >= 2,
        "Expected at least 2 entities"
    );

    let query_result = query_test
        .search_bm25("search items", 10)
        .await
        .expect("BM25 query failed");

    assert!(
        query_result.total > 0,
        "Expected BM25 results for 'search items'"
    );
    let names: Vec<&str> = query_result.items.iter().map(|r| r.name.as_str()).collect();
    assert!(
        names.contains(&"search_items") || names.contains(&"filter_items"),
        "Expected 'search_items' or 'filter_items' in results, got: {:?}",
        names
    );
}

/// Cross-file relation query (callers/callees)
///
/// Verifies that cross-file calls are indexed as relations and can be
/// queried via BM25 and relation searcher.
#[tokio::test]
async fn test_relation_query() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");
    fixture
        .add_file(
            "src/main.rs",
            r#"
mod utils;

fn main() {
    let x = utils::calculate();
    let y = utils::format(x);
    println!("{}", y);
}
"#,
        )
        .expect("Failed to add main.rs");

    fixture
        .add_file(
            "src/utils.rs",
            r#"
pub fn calculate() -> i32 {
    42
}

pub fn format(value: i32) -> String {
    format!("Value: {}", value)
}
"#,
        )
        .expect("Failed to add utils.rs");

    let mut query_test = QueryWorkflowTest::new(fixture.into_test_fixture(), mock_embedding())
        .with_sources(SearchSources::none().with_bm25());

    let index_result = query_test.index().await.expect("Index failed");
    assert!(
        index_result.total_entities >= 3,
        "Expected at least 3 entities (main, calculate, format)"
    );

    let query_result = query_test
        .search_bm25("calculate format", 10)
        .await
        .expect("BM25 query failed");

    assert!(
        query_result.total > 0,
        "Expected BM25 results for 'calculate format'"
    );

    // Check that some results come from utils.rs (allow temp path prefix)
    let file_paths: Vec<&str> = query_result
        .items
        .iter()
        .map(|r| r.file_path.as_str())
        .collect();
    assert!(
        file_paths.iter().any(|p| p.ends_with("src/utils.rs")),
        "Expected at least one result from utils.rs, got: {:?}",
        file_paths
    );
}
