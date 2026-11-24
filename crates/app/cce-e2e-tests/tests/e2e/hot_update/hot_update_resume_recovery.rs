//! Hot-update resume/recovery e2e tests.
//!
//! Simulates process crashes by dropping a coordinator instance mid-operation
//! (or by persisting an equivalent durable crash state) and then rebuilding a
//! fresh coordinator over the same SQLite/BM25/checkpoint state to resume the
//! interrupted operation. All assertions read the durable store — never
//! in-memory coordinator state.
//!
//! Default path is BM25-only (per `tests/workflow/README.md`): no LLM calls,
//! no external vector database.

use cce_config::AstToNlConfig;
use cce_orchestrator::operation::OperationStatus;
use cce_storage_common::MetadataStorage;
use cce_storage_common::metadb::CheckpointStatus;

use crate::helper::init_minimal_logging;
use crate::hot_update::resume_harness::{
    CrashFile, CrashManifest, CrashState, HotUpdateHarness, ProcessorSelection,
};

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

// ===========================================================================
// Crash-point matrix
// ===========================================================================

/// Crash after checkpoint persistence, before candidate preparation.
///
/// Durable state: in_progress operation checkpoint with per-file checkpoints,
/// no building manifest. Resume must re-clone, reprocess every file and
/// publish a complete generation.
#[tokio::test]
async fn test_rc1_crash_before_candidate_reclones_and_reprocesses() {
    init_minimal_logging();

    let harness = HotUpdateHarness::new().await.expect("harness");
    let files = two_file_fixture(&harness);

    // Baseline: change detection must see the files as new.
    let _instance_a = harness
        .build_instance(ProcessorSelection::bm25_only(), None, false)
        .await
        .expect("instance");

    // Simulate the crash state: checkpoint written, no candidate prepared.
    let crash = CrashState {
        operation_id: "rc1-op".to_string(),
        root_dir: ".".to_string(),
        files: files
            .iter()
            .map(|path| CrashFile::reparsing(path.clone()))
            .collect(),
        manifest: CrashManifest::None,
    };
    harness
        .create_crash_state(&crash)
        .await
        .expect("crash state");

    // Resume with the full chain: every file must be re-parsed from scratch.
    let mut instance_b = harness
        .build_instance(ProcessorSelection::bm25_only(), None, false)
        .await
        .expect("instance");
    let result = instance_b.run_hot_update().await.expect("resume failed");

    assert_eq!(
        result.operation_id, "rc1-op",
        "resume must adopt the crashed operation"
    );
    assert_eq!(result.status, OperationStatus::Completed);
    assert_eq!(
        harness.parse_count(),
        2,
        "no envelope -> every file re-parsed"
    );

    let active = harness
        .active_manifest()
        .await
        .expect("manifest")
        .expect("active");
    assert_eq!(active.data_epoch, 1, "first publication must be epoch 1");
    assert_eq!(
        harness.files_for_epoch(1).await,
        2,
        "published generation must hold a file row for every file"
    );
    assert_eq!(harness.entities_for_epoch(1).await, 2);
    assert_eq!(harness.bm25_documents_for_project().await, 2);
    let cp = harness.checkpoint_of("rc1-op").await.expect("checkpoint");
    assert_eq!(cp.status, CheckpointStatus::Completed);
}

/// Crash during the candidate clone.
///
/// Durable state: checkpoint + building manifest with `candidate_ready = 0`.
/// The candidate must NOT be adopted; resume falls back to a fresh clone and
/// processes every file.
#[tokio::test]
async fn test_rc2_unready_candidate_is_not_adopted() {
    init_minimal_logging();

    let harness = HotUpdateHarness::new().await.expect("harness");
    let files = two_file_fixture(&harness);

    let _instance_a = harness
        .build_instance(ProcessorSelection::bm25_only(), None, false)
        .await
        .expect("instance");

    let crash = CrashState {
        operation_id: "rc2-op".to_string(),
        root_dir: ".".to_string(),
        files: files
            .iter()
            .map(|path| CrashFile::reparsing(path.clone()))
            .collect(),
        manifest: CrashManifest::BuildingUnready,
    };
    harness
        .create_crash_state(&crash)
        .await
        .expect("crash state");

    // The manifest exists but was never marked candidate_ready.
    let before = harness
        .building_manifest("rc2-op")
        .await
        .expect("manifest")
        .expect("building manifest exists");
    assert!(!before.candidate_ready, "crash state must be unready");

    let mut instance_b = harness
        .build_instance(ProcessorSelection::bm25_only(), None, false)
        .await
        .expect("instance");
    let result = instance_b.run_hot_update().await.expect("resume failed");

    assert_eq!(result.status, OperationStatus::Completed);
    assert_eq!(
        harness.parse_count(),
        2,
        "unready candidate -> full reprocess"
    );

    let active = harness
        .active_manifest()
        .await
        .expect("manifest")
        .expect("active");
    assert_eq!(active.operation_id, "rc2-op");
    assert_eq!(active.data_epoch, 1);
    assert_eq!(harness.files_for_epoch(1).await, 2);
    assert_eq!(harness.entities_for_epoch(1).await, 2);
    assert_eq!(harness.bm25_documents_for_project().await, 2);
}

