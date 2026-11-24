//! Watch-triggered hot update and mode switch e2e tests.
//!
//! These tests exercise the production wiring that was previously missing:
//! a real filesystem event must flow through
//! `WatchCoordinator -> event loop -> pending_watch_changes -> background
//! processor -> run_operation`, without any manual `run_operation()` call.
//! They also verify the storm degrade / recovery mode switching driven by
//! `check_and_update_mode`.

use std::time::Duration;

use cce_config::HotUpdateConfig;
use cce_orchestrator::OperationStatus;
use cce_orchestrator::hot_update::HotUpdateMode;
use cce_orchestrator::hot_update::watcher::FileEvent;

use crate::helper::init_minimal_logging;
use crate::hot_update::resume_harness::{
    HotUpdateHarness, ProcessorSelection, fast_watch_config, start_watch_chain, stop_watch_chain,
    wait_until,
};

/// A hot-update config with a very low storm threshold so a handful of events
/// trips the degrade and a short quiet period triggers recovery.
fn storm_config() -> HotUpdateConfig {
    let mut config = HotUpdateConfig::default();
    config.debounce.pending_interval_secs = 1;
    config.debounce.max_wait_time_secs = 5;
    config.file_watch.event_threshold = 3;
    config.file_watch.storm_duration_secs = 0;
    config.file_watch.recovery_threshold = 3;
    config.file_watch.recovery_duration_secs = 1;
    config.file_watch.fallback_interval_secs = 1;
    config
}

/// a real filesystem modification is picked up by the
/// watcher, forwarded through the event loop to `pending_watch_changes`, and
/// consumed by the background processor which runs the full processor chain —
/// with no manual `run_operation()` call.
#[tokio::test]
async fn test_watch_event_auto_triggers_hot_update() {
    init_minimal_logging();

    let harness = HotUpdateHarness::new().await.expect("harness");
    harness
        .add_file("src/lib.rs", "pub fn alpha() -> i32 { 1 }")
        .expect("add lib.rs");

    // Baseline: one indexed file.
    let initial = harness.full_index().await.expect("full index");
    assert_eq!(initial.total_files, 1);

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

    // A new file write must be auto-indexed.
    harness
        .add_file("src/beta.rs", "pub fn beta() -> i32 { 2 }")
        .expect("add beta.rs");

    let indexed = wait_until(
        || async {
            harness.parse_count() >= 1 && {
                // The watched file must be searchable. BM25 retains the
                // previous generation by design, so the raw document count
                // grows across epochs; searchability is the observable.
                let hits = harness.query_bm25("beta").await;
                hits.iter().any(|(_, file)| file == "src/beta.rs")
            }
        },
        Duration::from_secs(30),
    )
    .await;
    assert!(
        indexed,
        "watch create event must auto-trigger the hot update"
    );

    // The baseline file must remain searchable after the watch-triggered
    // update (the changed-file candidate clones it into the new generation).
    let baseline_hits = harness.query_bm25("alpha").await;
    assert!(
        baseline_hits.iter().any(|(_, file)| file == "src/lib.rs"),
        "baseline file must remain searchable after the watch-triggered update"
    );

    // A subsequent modification must also flow through without manual calls.
    let before = harness.parse_count();
    std::fs::write(
        harness.file("src/beta.rs"),
        "pub fn beta() -> i32 { 200 }\npub fn gamma() -> i32 { 3 }",
    )
    .expect("modify beta.rs");

    let modified = wait_until(
        || async { harness.parse_count() > before },
        Duration::from_secs(30),
    )
    .await;
    assert!(
        modified,
        "watch modify event must auto-trigger another hot update"
    );

    // Clean up the real watcher so no notify task outlives the test.
    stop_watch_chain(&coordinator).await;
}

/// an event storm degrades FileWatch to PeriodicScan and a
/// quiet period recovers back to FileWatch (with the watcher restarted).
#[tokio::test]
async fn test_storm_degrades_and_recovers_mode() {
    init_minimal_logging();

    let harness = HotUpdateHarness::new().await.expect("harness");
    // Files must exist on disk so `handle_file_event`'s ownership check can
    // canonicalize them.
    for i in 0..6 {
        harness
            .add_file(
                &format!("src/storm{}.rs", i),
                &format!("pub fn f{i}() -> i32 {{ {i} }}"),
            )
            .expect("add storm file");
    }

    let mut instance = harness
        .build_instance_with_config(
            ProcessorSelection::default(),
            None,
            true,
            true,
            storm_config(),
        )
        .await
        .expect("instance");
    let root = harness.root().to_path_buf();
    instance
        .coordinator
        .start_watch(&root)
        .await
        .expect("start watch");

    // Inject file events directly (mirrors watcher -> event loop -> coordinator).
    // The storm threshold is 3 events/sec; 6 events must trip it.
    for i in 0..6 {
        let path = harness.file(&format!("src/storm{}.rs", i));
        instance
            .coordinator
            .handle_file_event(FileEvent::modified(path))
            .await
            .expect("handle event");
    }

    // Storm detected -> degrade to periodic scan.
    instance
        .coordinator
        .check_and_update_mode()
        .await
        .expect("check mode");
    assert_eq!(
        instance.coordinator.mode(),
        HotUpdateMode::PeriodicScan,
        "event storm must degrade to periodic scan"
    );

    // Wait for the 1s sliding window to clear, then the first recovery check
    // seeds the recovery timer (event rate is now 0).
    tokio::time::sleep(Duration::from_secs(3)).await;
    instance
        .coordinator
        .check_and_update_mode()
        .await
        .expect("check mode");
    assert_eq!(
        instance.coordinator.mode(),
        HotUpdateMode::PeriodicScan,
        "recovery must wait for the recovery duration to elapse"
    );

    // After the recovery duration, the mode returns to file watch.
    tokio::time::sleep(Duration::from_secs(2)).await;
    instance
        .coordinator
        .check_and_update_mode()
        .await
        .expect("check mode");
    assert_eq!(
        instance.coordinator.mode(),
        HotUpdateMode::FileWatch,
        "quiet period must recover to file watch"
    );

    let _ = instance.coordinator.stop_watch().await;
}

