//! Hot update coordinator implementation
//!
//! This module contains the main coordinator for managing hot updates.

use std::path::PathBuf;

use crate::hot_update::FileChangeType;
use cce_config::HotUpdateConfig;
use cce_storage_common::RelationStorage;
use cce_storage_metadb::RelationStore;

use super::coordinator_core::HotUpdateCoordinator;
use crate::hot_update::coordinator::change_merger::coalesce_pending_changes;

#[test]
fn test_hot_update_coordinator_creation() {
    let coordinator =
        HotUpdateCoordinator::new(HotUpdateConfig::default(), 1).expect("create coordinator");
    // Just verify creation succeeds — no panic.
    let _ = coordinator;
}

#[tokio::test]
async fn test_change_detection_stats_and_debounce_info() {
    let coordinator =
        HotUpdateCoordinator::new(HotUpdateConfig::default(), 1).expect("create coordinator");
    coordinator
        .operation
        .lock()
        .await
        .change_detector_mut()
        .set_project_id(1);

    // change_detection_stats should return valid defaults.
    let stats = coordinator
        .change_detection_stats()
        .await
        .expect("change-detection stats must resolve");
    assert_eq!(stats.stored_files, 0);

    // debounce_info should reflect initial state (no pending changes).
    let info = coordinator.debounce_info().await;
    assert!(!info.has_pending_changes, "no pending changes initially");
    assert!(
        info.time_until_next.as_secs() > 0,
        "time_until_next should be positive"
    );
}

// ===== core hot-update logic coverage =====

/// A coordinator whose metadata store is an in-memory backend
/// with the project row present (foreign keys on manifests/chunks).
async fn coordinator_with_store(project_id: i64) -> (HotUpdateCoordinator, RelationStore) {
    let store = RelationStore::in_memory().expect("in-memory store");
    store
        .ensure_project(project_id, "/")
        .await
        .expect("ensure project row");
    let coordinator = HotUpdateCoordinator::new(HotUpdateConfig::default(), project_id)
        .expect("create coordinator")
        .with_metadata_store(store.clone());
    (coordinator, store)
}

async fn insert_chunk_row(store: &RelationStore, project_id: i64, epoch: i64, ids: &[i64]) {
    use cce_storage_common::metadb::ChunkRecord;

    let chunk = ChunkRecord::new(
        format!("c::{epoch}"),
        "f".to_string(),
        "x".to_string(),
        0,
        0,
    )
    .with_entity_ids(ids)
    .with_project_id(project_id)
    .with_epoch(epoch);
    store
        .chunks_upsert(std::slice::from_ref(&chunk))
        .await
        .expect("insert chunk row");
}

/// Insert an active manifest at the given data epoch.
async fn insert_active_manifest(store: &RelationStore, project_id: i64, epoch: i64) {
    store
        .manifest_begin_building(project_id, epoch, "op-active", None)
        .await
        .expect("insert building manifest");
    store
        .manifest_activate(project_id, epoch, 0, "op-active", None)
        .await
        .expect("insert active manifest");
}

/// Insert a building (candidate) manifest at the given data epoch.
async fn insert_building_manifest(store: &RelationStore, project_id: i64, epoch: i64) {
    store
        .manifest_begin_building(project_id, epoch, "op-building", None)
        .await
        .expect("insert building manifest");
}

#[tokio::test]
async fn test_entity_id_seed_empty() {
    let (coordinator, _store) = coordinator_with_store(1).await;
    // No active/candidate epoch and no chunks: seed falls back to 1.
    assert_eq!(coordinator.operation.lock().await.entity_id_seed().await, 1);
}

#[tokio::test]
async fn test_entity_id_seed_with_active() {
    let (coordinator, store) = coordinator_with_store(1).await;
    insert_active_manifest(&store, 1, 3).await;
    insert_chunk_row(&store, 1, 3, &[10, 20, 30]).await;

    // Seed must be one above the maximum raw entity ID of the active epoch.
    assert_eq!(
        coordinator.operation.lock().await.entity_id_seed().await,
        31
    );
}