/// Crash after the bm25 module completed, before summary/export.
///
/// Durable state: adoptable candidate (`candidate_ready = 1`, next epoch) with
/// bm25 module progress markers on every file checkpoint. Resume must adopt
/// the candidate, skip the completed bm25 module and only fill in the missing
/// modules — without re-parsing or re-indexing anything.
#[tokio::test]
async fn test_rc3_adoptable_candidate_skips_completed_modules() {
    init_minimal_logging();

    let harness = HotUpdateHarness::new().await.expect("harness");
    let files = two_file_fixture(&harness);

    let _instance_a = harness
        .build_instance(ProcessorSelection::bm25_only(), None, false)
        .await
        .expect("instance");

    // Build the crash state: valid envelopes + bm25 module progress markers +
    // an adoptable building candidate.
    let mut crash_files = Vec::new();
    for path in &files {
        let content = std::fs::read(path).expect("read fixture file");
        let marker = harness.bm25_marker_for_content(&content);
        crash_files.push(
            harness
                .envelope_for(std::path::Path::new(path), &[("bm25", marker)])
                .await
                .expect("envelope"),
        );
    }
    let crash = CrashState {
        operation_id: "rc3-op".to_string(),
        root_dir: ".".to_string(),
        files: crash_files,
        manifest: CrashManifest::BuildingReady,
    };
    harness
        .create_crash_state(&crash)
        .await
        .expect("crash state");
    harness
        .seed_candidate_epoch_data(1, &files)
        .await
        .expect("seed candidate data");

    let before_parses = harness.parse_count();
    let before_indexes = harness.bm25_index_count();
    let before_summaries = harness.summary_count();

    let mut instance_b = harness
        .build_instance(ProcessorSelection::full(), None, false)
        .await
        .expect("instance");
    let result = instance_b.run_hot_update().await.expect("resume failed");

    assert_eq!(result.status, OperationStatus::Completed);
    assert_eq!(
        harness.parse_count(),
        before_parses,
        "must not re-parse (envelope reuse)"
    );
    assert_eq!(
        harness.bm25_index_count(),
        before_indexes,
        "must not re-index bm25 (module progress marker matches)"
    );
    assert_eq!(
        harness.summary_count(),
        before_summaries + 2,
        "must generate the missing summaries only"
    );

    // Module progress now covers every module for every file.
    for path in &files {
        let rel = harness.relativize(std::path::Path::new(path));
        let progress = harness.module_progress_of("rc3-op", &rel).await;
        assert!(
            progress.contains_key("bm25"),
            "bm25 marker preserved: {progress:?}"
        );
        assert!(
            progress.contains_key("summary"),
            "summary marker added: {progress:?}"
        );
    }

    let active = harness
        .active_manifest()
        .await
        .expect("manifest")
        .expect("active");
    assert_eq!(
        active.data_epoch, 1,
        "candidate epoch must be published as epoch 1"
    );
    assert_eq!(active.operation_id, "rc3-op");
    assert_eq!(harness.files_for_epoch(1).await, 2);
    assert_eq!(harness.entities_for_epoch(1).await, 2);
    // 2 per-function docs seeded with the candidate in BM25.
    assert_eq!(harness.bm25_documents_for_project().await, 2);
}

