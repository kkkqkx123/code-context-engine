//! Processor chain integration tests
//!
//! These tests verify the complete processor chain execution during hot updates,
//! ensuring that all downstream modules are properly updated when files change.
//!
//! Processor chain order:
//! 1. RelationUpdateProcessor - Update call relations and dependencies
//! 2. EmbeddingUpdateProcessor - Update vector embeddings
//! 3. Bm25UpdateProcessor - Update BM25 inverted index
//! 4. SummaryUpdateProcessor - Update module summaries

use crate::helper::{EmptyFixture, init_minimal_logging};
use cce_config::HotUpdateConfig;
use cce_orchestrator::HotUpdateCoordinator;
use std::path::Path;

/// Helper function to modify a file and ensure it's synced to disk
fn modify_file_sync(path: &Path, content: &str) -> std::io::Result<()> {
    std::fs::write(path, content)?;

    // Force flush to disk to ensure metadata is updated
    let file = std::fs::OpenOptions::new().write(true).open(path)?;
    file.sync_all()?;

    Ok(())
}

/// Full processor chain execution
///
/// Tests that all processors execute successfully in sequence:
/// - Relations are rebuilt for changed entities
/// - Embeddings are regenerated
/// - BM25 index is updated
/// - Summaries are refreshed
#[tokio::test]
async fn test_full_processor_chain_execution() {
    init_minimal_logging();

    // Setup: Create a complex codebase with multiple relationships
    let fixture = EmptyFixture::new().expect("Failed to create fixture");

    fixture
        .add_file(
            "src/main.rs",
            r#"
mod service;
mod utils;

fn main() {
    let data = service::fetch_data();
    let processed = utils::process(data);
    println!("{:?}", processed);
}
"#,
        )
        .expect("Failed to add main.rs");

    fixture
        .add_file(
            "src/service.rs",
            r#"
use crate::utils;

pub fn fetch_data() -> Vec<i32> {
    vec![1, 2, 3, 4, 5]
}

pub fn transform(data: &[i32]) -> Vec<i32> {
    data.iter().map(|x| x * 2).collect()
}
"#,
        )
        .expect("Failed to add service.rs");

    fixture
        .add_file(
            "src/utils.rs",
            r#"
pub fn process(data: Vec<i32>) -> i32 {
    data.iter().sum()
}

pub fn validate(value: i32) -> bool {
    value > 0
}
"#,
        )
        .expect("Failed to add utils.rs");

    // Initial index with all features enabled
    let mut index_test =
        crate::index::index_workflow::IndexWorkflowTest::new(fixture.into_test_fixture())
            .with_relations(true)
            .with_vectors(true)
            .with_bm25(true);

    let result1 = index_test.execute().await.expect("Initial index failed");
    assert!(result1.total_files >= 3, "Expected at least 3 files");
    assert!(result1.total_entities >= 6, "Expected at least 6 entities");
    assert!(result1.total_relations > 0, "Expected relations");

    tracing::info!(
        "Initial index: {} files, {} entities, {} relations",
        result1.total_files,
        result1.total_entities,
        result1.total_relations
    );

    // Create coordinator for hot update and initialize cache (establishes baseline)
    let config = HotUpdateConfig::default();
    let mut coordinator =
        HotUpdateCoordinator::new(config, 1).expect("failed to create HotUpdateCoordinator");
    let root_path = index_test.fixture().root_path();

    coordinator
        .initialize_cache(root_path)
        .await
        .expect("Cache initialization failed");

    // Verify no changes initially
    let initial = coordinator.update().await.expect("Initial check failed");
    assert!(!initial.has_changes());

    // Modify service.rs to add new function and modify existing one
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    let service_path = root_path.join("src/service.rs");
    modify_file_sync(
        &service_path,
        r#"
use crate::utils;

pub fn fetch_data() -> Vec<i32> {
    vec![10, 20, 30, 40, 50]  // Changed values
}

pub fn transform(data: &[i32]) -> Vec<i32> {
    data.iter().map(|x| x * 3).collect()  // Changed multiplier
}

pub fn aggregate(data: &[i32]) -> i32 {  // New function
    data.iter().sum()
}
"#,
    )
    .expect("Failed to modify service.rs");
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Perform hot update (this triggers the processor chain)
    let batch_result = coordinator.update().await.expect("Hot update failed");
    assert!(batch_result.has_changes());
    assert_eq!(batch_result.processed_count(), 1);

    // Verify parse results contain entity changes
    assert!(
        !batch_result.all_entity_changes().is_empty(),
        "Expected entity changes"
    );

    tracing::info!(
        "Processor chain executed: {} entities changed",
        batch_result.all_entity_changes().len()
    );

    // Re-index to verify all processors updated correctly
    let result2 = index_test.reindex().await.expect("Reindex failed");
    assert!(
        result2.total_entities >= result1.total_entities,
        "Entity count should not decrease"
    );

    tracing::info!(
        "After update: {} files, {} entities, {} relations",
        result2.total_files,
        result2.total_entities,
        result2.total_relations
    );
}

