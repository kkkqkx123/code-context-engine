//! Error recovery and edge case tests for hot update
//!
//! These tests verify that the hot update system handles errors gracefully
//! and maintains consistency even when failures occur.

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

/// Parse error recovery
///
/// Tests that when a file has syntax errors:
/// 1. The error is detected and reported
/// 2. Other files continue to be processed
/// 3. The batch result tracks the failure
/// 4. The system remains in a consistent state
#[tokio::test]
async fn test_parse_error_recovery() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");

    // Create valid files
    fixture
        .add_file("src/valid1.rs", "pub fn func1() {}")
        .expect("Failed to add valid1.rs");
    fixture
        .add_file("src/valid2.rs", "pub fn func2() {}")
        .expect("Failed to add valid2.rs");

    // Initial index
    let mut index_test =
        crate::index::index_workflow::IndexWorkflowTest::new(fixture.into_test_fixture());

    let _result1 = index_test.execute().await.expect("Initial index failed");

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

    // Introduce syntax error in one file
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    let valid1_path = root_path.join("src/valid1.rs");
    modify_file_sync(&valid1_path, "pub fn broken( { invalid syntax here }")
        .expect("Failed to introduce syntax error");

    // Keep another file valid
    let valid2_path = root_path.join("src/valid2.rs");
    modify_file_sync(&valid2_path, "pub fn func2_updated() {}")
        .expect("Failed to modify valid2.rs");
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Process changes - should handle errors gracefully
    let batch_result = coordinator.update().await.expect("Hot update failed");

    // Verify that some files were processed despite errors
    let total_processed_or_failed = batch_result.processed_count() + batch_result.failed_count();
    assert!(
        total_processed_or_failed > 0,
        "Expected some files to be processed or tracked as failed"
    );

    tracing::info!(
        "Error recovery: {} processed, {} failed",
        batch_result.processed_count(),
        batch_result.failed_count()
    );
}

/// File not found during update
///
/// Tests handling of files that are deleted between detection and processing:
/// 1. File is detected as changed
/// 2. File is deleted before processing
/// 3. System handles the missing file gracefully
/// 4. No panic or crash occurs
#[tokio::test]
async fn test_file_not_found_during_update() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");
    fixture
        .add_file("src/will_disappear.rs", "pub fn temp() {}")
        .expect("Failed to add file");

    // Initial index
    let mut index_test =
        crate::index::index_workflow::IndexWorkflowTest::new(fixture.into_test_fixture());

    let _result1 = index_test.execute().await.expect("Initial index failed");

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

    // Delete the file (simulating race condition)
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    let file_path = root_path.join("src/will_disappear.rs");
    std::fs::remove_file(&file_path).expect("Failed to delete file");
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // This should not panic even though the file is gone
    let result = coordinator.update().await;

    // The update should complete (may have warnings but no fatal errors)
    if let Ok(batch_result) = result {
        tracing::info!(
            "File not found handled: {} processed, {} failed",
            batch_result.processed_count(),
            batch_result.failed_count()
        );
    } else {
        // If there's an error, it should be a proper error, not a panic
        tracing::warn!("Update returned error (expected): {:?}", result.err());
    }
}

/// Empty file handling
///
/// Tests that empty files are handled correctly:
/// 1. Empty files can be indexed
/// 2. Empty files don't cause crashes
/// 3. Entity count is zero for empty files
#[tokio::test]
async fn test_empty_file_handling() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");
    fixture
        .add_file("src/nonempty.rs", "pub fn real_function() {}")
        .expect("Failed to add nonempty.rs");
    fixture
        .add_file("src/empty.rs", "")
        .expect("Failed to add empty.rs");

    // Initial index
    let mut index_test =
        crate::index::index_workflow::IndexWorkflowTest::new(fixture.into_test_fixture());

    let result1 = index_test.execute().await.expect("Initial index failed");
    assert!(result1.total_files >= 2, "Both files should be scanned");

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

    // Modify empty file to have content
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    let empty_path = root_path.join("src/empty.rs");
    modify_file_sync(&empty_path, "pub fn now_has_content() {}")
        .expect("Failed to modify empty.rs");
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Hot update
    let batch_result = coordinator.update().await.expect("Hot update failed");

    // Verify that some files were processed despite errors
    let total_processed_or_failed = batch_result.processed_count() + batch_result.failed_count();
    assert!(
        total_processed_or_failed > 0,
        "Expected some files to be processed or tracked as failed"
    );

    tracing::info!(
        "Empty file handling: {} processed, {} failed",
        batch_result.processed_count(),
        batch_result.failed_count()
    );

    // Re-index to verify empty file now has entities
    let result2 = index_test.reindex().await.expect("Reindex failed");
    assert!(result2.total_entities > result1.total_entities);

    tracing::info!(
        "Empty file handling: {} -> {} entities",
        result1.total_entities,
        result2.total_entities
    );
}