/// Abort voids the candidate but leaves module progress
/// markers behind. Resume must detect the voided candidate, clear the markers
/// and redo every module — otherwise the published generation silently loses
/// file data.
#[tokio::test]
async fn test_rc4_aborted_candidate_clears_module_progress_and_redoes_all() {
    init_minimal_logging();

    let harness = HotUpdateHarness::new().await.expect("harness");
    let files = two_file_fixture(&harness);

    // Phase 1: a run whose bm25 module succeeds and whose summary module
    // fails → abort → candidate voided, markers already persisted.
    let _instance_a = harness
        .build_instance(ProcessorSelection::bm25_only(), None, false)
        .await
        .expect("instance");

    // Persist the exact "aborted" durable state by hand: bm25 markers are
    // present, the candidate manifest was marked failed.
    let mut crash_files = Vec::new();
    for path in &files {
        let content = std::fs::read(path).expect("read fixture file");
        let marker = harness.bm25_marker_for_content(&content);
        crash_files.push(
            harness
                .envelope_for(std::path::Path::new(path), &[("bm25", marker)])
                .await
                .expect("envelope"),
        );
    }
    let crash = CrashState {
        operation_id: "rc4-op".to_string(),
        root_dir: ".".to_string(),
        files: crash_files,
        // A building manifest that was subsequently marked failed: the abort
        // path keeps the operation checkpoint in_progress but voids the
        // candidate. Recreate that exact state here.
        manifest: CrashManifest::None,
    };
    harness
        .create_crash_state(&crash)
        .await
        .expect("crash state");
    // Mark the candidate failed (abort) after the markers were persisted.
    {
        harness
            .store
            .manifest_begin_building(1, 1, "rc4-op", None)
            .await
            .expect("inject building manifest");
        harness
            .store
            .manifest_mark_failed(1, "rc4-op", "rc4 abort")
            .await
            .expect("inject failed manifest");
    }

    // Pre-condition: markers survive the abort.
    for path in &files {
        let rel = harness.relativize(std::path::Path::new(path));
        let progress = harness.module_progress_of("rc4-op", &rel).await;
        assert!(
            progress.contains_key("bm25"),
            "stale markers survive the abort ({progress:?})"
        );
    }
    let before_parses = harness.parse_count();
    let before_indexes = harness.bm25_index_count();

    // Resume: the voided candidate must clear the markers and redo bm25.
    let mut instance_b = harness
        .build_instance(ProcessorSelection::full(), None, false)
        .await
        .expect("instance");
    let result = instance_b.run_hot_update().await.expect("resume failed");

    assert_eq!(result.status, OperationStatus::Completed);
    assert_eq!(
        harness.bm25_index_count(),
        before_indexes + 2,
        "every module must redo after the candidate was voided"
    );
    assert_eq!(
        harness.parse_count(),
        before_parses,
        "envelopes stay valid on abort; only modules redo"
    );

    let active = harness
        .active_manifest()
        .await
        .expect("manifest")
        .expect("active");
    assert_eq!(active.data_epoch, 1);
    assert_eq!(
        harness.files_for_epoch(1).await,
        2,
        "published generation must hold a file row for every file"
    );
    assert_eq!(harness.entities_for_epoch(1).await, 2);
    // 2 per-function docs indexed into BM25.
    assert_eq!(harness.bm25_documents_for_project().await, 2);

    for path in &files {
        let rel = harness.relativize(std::path::Path::new(path));
        let progress = harness.module_progress_of("rc4-op", &rel).await;
        assert!(
            progress.contains_key("bm25"),
            "markers rewritten after redo: {progress:?}"
        );
        assert!(
            progress.contains_key("summary"),
            "summary markers present: {progress:?}"
        );
    }
}

/// Crash after manifest activation, before the checkpoint/hash commit.
///
/// The manifest is the durable publication record; resume must short-circuit
/// (commit hashes, mark complete) without preparing a new candidate or
/// publishing another epoch.
#[tokio::test]
async fn test_rc5_active_manifest_short_circuits_resume() {
    init_minimal_logging();

    let harness = HotUpdateHarness::new().await.expect("harness");
    two_file_fixture(&harness);

    let mut instance_a = harness
        .build_instance(ProcessorSelection::bm25_only(), None, false)
        .await
        .expect("instance");
    let result = instance_a.run_hot_update().await.expect("phase 1 failed");
    assert_eq!(result.status, OperationStatus::Completed);

    // Simulate the crash window: the manifest is active but the checkpoint
    // was never marked completed.
    let published_epoch = harness
        .active_manifest()
        .await
        .expect("manifest")
        .expect("active")
        .data_epoch;
    harness
        .reopen_checkpoint(&result.operation_id)
        .await
        .expect("reopen");

    let before_parses = harness.parse_count();
    let before_indexes = harness.bm25_index_count();
    let before_summaries = harness.summary_count();

    let mut instance_b = harness
        .build_instance(ProcessorSelection::full(), None, false)
        .await
        .expect("instance");
    let resume = instance_b.run_hot_update().await.expect("resume failed");

    assert_eq!(resume.status, OperationStatus::Completed);
    assert_eq!(
        harness.parse_count(),
        before_parses,
        "no re-parse on short-circuit"
    );
    assert_eq!(
        harness.bm25_index_count(),
        before_indexes,
        "no re-index on short-circuit"
    );
    assert_eq!(
        harness.summary_count(),
        before_summaries,
        "no re-summarize on short-circuit"
    );

    let active = harness
        .active_manifest()
        .await
        .expect("manifest")
        .expect("active");
    assert_eq!(
        active.data_epoch, published_epoch,
        "no new epoch may be published"
    );
    assert_eq!(active.operation_id, resume.operation_id);
    let cp = harness
        .checkpoint_of(&resume.operation_id)
        .await
        .expect("checkpoint");
    assert_eq!(
        cp.status,
        CheckpointStatus::Completed,
        "checkpoint marked complete"
    );
    assert_eq!(harness.files_for_epoch(published_epoch).await, 2);
    assert_eq!(harness.entities_for_epoch(published_epoch).await, 2);
    assert_eq!(harness.bm25_documents_for_project().await, 2);
}

