//! Evidence for the ingest-commit cutover decision.
//!
//! The ingest-commit entry runs a full index unconditionally. This test drives
//! the real handler over a small project and then measures, with the same code
//! the handler's `CCE_INGEST_COMMIT_SHADOW` path calls, the change set the
//! hot-update incremental path would process. It pins down the two facts the
//! cutover decision turns on: an incremental re-commit carries a small delta
//! against a whole-project rebuild, and the full baseline is correctly
//! established by a commit so that delta is measurable at all.
//!
//! Run:
//!   CCE_INGEST_COMMIT_SHADOW=1 RUST_LOG=info cargo test -p cce-server \
//!     --features admission --test ingest_shadow_evidence -- --nocapture
//! to also observe the in-handler `ingest commit shadow change-set
//! measurement` logs.
#![cfg(feature = "admission")]

mod common;

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use cce_api::models::CreateProjectRequest;
use cce_orchestrator::IndexOptions;
use cce_orchestrator::hot_update::measure_index_change_set;
use cce_server::api::handlers::ingest;
use cce_server::api::response::ApiResult;

const FILE_COUNT: usize = 10;

/// Create a project over `root` indexing `rs` sources, so the commit
/// handler's full index and this test's measurement scan the same set.
async fn create_rust_project(state: &cce_server::api::state::AppState, root: &str) -> i64 {
    let request = CreateProjectRequest {
        name: Some("shadow-evidence".to_string()),
        root_path: root.to_string(),
        extensions: vec!["rs".to_string()],
        exclude_dirs: Vec::new(),
        respect_gitignore: false,
        ignore_patterns: Vec::new(),
        supply_mode: None,
    };
    match cce_server::api::handlers::project::management::handle_create_project(
        State(state.clone()),
        Json(request),
    )
    .await
    {
        ApiResult::Success(detail) => detail.project.id.parse().expect("numeric project id"),
        ApiResult::Error(err) => panic!("project create must succeed: {}", err.error.message),
    }
}

async fn commit(state: &cce_server::api::state::AppState, id: i64) -> usize {
    match ingest::commit::handle_ingest_commit(State(state.clone()), Path(id)).await {
        ApiResult::Success(response) => {
            assert!(response.success, "commit must succeed");
            response.indexed_files
        }
        ApiResult::Error(err) => panic!("commit must succeed: {}", err.error.message),
    }
}

#[tokio::test]
async fn commit_reindexes_whole_project_for_a_one_file_change_set() {
    let _ = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .try_init();
    let env = common::test_env().await;
    let root = tempfile::TempDir::new().expect("evidence project root");
    let src = root.path().join("src");
    std::fs::create_dir_all(&src).expect("evidence src dir");
    for i in 0..FILE_COUNT {
        std::fs::write(
            src.join(format!("mod_{i}.rs")),
            format!("pub fn mod_{i}() -> i64 {{\n    {i}\n}}\n"),
        )
        .expect("evidence source file");
    }
    let id = create_rust_project(&env.state, &root.path().to_string_lossy()).await;

    let registry = env
        .state
        .engine
        .metadata_store_clone()
        .expect("engine must hold a metadata store");
    // File hashes and the active generation manifest live in the per-project
    // database, the same store the index pipeline reads, so the measurement
    // must be scoped to the project rather than the registry database.
    let project_store = registry
        .as_ref()
        .for_project(id)
        .expect("per-project store opens");
    let options = IndexOptions::new(root.path()).with_extensions(vec!["rs".to_string()]);

    assert_eq!(commit(&env.state, id).await, FILE_COUNT);

    // A commit publishes the full baseline: nothing is left for the
    // incremental path to reprocess immediately afterwards.
    let baseline = measure_index_change_set(Arc::clone(&project_store), &options, id)
        .await
        .expect("baseline change-set measurement");
    assert_eq!(
        baseline.total_changes(),
        0,
        "just-committed project has no delta"
    );
    assert_eq!(baseline.unchanged.len(), FILE_COUNT);

    // One edit out of ten files is the delta an incremental re-commit carries.
    std::fs::write(
        root.path().join("src/mod_3.rs"),
        "pub fn mod_3() -> i64 {\n    3000\n}\n",
    )
    .expect("edit one file");
    let after_edit = measure_index_change_set(Arc::clone(&project_store), &options, id)
        .await
        .expect("post-edit change-set measurement");
    assert_eq!(after_edit.modified.len(), 1, "exactly one file is modified");
    assert_eq!(after_edit.unchanged.len(), FILE_COUNT - 1);
    assert_eq!(after_edit.total_changes(), 1);
    let ratio_permille = (after_edit.total_changes() * 1000) / FILE_COUNT;
    assert_eq!(
        ratio_permille, 100,
        "a one-file edit on a ten-file project is a 100 permille change set"
    );

    // The commit entry re-indexes the whole project for that one-file delta.
    assert_eq!(commit(&env.state, id).await, FILE_COUNT);

    // The re-commit then re-establishes the baseline, so the incremental path
    // stays in sync with what was just rewritten in full.
    let after_commit = measure_index_change_set(Arc::clone(&project_store), &options, id)
        .await
        .expect("post-commit change-set measurement");
    assert_eq!(after_commit.total_changes(), 0);
    assert_eq!(after_commit.unchanged.len(), FILE_COUNT);
}
