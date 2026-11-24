//! Real mid-batch failure injection tests.
//!
//! Unlike the hand-crafted crash states of `hot_update_resume_recovery.rs`,
//! these tests make a real `UpdateProcessor` return an error through the
//! `FailingProcessor` wrapper, so the abort path (manifest failed, module
//! progress cleared, full redo on resume) executes with genuine failure
//! semantics instead of a simulated durable state.

use std::sync::Arc;

use cce_orchestrator::OperationStatus;

use crate::helper::init_minimal_logging;
use crate::hot_update::resume_harness::{FailingProcessor, HotUpdateHarness, ProcessorSelection};

fn two_file_fixture(harness: &HotUpdateHarness) -> Vec<String> {
    harness
        .add_file("src/lib.rs", "pub fn alpha() -> i32 { 1 }")
        .expect("add lib.rs");
    harness
        .add_file("src/util.rs", "pub fn beta() -> i32 { 2 }")
        .expect("add util.rs");
    vec![
        harness.file("src/lib.rs").to_string_lossy().to_string(),
        harness.file("src/util.rs").to_string_lossy().to_string(),
    ]
}

/// a real bm25 module failure on the first batch aborts the
/// operation (manifest marked failed via `fail_hot_update_candidate`), and a
/// resumed run over the same durable state clears module progress and redoes
/// every file from the persisted parse envelopes (no re-parse).
#[tokio::test]
async fn test_real_bm25_failure_triggers_abort_and_resume() {
    init_minimal_logging();

    let harness = HotUpdateHarness::new().await.expect("harness");
    two_file_fixture(&harness);

    // Phase 1: bm25 fails on its very first process_operation call.
    let instance_a = harness
        .build_instance_with_transform(
            ProcessorSelection::bm25_only(),
            None,
            false,
            false,
            cce_config::HotUpdateConfig::default(),
            |processor| Arc::new(FailingProcessor::new(processor, 1)),
        )
        .await
        .expect("instance a");
    let mut instance_a = instance_a;
    let result_a = instance_a.run_hot_update().await.expect("run 1 failed");

    assert_eq!(
        result_a.status,
        OperationStatus::PartiallyCompleted { failed_count: 2 },
        "the failing module must abort the whole operation"
    );
    assert_eq!(harness.parse_count(), 2, "both files parsed in run 1");
    assert_eq!(
        harness.bm25_index_count(),
        0,
        "the failing call must not index anything"
    );
    // The candidate was prepared and then retired as failed: no active epoch.
    assert!(
        harness.active_manifest().await.expect("manifest").is_none(),
        "an aborted candidate must never be activated"
    );

    // Phase 2: resume with a healthy processor over the same durable state.
    let before_indexes = harness.bm25_index_count();
    let before_parses = harness.parse_count();
    let mut instance_b = harness
        .build_instance(ProcessorSelection::bm25_only(), None, false)
        .await
        .expect("instance b");
    let result_b = instance_b.run_hot_update().await.expect("resume failed");

    assert_eq!(result_b.status, OperationStatus::Completed);
    assert_eq!(
        harness.bm25_index_count(),
        before_indexes + 2,
        "every file's bm25 module must redo after the abort"
    );
    assert_eq!(
        harness.parse_count(),
        before_parses,
        "the abort keeps envelopes valid; no re-parse on resume"
    );

    let active = harness
        .active_manifest()
        .await
        .expect("manifest")
        .expect("active");
    assert_eq!(active.data_epoch, 1);
    assert_eq!(harness.files_for_epoch(1).await, 2);
    assert_eq!(harness.entities_for_epoch(1).await, 2);

    // Both files must be searchable after the resumed publication.
    for (query, file) in [("alpha", "src/lib.rs"), ("beta", "src/util.rs")] {
        let hits = harness.query_bm25(query).await;
        assert!(
            hits.iter().any(|(_, path)| path == file),
            "{file} must be searchable after resume"
        );
    }
}