/// rapid watch events for the same file coalesce into a single
/// parse: 10 injected Modified events -> exactly one parse and one publish,
/// and the pending queue drains afterwards.
#[tokio::test]
async fn test_watcher_coalesces_rapid_events_for_same_file() {
    init_minimal_logging();

    let harness = HotUpdateHarness::new().await.expect("harness");
    harness
        .add_file("src/burst.rs", "pub fn burst() -> i32 { 1 }")
        .expect("add burst.rs");

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
    instance
        .coordinator
        .start_watch(harness.root())
        .await
        .expect("start watch");

    // One file write usually yields several events; 10 identical events
    // emulate the storm without touching the disk.
    let path = harness.file("src/burst.rs");
    for _ in 0..10 {
        instance
            .coordinator
            .handle_file_event(FileEvent::modified(path.clone()))
            .await
            .expect("handle event");
    }

    let result = instance.run_hot_update().await.expect("hot update failed");
    assert_eq!(result.status, OperationStatus::Completed);
    assert_eq!(
        harness.parse_count(),
        1,
        "10 events for the same path must coalesce into exactly one parse"
    );
    assert_eq!(harness.bm25_index_count(), 1, "the file is indexed once");
    assert_eq!(
        instance.coordinator.pending_changes_len().await,
        0,
        "the pending queue must drain after the batch"
    );

    let active = harness
        .active_manifest()
        .await
        .expect("manifest")
        .expect("active");
    assert_eq!(active.data_epoch, 1, "a single epoch is published");

    let _ = instance.coordinator.stop_watch().await;
}

/// a real filesystem rename flows through the watcher as
/// Deleted(old) + Created(new): the new path becomes searchable and the old
/// path's rows disappear from the published epoch.
#[tokio::test]
async fn test_watcher_renames_to_delete_plus_create() {
    init_minimal_logging();

    let harness = HotUpdateHarness::new().await.expect("harness");
    harness
        .add_file("src/rename_old.rs", "pub fn rename_old() -> i32 { 1 }")
        .expect("add old file");
    harness
        .add_file("src/keep.rs", "pub fn keep() -> i32 { 2 }")
        .expect("add keep file");

    // Baseline: both files indexed and searchable.
    harness.full_index().await.expect("full index");
    assert!(
        harness
            .query_bm25("rename_old")
            .await
            .iter()
            .any(|(_, path)| path == "src/rename_old.rs"),
        "baseline: old file is searchable"
    );

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

    std::fs::rename(
        harness.file("src/rename_old.rs"),
        harness.file("src/rename_new.rs"),
    )
    .expect("rename file");

    // The renamed file must become searchable under its new path (its content
    // still declares the `rename_old` function, so the title keeps that name
    // while the stored file_path tracks the new location).
    let renamed = wait_until(
        || async {
            let hits = harness.query_bm25("rename").await;
            hits.iter().any(|(_, path)| path == "src/rename_new.rs")
        },
        Duration::from_secs(30),
    )
    .await;
    assert!(
        renamed,
        "the rename must index the new path (Created(to) event)"
    );

    // ...and the published generation must no longer show the old path's
    // rows (Deleted(from) event); the untouched file survives. Under the
    // zero-copy inheritance model visibility resolves through the epoch view,
    // not through physical rows of one epoch.
    let active = harness
        .active_manifest()
        .await
        .expect("manifest")
        .expect("active");
    let epoch = active.data_epoch;
    assert!(epoch > 0, "the rename must publish a new epoch");
    assert_eq!(
        harness.visible_file_rows(epoch, "src/rename_old.rs").await,
        0,
        "the old path must be hidden from the published generation"
    );
    assert_eq!(
        harness.visible_file_rows(epoch, "src/keep.rs").await,
        1,
        "the untouched file survives the rename"
    );

    stop_watch_chain(&coordinator).await;
}