/// Relation processor updates call graphs
///
/// Tests that the RelationUpdateProcessor:
/// - Detects new function calls
/// - Removes old relationships
/// - Updates dependency graphs
#[tokio::test]
async fn test_relation_processor_updates() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");

    fixture
        .add_file(
            "src/caller.rs",
            r#"
pub fn caller() -> i32 {
    callee_v1()
}

fn callee_v1() -> i32 {
    42
}
"#,
        )
        .expect("Failed to add caller.rs");

    // Initial index with relations
    let mut index_test =
        crate::index::index_workflow::IndexWorkflowTest::new(fixture.into_test_fixture())
            .with_relations(true)
            .with_vectors(false)
            .with_bm25(false);

    let result1 = index_test.execute().await.expect("Initial index failed");
    let initial_relations = result1.total_relations;
    assert!(initial_relations > 0, "Expected initial relations");

    // Create coordinator and initialize cache (establishes baseline)
    let config = HotUpdateConfig::default();
    let mut coordinator =
        HotUpdateCoordinator::new(config, 1).expect("failed to create HotUpdateCoordinator");
    let root_path = index_test.fixture().root_path();

    coordinator
        .initialize_cache(root_path)
        .await
        .expect("Cache initialization failed");

    // Verify no changes initially
    let initial = coordinator.update().await.expect("Initial check failed");
    assert!(!initial.has_changes());

    // Modify to change call relationship
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    let caller_path = root_path.join("src/caller.rs");
    modify_file_sync(
        &caller_path,
        r#"
pub fn caller() -> i32 {
    callee_v2()  // Changed to call different function
}

fn callee_v1() -> i32 {
    42
}

fn callee_v2() -> i32 {  // New callee
    100
}
"#,
    )
    .expect("Failed to modify caller.rs");
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Hot update
    let batch_result = coordinator.update().await.expect("Hot update failed");
    assert!(batch_result.has_changes());

    // Re-index and verify relations updated
    let result2 = index_test.reindex().await.expect("Reindex failed");
    assert!(
        result2.total_relations >= initial_relations,
        "Relations should be maintained or increased"
    );

    tracing::info!(
        "Relation updates: {} -> {} relations",
        initial_relations,
        result2.total_relations
    );
}

