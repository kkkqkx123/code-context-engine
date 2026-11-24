//! Envelope-lost resume e2e tests.
//!
//! Covers the "envelope lost" resume window: without a persisted parse
//! envelope the resumed operation falls back to a full local re-parse of
//! every changed file from disk. The republished generation must be
//! identical in shape to the one produced by live processing, and disk
//! drift between crash and resume must still be picked up correctly.
//!
//! Default path is BM25-only (per `tests/workflow/README.md`): no LLM calls,
//! no external vector database.

use cce_orchestrator::operation::OperationStatus;

use crate::helper::init_minimal_logging;
use crate::hot_update::resume_harness::{
    CrashFile, CrashManifest, CrashState, HotUpdateHarness, ProcessorSelection,
};

const LIB_REL: &str = "src/lib.rs";
const UTIL_REL: &str = "src/util.rs";

/// Warm-up: full index at C0, then a real hot update to C1.
async fn warm_up(harness: &HotUpdateHarness) -> anyhow::Result<i64> {
    harness.add_file(LIB_REL, "pub fn alpha() -> i32 { 1 }")?;
    harness.add_file(UTIL_REL, "pub fn beta() -> i32 { 2 }")?;
    let _ = harness.full_index().await?;

    std::fs::write(
        harness.file(LIB_REL),
        "pub fn alpha() -> i32 { 10 }\npub fn lib_extra() -> i32 { 11 }",
    )?;
    std::fs::write(
        harness.file(UTIL_REL),
        "pub fn beta() -> i32 { 20 }\npub fn util_extra() -> i32 { 21 }",
    )?;

    let mut instance = harness
        .build_instance(ProcessorSelection::bm25_only(), None, false)
        .await?;
    let result = instance.run_hot_update().await?;
    assert_eq!(result.status, OperationStatus::Completed);

    let epoch = harness
        .active_manifest()
        .await?
        .expect("active manifest after warm-up")
        .data_epoch;
    Ok(epoch)
}

fn crash_files_for_current_disk(harness: &HotUpdateHarness) -> anyhow::Result<Vec<CrashFile>> {
    let mut files = Vec::new();
    for rel in [LIB_REL, UTIL_REL] {
        let content = std::fs::read(harness.file(rel))?;
        files.push(CrashFile {
            // Checkpoints record the absolute on-disk path; resume derives
            // the project-relative identity itself.
            path: harness.file(rel).to_string_lossy().into_owned(),
            envelope: None,
            content_hash: Some(cce_utils::hash::calculate_hash(&content)),
            module_progress: None,
        });
    }
    Ok(files)
}

/// Envelope lost, disk unchanged: resume re-parses both files from disk and
/// publishes a generation identical in shape to the one produced by live
/// processing.
#[tokio::test]
async fn test_envelope_lost_resume_reparses_and_matches_baseline() {
    init_minimal_logging();

    let harness = HotUpdateHarness::new().await.expect("harness");
    let baseline_epoch = warm_up(&harness).await.expect("warm-up failed");
    assert_eq!(baseline_epoch, 2);
    let baseline_files = harness.files_for_epoch(baseline_epoch).await;
    let baseline_entities = harness.entities_for_epoch(baseline_epoch).await;
    let baseline_chunks = harness.chunks_for_epoch(baseline_epoch).await;
    assert_eq!(baseline_entities, 4, "two functions per file");

    // Crash state without envelopes but with correct content hashes: the
    // resume must take the local re-parse fallback for every changed file.
    harness
        .create_crash_state(&CrashState {
            operation_id: "reparse-op".to_string(),
            root_dir: ".".to_string(),
            files: crash_files_for_current_disk(&harness).expect("crash files"),
            manifest: CrashManifest::None,
        })
        .await
        .expect("crash state");

    let before_parses = harness.parse_count();
    let mut instance = harness
        .build_instance(ProcessorSelection::bm25_only(), None, false)
        .await
        .expect("instance");
    let result = instance.run_hot_update().await.expect("resume failed");
    assert_eq!(result.status, OperationStatus::Completed);

    assert_eq!(
        harness.parse_count(),
        before_parses + 2,
        "every envelope-lost file must go through the local re-parse"
    );

    // The published generation matches what live processing produced.
    let resumed_epoch = harness
        .active_manifest()
        .await
        .expect("manifest")
        .expect("active")
        .data_epoch;
    assert_eq!(
        resumed_epoch,
        baseline_epoch + 1,
        "resume must publish a fresh generation"
    );
    assert_eq!(harness.files_for_epoch(resumed_epoch).await, baseline_files);
    assert_eq!(
        harness.entities_for_epoch(resumed_epoch).await,
        baseline_entities
    );
    assert_eq!(
        harness.chunks_for_epoch(resumed_epoch).await,
        baseline_chunks
    );

    // The rebuilt data is queryable: BM25 finds the warm-up symbols.
    let query = harness.query_bm25("lib_extra").await;
    assert!(
        query.iter().any(|(_, path)| path.contains("lib.rs")),
        "BM25 must see the rebuilt file: {query:?}"
    );
}

/// Disk content drifted for one file after the crash: the re-parse fallback
/// picks up the drifted content while the unchanged file keeps its entities.
#[tokio::test]
async fn test_envelope_lost_resume_handles_drifted_file() {
    init_minimal_logging();

    let harness = HotUpdateHarness::new().await.expect("harness");
    let _baseline_epoch = warm_up(&harness).await.expect("warm-up failed");

    harness
        .create_crash_state(&CrashState {
            operation_id: "drift-op".to_string(),
            root_dir: ".".to_string(),
            files: crash_files_for_current_disk(&harness).expect("crash files"),
            manifest: CrashManifest::None,
        })
        .await
        .expect("crash state");

    // Drift lib.rs after the checkpoint recorded its hash.
    std::fs::write(harness.file(LIB_REL), "pub fn alpha() -> i32 { 999 }").expect("drift lib.rs");

    let before_parses = harness.parse_count();
    let mut instance = harness
        .build_instance(ProcessorSelection::bm25_only(), None, false)
        .await
        .expect("instance");
    let result = instance.run_hot_update().await.expect("resume failed");
    assert_eq!(result.status, OperationStatus::Completed);

    assert_eq!(
        harness.parse_count(),
        before_parses + 2,
        "both files are rebuilt through the local re-parse"
    );

    let resumed_epoch = harness
        .active_manifest()
        .await
        .expect("manifest")
        .expect("active")
        .data_epoch;
    assert_eq!(harness.files_for_epoch(resumed_epoch).await, 2);
    assert_eq!(
        harness.entities_for_epoch(resumed_epoch).await,
        3,
        "drifted lib.rs holds one function again, util.rs keeps two"
    );
}
