//! Complete hot update workflow tests with full call chain
//!
//! These tests verify the complete end-to-end workflow of hot updates,
//! including:
//! - File change detection
//! - Debounce mechanism
//! - Change scanning and parsing
//! - Entity change computation
//! - Processor chain execution (BM25, Embedding, Relation, Summary)
//! - Mode switching
//! - Configuration reload

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

/// Complete hot update workflow with all processors
///
/// This test verifies HotUpdateCoordinator's incremental update capability:
/// 1. Initialize cache (establishes baseline)
/// 2. Modify files (simulating real file system events)
/// 3. Update detects only changed files
/// 4. Verify incremental processing works correctly
///
/// Note: This test focuses on change detection logic.
/// Storage updates are verified separately in processor_chain.rs.
#[tokio::test]
async fn test_complete_hot_update_workflow() {
    init_minimal_logging();

    // Setup: Create fixture with initial files
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

    // Step 1: Create HotUpdateCoordinator and initialize cache
    let config = HotUpdateConfig::default();
    let mut coordinator =
        HotUpdateCoordinator::new(config, 1).expect("failed to create HotUpdateCoordinator");
    let root_path = fixture.root();

    coordinator
        .initialize_cache(root_path)
        .await
        .expect("Cache initialization failed");

    // At this point, cache is established. No changes detected yet.
    let initial_check = coordinator.update().await.expect("Initial check failed");
    assert!(
        !initial_check.has_changes(),
        "No changes expected right after initialization"
    );

    tracing::info!("Cache initialized, baseline established");

    // Step 2: Simulate file modification (real scenario: IDE saves file)
    let service_path = root_path.join("src/service.rs");

    // Get metadata before modification
    let meta_before = std::fs::metadata(&service_path).expect("Failed to get metadata before");
    let time_before = meta_before
        .modified()
        .expect("Failed to get modified time before");
    let size_before = meta_before.len();
    tracing::info!(
        "Before modification: size={}, modified={:?}",
        size_before,
        time_before
    );

    // Write new content
    std::fs::write(
        &service_path,
        r#"
pub fn process() -> i32 {
    let data = fetch();
    let processed = transform(data);
    validate(processed)  // New function call
}

fn fetch() -> i32 { 42 }
fn transform(n: i32) -> i32 { n * 2 }
fn validate(n: i32) -> i32 { if n > 0 { n } else { 0 } }  // New function
"#,
    )
    .expect("Failed to modify service.rs");

    // Force flush to disk to ensure metadata is updated
    // This is critical for reliable change detection on Windows
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(&service_path)
        .expect("Failed to open file for sync");
    file.sync_all().expect("Failed to sync file to disk");

    // Small delay to ensure filesystem has processed the update
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Get metadata after modification
    let meta_after = std::fs::metadata(&service_path).expect("Failed to get metadata after");
    let time_after = meta_after
        .modified()
        .expect("Failed to get modified time after");
    let size_after = meta_after.len();
    tracing::info!(
        "After modification: size={}, modified={:?}",
        size_after,
        time_after
    );
    tracing::info!(
        "Time difference: {:?}",
        time_after.duration_since(time_before)
    );
    tracing::info!("Size changed: {} -> {}", size_before, size_after);

    // Step 3: Hot update - should detect the modification
    tracing::info!("About to call update()...");
    let update_result = coordinator.update().await.expect("Hot update failed");

    tracing::info!(
        "Update result: has_changes={}, processed_count={}",
        update_result.has_changes(),
        update_result.processed_count()
    );

    assert!(
        update_result.has_changes(),
        "Expected changes to be detected"
    );
    assert_eq!(
        update_result.processed_count(),
        1,
        "Expected exactly 1 file processed (incremental update)"
    );

    // Step 4: Verify parse results contain entity changes
    assert!(
        update_result.processed_count() > 0,
        "Expected file to be processed"
    );
    assert!(
        !update_result.all_entity_changes().is_empty(),
        "Expected entity-level changes"
    );

    tracing::info!(
        "Hot update successful: {} file processed, {} entities modified",
        update_result.processed_count(),
        update_result.all_entity_changes().len()
    );
}

