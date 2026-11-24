//! Incremental-behaviour guard and embedding-module coverage.
//!
//! Covers the two gaps the hot-update suite had before this file existed:
//!
//! - the incremental guarantee — when 1 of N files changes, only
//!   that file is parsed, embedded and BM25-indexed, and every unchanged file
//!   keeps its entity IDs across the published epoch boundary. This is the
//!   regression net for "a small change triggers a full re-parse/re-embed".
//! - the embedding module's resume skip logic (module progress
//!   markers) — never exercised before, because the harness only counted
//!   parses, summaries and BM25 writes.
//! - cross-epoch entity-ID stability for unchanged files and
//!   for deletion hot updates.
//!
//! Default path is BM25-only per `tests/workflow/README.md`; the embedding
//! processor stores chunk records and entity rows through the shared storage
//! (no external embedder/Qdrant required — vectors are chunk-records only).

use cce_orchestrator::operation::OperationStatus;
use cce_storage_metadb_sqlite::types::CheckpointStatus;

use crate::helper::init_minimal_logging;
use crate::hot_update::resume_harness::{
    CrashManifest, CrashState, HotUpdateHarness, ProcessorSelection,
};

fn three_file_fixture(harness: &HotUpdateHarness) -> Vec<String> {
    harness
        .add_file("src/lib.rs", "pub fn alpha() -> i32 { 1 }")
        .expect("add lib.rs");
    harness
        .add_file("src/util.rs", "pub fn beta() -> i32 { 2 }")
        .expect("add util.rs");
    harness
        .add_file("src/mod.rs", "pub fn gamma() -> i32 { 3 }")
        .expect("add mod.rs");
    vec![
        harness.file("src/lib.rs").to_string_lossy().to_string(),
        harness.file("src/util.rs").to_string_lossy().to_string(),
        harness.file("src/mod.rs").to_string_lossy().to_string(),
    ]
}

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
// the incremental guard
// ===========================================================================

/// a hot update that touches 1 of 3 files must parse, embed and
/// BM25-index exactly that file — nothing more — and must not disturb the
/// entity IDs of the unchanged files across the epoch boundary.
///
/// Any regression that turns a small change into a full re-parse or full
/// re-embed fails this test.
#[tokio::test]
async fn test_guard_001_only_changed_file_is_parsed_embedded_indexed() {
    init_minimal_logging();

    let harness = HotUpdateHarness::new().await.expect("harness");
    let _files = three_file_fixture(&harness);

    let initial = harness.full_index().await.expect("full index");
    assert_eq!(initial.total_files, 3);
    let epoch1 = harness
        .active_manifest()
        .await
        .expect("manifest")
        .expect("active")
        .data_epoch;
    assert_eq!(epoch1, 1);

    let mut instance = harness
        .build_instance(ProcessorSelection::embedding_bm25(), None, false)
        .await
        .expect("instance");

    // Modify one file and add a new symbol.
    std::fs::write(
        harness.file("src/lib.rs"),
        "pub fn alpha() -> i32 { 100 }\npub fn brand_new_symbol() -> i32 { 7 }",
    )
    .expect("modify lib.rs");

    let result = instance.run_hot_update().await.expect("hot update failed");
    assert_eq!(result.status, OperationStatus::Completed);

    // The core incremental guarantee: only the changed file worked.
    assert_eq!(
        harness.parse_count(),
        1,
        "only the changed file must be parsed"
    );
    assert_eq!(
        harness.embedding_count(),
        1,
        "only the changed file must be embedded"
    );
    assert_eq!(
        harness.bm25_index_count(),
        1,
        "only the changed file must be BM25-indexed"
    );

    let epoch2 = harness
        .active_manifest()
        .await
        .expect("manifest")
        .expect("active")
        .data_epoch;
    assert_eq!(epoch2, epoch1 + 1, "epoch must bump by exactly one");

    // Unchanged files keep their source entity IDs (as recorded on their chunk
    // rows) and chunk IDs; the changed file gets fresh source IDs seeded above
    // the previous maximum. The entities table's AUTOINCREMENT row ids are
    // renumbered by every candidate clone, so the stable cross-epoch identity
    // is the source ID carried by chunks, not the row id.
    for path in ["src/util.rs", "src/mod.rs"] {
        let before = harness.chunks_for_file_epoch(epoch1, path).await;
        let after = harness.chunks_for_file_epoch(epoch2, path).await;
        assert!(!before.is_empty(), "{path} must have chunks in epoch 1");
        assert_eq!(
            before, after,
            "unchanged file {path} must keep its chunk ids and source entity ids across the epoch boundary"
        );
    }
    let lib_before = harness.chunks_for_file_epoch(epoch1, "src/lib.rs").await;
    let lib_after = harness.chunks_for_file_epoch(epoch2, "src/lib.rs").await;
    assert!(!lib_before.is_empty());
    assert_eq!(
        lib_after.len(),
        1,
        "modified file keeps one file-level chunk"
    );
    let before_ids: Vec<i64> = lib_before
        .iter()
        .flat_map(|(_, ids)| ids.iter().copied())
        .collect();
    let after_ids: Vec<i64> = lib_after
        .iter()
        .flat_map(|(_, ids)| ids.iter().copied())
        .collect();
    assert_eq!(
        before_ids.len(),
        1,
        "epoch 1 chunk must reference one entity"
    );
    assert_eq!(
        after_ids.len(),
        2,
        "modified file chunk must reference every entity of the fresh parse"
    );
    assert!(
        after_ids.iter().all(|id| !before_ids.contains(id)),
        "changed file must not reuse stale source entity IDs (hot-update seed)"
    );
    let mut sorted_after = after_ids.clone();
    sorted_after.sort_unstable();
    assert!(
        sorted_after.first() >= before_ids.first(),
        "fresh source IDs must be seeded above the previous maximum"
    );

    // The published generation is complete and searchable.
    assert_eq!(harness.files_for_epoch(epoch2).await, 3);
    assert_eq!(harness.entities_for_epoch(epoch2).await, 4);
    let query = harness.query_bm25("brand_new_symbol").await;
    assert!(
        query
            .iter()
            .any(|(text, path)| text.contains("brand_new_symbol") || path.contains("lib.rs")),
        "BM25 must return the new symbol: {query:?}"
    );
}