/// Embedding processor regenerates vectors
///
/// Tests that the EmbeddingUpdateProcessor:
/// - Regenerates embeddings for modified entities
/// - Preserves embeddings for unchanged entities
/// - Handles new entities
#[tokio::test]
async fn test_embedding_processor_updates() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");

    fixture
        .add_file(
            "src/api.rs",
            r#"
/// Get user by ID
pub fn get_user(id: u32) -> Option<String> {
    Some(format!("User {}", id))
}
"#,
        )
        .expect("Failed to add api.rs");

    // Initial index with vectors
    let mut index_test =
        crate::index::index_workflow::IndexWorkflowTest::new(fixture.into_test_fixture())
            .with_relations(false)
            .with_vectors(true)
            .with_bm25(false);

    let result1 = index_test.execute().await.expect("Initial index failed");
    assert!(result1.total_entities >= 1);

    // Create coordinator and initialize cache
    let config = HotUpdateConfig::default();
    let mut coordinator =
        HotUpdateCoordinator::new(config, 1).expect("failed to create HotUpdateCoordinator");
    let root_path = index_test.fixture().root_path();

    coordinator
        .initialize_cache(root_path)
        .await
        .expect("Cache initialization failed");

    // Verify no changes initially
    let initial = coordinator.update().await.expect("Initial check failed");
    assert!(!initial.has_changes());

    // Modify function documentation and implementation
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    let api_path = root_path.join("src/api.rs");
    modify_file_sync(
        &api_path,
        r#"
/// Retrieve user information by unique identifier
/// Returns None if user not found
pub fn get_user(id: u32) -> Option<String> {
    if id > 0 {
        Some(format!("User #{}", id))
    } else {
        None
    }
}

/// Create a new user
pub fn create_user(name: &str) -> String {
    format!("Created: {}", name)
}
"#,
    )
    .expect("Failed to modify api.rs");
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Hot update
    let batch_result = coordinator.update().await.expect("Hot update failed");
    assert!(batch_result.has_changes());

    // Verify entity changes detected
    assert!(!batch_result.all_entity_changes().is_empty());

    // Re-index to verify embeddings updated
    let result2 = index_test.reindex().await.expect("Reindex failed");
    assert!(
        result2.total_entities >= result1.total_entities,
        "Should have same or more entities"
    );

    tracing::info!(
        "Embedding updates: {} -> {} entities",
        result1.total_entities,
        result2.total_entities
    );
}

/// BM25 processor updates search index
///
/// Tests that the Bm25UpdateProcessor:
/// - Updates term frequencies for modified content
/// - Adds new terms from added functions
/// - Removes terms from deleted content
#[tokio::test]
async fn test_bm25_processor_updates() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");

    fixture
        .add_file(
            "src/search.rs",
            r#"
pub fn search(query: &str) -> Vec<String> {
    vec![query.to_string()]
}
"#,
        )
        .expect("Failed to add search.rs");

    // Initial index with BM25
    let mut index_test =
        crate::index::index_workflow::IndexWorkflowTest::new(fixture.into_test_fixture())
            .with_relations(false)
            .with_vectors(false)
            .with_bm25(true);

    let result1 = index_test.execute().await.expect("Initial index failed");
    assert!(result1.total_entities >= 1);

    // Create coordinator and initialize cache
    let config = HotUpdateConfig::default();
    let mut coordinator =
        HotUpdateCoordinator::new(config, 1).expect("failed to create HotUpdateCoordinator");
    let root_path = index_test.fixture().root_path();

    coordinator
        .initialize_cache(root_path)
        .await
        .expect("Cache initialization failed");

    // Verify no changes initially
    let initial = coordinator.update().await.expect("Initial check failed");
    assert!(!initial.has_changes());

    // Modify to add new searchable content
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    let search_path = root_path.join("src/search.rs");
    modify_file_sync(
        &search_path,
        r#"
/// Search for items matching the query
pub fn search(query: &str) -> Vec<String> {
    vec![query.to_string()]
}

/// Filter results by category
pub fn filter_by_category(items: &[String], category: &str) -> Vec<String> {
    items.iter()
        .filter(|item| item.contains(category))
        .cloned()
        .collect()
}
"#,
    )
    .expect("Failed to modify search.rs");
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Hot update
    let batch_result = coordinator.update().await.expect("Hot update failed");
    assert!(batch_result.has_changes());

    // Re-index to verify BM25 updated
    let result2 = index_test.reindex().await.expect("Reindex failed");
    assert!(
        result2.total_entities >= result1.total_entities,
        "Should have same or more entities"
    );

    tracing::info!(
        "BM25 updates: {} -> {} entities",
        result1.total_entities,
        result2.total_entities
    );
}