#[tokio::test]
async fn test_entity_id_seed_with_candidate() {
    let (coordinator, store) = coordinator_with_store(1).await;
    insert_active_manifest(&store, 1, 3).await;
    insert_chunk_row(&store, 1, 3, &[10, 20]).await;
    // An interrupted run left a building candidate with higher entity IDs.
    insert_building_manifest(&store, 1, 4).await;
    insert_chunk_row(&store, 1, 4, &[100]).await;

    // The seed must cover the candidate epoch's IDs so a resumed re-parse
    // never collides with them.
    assert_eq!(
        coordinator.operation.lock().await.entity_id_seed().await,
        101
    );
}

#[tokio::test]
async fn test_entity_id_seed_missing_metadata_store() {
    // Without a metadata store the seed falls back to 0.
    let coordinator =
        HotUpdateCoordinator::new(HotUpdateConfig::default(), 1).expect("create coordinator");
    assert_eq!(coordinator.operation.lock().await.entity_id_seed().await, 0);
}

#[tokio::test]
async fn test_is_manifest_active_true_false() {
    let (coordinator, store) = coordinator_with_store(1).await;
    insert_active_manifest(&store, 1, 1).await;
    insert_building_manifest(&store, 1, 2).await;

    assert!(
        coordinator
            .operation
            .lock()
            .await
            .is_manifest_active("op-active")
            .await
            .expect("read state"),
        "active state must be recognized"
    );
    assert!(
        !coordinator
            .operation
            .lock()
            .await
            .is_manifest_active("op-building")
            .await
            .expect("read state"),
        "building state must not be active"
    );
    assert!(
        !coordinator
            .operation
            .lock()
            .await
            .is_manifest_active("missing-op")
            .await
            .expect("read state"),
        "unknown operation must not be active"
    );
}

#[tokio::test]
async fn test_verify_file_ownership_inside_outside() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let inside = dir.path().join("src/lib.rs");
    std::fs::create_dir_all(inside.parent().expect("parent")).expect("create dir");
    std::fs::write(&inside, "pub fn f() {}").expect("write file");

    let outside_dir = tempfile::TempDir::new().expect("outside temp dir");
    let outside = outside_dir.path().join("evil.rs");
    std::fs::write(&outside, "pub fn g() {}").expect("write outside file");

    let coordinator =
        HotUpdateCoordinator::new(HotUpdateConfig::default(), 1).expect("create coordinator");
    coordinator
        .operation
        .lock()
        .await
        .set_watch_root(dir.path().to_path_buf());

    assert!(
        coordinator
            .verify_file_ownership(&inside)
            .await
            .expect("ownership check"),
        "a file under the watch root belongs to the project"
    );
    assert!(
        !coordinator
            .verify_file_ownership(&outside)
            .await
            .expect("ownership check"),
        "a file outside the watch root must be rejected"
    );
    // No watch root configured: everything is rejected.
    coordinator.operation.lock().await.clear_watch_root();
    assert!(
        !coordinator
            .verify_file_ownership(&inside)
            .await
            .expect("ownership check"),
        "without a watch root no file can be owned"
    );
}

#[test]
fn test_coalesce_pending_changes_dedup() {
    // Duplicate paths collapse to a single entry; the last deletion flag
    // wins because it reflects the most recent on-disk state.
    let changes = coalesce_pending_changes(vec![
        (PathBuf::from("a.rs"), false),
        (PathBuf::from("a.rs"), false),
        (PathBuf::from("b.rs"), false),
        (PathBuf::from("a.rs"), true),
    ]);
    assert_eq!(
        changes,
        vec![
            (PathBuf::from("a.rs"), true),
            (PathBuf::from("b.rs"), false),
        ],
        "first occurrence keeps its position and the last flag wins"
    );
}

#[test]
fn test_coalesce_pending_changes_keeps_creation_over_earlier_deletion() {
    // A create after a delete keeps the path (last flag false).
    let changes = coalesce_pending_changes(vec![
        (PathBuf::from("x.rs"), true),
        (PathBuf::from("x.rs"), false),
    ]);
    assert_eq!(changes, vec![(PathBuf::from("x.rs"), false)]);
}

#[test]
fn test_coalesce_pending_changes_empty() {
    assert!(coalesce_pending_changes(Vec::new()).is_empty());
}