// ===========================================================================
// Embedding-module resume coverage
// ===========================================================================

/// a crash after the embedding module completed (markers present,
/// candidate adoptable) must skip re-embedding on resume while other modules
/// still redo their work.
#[tokio::test]
async fn test_incr_001_embedding_resume_skips_completed_module() {
    init_minimal_logging();

    let harness = HotUpdateHarness::new().await.expect("harness");
    let files = two_file_fixture(&harness);

    let _instance_a = harness
        .build_instance(ProcessorSelection::embedding_bm25(), None, false)
        .await
        .expect("instance");

    // Crash state: envelopes + embedding module markers + adoptable candidate.
    let mut crash_files = Vec::new();
    for path in &files {
        let content = std::fs::read(path).expect("read fixture file");
        let marker = harness.embedding_marker_for_content(&content);
        crash_files.push(
            harness
                .envelope_for(std::path::Path::new(path), &[("embedding", marker)])
                .await
                .expect("envelope"),
        );
    }
    let crash = CrashState {
        operation_id: "incr1-op".to_string(),
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
    let before_embeds = harness.embedding_count();

    let mut instance_b = harness
        .build_instance(ProcessorSelection::embedding_bm25(), None, false)
        .await
        .expect("instance");
    let result = instance_b.run_hot_update().await.expect("resume failed");

    assert_eq!(result.status, OperationStatus::Completed);
    assert_eq!(
        harness.parse_count(),
        before_parses,
        "embedding resume must not re-parse (envelope reuse)"
    );
    assert_eq!(
        harness.embedding_count(),
        before_embeds,
        "embedding module marker must be honored on resume"
    );
    assert_eq!(
        harness.bm25_index_count(),
        2,
        "bm25 (no marker in crash state) redoes every file — markers are module-scoped"
    );

    // The embedding marker survived the resume on every file checkpoint.
    for path in &files {
        let rel = harness.relativize(std::path::Path::new(path));
        let progress = harness.module_progress_of("incr1-op", &rel).await;
        assert!(
            progress.contains_key("embedding"),
            "embedding marker preserved: {progress:?}"
        );
    }

    let active = harness
        .active_manifest()
        .await
        .expect("manifest")
        .expect("active");
    assert_eq!(
        active.data_epoch, 1,
        "adopted candidate published as epoch 1"
    );
    assert_eq!(harness.files_for_epoch(1).await, 2);
    assert_eq!(harness.entities_for_epoch(1).await, 2);
    let cp = harness.checkpoint_of("incr1-op").await.expect("checkpoint");
    assert_eq!(cp.status, CheckpointStatus::Completed);
}

/// a file changed on disk between crash and resume is the only
/// file that gets re-parsed and re-embedded; the untouched file's modules are
/// still skipped.
#[tokio::test]
async fn test_incr_002_disk_change_forces_reembed_only_changed_file() {
    init_minimal_logging();

    let harness = HotUpdateHarness::new().await.expect("harness");
    let files = two_file_fixture(&harness);

    let _instance_a = harness
        .build_instance(ProcessorSelection::embedding_bm25(), None, false)
        .await
        .expect("instance");

    // Crash state: BOTH module markers on every file + adoptable candidate.
    let mut crash_files = Vec::new();
    for path in &files {
        let content = std::fs::read(path).expect("read fixture file");
        let embed_marker = harness.embedding_marker_for_content(&content);
        let bm25_marker = harness.bm25_marker_for_content(&content);
        crash_files.push(
            harness
                .envelope_for(
                    std::path::Path::new(path),
                    &[("embedding", embed_marker), ("bm25", bm25_marker)],
                )
                .await
                .expect("envelope"),
        );
    }
    let crash = CrashState {
        operation_id: "incr2-op".to_string(),
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

    // The first file changes on disk while the operation is interrupted.
    std::fs::write(&files[0], "pub fn alpha() -> i32 { 42 }").expect("modify lib.rs on disk");

    let before_parses = harness.parse_count();
    let before_embeds = harness.embedding_count();

    let mut instance_b = harness
        .build_instance(ProcessorSelection::embedding_bm25(), None, false)
        .await
        .expect("instance");
    let result = instance_b.run_hot_update().await.expect("resume failed");

    assert_eq!(result.status, OperationStatus::Completed);
    assert_eq!(
        harness.parse_count(),
        before_parses + 1,
        "disk-hash mismatch re-parses exactly the changed file"
    );
    assert_eq!(
        harness.embedding_count(),
        before_embeds + 1,
        "only the changed file is re-embedded"
    );
    assert_eq!(
        harness.bm25_index_count(),
        1,
        "only the changed file's BM25 module redo (unchanged file markers match)"
    );

    let active = harness
        .active_manifest()
        .await
        .expect("manifest")
        .expect("active");
    assert_eq!(active.data_epoch, 1);
    assert_eq!(harness.files_for_epoch(1).await, 2);
    assert_eq!(harness.entities_for_epoch(1).await, 2);
}

// ===========================================================================
// Cross-epoch entity-ID stability
// ===========================================================================

/// a deletion hot update keeps every remaining file's entity IDs
/// stable and removes the deleted file's entities from the new epoch.
#[tokio::test]
async fn test_incr_006_deletion_keeps_survivor_entity_ids_stable() {
    init_minimal_logging();

    let harness = HotUpdateHarness::new().await.expect("harness");
    let _files = three_file_fixture(&harness);

    let initial = harness.full_index().await.expect("full index");
    assert_eq!(initial.total_files, 3);
    let epoch1 = harness
        .active_manifest()
        .await
        .expect("manifest")
        .expect("active")
        .data_epoch;

    let mut instance = harness
        .build_instance(ProcessorSelection::embedding_bm25(), None, false)
        .await
        .expect("instance");

    std::fs::remove_file(harness.file("src/mod.rs")).expect("delete mod.rs");
    let result = instance
        .run_hot_update()
        .await
        .expect("deletion hot update failed");
    assert_eq!(result.status, OperationStatus::Completed);

    let epoch2 = harness
        .active_manifest()
        .await
        .expect("manifest")
        .expect("active")
        .data_epoch;
    assert_eq!(epoch2, epoch1 + 1);

    // A pure deletion embeds nothing and indexes nothing new.
    assert_eq!(harness.parse_count(), 0, "deletion must not parse");
    assert_eq!(harness.embedding_count(), 0, "deletion must not embed");
    assert_eq!(harness.bm25_index_count(), 0, "deletion must not index");

    // Survivors keep their chunk ids and source entity ids; the deleted file's
    // chunks are gone.
    for path in ["src/lib.rs", "src/util.rs"] {
        let before = harness.chunks_for_file_epoch(epoch1, path).await;
        let after = harness.chunks_for_file_epoch(epoch2, path).await;
        assert!(!before.is_empty());
        assert_eq!(
            before, after,
            "survivor {path} must keep its chunks across a deletion"
        );
    }
    assert!(
        harness
            .chunks_for_file_epoch(epoch2, "src/mod.rs")
            .await
            .is_empty(),
        "deleted file must leave no chunks behind"
    );
    assert_eq!(harness.entities_for_epoch(epoch2).await, 2);
    assert_eq!(harness.files_for_epoch(epoch2).await, 2);
}