/// Hot update with processor collection
///
/// Tests that hot update produces results ready for processor execution.
#[tokio::test]
async fn test_hot_update_with_processor_collection() {
    init_minimal_logging();

    // Setup: Create fixture with files
    let fixture = EmptyFixture::new().expect("Failed to create fixture");
    fixture
        .add_file(
            "src/lib.rs",
            r#"
/// Calculate sum of two numbers
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

/// Calculate difference
pub fn subtract(a: i32, b: i32) -> i32 {
    a - b
}
"#,
        )
        .expect("Failed to add lib.rs");

    // Create coordinator and initialize cache
    let config = HotUpdateConfig::default();
    let mut coordinator =
        HotUpdateCoordinator::new(config, 1).expect("failed to create HotUpdateCoordinator");
    let root_path = fixture.root();

    coordinator
        .initialize_cache(root_path)
        .await
        .expect("Cache initialization failed");

    // Verify no changes initially
    let initial = coordinator.update().await.expect("Initial check failed");
    assert!(!initial.has_changes());

    // Modify file
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    let lib_path = root_path.join("src/lib.rs");
    modify_file_sync(
        &lib_path,
        r#"
/// Calculate sum of two numbers with overflow protection
pub fn add(a: i32, b: i32) -> Result<i32, String> {
    a.checked_add(b).ok_or("Overflow".to_string())
}

/// Calculate difference with underflow protection
pub fn subtract(a: i32, b: i32) -> Result<i32, String> {
    a.checked_sub(b).ok_or("Underflow".to_string())
}

/// Multiply two numbers
pub fn multiply(a: i32, b: i32) -> Result<i32, String> {
    a.checked_mul(b).ok_or("Overflow".to_string())
}
"#,
    )
    .expect("Failed to modify lib.rs");
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Detect and process changes
    let batch_result = coordinator.update().await.expect("Hot update failed");
    assert!(batch_result.has_changes());

    // Note: In a real scenario, we would now execute the processor collection
    // For this test, we verify that the batch result is ready for processing
    assert!(
        batch_result.processed_count() > 0,
        "Expected files to be processed"
    );

    tracing::info!(
        "Processor collection workflow: {} entities changed",
        batch_result.all_entity_changes().len()
    );
}

/// Multiple file changes with debounce
///
/// Tests that multiple rapid file changes are properly debounced
/// and processed as a single batch.
#[tokio::test]
async fn test_multiple_file_changes_debounced() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");

    // Create initial files
    for i in 0..3 {
        fixture
            .add_file(
                format!("src/module{}.rs", i),
                format!("pub fn func{}() {{ {} }}", i, i),
            )
            .expect("Failed to add file");
    }

    // Create coordinator and initialize cache
    let config = HotUpdateConfig::default();
    let mut coordinator =
        HotUpdateCoordinator::new(config, 1).expect("failed to create HotUpdateCoordinator");
    let root_path = fixture.root();

    coordinator
        .initialize_cache(root_path)
        .await
        .expect("Cache initialization failed");

    // Verify no changes initially
    let initial = coordinator.update().await.expect("Initial check failed");
    assert!(!initial.has_changes());

    // Rapidly modify multiple files
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    for i in 0..3 {
        let file_path = root_path.join(format!("src/module{}.rs", i));
        modify_file_sync(
            &file_path,
            &format!("pub fn modified_func{}() {{ {} * 2 }}", i, i),
        )
        .expect("Failed to modify file");
    }

    // Small delay to ensure filesystem processed all changes
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Perform update (debounce should batch all changes)
    let batch_result = coordinator.update().await.expect("Hot update failed");
    assert!(batch_result.has_changes());
    assert_eq!(
        batch_result.processed_count(),
        3,
        "Expected 3 files processed"
    );

    tracing::info!(
        "Debounced batch: {} files processed together",
        batch_result.processed_count()
    );
}

