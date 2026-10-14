//! Commit routes through the incremental path after the baseline lands.
//!
//! The first commit still runs a full index to establish the baseline. Later
//! commits run one hot-update operation, so a one-file edit only reprocesses
//! that file and an untouched tree reprocesses nothing.
#![cfg(feature = "admission")]

mod common;

use axum::Json;
use axum::extract::{Path, State};
use cce_api::models::CreateProjectRequest;
use cce_server::api::handlers::ingest;
use cce_server::api::response::ApiResult;

const FILE_COUNT: usize = 10;

/// Create a project over `root` indexing `rs` sources, so the commit
/// handler's full index and the incremental path scan the same set.
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

async fn stored_files(state: &cce_server::api::state::AppState, id: i64) -> usize {
    let coordinator = state
        .engine
        .get_hot_update_coordinator(id)
        .await
        .expect("hot-update coordinator builds");
    coordinator
        .lock()
        .await
        .change_detection_stats()
        .await
        .expect("change-detection stats read")
        .stored_files
}

#[tokio::test]
async fn commit_applies_one_file_delta_incrementally() {
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

    assert_eq!(
        commit(&env.state, id).await,
        FILE_COUNT,
        "first commit establishes the full baseline"
    );
    assert_eq!(stored_files(&env.state, id).await, FILE_COUNT);

    std::fs::write(
        root.path().join("src/mod_3.rs"),
        "pub fn mod_3() -> i64 {\n    3000\n}\n",
    )
    .expect("edit one file");
    assert_eq!(
        commit(&env.state, id).await,
        1,
        "a one-file edit reprocesses one file instead of the whole project"
    );
    assert_eq!(stored_files(&env.state, id).await, FILE_COUNT);

    assert_eq!(
        commit(&env.state, id).await,
        0,
        "an untouched tree reprocesses nothing"
    );
    assert_eq!(stored_files(&env.state, id).await, FILE_COUNT);
}
