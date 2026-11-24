//! Real debounce behavior tests for hot update.
//!
//! These tests exercise the full `notify watcher -> event loop (GlobalDebounce)
//! -> pending_watch_changes -> background processor` chain, so the debounce
//! windowing and the coordinator integration are verified together instead of
//! through full-index re-runs.
//!
//! Fixture files are always pre-created before the watch chain starts: the
//! recursive notify watcher cannot deliver events for files inside a
//! directory that did not exist when the watch was registered (the new-dir
//! watch races the file creation and silently drops the burst). The tests
//! then trigger events by rewriting file contents.

use std::time::Duration;

use cce_config::HotUpdateConfig;

use crate::helper::init_minimal_logging;
use crate::hot_update::resume_harness::{
    HotUpdateHarness, ProcessorSelection, fast_watch_config, start_watch_chain, stop_watch_chain,
    wait_until,
};

/// A config whose debounce window is long enough to observe accumulation but
/// short enough to keep the test fast.
fn accumulate_config() -> HotUpdateConfig {
    let mut config = fast_watch_config();
    config.debounce.pending_interval_secs = 2;
    config.debounce.max_wait_time_secs = 5;
    config
}

/// several writes inside the debounce window are accumulated
/// into one operation (single batch) rather than processed immediately;
/// nothing is processed before the window elapses.
#[tokio::test]
async fn test_debounce_accumulates_events_in_window() {
    init_minimal_logging();

    let harness = HotUpdateHarness::new().await.expect("harness");
    // Pre-create the files so the watcher sees their directories; the writes
    // below are what trigger the events.
    for i in 0..3 {
        harness
            .add_file(
                &format!("src/batch{}.rs", i),
                &format!("pub fn batch{}() -> i32 {{ {} }}", i, i),
            )
            .expect("add file");
    }

    let mut instance = harness
        .build_instance_with_config(
            ProcessorSelection::bm25_only(),
            None,
            true,
            false,
            accumulate_config(),
        )
        .await
        .expect("instance");
    let coordinator = start_watch_chain(&mut instance, harness.root(), harness.project_id).await;

    // Three writes inside the 2s window.
    for i in 0..3 {
        std::fs::write(
            harness.file(&format!("src/batch{}.rs", i)),
            format!("pub fn batch{}() -> i32 {{ {} }} // v2", i, i),
        )
        .expect("modify file");
    }

    // Nothing may be parsed before the pending interval elapses.
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(
        harness.parse_count(),
        0,
        "events inside the debounce window must not be processed immediately"
    );

    // After the window the single batch is processed: exactly one parse per
    // file, no duplicates, and the pending queue drains.
    let processed = wait_until(
        || async { harness.parse_count() >= 3 },
        Duration::from_secs(30),
    )
    .await;
    assert!(processed, "the debounced batch must be processed");
    assert_eq!(
        harness.parse_count(),
        3,
        "the 3-file batch must be processed exactly once"
    );
    assert_eq!(
        harness.bm25_index_count(),
        3,
        "every batched file must be indexed once"
    );
    assert_eq!(
        coordinator.lock().await.pending_changes_len().await,
        0,
        "the pending queue must drain after processing"
    );

    for i in 0..3 {
        let file = format!("src/batch{}.rs", i);
        let hits = harness.query_bm25(&format!("batch{i}")).await;
        assert!(
            hits.iter().any(|(_, path)| path == &file),
            "{file} must be searchable"
        );
    }

    stop_watch_chain(&coordinator).await;
}

/// a lone event is forwarded automatically once the pending
/// interval elapses (no second event is needed to trigger processing).
#[tokio::test]
async fn test_debounce_triggers_on_pending_interval() {
    init_minimal_logging();

    let harness = HotUpdateHarness::new().await.expect("harness");
    harness
        .add_file("src/lone.rs", "pub fn lone() -> i32 { 7 }")
        .expect("add file");

    let mut instance = harness
        .build_instance_with_config(
            ProcessorSelection::bm25_only(),
            None,
            true,
            false,
            fast_watch_config(),
        )
        .await
        .expect("instance");
    let coordinator = start_watch_chain(&mut instance, harness.root(), harness.project_id).await;

    std::fs::write(harness.file("src/lone.rs"), "pub fn lone() -> i32 { 70 }")
        .expect("modify file");

    let indexed = wait_until(
        || async {
            harness.parse_count() >= 1 && {
                let hits = harness.query_bm25("lone").await;
                hits.iter().any(|(_, path)| path == "src/lone.rs")
            }
        },
        Duration::from_secs(30),
    )
    .await;
    assert!(
        indexed,
        "a lone event must auto-trigger after the pending interval"
    );
    assert_eq!(
        harness.parse_count(),
        1,
        "the lone file parses exactly once"
    );

    stop_watch_chain(&coordinator).await;
}