/// A fully completed operation must not be resumed — a new operation
/// starts and finds no changes.
#[tokio::test]
async fn test_rc6_completed_operation_is_not_resumed() {
    init_minimal_logging();

    let harness = HotUpdateHarness::new().await.expect("harness");
    two_file_fixture(&harness);

    let mut instance_a = harness
        .build_instance(ProcessorSelection::bm25_only(), None, false)
        .await
        .expect("instance");
    let first = instance_a.run_hot_update().await.expect("phase 1 failed");
    let published_epoch = harness
        .active_manifest()
        .await
        .expect("manifest")
        .expect("active")
        .data_epoch;
    // Restart: the completed checkpoint must not be recovered; the fresh
    // operation finds no file changes and returns quickly.
    let before_indexes = harness.bm25_index_count();
    let mut instance_b = harness
        .build_instance(ProcessorSelection::bm25_only(), None, false)
        .await
        .expect("instance");
    let second = instance_b.run_hot_update().await.expect("phase 2 failed");

    assert_ne!(
        second.operation_id, first.operation_id,
        "a completed operation must not be resumed"
    );
    assert_eq!(second.status, OperationStatus::Completed);
    assert_eq!(second.summary.total_files_processed, 0);
    assert_eq!(harness.bm25_index_count(), before_indexes, "no re-indexing");
    let active = harness
        .active_manifest()
        .await
        .expect("manifest")
        .expect("active");
    assert_eq!(active.data_epoch, published_epoch, "no new epoch");

    // The no-change operation must not leave a resumable checkpoint behind.
    let leftover = harness
        .checkpoint_of(&second.operation_id)
        .await
        .expect("checkpoint");
    assert_eq!(
        leftover.status,
        CheckpointStatus::Completed,
        "no-change operations must not leave in_progress checkpoints"
    );
}

// ===========================================================================
// No-duplicate-work and recovery-correctness
// ===========================================================================

/// Changing the file content on disk between crash and resume
/// breaks the disk-hash match and forces a re-parse + module redo.
#[tokio::test]
async fn test_rcv006_disk_change_forces_reparse() {
    init_minimal_logging();

    let harness = HotUpdateHarness::new().await.expect("harness");
    let files = two_file_fixture(&harness);

    let _instance_a = harness
        .build_instance(ProcessorSelection::bm25_only(), None, false)
        .await
        .expect("instance");

    // Crash state with bm25 markers for the ORIGINAL content.
    let mut crash_files = Vec::new();
    for path in &files {
        let content = std::fs::read(path).expect("read fixture file");
        let marker = harness.bm25_marker_for_content(&content);
        crash_files.push(
            harness
                .envelope_for(std::path::Path::new(path), &[("bm25", marker)])
                .await
                .expect("envelope"),
        );
    }
    harness
        .create_crash_state(&CrashState {
            operation_id: "rcv6-op".to_string(),
            root_dir: ".".to_string(),
            files: crash_files,
            manifest: CrashManifest::BuildingReady,
        })
        .await
        .expect("crash state");
    harness
        .seed_candidate_epoch_data(1, &files)
        .await
        .expect("seed candidate data");

    // Change one file on disk before the resume.
    let lib_path = harness.file("src/lib.rs");
    std::fs::write(
        &lib_path,
        "pub fn alpha() -> i32 { 100 }\npub fn gamma() -> i32 { 3 }",
    )
    .expect("modify lib.rs on disk");

    let before_parses = harness.parse_count();
    let before_indexes = harness.bm25_index_count();

    let mut instance_b = harness
        .build_instance(ProcessorSelection::full(), None, false)
        .await
        .expect("instance");
    let result = instance_b.run_hot_update().await.expect("resume failed");
    assert_eq!(result.status, OperationStatus::Completed);

    assert_eq!(
        harness.parse_count(),
        before_parses + 1,
        "RCV-006: the changed file must be re-parsed"
    );
    assert_eq!(
        harness.bm25_index_count(),
        before_indexes + 1,
        "RCV-006: the changed file's bm25 module must redo"
    );

    let active = harness
        .active_manifest()
        .await
        .expect("manifest")
        .expect("active");
    assert_eq!(harness.files_for_epoch(active.data_epoch).await, 2);
    assert_eq!(harness.entities_for_epoch(active.data_epoch).await, 3);
    // 1 merged per-function doc for lib.rs (alpha+gamma merge as a small
    // fragment) + 1 for util.rs in BM25.
    assert_eq!(harness.bm25_documents_for_project().await, 2);
}