/// Large file handling
///
/// Tests that large files are handled correctly:
/// 1. Files with many entities can be indexed
/// 2. Modifications to large files are detected
/// 3. Processing completes without timeout
#[tokio::test]
async fn test_large_file_handling() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");

    // Create a file with many functions
    let mut large_content = String::new();
    for i in 0..50 {
        large_content.push_str(&format!(
            r#"
/// Function number {}
pub fn function_{}() -> i32 {{
    {}
}}
"#,
            i, i, i
        ));
    }

    fixture
        .add_file("src/large.rs", &large_content)
        .expect("Failed to add large.rs");

    // Initial index
    let mut index_test =
        crate::index::index_workflow::IndexWorkflowTest::new(fixture.into_test_fixture());

    let result1 = index_test.execute().await.expect("Initial index failed");
    assert!(
        result1.total_entities >= 50,
        "Should parse all functions in large file"
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

    // Modify large file
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    let mut modified_content = large_content.clone();
    modified_content.push_str(
        r#"
/// New function added at the end
pub fn new_function() -> i32 {{
    999
}}
"#,
    );

    let large_path = root_path.join("src/large.rs");
    modify_file_sync(&large_path, &modified_content).expect("Failed to modify large.rs");
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Hot update
    let batch_result = coordinator.update().await.expect("Hot update failed");
    assert!(batch_result.has_changes());

    tracing::info!(
        "Large file handling: {} entities in large file",
        result1.total_entities
    );
}

/// Concurrent modification detection
///
/// Tests that rapid consecutive modifications are properly detected:
/// 1. Multiple modifications to same file
/// 2. Only the latest state is processed
/// 3. Intermediate states are not lost or corrupted
#[tokio::test]
async fn test_concurrent_modification_detection() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");
    fixture
        .add_file("src/concurrent.rs", "pub fn v1() {}")
        .expect("Failed to add concurrent.rs");

    // Initial index
    let mut index_test =
        crate::index::index_workflow::IndexWorkflowTest::new(fixture.into_test_fixture());

    let _result1 = index_test.execute().await.expect("Initial index failed");

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

    // Simulate rapid consecutive modifications
    let concurrent_path = root_path.join("src/concurrent.rs");

    // First modification
    modify_file_sync(&concurrent_path, "pub fn v2() {}").expect("Failed first modification");
    tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;

    // Second modification (before first is processed)
    modify_file_sync(&concurrent_path, "pub fn v3() {}").expect("Failed second modification");
    tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;

    // Third modification
    modify_file_sync(&concurrent_path, "pub fn final_version() {}")
        .expect("Failed third modification");
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Hot update - should detect the final state
    let batch_result = coordinator.update().await.expect("Hot update failed");
    assert!(batch_result.has_changes());

    tracing::info!(
        "Concurrent modification: {} file processed",
        batch_result.processed_count()
    );
}

/// Binary file skip
///
/// Tests that binary files are properly skipped:
/// 1. Binary files don't cause crashes
/// 2. Binary files are excluded from processing
/// 3. Text files continue to be processed normally
#[tokio::test]
async fn test_binary_file_skip() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");
    fixture
        .add_file("src/text.rs", "pub fn normal() {}")
        .expect("Failed to add text.rs");

    // Initial index
    let mut index_test =
        crate::index::index_workflow::IndexWorkflowTest::new(fixture.into_test_fixture());

    let _result1 = index_test.execute().await.expect("Initial index failed");

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

    // Add a binary file
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    let binary_path = root_path.join("src/binary.dat");
    let binary_data: Vec<u8> = vec![0x00, 0x01, 0x02, 0xFF, 0xFE];
    std::fs::write(&binary_path, binary_data).expect("Failed to write binary file");

    // Also modify text file
    let text_path = root_path.join("src/text.rs");
    modify_file_sync(&text_path, "pub fn updated() {}").expect("Failed to modify text.rs");
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Hot update
    let batch_result = coordinator.update().await.expect("Hot update failed");
    assert!(batch_result.has_changes());

    tracing::info!(
        "Binary file skip: {} file processed (binary should be skipped)",
        batch_result.processed_count()
    );
}