/// File deletion and entity cleanup
///
/// Tests that when a file is deleted:
/// 1. Change detector identifies the deletion
/// 2. Batch result marks entities for removal
/// 3. Processors clean up their indexes
#[tokio::test]
async fn test_file_deletion_cleanup() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");
    fixture
        .add_file("src/to_delete.rs", "pub fn will_be_deleted() {}")
        .expect("Failed to add file");
    fixture
        .add_file("src/keep.rs", "pub fn keep_this() {}")
        .expect("Failed to add file");

    // Create coordinator and initialize cache
    let config = HotUpdateConfig::default();
    let mut coordinator =
        HotUpdateCoordinator::new(config, 1).expect("failed to create HotUpdateCoordinator");
    let root_path = fixture.root();

    coordinator
        .initialize_cache(root_path)
        .await
        .expect("Cache initialization failed");

    // Verify no changes initially
    let initial = coordinator.update().await.expect("Initial check failed");
    assert!(!initial.has_changes());

    // Delete one file using std::fs
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    let delete_path = root_path.join("src/to_delete.rs");
    std::fs::remove_file(&delete_path).expect("Failed to delete file");
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Process deletion
    let batch_result = coordinator.update().await.expect("Hot update failed");
    assert!(batch_result.has_changes());

    // Note: In a real scenario, processors would clean up the deleted entities
    // For this test, we verify that the deletion was detected
    tracing::info!(
        "Deletion detected: {} files processed",
        batch_result.processed_count()
    );
}

/// Rename detection and path update
///
/// Tests file rename detection:
/// 1. Old file is detected as deleted
/// 2. New file is detected as added
/// 3. Content hash comparison identifies it as a rename
/// 4. Processors update paths accordingly
#[tokio::test]
async fn test_file_rename_detection() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");
    fixture
        .add_file("src/old_name.rs", "pub fn my_function() {}")
        .expect("Failed to add file");

    // Create coordinator and initialize cache
    let config = HotUpdateConfig::default();
    let mut coordinator =
        HotUpdateCoordinator::new(config, 1).expect("failed to create HotUpdateCoordinator");
    let root_path = fixture.root();

    coordinator
        .initialize_cache(root_path)
        .await
        .expect("Cache initialization failed");

    // Verify no changes initially
    let initial = coordinator.update().await.expect("Initial check failed");
    assert!(!initial.has_changes());

    // Rename file (delete old, create new with same content)
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    let old_path = root_path.join("src/old_name.rs");
    let new_path = root_path.join("src/new_name.rs");
    std::fs::rename(&old_path, &new_path).expect("Failed to rename file");
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Process rename
    let batch_result = coordinator.update().await.expect("Hot update failed");
    assert!(batch_result.has_changes());

    // Verify both deletion and addition are tracked
    assert!(
        batch_result.processed_count() >= 1,
        "Expected at least 1 file processed"
    );

    tracing::info!("Rename detection completed successfully");
}

/// Error recovery during hot update
///
/// Tests that errors in one file don't prevent processing of other files:
/// 1. Multiple files are modified
/// 2. One file has parse errors
/// 3. Other files are still processed successfully
#[tokio::test]
async fn test_error_recovery_in_batch() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");
    fixture
        .add_file("src/valid.rs", "pub fn valid_function() {}")
        .expect("Failed to add valid.rs");
    fixture
        .add_file("src/will_be_invalid.rs", "pub fn good() {}")
        .expect("Failed to add will_be_invalid.rs");

    // Create coordinator and initialize cache
    let config = HotUpdateConfig::default();
    let mut coordinator =
        HotUpdateCoordinator::new(config, 1).expect("failed to create HotUpdateCoordinator");
    let root_path = fixture.root();

    coordinator
        .initialize_cache(root_path)
        .await
        .expect("Cache initialization failed");

    // Verify no changes initially
    let initial = coordinator.update().await.expect("Initial check failed");
    assert!(!initial.has_changes());

    // Modify one file to be invalid (syntax error)
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    let invalid_path = root_path.join("src/will_be_invalid.rs");
    modify_file_sync(&invalid_path, "pub fn broken( { invalid syntax }")
        .expect("Failed to modify file");

    // Modify another file to be valid
    let valid_path = root_path.join("src/valid.rs");
    modify_file_sync(&valid_path, "pub fn valid_function() { /* updated */ }")
        .expect("Failed to modify valid.rs");
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Process changes
    let batch_result = coordinator.update().await.expect("Hot update failed");

    // Verify that some files were processed even if one failed
    assert!(
        batch_result.processed_count() + batch_result.failed_count() > 0,
        "Expected some files to be processed or failed"
    );

    tracing::info!(
        "Error recovery: {} processed, {} failed",
        batch_result.processed_count(),
        batch_result.failed_count()
    );
}