/// A chunking-config change between crash and resume changes
/// the module fingerprint and forces the module to redo.
#[tokio::test]
async fn test_rcv007_config_change_forces_module_redo() {
    init_minimal_logging();

    let harness = HotUpdateHarness::new().await.expect("harness");
    let files = two_file_fixture(&harness);

    let _instance_a = harness
        .build_instance(ProcessorSelection::bm25_only(), None, false)
        .await
        .expect("instance");

    // Crash state with bm25 markers computed for the OLD chunking config.
    let mut crash_files = Vec::new();
    for path in &files {
        let content = std::fs::read(path).expect("read fixture file");
        let marker = harness.bm25_marker_for_content(&content);
        crash_files.push(
            harness
                .envelope_for(std::path::Path::new(path), &[("bm25", marker)])
                .await
                .expect("envelope"),
        );
    }
    harness
        .create_crash_state(&CrashState {
            operation_id: "rcv7-op".to_string(),
            root_dir: ".".to_string(),
            files: crash_files,
            manifest: CrashManifest::BuildingReady,
        })
        .await
        .expect("crash state");
    harness
        .seed_candidate_epoch_data(1, &files)
        .await
        .expect("seed candidate data");

    // The resumed run uses a different chunking configuration.
    let mut changed = AstToNlConfig::default();
    changed.chunking.max_tokens = 1_000_000;
    let chunking_config = Some(&changed);

    let before_indexes = harness.bm25_index_count();

    let mut instance_b = harness
        .build_instance(ProcessorSelection::full(), chunking_config, false)
        .await
        .expect("instance");
    let result = instance_b.run_hot_update().await.expect("resume failed");
    assert_eq!(result.status, OperationStatus::Completed);

    assert_eq!(
        harness.bm25_index_count(),
        before_indexes + 2,
        "RCV-007: chunking-config drift must invalidate the bm25 markers"
    );

    let active = harness
        .active_manifest()
        .await
        .expect("manifest")
        .expect("active");
    assert_eq!(harness.files_for_epoch(active.data_epoch).await, 2);
    assert_eq!(harness.entities_for_epoch(active.data_epoch).await, 2);
    // 2 per-function docs indexed into BM25.
    assert_eq!(harness.bm25_documents_for_project().await, 2);
}

/// An in_progress FULL-INDEX checkpoint of the same
/// project must never be adopted as a hot-update resume.
#[tokio::test]
async fn test_rcv008_full_index_checkpoint_is_not_adopted() {
    init_minimal_logging();

    let harness = HotUpdateHarness::new().await.expect("harness");
    let files = two_file_fixture(&harness);

    let _instance_a = harness
        .build_instance(ProcessorSelection::bm25_only(), None, false)
        .await
        .expect("instance");

    // A real full-index in_progress checkpoint (different operation type).
    let crash = CrashState {
        operation_id: "full-index-op".to_string(),
        root_dir: ".".to_string(),
        files: files
            .iter()
            .map(|path| CrashFile::reparsing(path.clone()))
            .collect(),
        manifest: CrashManifest::None,
    };
    harness
        .create_crash_state(&crash)
        .await
        .expect("crash state");
    {
        let mut checkpoint = harness
            .store
            .checkpoint_get(harness.project_id, "full-index-op")
            .await
            .expect("read checkpoint")
            .expect("checkpoint exists");
        checkpoint.operation_type = "full_index".to_string();
        harness
            .store
            .checkpoint_update(harness.project_id, &checkpoint)
            .await
            .expect("rewrite operation type");
    }

    // The hot-update resume must pick a fresh operation, not the full-index one.
    let mut instance_b = harness
        .build_instance(ProcessorSelection::bm25_only(), None, false)
        .await
        .expect("instance");
    let result = instance_b
        .run_hot_update()
        .await
        .expect("hot update failed");
    assert_ne!(
        result.operation_id, "full-index-op",
        "a hot-update must not adopt an in_progress full-index checkpoint"
    );
}

/// An incompatible envelope schema version forces a fresh
/// parse instead of envelope reuse.
#[tokio::test]
async fn test_data006_incompatible_envelope_forces_reparse() {
    init_minimal_logging();

    let harness = HotUpdateHarness::new().await.expect("harness");
    let files = two_file_fixture(&harness);

    let _instance_a = harness
        .build_instance(ProcessorSelection::bm25_only(), None, false)
        .await
        .expect("instance");

    let mut crash_files = Vec::new();
    for path in &files {
        let content = std::fs::read(path).expect("read fixture file");
        let marker = harness.bm25_marker_for_content(&content);
        crash_files.push(
            harness
                .envelope_for(std::path::Path::new(path), &[("bm25", marker)])
                .await
                .expect("envelope"),
        );
    }
    harness
        .create_crash_state(&CrashState {
            operation_id: "data6-op".to_string(),
            root_dir: ".".to_string(),
            files: crash_files,
            manifest: CrashManifest::BuildingReady,
        })
        .await
        .expect("crash state");
    harness
        .seed_candidate_epoch_data(1, &files)
        .await
        .expect("seed candidate data");
    harness
        .tamper_envelope_schema_version("data6-op")
        .await
        .expect("tamper envelopes");

    let before_parses = harness.parse_count();
    let mut instance_b = harness
        .build_instance(ProcessorSelection::bm25_only(), None, false)
        .await
        .expect("instance");
    let result = instance_b.run_hot_update().await.expect("resume failed");
    assert_eq!(result.status, OperationStatus::Completed);
    assert_eq!(
        harness.parse_count(),
        before_parses + 2,
        "DATA-006: incompatible envelopes must be re-parsed"
    );
    let active = harness
        .active_manifest()
        .await
        .expect("manifest")
        .expect("active");
    assert_eq!(harness.files_for_epoch(active.data_epoch).await, 2);
    assert_eq!(harness.entities_for_epoch(active.data_epoch).await, 2);
}

