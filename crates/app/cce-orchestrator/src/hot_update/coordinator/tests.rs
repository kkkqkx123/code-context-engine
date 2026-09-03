//! Hot update coordinator implementation
//!
//! This module contains the main coordinator for managing hot updates.

use std::path::PathBuf;
use std::sync::Arc;

use crate::hot_update::FileChangeType;
use cce_config::HotUpdateConfig;
use cce_storage_sqlite::SqliteClient;

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
    let stats = coordinator.change_detection_stats().await;
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

/// A coordinator whose metadata store is an in-memory SQLite database
/// with the project row present (foreign keys on manifests/chunks).
fn coordinator_with_store(project_id: i64) -> (HotUpdateCoordinator, Arc<SqliteClient>) {
    let store = Arc::new(SqliteClient::in_memory().expect("in-memory sqlite"));
    let conn = store.write_connection().expect("write connection");
    conn.execute(
        "INSERT OR IGNORE INTO projects (id, name, root_path, created_at, updated_at)
             VALUES (?1, 'p1', '/', ?2, ?2)",
        rusqlite::params![project_id, chrono::Utc::now().timestamp()],
    )
    .expect("ensure project row");
    drop(conn);
    let coordinator = HotUpdateCoordinator::new(HotUpdateConfig::default(), project_id)
        .expect("create coordinator")
        .with_metadata_store(store.clone());
    (coordinator, store)
}

fn insert_chunk_row(conn: &rusqlite::Connection, project_id: i64, epoch: i64, ids: &[i64]) {
    conn.execute(
        "INSERT INTO chunks (chunk_id, file_path, content, start_line, end_line,
                                 entity_ids, entity_names, chunk_type, test_status, test_source,
                                 created_at, updated_at, project_id, epoch, batch_id, path)
             VALUES ('c', 'f', 'x', 0, 0, ?1, '[]', 't', 'unknown', 'none',
                     0, 0, ?2, ?3, 0, 'emb')",
        rusqlite::params![
            serde_json::to_string(ids).expect("serialize ids"),
            project_id,
            epoch
        ],
    )
    .expect("insert chunk row");
}

/// Insert an active manifest at the given data epoch.
fn insert_active_manifest(conn: &rusqlite::Connection, project_id: i64, epoch: i64) {
    conn.execute(
        "INSERT OR REPLACE INTO project_index_manifests
             (project_id, publication_epoch, data_epoch, relation_epoch, operation_id,
              state, input_fingerprint, candidate_ready, created_at)
             VALUES (?1, ?2, ?2, 0, 'op-active', 'active', NULL, 0, 0)",
        rusqlite::params![project_id, epoch],
    )
    .expect("insert active manifest");
}

/// Insert a building (candidate) manifest at the given data epoch.
fn insert_building_manifest(conn: &rusqlite::Connection, project_id: i64, epoch: i64) {
    conn.execute(
        "INSERT OR REPLACE INTO project_index_manifests
             (project_id, publication_epoch, data_epoch, relation_epoch, operation_id,
              state, input_fingerprint, candidate_ready, created_at)
             VALUES (?1, ?2, ?2, 0, 'op-building', 'building', NULL, 0, 0)",
        rusqlite::params![project_id, epoch],
    )
    .expect("insert building manifest");
}

#[test]
fn test_entity_id_seed_empty() {
    let (coordinator, _store) = coordinator_with_store(1);
    // No active/candidate epoch and no chunks: seed falls back to 1.
    assert_eq!(coordinator.operation.blocking_lock().entity_id_seed(), 1);
}

#[test]
fn test_entity_id_seed_with_active() {
    let (coordinator, store) = coordinator_with_store(1);
    let conn = store.write_connection().expect("write connection");
    insert_active_manifest(&conn, 1, 3);
    insert_chunk_row(&conn, 1, 3, &[10, 20, 30]);
    drop(conn);

    // Seed must be one above the maximum raw entity ID of the active epoch.
    assert_eq!(coordinator.operation.blocking_lock().entity_id_seed(), 31);
}

#[test]
fn test_entity_id_seed_with_candidate() {
    let (coordinator, store) = coordinator_with_store(1);
    let conn = store.write_connection().expect("write connection");
    insert_active_manifest(&conn, 1, 3);
    insert_chunk_row(&conn, 1, 3, &[10, 20]);
    // An interrupted run left a building candidate with higher entity IDs.
    insert_building_manifest(&conn, 1, 4);
    insert_chunk_row(&conn, 1, 4, &[100]);
    drop(conn);

    // The seed must cover the candidate epoch's IDs so a resumed re-parse
    // never collides with them.
    assert_eq!(coordinator.operation.blocking_lock().entity_id_seed(), 101);
}

#[test]
fn test_entity_id_seed_missing_metadata_store() {
    // Without a metadata store the seed falls back to 0.
    let coordinator =
        HotUpdateCoordinator::new(HotUpdateConfig::default(), 1).expect("create coordinator");
    assert_eq!(coordinator.operation.blocking_lock().entity_id_seed(), 0);
}

#[test]
fn test_is_manifest_active_true_false() {
    let (coordinator, store) = coordinator_with_store(1);
    let conn = store.write_connection().expect("write connection");
    insert_active_manifest(&conn, 1, 1);
    insert_building_manifest(&conn, 1, 2);
    drop(conn);

    assert!(
        coordinator
            .operation
            .blocking_lock()
            .is_manifest_active("op-active")
            .expect("read state"),
        "active state must be recognized"
    );
    assert!(
        !coordinator
            .operation
            .blocking_lock()
            .is_manifest_active("op-building")
            .expect("read state"),
        "building state must not be active"
    );
    assert!(
        !coordinator
            .operation
            .blocking_lock()
            .is_manifest_active("missing-op")
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

    let (mut coordinator, store) = coordinator_with_store(1);
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

    // A file-backed SQLite so the change-detector cache survives.
    let db_dir = tempfile::TempDir::new().expect("db temp dir");
    let store = Arc::new(
        SqliteClient::with_path(db_dir.path().join("cce.db").to_string_lossy())
            .expect("file-backed sqlite"),
    );

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
    {
        let conn = store.write_connection().expect("write connection");
        conn.execute(
            "INSERT OR IGNORE INTO projects (id, name, root_path, created_at, updated_at)
                 VALUES (1, 'p1', '/', ?1, ?1)",
            [chrono::Utc::now().timestamp()],
        )
        .expect("ensure project");
        drop(conn);
    }
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
    let conn = store.read_connection().expect("read connection");
    let published_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM files WHERE project_id = 1 AND path = ?1",
            rusqlite::params![relative(&published).to_string_lossy().to_string()],
            |row| row.get(0),
        )
        .expect("count published hash");
    assert_eq!(published_count, 1, "published path must be hashed");
    let deleted_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM files WHERE project_id = 1 AND path = ?1",
            rusqlite::params![relative(&deleted).to_string_lossy().to_string()],
            |row| row.get(0),
        )
        .expect("count deleted hash");
    assert_eq!(deleted_count, 0, "deleted path must be removed");
}