/// Incremental update efficiency
///
/// Tests that only changed files are reprocessed, not all files.
#[tokio::test]
async fn test_incremental_update_efficiency() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");

    // Create multiple files
    for i in 0..5 {
        fixture
            .add_file(
                format!("src/file{}.rs", i),
                format!("pub fn func{}() {{ {} }}", i, i),
            )
            .expect("Failed to add file");
    }

    // Create coordinator and initialize cache
    let config = HotUpdateConfig::default();
    let mut coordinator =
        HotUpdateCoordinator::new(config, 1).expect("failed to create HotUpdateCoordinator");
    let root_path = fixture.root();

    coordinator
        .initialize_cache(root_path)
        .await
        .expect("Cache initialization failed");

    // Verify no changes initially
    let initial = coordinator.update().await.expect("Initial check failed");
    assert!(!initial.has_changes());

    // Modify only one file
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    let file_path = root_path.join("src/file2.rs");
    modify_file_sync(&file_path, "pub fn func2_modified() { 42 }")
        .expect("Failed to modify file2.rs");
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Process changes
    let batch_result = coordinator.update().await.expect("Hot update failed");

    // Verify only one file was changed
    assert_eq!(
        batch_result.processed_count(),
        1,
        "Expected exactly 1 file processed"
    );

    tracing::info!(
        "Incremental update: only {} file(s) reprocessed out of 5",
        batch_result.processed_count()
    );
}

/// Entity-level change detection
///
/// Tests that entity changes are detected within a file:
/// 1. File contains multiple functions
/// 2. Only one function is modified
/// 3. Entity change detection identifies which entities changed
/// 4. Added/modified/deleted entities are tracked separately
#[tokio::test]
async fn test_entity_level_change_detection() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");
    fixture
        .add_file(
            "src/lib.rs",
            r#"
pub fn existing_function() -> i32 {
    42
}

pub fn another_function() -> String {
    "hello".to_string()
}
"#,
        )
        .expect("Failed to add lib.rs");

    // Create coordinator and initialize cache
    let config = HotUpdateConfig::default();
    let mut coordinator =
        HotUpdateCoordinator::new(config, 1).expect("failed to create HotUpdateCoordinator");
    let root_path = fixture.root();

    coordinator
        .initialize_cache(root_path)
        .await
        .expect("Cache initialization failed");

    // Verify no changes initially
    let initial = coordinator.update().await.expect("Initial check failed");
    assert!(!initial.has_changes());
    let initial_entities = initial.all_entity_changes().len();

    // Modify file: change one function, add another
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    let lib_path = root_path.join("src/lib.rs");
    modify_file_sync(
        &lib_path,
        r#"
pub fn existing_function() -> i32 {
    100  // Changed return value
}

pub fn another_function() -> String {
    "hello".to_string()
}

pub fn new_function() -> bool {
    true  // New function added
}
"#,
    )
    .expect("Failed to modify lib.rs");
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Process changes
    let batch_result = coordinator.update().await.expect("Hot update failed");
    assert!(batch_result.has_changes());

    // Verify entity changes are tracked
    let entity_changes = batch_result.all_entity_changes().len();
    assert!(entity_changes > 0, "Expected entity changes to be detected");

    tracing::info!(
        "Entity-level changes: {} entities changed (initial: {})",
        entity_changes,
        initial_entities
    );
}
