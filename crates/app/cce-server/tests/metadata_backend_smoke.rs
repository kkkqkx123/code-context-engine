//! Service-level smoke coverage for the remote metadata backend.
//!
//! The suite runs only when `CCE_TEST_PG_URL` points at a PostgreSQL service.
//! It exercises the same service assembly as the HTTP tests: project create,
//! full index, query, hot-update metadata reads, checkpoint recovery, and
//! graph export all share the one backend handle built at startup.

mod common;

use axum::Json;
use axum::extract::{Path, Query, State};

use cce_api::models::{ExportQuery, SearchRequest};
use cce_orchestrator::operation::checkpoint::{CheckpointManager, CreateCheckpointParams};
use cce_server::api::handlers::graph;
use cce_server::api::handlers::search;
use cce_server::api::response::ApiResult;
use cce_server_shared::context::SharedContext;
use cce_storage_common::MetadataStorage;
use cce_types::OperationKind;

use common::{
    TestBackends, create_project_named, index_fixture_project, test_env_with_backends,
    write_code_fixture,
};

fn query<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> Query<T> {
    Query(serde_json::from_value(value).expect("query deserializes"))
}

async fn search_project(state: &SharedContext, project_id: i64) -> usize {
    let request = SearchRequest {
        project_id: Some(project_id),
        project_path: None,
        query: "fixture_caller".to_string(),
        query_type: "bm25".to_string(),
        limit: 10,
        ..SearchRequest::default()
    };
    match search::handle_search(State(state.clone()), Json(request)).await {
        ApiResult::Success(response) => {
            assert!(response.success, "search must report success");
            response.total
        }
        ApiResult::Error(error) => panic!("search must succeed: {}", error.error.message),
    }
}

async fn checkpoint_round_trip(store: &cce_orchestrator::index::MetadataStore, project_id: i64) {
    let manager = CheckpointManager::new_for_project(project_id, store.clone());
    let operation_id = format!("pg-smoke-checkpoint-{project_id}");
    manager
        .create_checkpoint(CreateCheckpointParams {
            operation_id: &operation_id,
            operation_type: OperationKind::FullIndex,
            root_dir: "/repo/pg-smoke",
            total_files: 2,
            batch_size: 2,
            file_list_hash: "smoke-hash",
        })
        .await
        .expect("checkpoint created through the shared remote handle");
    let loaded = manager
        .get_checkpoint(&operation_id)
        .await
        .expect("checkpoint read back");
    assert!(loaded.is_some(), "remote checkpoint must round-trip");
}

#[tokio::test]
async fn remote_metadata_service_smoke() {
    let Some(url) = std::env::var("CCE_TEST_PG_URL")
        .ok()
        .filter(|value| !value.is_empty())
    else {
        eprintln!("skipping remote metadata service smoke: CCE_TEST_PG_URL is not set");
        return;
    };
    let env = test_env_with_backends(TestBackends {
        metadata_backend: cce_config::modules::MetadataBackend::Remote,
        metadata_url: Some(url),
        ..TestBackends::default()
    })
    .await;
    let store = env
        .state
        .metadata_store()
        .expect("engine always exposes a metadata handle");
    assert_eq!(MetadataStorage::backend_name(&store), "remote");
    assert!(!MetadataStorage::is_per_project_db(&store));

    let root = tempfile::TempDir::new().expect("fixture root");
    write_code_fixture(root.path());
    let project_name = format!(
        "pg-smoke-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    );
    let project_id =
        create_project_named(&env.state, &root.path().to_string_lossy(), &project_name).await;

    let indexed = index_fixture_project(&env.state, project_id, root.path()).await;
    assert!(
        indexed.errors.is_empty(),
        "index errors: {:?}",
        indexed.errors
    );
    assert!(indexed.files_indexed >= 2, "fixture must be indexed");
    assert!(search_project(&env.state, project_id).await >= 1);

    // Hot update reads the published generation and its file hashes from the
    // same handle built during service assembly.
    let coordinator = env
        .state
        .engine
        .get_hot_update_coordinator(project_id)
        .await
        .expect("hot-update coordinator builds");
    let stats = coordinator
        .lock()
        .await
        .change_detection_stats()
        .await
        .expect("change-detection stats read over remote metadata");
    assert!(stats.stored_files >= 2);

    checkpoint_round_trip(&store, project_id).await;

    match graph::handle_graph_export(
        State(env.state.clone()),
        Path(project_id),
        query::<ExportQuery>(serde_json::json!({"limit": 100})),
    )
    .await
    {
        ApiResult::Success(response) => assert!(response.success),
        ApiResult::Error(error) => panic!("export must succeed: {}", error.error.message),
    }

    MetadataStorage::snapshot_delete_project(&store, project_id)
        .await
        .expect("cleanup smoke snapshots");
    MetadataStorage::project_delete_db(&store, project_id)
        .await
        .expect("cleanup smoke project");
}
