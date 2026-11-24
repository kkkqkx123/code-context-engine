//! Config-reload hot path tests.
//!
//! These tests verify the runtime configuration-change semantics of the
//! hot-update pipeline: module-fingerprint invalidation (chunking / summary
//! config drift), scanner exclude-pattern changes, and the
//! `ConfigReloadManager -> on_config_change` delivery chain.

use std::sync::Arc;
use std::time::Duration;

use cce_config::{AstToNlConfig, HotUpdateConfig};
use cce_orchestrator::IndexOrchestrator;
use cce_orchestrator::hot_update::progress::module_input_fingerprint;
use cce_orchestrator::index::IndexOptions;
use cce_storage_common::MetadataStorage;

use crate::helper::init_minimal_logging;
use crate::hot_update::resume_harness::{
    CrashManifest, CrashState, HotUpdateHarness, ProcessorSelection, fast_watch_config,
    start_watch_chain, stop_watch_chain, wait_until,
};
use cce_e2e_tests::index_test::TestRelationPublisher;

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

/// a chunking-config drift between runs invalidates the
/// embedding and bm25 module fingerprints, so both modules redo while the
/// parse envelopes are reused (no file re-parsing).
#[tokio::test]
async fn test_chunking_config_change_invalidates_embedding_bm25() {
    init_minimal_logging();

    let harness = HotUpdateHarness::new().await.expect("harness");
    let files = two_file_fixture(&harness);

    let _instance_a = harness
        .build_instance(ProcessorSelection::embedding_bm25(), None, false)
        .await
        .expect("instance");

    // Crash state with embedding + bm25 markers computed for the OLD chunking
    // config; the candidate is adoptable (BuildingReady).
    let mut crash_files = Vec::new();
    for path in &files {
        let content = std::fs::read(path).expect("read fixture file");
        let bm25_marker = harness.bm25_marker_for_content(&content);
        let embedding_marker = harness.embedding_marker_for_content(&content);
        crash_files.push(
            harness
                .envelope_for(
                    std::path::Path::new(path),
                    &[("bm25", bm25_marker), ("embedding", embedding_marker)],
                )
                .await
                .expect("envelope"),
        );
    }
    harness
        .create_crash_state(&CrashState {
            operation_id: "cfg001-op".to_string(),
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
    let before_embeddings = harness.embedding_count();
    let before_parses = harness.parse_count();

    let mut instance_b = harness
        .build_instance(ProcessorSelection::embedding_bm25(), chunking_config, false)
        .await
        .expect("instance b");
    let result = instance_b.run_hot_update().await.expect("resume failed");

    assert_eq!(result.status, cce_orchestrator::OperationStatus::Completed);
    assert_eq!(
        harness.bm25_index_count(),
        before_indexes + 2,
        "chunking drift must invalidate the bm25 markers"
    );
    assert_eq!(
        harness.embedding_count(),
        before_embeddings + 2,
        "chunking drift must invalidate the embedding markers"
    );
    assert_eq!(
        harness.parse_count(),
        before_parses,
        "no file re-parses on config drift (envelope reuse)"
    );

    let active = harness
        .active_manifest()
        .await
        .expect("manifest")
        .expect("active");
    assert_eq!(harness.files_for_epoch(active.data_epoch).await, 2);
}

/// a summary-config drift invalidates only the summary module;
/// embedding and bm25 keep their (still-matching) markers.
#[tokio::test]
async fn test_summary_config_change_invalidates_summary_only() {
    init_minimal_logging();

    let harness = HotUpdateHarness::new().await.expect("harness");
    let files = two_file_fixture(&harness);

    let _instance_a = harness
        .build_instance(ProcessorSelection::full(), None, false)
        .await
        .expect("instance");

    // Crash state with markers for every module under the DEFAULT summary
    // fingerprint (the empty string used by the harness).
    let mut crash_files = Vec::new();
    for path in &files {
        let content = std::fs::read(path).expect("read fixture file");
        let bm25_marker = harness.bm25_marker_for_content(&content);
        let embedding_marker = harness.embedding_marker_for_content(&content);
        let content_hash = cce_utils::hash::calculate_hash(&content);
        let summary_marker = module_input_fingerprint("", &content_hash);
        crash_files.push(
            harness
                .envelope_for(
                    std::path::Path::new(path),
                    &[
                        ("bm25", bm25_marker),
                        ("embedding", embedding_marker),
                        ("summary", summary_marker),
                    ],
                )
                .await
                .expect("envelope"),
        );
    }
    harness
        .create_crash_state(&CrashState {
            operation_id: "cfg002-op".to_string(),
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

    let before_indexes = harness.bm25_index_count();
    let before_embeddings = harness.embedding_count();
    let before_summaries = harness.summary_count();
    let before_parses = harness.parse_count();

    // The resumed run uses a different summary configuration fingerprint.
    let mut instance_b = harness
        .build_instance_with_summary_fingerprint(
            ProcessorSelection::full(),
            None,
            false,
            false,
            HotUpdateConfig::default(),
            "changed-summary-config".to_string(),
        )
        .await
        .expect("instance b");
    let result = instance_b.run_hot_update().await.expect("resume failed");

    assert_eq!(result.status, cce_orchestrator::OperationStatus::Completed);
    assert_eq!(
        harness.summary_count(),
        before_summaries + 2,
        "summary drift must regenerate every summary"
    );
    assert_eq!(
        harness.bm25_index_count(),
        before_indexes,
        "bm25 markers still match; no redo"
    );
    assert_eq!(
        harness.embedding_count(),
        before_embeddings,
        "embedding markers still match; no redo"
    );
    assert_eq!(
        harness.parse_count(),
        before_parses,
        "no file re-parses (envelope reuse)"
    );
}

/// adding an exclude pattern to the scanner configuration makes
/// the next hot-update scan skip the excluded files.
#[tokio::test]
async fn test_exclude_pattern_change_affects_next_scan() {
    init_minimal_logging();

    let harness = HotUpdateHarness::new().await.expect("harness");
    harness
        .add_file("src/keep.rs", "pub fn kept() -> i32 { 1 }")
        .expect("add keep.rs");
    harness
        .add_file("tests/skip.rs", "pub fn skipped() -> i32 { 2 }")
        .expect("add skip.rs");

    // Phase 1: no exclusion, both files are indexed.
    let mut instance_a = harness
        .build_instance(ProcessorSelection::bm25_only(), None, false)
        .await
        .expect("instance a");
    let result_1 = instance_a.run_hot_update().await.expect("run 1 failed");
    assert_eq!(
        result_1.status,
        cce_orchestrator::OperationStatus::Completed
    );
    assert_eq!(
        harness.parse_count(),
        2,
        "both files parsed without excludes"
    );

    // Phase 2: exclude `tests/`, modify both files, scan again.
    let mut config = HotUpdateConfig::default();
    let mut scanner = harness_scanner_config();
    scanner.exclude_patterns.push("tests/**".to_string());
    config.scanner = Some(scanner);

    let mut instance_b = harness
        .build_instance_with_config(ProcessorSelection::bm25_only(), None, false, true, config)
        .await
        .expect("instance b");
    for (path, content) in [
        ("src/keep.rs", "pub fn kept() -> i32 { 10 }"),
        ("tests/skip.rs", "pub fn skipped() -> i32 { 20 }"),
    ] {
        std::fs::write(harness.file(path), content).expect("modify file");
    }

    let before_parses = harness.parse_count();
    let result_2 = instance_b.run_hot_update().await.expect("run 2 failed");
    assert_eq!(
        result_2.status,
        cce_orchestrator::OperationStatus::Completed
    );
    assert_eq!(
        harness.parse_count(),
        before_parses + 1,
        "only the non-excluded file is re-parsed"
    );

    // The excluded file must not appear in the published epoch.
    let active = harness
        .active_manifest()
        .await
        .expect("manifest")
        .expect("active");
    let skipped_visible = harness
        .store
        .files_exists_at_epoch(harness.project_id, active.data_epoch, "tests/skip.rs")
        .await
        .expect("count excluded rows");
    assert!(
        !skipped_visible,
        "the excluded file must be absent from the new epoch"
    );
}

/// The scanner config the harness uses, exposed for tests that customize it.
fn harness_scanner_config() -> cce_config::ScannerConfig {
    let mut scanner = cce_config::ScannerConfig::default();
    scanner.exclude_patterns.push(".cce".to_string());
    scanner
}

/// A fixture whose relation graph spans the two source files, with a valid
/// Cargo.toml for the build-config parser.
fn relation_fixture(harness: &HotUpdateHarness) {
    harness
        .add_file(
            "Cargo.toml",
            "[package]\nname = \"cfg004\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .expect("add Cargo.toml");
    harness
        .add_file("src/lib.rs", "pub fn alpha() -> i32 { util::beta() }")
        .expect("add lib.rs");
    harness
        .add_file(
            "src/util.rs",
            "pub fn beta() -> i32 { 2 }\npub fn gamma() -> i32 { 3 }",
        )
        .expect("add util.rs");
}

/// a Cargo.toml modification flows through the watcher into the
/// ConfigReloadManager and is delivered to the relation processor's
/// `on_config_change`, which performs the affected-file relation rebuild and
/// publishes a new relation epoch.
#[tokio::test]
async fn test_build_config_change_triggers_relation_rebuild() {
    init_minimal_logging();

    let harness = HotUpdateHarness::new().await.expect("harness");
    relation_fixture(&harness);

    // Publish the initial relation snapshot into the harness's store so the
    // config-rebuild has a base to diff against.
    let publisher = Arc::new(TestRelationPublisher::with_store(harness.store.clone()));
    let mut orchestrator = IndexOrchestrator::new(harness.project_id)
        .expect("index orchestrator")
        .with_metadata_store(harness.store.clone())
        .with_checkpoint_manager(harness.checkpoint_manager.clone())
        .with_relation_publisher(publisher.clone());
    let options = IndexOptions {
        root_dir: harness.root().to_path_buf(),
        extensions: vec!["rs".to_string()],
        store_vectors: false,
        store_bm25: false,
        store_summaries: false,
        build_relations: true,
        ..Default::default()
    };
    let initial = orchestrator.execute(options).await.expect("relation index");
    assert!(initial.total_relations > 0, "initial relations must exist");

    // The relation processor wired with persistence + the shared storage.
    let storage = {
        // Reuse the harness storage builder semantics: current active epoch.
        let active_epoch = harness
            .active_manifest()
            .await
            .expect("manifest")
            .map(|m| m.data_epoch)
            .unwrap_or(0);
        Arc::new(
            cce_orchestrator::index::StorageCoordinator::new(harness.project_id)
                .expect("storage coordinator")
                .with_metadata_store(harness.store.clone())
                .with_epoch(active_epoch)
                .with_batch_id(0),
        )
    };
    let relation_processor =
        cce_orchestrator::hot_update::RelationUpdateProcessor::with_persistence_and_config(
            harness.store.clone(),
            harness.root(),
            harness.project_id,
        )
        .with_storage(storage.clone())
        .with_publisher(publisher);

    let mut instance = harness
        .build_instance_with_transform(
            ProcessorSelection::bm25_only(),
            None,
            true,
            false,
            fast_watch_config(),
            |processor| processor,
        )
        .await
        .expect("instance");
    // Replace the stored processors with the relation processor so the
    // config reload delivery reaches it.
    instance.processors =
        vec![Arc::new(relation_processor) as Arc<dyn cce_orchestrator::UpdateProcessor>];
    instance.coordinator = instance
        .coordinator
        .with_processors(instance.processors.clone());
    let coordinator = start_watch_chain(&mut instance, harness.root(), harness.project_id).await;

    let before_relation_epoch = harness
        .store
        .project_meta_get_int(harness.project_id, "active_relation_epoch")
        .await
        .expect("read active relation epoch");

    // Modify Cargo.toml (a comment change keeps the dependency fingerprint
    // stable, so the delta rebuild is accepted).
    std::fs::write(
        harness.file("Cargo.toml"),
        "[package]\nname = \"cfg004\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n# reconfigured at runtime\n",
    )
    .expect("modify Cargo.toml");

    // The config event must be queued in the ConfigReloadManager, not in the
    // pending watch changes.
    let config_queued = wait_until(
        || async { coordinator.lock().await.has_pending_config_changes().await },
        Duration::from_secs(10),
    )
    .await;
    assert!(
        config_queued,
        "a Cargo.toml event must be routed to the config reload manager"
    );

    // Deliver the pending config change through the hot path.
    coordinator
        .lock()
        .await
        .process_pending_config_changes(
            &instance
                .processors
                .iter()
                .map(|p| p.as_ref())
                .collect::<Vec<_>>(),
        )
        .await
        .expect("process config changes");

    // The relation rebuild must publish a new relation epoch.
    let after_relation_epoch = harness
        .store
        .project_meta_get_int(harness.project_id, "active_relation_epoch")
        .await
        .expect("read active relation epoch");
    assert!(
        after_relation_epoch > before_relation_epoch,
        "the relation rebuild must advance the relation epoch ({before_relation_epoch} -> {after_relation_epoch})"
    );

    // The rebuilt epoch must be loadable and contain the rebuilt files.
    use cce_codegraph::index::snapshot_loader::RelationSnapshotLoader;
    use cce_codegraph::index::snapshot_query::SnapshotFileQueryOps;
    let rebuilt =
        RelationSnapshotLoader::load(&harness.store, harness.project_id, after_relation_epoch)
            .expect("rebuilt relation snapshot must load");
    assert!(
        rebuilt.contains_file("src/lib.rs") && rebuilt.contains_file("src/util.rs"),
        "both relation files must be present after the rebuild"
    );
    assert_eq!(
        rebuilt.file_count(),
        3,
        "two source files plus the synthetic Cargo.toml governance node must be present"
    );

    // The config change must be fully consumed.
    assert!(
        !coordinator.lock().await.has_pending_config_changes().await,
        "the config change must be consumed"
    );

    stop_watch_chain(&coordinator).await;
}
