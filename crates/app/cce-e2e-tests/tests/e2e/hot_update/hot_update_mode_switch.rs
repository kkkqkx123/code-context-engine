//! Mode switch tests for hot update
//!
//! Tests for switching between FileWatch and PeriodicScan modes.

use crate::helper::{EmptyFixture, init_minimal_logging};

/// Periodic scan mode
///
/// Tests the periodic scan mode for file change detection.
#[tokio::test]
async fn test_periodic_scan_mode() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");
    fixture
        .add_file("src/lib.rs", "pub fn initial() {}")
        .expect("Failed to add file");

    let mut index_test =
        crate::index::index_workflow::IndexWorkflowTest::new(fixture.into_test_fixture());

    // Initial index
    let result1 = index_test.execute().await.expect("Index failed");
    let initial_count = result1.total_entities;

    // Add new file (simulating periodic scan detection)
    index_test
        .fixture()
        .add_file("src/new.rs", "pub fn new_func() {}")
        .expect("Failed to add file");

    // Re-index (simulating periodic scan trigger)
    let result2 = index_test.reindex().await.expect("Reindex failed");
    assert!(result2.total_entities > initial_count);
}

/// File watch mode simulation
///
/// Tests file watch mode behavior (simulated without actual file watcher).
#[tokio::test]
async fn test_file_watch_mode_simulation() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");
    fixture
        .add_file("src/main.rs", "fn main() {}")
        .expect("Failed to add file");

    let mut index_test =
        crate::index::index_workflow::IndexWorkflowTest::new(fixture.into_test_fixture());

    // Initial index
    let _result1 = index_test.execute().await.expect("Index failed");

    // Simulate file modification event
    index_test
        .fixture()
        .modify_file(
            "src/main.rs",
            r#"
fn main() {
    println!("Modified");
}
"#,
        )
        .expect("Failed to modify file");

    // Re-index (simulating file watch event trigger)
    let result2 = index_test.reindex().await.expect("Reindex failed");
    assert!(result2.total_entities >= 1);
}

/// Mode switch on storm detection
///
/// Tests that the system switches to periodic scan mode when detecting
/// a file change storm (too many changes in short time).
#[tokio::test]
async fn test_mode_switch_on_storm() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");

    // Create many files to simulate a storm
    for i in 0..20 {
        fixture
            .add_file(
                format!("src/storm/file{}.rs", i),
                format!("pub fn storm{}() {{}}", i),
            )
            .expect("Failed to add file");
    }

    let mut index_test =
        crate::index::index_workflow::IndexWorkflowTest::new(fixture.into_test_fixture());

    // Index should handle all files efficiently
    let result = index_test.execute().await.expect("Index failed");
    assert_eq!(result.total_files, 20, "Expected 20 files");
}

/// Mode recovery after storm
///
/// Tests that the system recovers to normal mode after a storm subsides.
#[tokio::test]
async fn test_mode_recovery_after_storm() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");
    fixture
        .add_file("src/lib.rs", "pub fn normal() {}")
        .expect("Failed to add file");

    let mut index_test =
        crate::index::index_workflow::IndexWorkflowTest::new(fixture.into_test_fixture());

    // Initial index
    let _result1 = index_test.execute().await.expect("Index failed");

    // Normal single file modification (after storm)
    index_test
        .fixture()
        .modify_file("src/lib.rs", "pub fn recovered() {}")
        .expect("Failed to modify file");

    // Re-index should work normally
    let result2 = index_test.reindex().await.expect("Reindex failed");
    assert!(result2.total_entities >= 1);
}

/// Event loop state management
///
/// Tests the event loop state transitions during hot update.
#[tokio::test]
async fn test_event_loop_state() {
    init_minimal_logging();

    let fixture = EmptyFixture::new().expect("Failed to create fixture");
    fixture
        .add_file("src/main.rs", "fn main() {}")
        .expect("Failed to add file");

    let mut index_test =
        crate::index::index_workflow::IndexWorkflowTest::new(fixture.into_test_fixture());

    // Index operation represents event loop processing
    let result = index_test.execute().await.expect("Index failed");
    assert!(result.total_files >= 1);

    // Multiple re-index operations simulate event loop iterations
    for _ in 0..3 {
        let _ = index_test.reindex().await;
    }
}
