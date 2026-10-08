//! Dual-backend relation storage contract tests.
//!
//! The same case suite runs against both backends so project isolation,
//! content roundtrips, manifest lifecycle, snapshot operations, overrides,
//! checkpoints, and admission audit stay identical: the local backend always
//! runs (embedded), the PostgreSQL branch runs when `CCE_TEST_PG_URL` points
//! at a live service and skips otherwise. Each backend works under its own
//! project ids, and the remote run cleans up with `delete_project_db` plus
//! `snapshot_delete_project`.

use cce_storage_common::metadb::{
    CheckpointRecord, CheckpointStatus, ChunkRecord, EntityDetailMapping, EntityRecord,
    FileCheckpointRecord, FileRecord, GenerationOverride, OverrideDisposition,
    ProjectIndexManifestState, RelationStorage, WorkUnitCheckpointRecord, WorkUnitStatus,
};
use cce_types::CanonicalRelationSnapshot;

fn file(project_id: i64, path: &str) -> FileRecord {
    FileRecord {
        id: 0,
        path: path.to_string(),
        language: "rust".to_string(),
        category: 0,
        last_modified: 1,
        created_at: 1,
        project_id,
        content_hash: Some("hash".to_string()),
    }
}

fn entity(project_id: i64, epoch: i64, name: &str) -> EntityRecord {
    EntityRecord {
        id: 0,
        name: name.to_string(),
        kind: "function".to_string(),
        file_id: 0,
        signature: Some(format!("fn {name}()")),
        span_start_row: Some(1),
        span_end_row: Some(1),
        span_start_column: None,
        span_end_column: None,
        span_start_byte: None,
        span_end_byte: None,
        scoped_name: None,
        depth: None,
        parent_id: None,
        metadata: None,
        parameters_json: None,
        return_type: None,
        doc_comment: None,
        modifiers_json: None,
        project_id,
        epoch,
        batch_id: 0,
        rank: 0.0,
    }
}

fn chunk(project_id: i64, epoch: i64, chunk_id: &str, file_path: &str) -> ChunkRecord {
    ChunkRecord {
        chunk_id: chunk_id.to_string(),
        file_path: file_path.to_string(),
        content: format!("fn {chunk_id}() {{}}"),
        start_line: 1,
        end_line: 1,
        entity_ids: "[]".to_string(),
        entity_names: "[]".to_string(),
        chunk_type: "code".to_string(),
        test_status: 0,
        test_source: 0,
        created_at: 1,
        updated_at: 1,
        project_id: Some(project_id),
        epoch,
        batch_id: 0,
        path: "emb".to_string(),
        bm25_keywords: String::new(),
        segment_id: String::new(),
        truncated: 0,
    }
}