/// A stale checkpoint without module progress (legacy
/// format) falls back to a conservative full redo.
#[tokio::test]
async fn test_data007_legacy_checkpoint_redoes_conservatively() {
    init_minimal_logging();

    let harness = HotUpdateHarness::new().await.expect("harness");
    let files = two_file_fixture(&harness);

    let _instance_a = harness
        .build_instance(ProcessorSelection::bm25_only(), None, false)
        .await
        .expect("instance");

    // Crash state WITH bm25 markers, then strip them like a legacy checkpoint.
    let mut crash_files = Vec::new();
    for path in &files {
        let content = std::fs::read(path).expect("read fixture file");
        let marker = harness.bm25_marker_for_content(&content);
        crash_files.push(
            harness
                .envelope_for(std::path::Path::new(path), &[("bm25", marker)])
                .await
                .expect("envelope"),
        );
    }
    harness
        .create_crash_state(&CrashState {
            operation_id: "data7-op".to_string(),
            root_dir: ".".to_string(),
            files: crash_files,
            manifest: CrashManifest::BuildingReady,
        })
        .await
        .expect("crash state");
    harness
        .seed_candidate_epoch_data(1, &files)
        .await
        .expect("seed candidate data");
    harness
        .clear_module_progress_direct("data7-op")
        .await
        .expect("strip markers");

    let before_indexes = harness.bm25_index_count();
    let mut instance_b = harness
        .build_instance(ProcessorSelection::full(), None, false)
        .await
        .expect("instance");
    let result = instance_b.run_hot_update().await.expect("resume failed");
    assert_eq!(result.status, OperationStatus::Completed);
    assert_eq!(
        harness.bm25_index_count(),
        before_indexes + 2,
        "DATA-007: missing module progress forces every module to redo"
    );
    let active = harness
        .active_manifest()
        .await
        .expect("manifest")
        .expect("active");
    assert_eq!(harness.files_for_epoch(active.data_epoch).await, 2);
    assert_eq!(harness.entities_for_epoch(active.data_epoch).await, 2);
    // 2 per-function docs indexed into BM25.
    assert_eq!(harness.bm25_documents_for_project().await, 2);
}

// ===========================================================================
// Resume data recovery
// ===========================================================================

/// A real run persists full envelopes (parsed
/// file + summary), correct content hashes (disk match) and module progress
/// markers; a resumed run restores them.
#[tokio::test]
async fn test_data001_004_checkpoint_records_are_complete() {
    init_minimal_logging();

    let harness = HotUpdateHarness::new().await.expect("harness");
    two_file_fixture(&harness);

    let mut instance_a = harness
        .build_instance(ProcessorSelection::full(), None, false)
        .await
        .expect("instance");
    let result = instance_a.run_hot_update().await.expect("phase 1 failed");
    assert_eq!(result.status, OperationStatus::Completed);

    for rel in ["src/lib.rs", "src/util.rs"] {
        let record = harness
            .checkpoint_manager
            .get_file_checkpoint(&result.operation_id, rel)
            .await
            .expect("read checkpoint")
            .expect("file checkpoint exists");

        // DATA-001: payload decodes and is compatible.
        use cce_orchestrator::operation::checkpoint::{
            ParsedCheckpointPayload, decode_summary_checkpoint,
        };
        let payload = cce_orchestrator::operation::checkpoint::decode_parsed_checkpoint(
            record.parsed_data.as_deref().expect("parsed data"),
        )
        .expect("payload decodes");
        assert!(payload.is_compatible(), "payload must be compatible");
        let ParsedCheckpointPayload::Parsed(envelope) = payload else {
            panic!("live parse checkpoints must carry a parsed envelope");
        };
        assert_eq!(
            envelope.change_type,
            cce_orchestrator::hot_update::FileChangeType::Added
        );
        let parsed = envelope.parsed_file;
        assert_eq!(parsed.path, rel);

        // DATA-002: disk hash matches the checkpoint content hash.
        let disk = std::fs::read(harness.file(rel)).expect("read file");
        let disk_hash = cce_utils::hash::calculate_hash(&disk);
        assert_eq!(
            record.content_hash.as_deref(),
            Some(disk_hash.as_str()),
            "disk content must match the checkpoint hash"
        );

        // DATA-003: the summary rides in the record's summary column.
        let summary_payload =
            decode_summary_checkpoint(record.summary_data.as_deref().expect("summary data"))
                .expect("summary payload decodes");
        assert!(summary_payload.is_compatible());
        assert!(
            !summary_payload.file_summary.summary_text.is_empty()
                || !summary_payload.file_summary.main_entities.is_empty(),
            "summary must be persisted next to the parse checkpoint"
        );

        // DATA-004: module progress covers bm25 + summary for every file.
        let progress = harness.module_progress_of(&result.operation_id, rel).await;
        assert!(progress.contains_key("bm25"), "bm25 marker: {progress:?}");
        assert!(
            progress.contains_key("summary"),
            "summary marker: {progress:?}"
        );
    }
}