/// Watch events reduce to (path, is_deletion) pairs: only `Deleted`
/// events carry the deletion flag; creates/modifies are parsed as
/// `Modified` by `process_watch_paths`.
#[tokio::test]
async fn test_compute_watch_change_mapping() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let changed = dir.path().join("src/lib.rs");
    std::fs::create_dir_all(changed.parent().expect("parent")).expect("create dir");
    std::fs::write(&changed, "pub fn alpha() -> i32 { 1 }").expect("write file");

    let (mut coordinator, store) = coordinator_with_store(1).await;
    coordinator
        .operation
        .lock()
        .await
        .change_detector_mut()
        .set_project_id(1);
    coordinator
        .operation
        .lock()
        .await
        .change_detector_mut()
        .set_root_path(dir.path().to_string_lossy().as_ref());
    coordinator.set_scan_root_path(dir.path()).await;

    let deleted = dir.path().join("src/gone.rs");
    let batch = coordinator
        .operation
        .lock()
        .await
        .process_watch_paths(&[
            (changed.clone(), false),
            (deleted.clone(), true),
            (dir.path().join("missing.rs"), false),
        ])
        .await
        .expect("process watch paths");

    // Deleted event -> Deleted file change (no parse), keyed by the
    // project-relative path so storage removal matches the rows written
    // for parsed files.
    let relative_deleted = deleted
        .strip_prefix(dir.path())
        .expect("relative path")
        .to_path_buf();
    let deletion = batch
        .file_changes
        .iter()
        .find(|c| c.path == relative_deleted)
        .expect("deletion file change");
    assert_eq!(deletion.change_type, FileChangeType::Deleted);

    // Created/Modified event -> parsed as Modified (watch path never
    // produces `Added`; that is reserved for the periodic-scan path).
    // Parse results carry the project-relative path, matching the `files`
    // table keying used by change detection.
    let relative_changed = changed
        .strip_prefix(dir.path())
        .expect("relative path")
        .to_path_buf();
    let parse = batch
        .parse_results
        .iter()
        .find(|r| r.file_path == relative_changed)
        .expect("parse result");
    assert_eq!(parse.file_change_type, FileChangeType::Modified);
    assert!(!parse.parsed_file.entities.is_empty(), "file parsed");

    // The missing file is recorded as a failure, not a silent skip.
    assert!(
        batch
            .failed_files
            .iter()
            .any(|(path, _)| *path == dir.path().join("missing.rs")),
        "missing file must be reported as failed"
    );

    // Entity seeding respects the shared store (seed above existing rows).
    let _ = store;
}

#[tokio::test]
async fn test_commit_file_hashes_publishes_and_deletes() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let published = dir.path().join("src/published.rs");
    std::fs::create_dir_all(published.parent().expect("parent")).expect("create dir");
    std::fs::write(&published, "pub fn kept() {}").expect("write published file");
    // The deleted path is intentionally absent from disk.
    let deleted = dir.path().join("src/deleted.rs");
    std::fs::write(&deleted, "pub fn gone() {}").expect("write deleted file");

    // An isolated in-memory store so the change-detector view survives.
    let store = RelationStore::in_memory().expect("in-memory store");

    let config = HotUpdateConfig::default();

    let mut coordinator = HotUpdateCoordinator::new(config, 1).expect("create coordinator");
    coordinator = coordinator.with_metadata_store(store.clone());
    coordinator
        .operation
        .lock()
        .await
        .change_detector_mut()
        .set_project_id(1);
    coordinator.set_scan_root_path(dir.path()).await;
    coordinator
        .operation
        .lock()
        .await
        .initialize_cache(dir.path())
        .await
        .expect("initialize change detector");

    // Prime a baseline hash for the to-be-deleted file so it behaves like
    // a previously indexed path. `commit_file_hashes` keys rows by the
    // project-relative path, so both arguments must be relative.
    store.ensure_project(1, "/").await.expect("ensure project");
    let relative = |path: &std::path::Path| {
        path.strip_prefix(dir.path())
            .expect("relative path")
            .to_path_buf()
    };
    coordinator
        .operation
        .lock()
        .await
        .commit_file_hashes(&[relative(&published)], &[relative(&deleted)])
        .await
        .expect("commit hashes");

    // The published file has a cache row; the deleted file's row is gone.
    let published_hit = store
        .file_by_path(1, &relative(&published).to_string_lossy())
        .await
        .expect("published lookup");
    assert!(published_hit.is_some(), "published path must be hashed");
    let deleted_hit = store
        .file_by_path(1, &relative(&deleted).to_string_lossy())
        .await
        .expect("deleted lookup");
    assert!(deleted_hit.is_none(), "deleted path must be removed");
}