/// Shared contract suite: identical assertions for every backend.
async fn run_contract_suite(store: &impl RelationStorage, tag: &str, project: i64, other: i64) {
    // Project directory: ensure is idempotent, missing reads empty.
    store
        .ensure_project(project, &format!("/repo/{tag}"))
        .await
        .expect("ensure project");
    store
        .ensure_project(project, &format!("/repo/{tag}"))
        .await
        .expect("ensure project replay");
    assert!(
        store
            .project_record(project)
            .await
            .expect("project record")
            .is_some()
    );
    assert!(
        store
            .project_record(other)
            .await
            .expect("missing project record")
            .is_none()
    );
    store
        .project_meta_set_int(project, "active_epoch", 3)
        .await
        .expect("set meta");
    assert_eq!(
        store
            .project_meta_get_int(project, "active_epoch")
            .await
            .expect("get meta"),
        3
    );

    // Manifest lifecycle: begin, ready, activate, then active reads back.
    let manifest = store
        .manifest_begin_building(project, 3, &format!("{tag}-op-1"), None)
        .await
        .expect("begin building");
    assert_eq!(manifest.data_epoch, 3);
    store
        .manifest_mark_candidate_ready(project, &format!("{tag}-op-1"))
        .await
        .expect("mark ready");
    let active = store
        .manifest_activate(project, 3, 0, &format!("{tag}-op-1"), None)
        .await
        .expect("activate");
    assert_eq!(active.state, ProjectIndexManifestState::Active);
    assert!(
        store
            .manifest_active(project)
            .await
            .expect("active")
            .is_some()
    );
    store
        .manifest_begin_building(project, 4, &format!("{tag}-op-2"), None)
        .await
        .expect("begin building");
    store
        .manifest_mark_failed(project, &format!("{tag}-op-2"), "boom")
        .await
        .expect("mark failed");

    // Content roundtrip plus idempotent replay.
    assert_eq!(
        store
            .files_upsert(project, 3, &[file(project, "a.rs")])
            .await
            .expect("upsert files"),
        1
    );
    assert_eq!(
        store
            .files_upsert(project, 3, &[file(project, "a.rs")])
            .await
            .expect("replay is idempotent"),
        1
    );
    assert_eq!(
        store
            .chunks_upsert(&[chunk(project, 3, "c1", "a.rs")])
            .await
            .expect("upsert chunks"),
        1
    );
    assert_eq!(
        store.chunks_count(project, 3).await.expect("count chunks"),
        1
    );
    let readback = store
        .chunks_by_ids(project, &["c1".to_string()], &[3])
        .await
        .expect("readback");
    assert_eq!(readback.len(), 1);
    assert_eq!(readback[0].content, "fn c1() {}");

    // Project isolation: the other project sees none of this, and a project
    // that was never indexed reads empty everywhere.
    assert_eq!(store.chunks_count(other, 3).await.expect("count"), 0);
    assert!(
        store
            .chunks_by_ids(other, &["c1".to_string()], &[3])
            .await
            .expect("cross-project readback")
            .is_empty()
    );
    assert_eq!(store.entities_count(other, 3).await.expect("count"), 0);

    // Overrides (summaries need a real file row id, so they are covered in
    // the local-only section below like entities).
    store
        .overrides_replace(
            project,
            3,
            &[GenerationOverride {
                file_path: "a.rs".to_string(),
                disposition: OverrideDisposition::Replaced,
            }],
        )
        .await
        .expect("replace overrides");
    assert_eq!(
        store
            .overrides_for_generation(project, 3)
            .await
            .expect("list overrides")
            .len(),
        1
    );

    // Checkpoints and work units.
    let checkpoint = CheckpointRecord {
        id: None,
        project_id: project,
        operation_id: format!("{tag}-op-1"),
        operation_type: "index".to_string(),
        root_dir: format!("/repo/{tag}"),
        total_files: 1,
        batch_size: 1,
        current_batch_index: 0,
        current_phase: "scan".to_string(),
        file_list_hash: None,
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
        last_error: None,
        failure_count: 0,
        status: CheckpointStatus::InProgress,
        active_flag: true,
        priority: 0,
        last_heartbeat: None,
        failed_at: None,
    };
    let checkpoint_id = store
        .checkpoint_create(project, &checkpoint)
        .await
        .expect("create checkpoint");
    assert!(checkpoint_id > 0);
    assert!(
        store
            .checkpoint_get(project, &format!("{tag}-op-1"))
            .await
            .expect("get checkpoint")
            .is_some()
    );
    store
        .checkpoint_set_status(project, &format!("{tag}-op-1"), CheckpointStatus::Completed)
        .await
        .expect("set checkpoint status");
    assert!(
        store
            .file_checkpoint_get(project, &format!("{tag}-op-1"), "a.rs")
            .await
            .expect("get missing file checkpoint")
            .is_none()
    );
    assert_eq!(
        store
            .checkpoint_files_delete_by_operation(project, &format!("{tag}-missing"))
            .await
            .expect("delete missing operation files"),
        0
    );
    store
        .work_unit_insert(&WorkUnitCheckpointRecord {
            id: None,
            project_id: project,
            operation_id: format!("{tag}-op-1"),
            stage: "bm25_commit".to_string(),
            target_epoch: 3,
            work_unit_hash: "hash".to_string(),
            status: WorkUnitStatus::Running,
            item_count: 1,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        })
        .await
        .expect("insert work unit");
    store
        .work_unit_set_status(
            project,
            &format!("{tag}-op-1"),
            "bm25_commit",
            "hash",
            WorkUnitStatus::Committed,
        )
        .await
        .expect("set work unit status");
    assert_eq!(
        store
            .work_units_list(project, &format!("{tag}-op-1"), "bm25_commit")
            .await
            .expect("list work units")
            .len(),
        1
    );
    assert!(
        store
            .work_unit_by_hash(project, &format!("{tag}-op-1"), "bm25_commit", "hash")
            .await
            .expect("get work unit")
            .is_some()
    );

    // Snapshot operations: allocate, write, read back, then clean up.
    let epoch = store
        .snapshot_allocate(project, &format!("{tag}-snap-op"), "config")
        .await
        .expect("allocate snapshot epoch");
    let snapshot = CanonicalRelationSnapshot::new("config".to_string());
    store
        .snapshot_write_ready(project, epoch, &snapshot, "config", "snap")
        .await
        .expect("write snapshot");
    let readback = store
        .snapshot_read(project, epoch)
        .await
        .expect("read snapshot");
    assert_eq!(readback.config_fingerprint, "config");
    assert!(
        store
            .snapshot_manifest(project, epoch)
            .await
            .expect("snapshot manifest")
            .is_some()
    );
    assert!(
        store
            .snapshot_delta_chain(project, 0, epoch)
            .await
            .expect("delta chain")
            .is_empty()
    );
    assert!(
        store
            .snapshot_find_base(project, 999_999)
            .await
            .expect("find base of missing epoch")
            .is_none()
    );

    // Admission audit roundtrip.
    store
        .admission_record_admitted(&format!("{tag}-fp"), &[project], None, 10)
        .await
        .expect("admit");
    assert!(
        store
            .admission_get(&format!("{tag}-fp"))
            .await
            .expect("get audit")
            .is_some()
    );

    // Deletes stay scoped: epoch deletes, then project deletes.
    assert_eq!(
        store
            .chunks_delete_by_project_epoch(project, 3)
            .await
            .expect("delete chunks epoch"),
        1
    );
    assert_eq!(
        store
            .files_delete_by_project_epoch(project, 3)
            .await
            .expect("delete files epoch"),
        1
    );
    assert_eq!(
        store
            .files_delete_by_project(project)
            .await
            .expect("delete files project"),
        0
    );
    store.db_size().await.expect("db size probe");
}