/// Exported documents and their checkpoint markers are
/// restored; a resumed short-circuit leaves the document untouched.
#[tokio::test]
async fn test_data005_export_markers_and_mtime_are_preserved() {
    init_minimal_logging();

    let harness = HotUpdateHarness::new().await.expect("harness");
    two_file_fixture(&harness);

    // Watch mode is required: the export skip validates `export_path`
    // existence against the watch root.
    let mut instance_a = harness
        .build_instance(ProcessorSelection::full(), None, true)
        .await
        .expect("instance");
    let root = harness.root().to_path_buf();
    instance_a
        .coordinator
        .start_watch(&root)
        .await
        .expect("start watch");
    let result = instance_a.run_hot_update().await.expect("phase 1 failed");

    // Documents were exported and marked on the checkpoints.
    let mut mtimes = std::collections::HashMap::new();
    for rel in ["src/lib.rs", "src/util.rs"] {
        let doc = harness.exported_doc_path(rel);
        assert!(
            doc.exists(),
            "exported document must exist: {}",
            doc.display()
        );
        let record = harness
            .checkpoint_manager
            .get_file_checkpoint(&result.operation_id, rel)
            .await
            .expect("read checkpoint")
            .expect("file checkpoint exists");
        assert!(
            record.export_path.is_some(),
            "export_path marker must be persisted"
        );
        assert!(
            record.render_fingerprint.is_some(),
            "render fingerprint must be persisted"
        );
        mtimes.insert(
            doc.to_string_lossy().to_string(),
            std::fs::metadata(&doc)
                .expect("metadata")
                .modified()
                .expect("mtime"),
        );
    }

    // Simulate the crash window after publication.
    harness
        .reopen_checkpoint(&result.operation_id)
        .await
        .expect("reopen");

    let mut instance_b = harness
        .build_instance(ProcessorSelection::full(), None, true)
        .await
        .expect("instance");
    instance_b
        .coordinator
        .start_watch(&root)
        .await
        .expect("start watch");
    let resume = instance_b.run_hot_update().await.expect("resume failed");
    assert_eq!(resume.status, OperationStatus::Completed);

    // DATA-005: documents are not rewritten — mtime stays identical.
    for (doc, mtime) in &mtimes {
        let current = std::fs::metadata(doc)
            .expect("metadata")
            .modified()
            .expect("mtime");
        assert_eq!(&current, mtime, "document must not be re-exported: {doc}");
    }
}

// ===========================================================================
// Full-chain workflow consistency (BM25-only path)
// ===========================================================================

/// Full index → hot update → query. Only the affected file's
/// data changes; the epoch bumps by exactly one; the new symbol is findable.
#[tokio::test]
async fn test_wf001_incremental_update_publishes_and_queries() {
    init_minimal_logging();

    let harness = HotUpdateHarness::new().await.expect("harness");
    harness
        .add_file("src/lib.rs", "pub fn alpha() -> i32 { 1 }")
        .expect("add lib.rs");
    harness
        .add_file("src/util.rs", "pub fn beta() -> i32 { 2 }")
        .expect("add util.rs");

    let initial = harness.full_index().await.expect("full index");
    assert_eq!(initial.total_files, 2);

    let epoch_before = harness
        .active_manifest()
        .await
        .expect("manifest")
        .expect("active")
        .data_epoch;
    assert_eq!(epoch_before, 1);
    assert_eq!(harness.chunks_for_epoch(1).await, 2);

    // Baseline change detection sees no changes.
    let mut instance_a = harness
        .build_instance(ProcessorSelection::full(), None, false)
        .await
        .expect("instance");

    // Modify one file and add a new symbol.
    let lib_path = harness.file("src/lib.rs");
    std::fs::write(
        &lib_path,
        "pub fn alpha() -> i32 { 100 }\npub fn brand_new_symbol() -> i32 { 7 }",
    )
    .expect("modify lib.rs");

    let result = instance_a
        .run_hot_update()
        .await
        .expect("hot update failed");
    assert_eq!(result.status, OperationStatus::Completed);

    let epoch_after = harness
        .active_manifest()
        .await
        .expect("manifest")
        .expect("active")
        .data_epoch;
    assert_eq!(
        epoch_after,
        epoch_before + 1,
        "epoch must bump by exactly one"
    );
    assert_eq!(harness.files_for_epoch(epoch_after).await, 2);
    assert_eq!(harness.entities_for_epoch(epoch_after).await, 3);

    // BM25 sees the new symbol.
    let query = harness.query_bm25("brand_new_symbol").await;
    assert!(
        query
            .iter()
            .any(|(text, path)| { text.contains("brand_new_symbol") || path.contains("lib.rs") }),
        "BM25 must return the new symbol: {query:?}"
    );
}