/// the max-wait guarantee forces processing even when the
/// pending interval is much longer than the wait time and no further events
/// arrive.
#[tokio::test]
async fn test_debounce_max_wait_guarantee() {
    init_minimal_logging();

    let harness = HotUpdateHarness::new().await.expect("harness");
    harness
        .add_file("src/wait.rs", "pub fn waited() -> i32 { 9 }")
        .expect("add file");

    // Pending interval 30s, but max wait 2s: the guarantee must win.
    let mut config = fast_watch_config();
    config.debounce.pending_interval_secs = 30;
    config.debounce.max_wait_time_secs = 2;

    let mut instance = harness
        .build_instance_with_config(ProcessorSelection::bm25_only(), None, true, false, config)
        .await
        .expect("instance");
    let coordinator = start_watch_chain(&mut instance, harness.root(), harness.project_id).await;

    std::fs::write(harness.file("src/wait.rs"), "pub fn waited() -> i32 { 90 }")
        .expect("modify file");

    // Well inside the pending interval but past the max wait: processed.
    let indexed = wait_until(
        || async {
            harness.parse_count() >= 1 && {
                let hits = harness.query_bm25("waited").await;
                hits.iter().any(|(_, path)| path == "src/wait.rs")
            }
        },
        Duration::from_secs(15),
    )
    .await;
    assert!(
        indexed,
        "max-wait must force processing before the pending interval"
    );

    stop_watch_chain(&coordinator).await;
}

/// events arriving after a batch has been consumed are queued
/// for the next batch (nothing is lost).
#[tokio::test]
async fn test_debounce_events_queued_during_processing() {
    init_minimal_logging();

    let harness = HotUpdateHarness::new().await.expect("harness");
    for i in 0..3 {
        harness
            .add_file(
                &format!("src/queued{}.rs", i),
                &format!("pub fn queued{}() -> i32 {{ {} }}", i, i),
            )
            .expect("add file");
    }

    let mut instance = harness
        .build_instance_with_config(
            ProcessorSelection::bm25_only(),
            None,
            true,
            false,
            fast_watch_config(),
        )
        .await
        .expect("instance");
    let coordinator = start_watch_chain(&mut instance, harness.root(), harness.project_id).await;

    // First batch: one file.
    std::fs::write(
        harness.file("src/queued0.rs"),
        "pub fn queued0() -> i32 { 10 }",
    )
    .expect("modify first file");
    let first = wait_until(
        || async { harness.parse_count() >= 1 },
        Duration::from_secs(30),
    )
    .await;
    assert!(first, "first batch must be processed");

    // Two more files arrive (queued while processing / right after).
    std::fs::write(
        harness.file("src/queued1.rs"),
        "pub fn queued1() -> i32 { 20 }",
    )
    .expect("modify second file");
    std::fs::write(
        harness.file("src/queued2.rs"),
        "pub fn queued2() -> i32 { 30 }",
    )
    .expect("modify third file");

    let rest = wait_until(
        || async {
            harness.parse_count() >= 3
                && {
                    let hits = harness.query_bm25("queued1").await;
                    hits.iter().any(|(_, path)| path == "src/queued1.rs")
                }
                && {
                    let hits = harness.query_bm25("queued2").await;
                    hits.iter().any(|(_, path)| path == "src/queued2.rs")
                }
        },
        Duration::from_secs(30),
    )
    .await;
    if !rest {
        tracing::error!(
            parse_count = harness.parse_count(),
            bm25_count = harness.bm25_index_count(),
            pending_len = coordinator.lock().await.pending_changes_len().await,
            "queued watch events were lost"
        );
    }
    assert!(
        rest,
        "events arriving during/after a batch must be queued, not lost"
    );
    assert_eq!(
        harness.parse_count(),
        3,
        "every file must be parsed exactly once across batches"
    );

    stop_watch_chain(&coordinator).await;
}