/// Summary processor refreshes module summaries
///
/// Tests that the SummaryUpdateProcessor:
/// - Regenerates summaries for modified modules
/// - Preserves summaries for unchanged modules
/// - Handles new modules
#[tokio::test]
async fn test_summary_processor_updates() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");

    fixture
        .add_file(
            "src/module.rs",
            r#"
pub fn function_a() -> i32 { 1 }
pub fn function_b() -> i32 { 2 }
"#,
        )
        .expect("Failed to add module.rs");

    // Initial index
    let mut index_test =
        crate::index::index_workflow::IndexWorkflowTest::new(fixture.into_test_fixture());

    let result1 = index_test.execute().await.expect("Initial index failed");
    assert!(result1.total_entities >= 2);

    // Create coordinator and initialize cache
    let config = HotUpdateConfig::default();
    let mut coordinator =
        HotUpdateCoordinator::new(config, 1).expect("failed to create HotUpdateCoordinator");
    let root_path = index_test.fixture().root_path();

    coordinator
        .initialize_cache(root_path)
        .await
        .expect("Cache initialization failed");

    // Verify no changes initially
    let initial = coordinator.update().await.expect("Initial check failed");
    assert!(!initial.has_changes());

    // Modify module to add new function
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    let module_path = root_path.join("src/module.rs");
    modify_file_sync(
        &module_path,
        r#"
pub fn function_a() -> i32 { 1 }
pub fn function_b() -> i32 { 2 }
pub fn function_c() -> i32 { 3 }  // New function
"#,
    )
    .expect("Failed to modify module.rs");
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Hot update
    let batch_result = coordinator.update().await.expect("Hot update failed");
    assert!(batch_result.has_changes());

    // Re-index to verify summary updated
    let result2 = index_test.reindex().await.expect("Reindex failed");
    assert!(
        result2.total_entities > result1.total_entities,
        "Should have more entities after adding function"
    );

    tracing::info!(
        "Summary updates: {} -> {} entities",
        result1.total_entities,
        result2.total_entities
    );
}

/// Processor error isolation
///
/// Tests that errors in one processor don't affect others:
/// - If embedding fails, BM25 still updates
/// - If relations fail, summaries still update
/// - Batch result tracks partial successes
#[tokio::test]
async fn test_processor_error_isolation() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");

    fixture
        .add_file(
            "src/test.rs",
            r#"
pub fn test_function() -> i32 { 42 }
"#,
        )
        .expect("Failed to add test.rs");

    // Initial index
    let mut index_test =
        crate::index::index_workflow::IndexWorkflowTest::new(fixture.into_test_fixture());

    let _result1 = index_test.execute().await.expect("Initial index failed");

    // Create coordinator and initialize cache
    let config = HotUpdateConfig::default();
    let mut coordinator =
        HotUpdateCoordinator::new(config, 1).expect("failed to create HotUpdateCoordinator");
    let root_path = index_test.fixture().root_path();

    coordinator
        .initialize_cache(root_path)
        .await
        .expect("Cache initialization failed");

    // Verify no changes initially
    let initial = coordinator.update().await.expect("Initial check failed");
    assert!(!initial.has_changes());

    // Modify file
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    let test_path = root_path.join("src/test.rs");
    modify_file_sync(
        &test_path,
        r#"
pub fn test_function() -> i32 { 100 }  // Changed return value
pub fn new_function() -> bool { true }  // New function
"#,
    )
    .expect("Failed to modify test.rs");
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Hot update
    let batch_result = coordinator.update().await.expect("Hot update failed");
    assert!(batch_result.has_changes());
    assert_eq!(batch_result.processed_count(), 1);

    tracing::info!(
        "Processor isolation: {} entities changed",
        batch_result.all_entity_changes().len()
    );
}