/// Deleting a file removes its data from the published
/// generation and BM25.
#[tokio::test]
async fn test_wf002_deletion_removes_all_file_data() {
    init_minimal_logging();

    let harness = HotUpdateHarness::new().await.expect("harness");
    harness
        .add_file("src/keep.rs", "pub fn kept() -> i32 { 1 }")
        .expect("add keep.rs");
    harness
        .add_file("src/gone.rs", "pub fn doomed() -> i32 { 2 }")
        .expect("add gone.rs");

    let _ = harness.full_index().await.expect("full index");
    let mut instance_a = harness
        .build_instance(ProcessorSelection::full(), None, false)
        .await
        .expect("instance");

    std::fs::remove_file(harness.file("src/gone.rs")).expect("delete gone.rs");
    let result = instance_a
        .run_hot_update()
        .await
        .expect("deletion hot update failed");
    assert_eq!(result.status, OperationStatus::Completed);

    let epoch = harness
        .active_manifest()
        .await
        .expect("manifest")
        .expect("active")
        .data_epoch;
    let gone_chunks: usize = harness
        .store
        .chunks_by_file(harness.project_id, "src/gone.rs", epoch)
        .await
        .expect("count gone chunks")
        .len();
    assert_eq!(
        gone_chunks, 0,
        "WF-002: deleted file must leave no chunks behind"
    );
    assert_eq!(
        harness.files_for_epoch(epoch).await,
        1,
        "only the kept file remains"
    );
    assert_eq!(harness.entities_for_epoch(epoch).await, 1);
    // 1 per-function doc indexed into BM25 at the published
    // epoch (older retained generations hold gone.rs until GC retires them).
    assert_eq!(harness.bm25_documents_for_epoch(epoch).await, 1);

    let gone_entities: usize = harness
        .store
        .entity_ids_by_file_epoch(harness.project_id, epoch, "src/gone.rs")
        .await
        .expect("count gone entities")
        .len();
    assert_eq!(gone_entities, 0);
}

/// Adding a file makes it searchable; existing data is
/// unchanged.
#[tokio::test]
async fn test_wf003_added_file_becomes_searchable() {
    init_minimal_logging();

    let harness = HotUpdateHarness::new().await.expect("harness");
    harness
        .add_file("src/lib.rs", "pub fn alpha() -> i32 { 1 }")
        .expect("add lib.rs");

    let _ = harness.full_index().await.expect("full index");
    let mut instance_a = harness
        .build_instance(ProcessorSelection::full(), None, false)
        .await
        .expect("instance");

    harness
        .add_file("src/new.rs", "pub fn shiny_new() -> i32 { 42 }")
        .expect("add new.rs");
    let result = instance_a
        .run_hot_update()
        .await
        .expect("add hot update failed");
    assert_eq!(result.status, OperationStatus::Completed);

    let epoch = harness
        .active_manifest()
        .await
        .expect("manifest")
        .expect("active")
        .data_epoch;
    assert_eq!(harness.files_for_epoch(epoch).await, 2);
    assert_eq!(harness.entities_for_epoch(epoch).await, 2);

    let query = harness.query_bm25("shiny_new").await;
    assert!(
        query
            .iter()
            .any(|(text, path)| { text.contains("shiny_new") || path.contains("new.rs") }),
        "BM25 must find the added file: {query:?}"
    );
}

/// An operation with no changes returns quickly, publishes no
/// epoch and leaves no recoverable checkpoint.
#[tokio::test]
async fn test_wf004_noop_path_is_quick_and_clean() {
    init_minimal_logging();

    let harness = HotUpdateHarness::new().await.expect("harness");
    harness
        .add_file("src/lib.rs", "pub fn alpha() -> i32 { 1 }")
        .expect("add lib.rs");

    let _ = harness.full_index().await.expect("full index");
    let mut instance_a = harness
        .build_instance(ProcessorSelection::full(), None, false)
        .await
        .expect("instance");

    let epoch_before = harness
        .active_manifest()
        .await
        .expect("manifest")
        .expect("active")
        .data_epoch;

    let started = std::time::Instant::now();
    let result = instance_a
        .run_hot_update()
        .await
        .expect("noop hot update failed");
    assert_eq!(result.status, OperationStatus::Completed);
    assert_eq!(result.summary.total_files_processed, 0);
    assert!(
        started.elapsed().as_secs() < 30,
        "noop path must return quickly"
    );

    let active = harness
        .active_manifest()
        .await
        .expect("manifest")
        .expect("active");
    assert_eq!(active.data_epoch, epoch_before, "no new epoch on noop");
    // 1 per-function doc indexed by the full index.
    assert_eq!(harness.bm25_documents_for_project().await, 1);

    // A second noop run must not be affected by leftover state from the first.
    let second = instance_a
        .run_hot_update()
        .await
        .expect("second noop failed");
    assert_eq!(second.status, OperationStatus::Completed);
    assert_eq!(second.summary.total_files_processed, 0);
}