/// a failure in the second operation batch (the first batch
/// published successfully) aborts only that batch; the next run resumes the
/// interrupted operation and completes it, and the earlier published state
/// stays queryable.
#[tokio::test]
async fn test_partial_batch_failure_continues() {
    init_minimal_logging();

    let harness = HotUpdateHarness::new().await.expect("harness");
    let files = two_file_fixture(&harness);

    // Batch 1: healthy run -> publishes epoch 1.
    let mut instance_a = harness
        .build_instance(ProcessorSelection::bm25_only(), None, false)
        .await
        .expect("instance a");
    let result_1 = instance_a.run_hot_update().await.expect("run 1 failed");
    assert_eq!(result_1.status, OperationStatus::Completed);
    assert_eq!(harness.parse_count(), 2);
    assert_eq!(harness.bm25_index_count(), 2);

    // Batch 2: modify both files; the bm25 module fails on its first call of
    // this run (fresh wrapper instance), aborting the batch.
    for path in &files {
        let content = std::fs::read_to_string(path).expect("read file");
        std::fs::write(path, format!("{content}\n// touched")).expect("modify file");
    }
    let mut instance_b = harness
        .build_instance_with_transform(
            ProcessorSelection::bm25_only(),
            None,
            false,
            false,
            cce_config::HotUpdateConfig::default(),
            |processor| Arc::new(FailingProcessor::new(processor, 1)),
        )
        .await
        .expect("instance b");
    let result_2 = instance_b.run_hot_update().await.expect("run 2 failed");
    assert_eq!(
        result_2.status,
        OperationStatus::PartiallyCompleted { failed_count: 2 },
        "the second batch must abort"
    );
    let before_indexes = harness.bm25_index_count();
    let before_parses = harness.parse_count();
    assert!(before_parses >= 4, "batch 2 parsed the modified files");

    // The earlier published state remains queryable despite the abort.
    let hits = harness.query_bm25("alpha").await;
    assert!(
        hits.iter().any(|(_, path)| path == "src/lib.rs"),
        "batch 1's published state must survive the aborted batch 2"
    );

    // Batch 3: resume the interrupted operation and complete it.
    let mut instance_c = harness
        .build_instance(ProcessorSelection::bm25_only(), None, false)
        .await
        .expect("instance c");
    let result_3 = instance_c.run_hot_update().await.expect("run 3 failed");
    assert_eq!(result_3.status, OperationStatus::Completed);
    assert_eq!(
        harness.bm25_index_count(),
        before_indexes + 2,
        "resumed run redoes every file of the aborted batch"
    );
    assert_eq!(
        harness.parse_count(),
        before_parses,
        "envelopes from the aborted batch are reused"
    );

    // The resumed run republished the touched content into a new epoch.
    let active = harness
        .active_manifest()
        .await
        .expect("manifest")
        .expect("active");
    assert_eq!(active.data_epoch, 2);
}

/// A bm25 processor must tolerate a per-file failure inside its own batch
/// (reported as a module failure, not an error that kills the chain) and the
/// operation result must surface the failure for later retry.
#[tokio::test]
async fn test_module_failure_is_reported_not_silent() {
    init_minimal_logging();

    let harness = HotUpdateHarness::new().await.expect("harness");
    two_file_fixture(&harness);

    // Fail on call 1 of a single-batch run: the failure must be reflected in
    // the result's failed_modules and can_resume, and a follow-up run resumes.
    let mut instance_a = harness
        .build_instance_with_transform(
            ProcessorSelection::bm25_only(),
            None,
            false,
            false,
            cce_config::HotUpdateConfig::default(),
            |processor| Arc::new(FailingProcessor::new(processor, 1)),
        )
        .await
        .expect("instance a");
    let result = instance_a.run_hot_update().await.expect("run failed");
    assert_eq!(
        result.summary.total_files_failed, 2,
        "failure must be counted, not swallowed"
    );
    assert!(
        result.summary.can_resume,
        "a failed operation must be resumable"
    );
    assert!(
        result
            .failed_modules
            .iter()
            .any(|f| f.module_name == "bm25"),
        "the failing module must be reported in failed_modules"
    );

    let mut instance_b = harness
        .build_instance(ProcessorSelection::bm25_only(), None, false)
        .await
        .expect("instance b");
    let resumed = instance_b.run_hot_update().await.expect("resume failed");
    assert_eq!(resumed.status, OperationStatus::Completed);
    assert_eq!(harness.bm25_index_count(), 2, "resume completes the module");
}