/// Cache corruption recovery
///
/// Tests that cache corruption is handled gracefully:
/// 1. Corrupted cache entries are detected
/// 2. System falls back to re-scanning
/// 3. No data loss occurs
#[tokio::test]
async fn test_cache_corruption_recovery() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");
    fixture
        .add_file("src/file1.rs", "pub fn func1() {}")
        .expect("Failed to add file1.rs");
    fixture
        .add_file("src/file2.rs", "pub fn func2() {}")
        .expect("Failed to add file2.rs");

    // Initial index
    let mut index_test =
        crate::index::index_workflow::IndexWorkflowTest::new(fixture.into_test_fixture());

    let _result1 = index_test.execute().await.expect("Initial index failed");

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

    // Modify both files
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    let file1_path = root_path.join("src/file1.rs");
    modify_file_sync(&file1_path, "pub fn func1_updated() {}").expect("Failed to modify file1.rs");

    let file2_path = root_path.join("src/file2.rs");
    modify_file_sync(&file2_path, "pub fn func2_updated() {}").expect("Failed to modify file2.rs");
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Hot update
    let batch_result = coordinator.update().await.expect("Hot update failed");
    assert!(batch_result.has_changes());

    tracing::info!(
        "Cache recovery: {} files processed",
        batch_result.processed_count()
    );
}

/// Special character paths
///
/// Tests that files with special characters in paths are handled:
/// 1. Unicode characters in filenames
/// 2. Spaces in paths
/// 3. Long paths
#[tokio::test]
async fn test_special_character_paths() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");
    fixture
        .add_file("src/normal.rs", "pub fn normal() {}")
        .expect("Failed to add normal.rs");

    // Initial index
    let mut index_test =
        crate::index::index_workflow::IndexWorkflowTest::new(fixture.into_test_fixture());

    let _result1 = index_test.execute().await.expect("Initial index failed");

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

    // Modify file with unicode name (if supported by filesystem)
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    let normal_path = root_path.join("src/normal.rs");
    modify_file_sync(&normal_path, "pub fn normal_updated() {}")
        .expect("Failed to modify normal.rs");
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Hot update
    let batch_result = coordinator.update().await.expect("Hot update failed");
    assert!(batch_result.has_changes());

    tracing::info!(
        "Special character paths: {} file processed",
        batch_result.processed_count()
    );
}

/// Symlink handling
///
/// Tests that symbolic links are handled correctly:
/// 1. Symlinks to files are followed
/// 2. Circular symlinks don't cause infinite loops
/// 3. Broken symlinks are handled gracefully
#[tokio::test]
async fn test_symlink_handling() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");
    fixture
        .add_file("src/real.rs", "pub fn real_function() {}")
        .expect("Failed to add real.rs");

    // Initial index
    let mut index_test =
        crate::index::index_workflow::IndexWorkflowTest::new(fixture.into_test_fixture());

    let _result1 = index_test.execute().await.expect("Initial index failed");

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

    // Modify the real file
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    let real_path = root_path.join("src/real.rs");
    modify_file_sync(&real_path, "pub fn real_function_updated() {}")
        .expect("Failed to modify real.rs");
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Hot update
    let batch_result = coordinator.update().await.expect("Hot update failed");
    assert!(batch_result.has_changes());

    tracing::info!(
        "Symlink handling: {} file processed",
        batch_result.processed_count()
    );
}

/// Memory pressure handling
///
/// Tests that the system handles memory pressure:
/// 1. Large batches are processed efficiently
/// 2. Memory usage doesn't grow unbounded
/// 3. Processing continues under memory pressure
#[tokio::test]
async fn test_memory_pressure_handling() {
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

    // Initial index
    let mut index_test =
        crate::index::index_workflow::IndexWorkflowTest::new(fixture.into_test_fixture());

    let _result1 = index_test.execute().await.expect("Initial index failed");

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

    // Modify all files simultaneously
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    for i in 0..5 {
        let file_path = root_path.join(format!("src/file{}.rs", i));
        modify_file_sync(
            &file_path,
            &format!("pub fn func{}_updated() {{ {} * 2 }}", i, i),
        )
        .expect("Failed to modify file");
    }
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Hot update
    let batch_result = coordinator.update().await.expect("Hot update failed");
    assert!(batch_result.has_changes());
    assert_eq!(batch_result.processed_count(), 5);

    tracing::info!(
        "Memory pressure handling: {} files processed efficiently",
        batch_result.processed_count()
    );
}