#[tokio::test]
async fn local_backend_contract() {
    use cce_storage_metadb_sqlite::SqliteClient;
    let client = SqliteClient::in_memory().expect("in-memory client");
    run_contract_suite(&client, "local", 7, 8).await;
    // Local-only: FK-linked rows need real row ids, which neither branch
    // exposes through the contract (see the capability inventory). Resolve
    // them with direct reads and cover the entity/mapping roundtrip here on
    // a fresh epoch so earlier deletes cannot interfere.
    RelationStorage::files_upsert(&client, 7, 9, &[file(7, "a.rs")])
        .await
        .expect("upsert files");
    let file_id: i64 = {
        let conn = client.read_connection().expect("read connection");
        conn.query_row(
            "SELECT id FROM files WHERE project_id = 7 AND epoch = 9 AND path = 'a.rs'",
            [],
            |row| row.get(0),
        )
        .expect("resolve file id")
    };
    let mut named = entity(7, 9, "alpha");
    named.file_id = file_id;
    assert_eq!(
        RelationStorage::entities_upsert(&client, &[named])
            .await
            .expect("upsert entities"),
        1
    );
    assert_eq!(
        RelationStorage::entities_count(&client, 7, 9)
            .await
            .expect("count entities"),
        1
    );
    let entity_id: i64 = {
        let conn = client.read_connection().expect("read connection");
        conn.query_row(
            "SELECT id FROM entities WHERE project_id = 7 AND epoch = 9 AND name = 'alpha'",
            [],
            |row| row.get(0),
        )
        .expect("resolve entity id")
    };
    assert_eq!(
        RelationStorage::mappings_upsert(
            &client,
            &[EntityDetailMapping {
                id: 0,
                entity_id,
                project_id: Some(7),
                epoch: 9,
                qdrant_point_ids: "p1".to_string(),
                bm25_doc_ids: "d1".to_string(),
                chunk_count: 1,
                created_at: 1,
                updated_at: 1,
            }]
        )
        .await
        .expect("upsert mappings"),
        1
    );
    assert_eq!(
        RelationStorage::mappings_delete_by_project_epoch(&client, 7, 9)
            .await
            .expect("delete mappings"),
        1
    );
    use cce_storage_metadb_sqlite::repo::CheckpointRepository;
    client
        .with_transaction(|tx| {
            CheckpointRepository::insert_batch_checkpoint(
                tx,
                7,
                &cce_storage_metadb_sqlite::types::BatchCheckpointRecord {
                    id: None,
                    operation_id: "local-op-1".to_string(),
                    batch_index: 0,
                    first_file: "a.rs".to_string(),
                    last_file: "a.rs".to_string(),
                    file_count: 1,
                    processed_files: 0,
                    failed_files: 0,
                    entities_extracted: 0,
                    relations_found: 0,
                    chunks_generated: 0,
                    vectors_stored: 0,
                    start_time: "2026-01-01T00:00:00Z".to_string(),
                    end_time: None,
                    duration_ms: None,
                    created_at: "2026-01-01T00:00:00Z".to_string(),
                    updated_at: "2026-01-01T00:00:00Z".to_string(),
                },
            )
            .map(|_| ())
        })
        .expect("insert batch checkpoint");
    RelationStorage::file_checkpoint_upsert(
        &client,
        7,
        &FileCheckpointRecord {
            id: None,
            operation_id: "local-op-1".to_string(),
            batch_index: 0,
            file_path: "a.rs".to_string(),
            file_id: Some(file_id),
            language: Some("rust".to_string()),
            file_size: None,
            content_hash: None,
            parsed_data: None,
            parse_error: None,
            summary_data: None,
            embedding_count: 0,
            bm25_doc_id: None,
            export_path: None,
            render_fingerprint: None,
            module_progress: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        },
    )
    .await
    .expect("upsert file checkpoint");
    assert!(
        RelationStorage::file_checkpoint_get(&client, 7, "local-op-1", "a.rs")
            .await
            .expect("get file checkpoint")
            .is_some()
    );
    assert_eq!(
        RelationStorage::checkpoint_files_delete_by_operation(&client, 7, "local-op-1")
            .await
            .expect("delete operation files"),
        1
    );
    RelationStorage::summary_upsert(&client, file_id, 9, r#"{"summary_text":"hi"}"#)
        .await
        .expect("upsert summary");
    assert!(
        RelationStorage::summary_at_epoch(&client, file_id, 9)
            .await
            .expect("read summary")
            .is_some()
    );
    assert_eq!(
        RelationStorage::summaries_by_epoch(&client, 7, 9)
            .await
            .expect("list summaries")
            .len(),
        1
    );
    assert_eq!(
        RelationStorage::summaries_delete_by_project_epoch(&client, 7, 9)
            .await
            .expect("delete summaries"),
        1
    );
    assert_eq!(
        RelationStorage::entities_delete_by_project_epoch(&client, 7, 9)
            .await
            .expect("delete entities"),
        1
    );

    // Full cleanup removes every row of the project.
    RelationStorage::snapshot_delete_project(&client, 7)
        .await
        .expect("delete snapshots");
    RelationStorage::delete_project_db(&client, 7)
        .await
        .expect("delete project db");
}

#[tokio::test]
async fn remote_backend_contract() {
    let Some(url) = std::env::var("CCE_TEST_PG_URL")
        .ok()
        .filter(|v| !v.is_empty())
    else {
        eprintln!("skipping remote relation contract: CCE_TEST_PG_URL is not set");
        return;
    };
    use cce_config::modules::RelationRemoteConfig;
    use cce_storage_metadb_pg::{PostgresClient, PostgresConfig};
    let project = 700_001 + (std::process::id() as i64 % 1000);
    let other = project + 1;
    let remote = RelationRemoteConfig {
        url: Some(url),
        ..RelationRemoteConfig::default()
    };
    let config = PostgresConfig::from_remote(&remote).expect("remote config");
    let client = PostgresClient::new(config).expect("remote client must build");
    client.ensure_migrated().await.expect("migrate schema");
    run_contract_suite(&client, "remote", project, other).await;
    RelationStorage::snapshot_delete_project(&client, project)
        .await
        .expect("delete snapshots");
    RelationStorage::snapshot_delete_project(&client, other)
        .await
        .expect("delete other snapshots");
    RelationStorage::delete_project_db(&client, project)
        .await
        .expect("delete project db");
    RelationStorage::delete_project_db(&client, other)
        .await
        .expect("delete other project db");
}
