mod common;

use axum::extract::{Path, Query, State};
use serde_json::json;

use cce_api::models::{ClassificationQueryParams, error_codes};
use cce_server::api::handlers::{entity, graph};
use cce_server::api::response::ApiResult;

use cce_server::api::handlers::{qdrant_admin, watch};
use common::{create_project, seeded_project, test_env};

fn query<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> Query<T> {
    Query(serde_json::from_value(value).expect("query deserializes"))
}

#[tokio::test]
async fn entity_detail_rejects_nonpositive_project() {
    let env = test_env().await;
    match entity::detail::handle_function_detail(State(env.state), Path((0, "sym_x".to_string())))
        .await
    {
        ApiResult::Error(err) => assert_eq!(err.error.code, error_codes::INVALID_REQUEST),
        ApiResult::Success(_) => panic!("project_id 0 must be rejected"),
    }
}

#[tokio::test]
async fn entity_calls_and_callers_reject_nonpositive_project() {
    let env = test_env().await;
    let calls_query = query(json!({}));
    match entity::calls::handle_function_calls(
        State(env.state.clone()),
        Path((0, "sym_x".to_string())),
        calls_query,
    )
    .await
    {
        ApiResult::Error(err) => assert_eq!(err.error.code, error_codes::INVALID_REQUEST),
        ApiResult::Success(_) => panic!("calls with project_id 0 must be rejected"),
    }
    let callers_query = query(json!({}));
    match entity::calls::handle_function_callers(
        State(env.state),
        Path((-1, "sym_x".to_string())),
        callers_query,
    )
    .await
    {
        ApiResult::Error(err) => assert_eq!(err.error.code, error_codes::INVALID_REQUEST),
        ApiResult::Success(_) => panic!("callers with negative project must be rejected"),
    }
}

#[tokio::test]
async fn entity_relation_endpoints_reject_nonpositive_project() {
    let env = test_env().await;
    let direction_query = query(json!({}));
    match entity::relation::handle_call_chain(
        State(env.state.clone()),
        Path((0, "sym_x".to_string())),
        direction_query,
    )
    .await
    {
        ApiResult::Error(err) => assert_eq!(err.error.code, error_codes::INVALID_REQUEST),
        ApiResult::Success(_) => panic!("call chain with project_id 0 must be rejected"),
    }
    let path_query = query(json!({"start_id": "a", "end_id": "b"}));
    match entity::relation::handle_call_path(State(env.state.clone()), Path(0), path_query).await {
        ApiResult::Error(err) => assert_eq!(err.error.code, error_codes::INVALID_REQUEST),
        ApiResult::Success(_) => panic!("call path with project_id 0 must be rejected"),
    }
    match entity::relation::handle_class_inheritance(
        State(env.state.clone()),
        Path((0, "C".to_string())),
    )
    .await
    {
        ApiResult::Error(err) => assert_eq!(err.error.code, error_codes::INVALID_REQUEST),
        ApiResult::Success(_) => panic!("inheritance with project_id 0 must be rejected"),
    }
    match entity::relation::handle_class_implementations(
        State(env.state),
        Path((0, "I".to_string())),
    )
    .await
    {
        ApiResult::Error(err) => assert_eq!(err.error.code, error_codes::INVALID_REQUEST),
        ApiResult::Success(_) => panic!("implementations with project_id 0 must be rejected"),
    }
}

#[tokio::test]
async fn entity_unknown_symbol_is_error_not_success() {
    let env = test_env().await;
    let root = tempfile::TempDir::new().expect("project root");
    let id = create_project(&env.state, &root.path().to_string_lossy()).await;
    match entity::detail::handle_function_detail(
        State(env.state.clone()),
        Path((id, "sym_does_not_exist_zzz".to_string())),
    )
    .await
    {
        ApiResult::Error(_) => {}
        ApiResult::Success(_) => panic!("unknown symbol must not succeed"),
    }
    let calls_query = query(json!({}));
    match entity::calls::handle_function_calls(
        State(env.state),
        Path((id, "sym_does_not_exist_zzz".to_string())),
        calls_query,
    )
    .await
    {
        ApiResult::Error(_) => {}
        ApiResult::Success(_) => panic!("unknown symbol calls must not succeed"),
    }
}

#[tokio::test]
async fn classification_rejects_nonpositive_project() {
    let env = test_env().await;
    match entity::classification::get_classification_stats(State(env.state.clone()), Path(0)).await
    {
        ApiResult::Error(err) => assert_eq!(err.error.code, error_codes::INVALID_REQUEST),
        ApiResult::Success(_) => panic!("stats with project_id 0 must be rejected"),
    }
    let params = Query(ClassificationQueryParams { limit: None });
    match entity::classification::get_relations_by_classification(
        State(env.state),
        Path((0, "stdlib".to_string())),
        params,
    )
    .await
    {
        ApiResult::Error(err) => assert_eq!(err.error.code, error_codes::INVALID_REQUEST),
        ApiResult::Success(_) => panic!("classification with project_id 0 must be rejected"),
    }
}

#[tokio::test]
async fn classification_invalid_label_rejected_without_index() {
    let env = test_env().await;
    let root = tempfile::TempDir::new().expect("project root");
    let id = create_project(&env.state, &root.path().to_string_lossy()).await;
    // Without an index the snapshot is unavailable, so an invalid label can
    // never succeed regardless of which check fires first.
    let params = Query(ClassificationQueryParams { limit: Some(5) });
    match entity::classification::get_relations_by_classification(
        State(env.state),
        Path((id, "no-such-class".to_string())),
        params,
    )
    .await
    {
        ApiResult::Error(_) => {}
        ApiResult::Success(_) => panic!("unknown classification must not succeed"),
    }
}

#[tokio::test]
async fn graph_traversal_rejects_nonpositive_project() {
    let env = test_env().await;
    let ego = query(json!({"entity_id": "sym_x"}));
    match graph::handle_graph_ego(State(env.state.clone()), Path(0), ego).await {
        ApiResult::Error(err) => assert_eq!(err.error.code, error_codes::INVALID_REQUEST),
        ApiResult::Success(_) => panic!("ego with project_id 0 must be rejected"),
    }
    let path = query(json!({"start": "a", "end": "b"}));
    match graph::handle_graph_path(State(env.state.clone()), Path(0), path).await {
        ApiResult::Error(err) => assert_eq!(err.error.code, error_codes::INVALID_REQUEST),
        ApiResult::Success(_) => panic!("path with project_id 0 must be rejected"),
    }
    let subgraph = query(json!({"ids": "a,b"}));
    match graph::handle_graph_subgraph(State(env.state.clone()), Path(0), subgraph).await {
        ApiResult::Error(err) => assert_eq!(err.error.code, error_codes::INVALID_REQUEST),
        ApiResult::Success(_) => panic!("subgraph with project_id 0 must be rejected"),
    }
    let components = query(json!({}));
    match graph::handle_graph_components(State(env.state.clone()), Path(0), components).await {
        ApiResult::Error(err) => assert_eq!(err.error.code, error_codes::INVALID_REQUEST),
        ApiResult::Success(_) => panic!("components with project_id 0 must be rejected"),
    }
    let export = query(json!({}));
    match graph::handle_graph_export(State(env.state), Path(-5), export).await {
        ApiResult::Error(err) => assert_eq!(err.error.code, error_codes::INVALID_REQUEST),
        ApiResult::Success(_) => panic!("export with negative project must be rejected"),
    }
}

#[tokio::test]
async fn graph_analysis_rejects_nonpositive_project_and_empty_file() {
    let env = test_env().await;
    let impact = query(json!({"file": "src/main.rs"}));
    match graph::handle_graph_impact(State(env.state.clone()), Path(0), impact).await {
        ApiResult::Error(err) => assert_eq!(err.error.code, error_codes::INVALID_REQUEST),
        ApiResult::Success(_) => panic!("impact with project_id 0 must be rejected"),
    }
    let entity_impact = query(json!({"entity_id": "sym_x"}));
    match graph::handle_graph_entity_impact(State(env.state.clone()), Path(0), entity_impact).await
    {
        ApiResult::Error(err) => assert_eq!(err.error.code, error_codes::INVALID_REQUEST),
        ApiResult::Success(_) => panic!("entity impact with project_id 0 must be rejected"),
    }
    let cycles = query(json!({}));
    match graph::handle_graph_cycles(State(env.state.clone()), Path(0), cycles).await {
        ApiResult::Error(err) => assert_eq!(err.error.code, error_codes::INVALID_REQUEST),
        ApiResult::Success(_) => panic!("cycles with project_id 0 must be rejected"),
    }
    let structural = query(json!({"entity_id": "sym_x", "kind": "child_elements"}));
    match graph::handle_graph_structural(State(env.state.clone()), Path(0), structural).await {
        ApiResult::Error(err) => assert_eq!(err.error.code, error_codes::INVALID_REQUEST),
        ApiResult::Success(_) => panic!("structural with project_id 0 must be rejected"),
    }
    let module = query(json!({"file": "src/main.rs"}));
    match graph::handle_graph_module(State(env.state.clone()), Path(0), module).await {
        ApiResult::Error(err) => assert_eq!(err.error.code, error_codes::INVALID_REQUEST),
        ApiResult::Success(_) => panic!("module with project_id 0 must be rejected"),
    }

    let root = tempfile::TempDir::new().expect("project root");
    let id = create_project(&env.state, &root.path().to_string_lossy()).await;
    let empty = query(json!({"file": "   "}));
    match graph::handle_graph_impact(State(env.state), Path(id), empty).await {
        ApiResult::Error(_) => {}
        ApiResult::Success(_) => panic!("empty file impact must not succeed"),
    }
}

#[tokio::test]
async fn graph_unknown_symbol_is_error_not_success() {
    let env = test_env().await;
    let root = tempfile::TempDir::new().expect("project root");
    let id = create_project(&env.state, &root.path().to_string_lossy()).await;
    let ego = query(json!({"entity_id": "sym_does_not_exist_zzz"}));
    match graph::handle_graph_ego(State(env.state), Path(id), ego).await {
        ApiResult::Error(_) => {}
        ApiResult::Success(_) => panic!("ego with unknown symbol must not succeed"),
    }
}

// ---- Indexed success paths (fixture-backed) ----

#[tokio::test]
async fn entity_detail_and_neighbours_succeed_on_fixture_symbol() {
    let (env, _root, id) = seeded_project().await;
    match entity::detail::handle_function_detail(
        State(env.state.clone()),
        Path((id, "fixture_caller".to_string())),
    )
    .await
    {
        ApiResult::Success(response) => {
            assert!(response.success);
            assert_eq!(response.function.name, "fixture_caller");
        }
        ApiResult::Error(err) => panic!("detail must succeed: {}", err.error.message),
    }
    match entity::calls::handle_function_calls(
        State(env.state.clone()),
        Path((id, "fixture_caller".to_string())),
        query(json!({})),
    )
    .await
    {
        ApiResult::Success(response) => assert!(response.success),
        ApiResult::Error(err) => panic!("calls must succeed: {}", err.error.message),
    }
    match entity::calls::handle_function_callers(
        State(env.state),
        Path((id, "fixture_callee".to_string())),
        query(json!({})),
    )
    .await
    {
        ApiResult::Success(response) => assert!(response.success),
        ApiResult::Error(err) => panic!("callers must succeed: {}", err.error.message),
    }
}

#[tokio::test]
async fn call_chain_and_path_succeed_between_fixture_symbols() {
    let (env, _root, id) = seeded_project().await;
    match entity::relation::handle_call_chain(
        State(env.state.clone()),
        Path((id, "fixture_caller".to_string())),
        query(json!({"direction": "down"})),
    )
    .await
    {
        ApiResult::Success(response) => assert!(response.success),
        ApiResult::Error(err) => panic!("call chain must succeed: {}", err.error.message),
    }
    match entity::relation::handle_call_path(
        State(env.state),
        Path(id),
        query(json!({"start_id": "fixture_caller", "end_id": "fixture_callee"})),
    )
    .await
    {
        ApiResult::Success(response) => assert!(response.success),
        ApiResult::Error(err) => panic!("call path must succeed: {}", err.error.message),
    }
}

#[tokio::test]
async fn classification_buckets_behave_on_indexed_project() {
    let (env, _root, id) = seeded_project().await;
    match entity::classification::get_classification_stats(State(env.state.clone()), Path(id)).await
    {
        ApiResult::Success(response) => {
            // The total is derived from the per-bucket counts, so they must agree.
            assert_eq!(response.total, response.stats.values().sum::<usize>());
        }
        ApiResult::Error(err) => panic!("classification stats must succeed: {}", err.error.message),
    }
    // A valid bucket label succeeds even when the bucket holds no relations.
    let params = Query(ClassificationQueryParams { limit: Some(50) });
    match entity::classification::get_relations_by_classification(
        State(env.state.clone()),
        Path((id, "unknown".to_string())),
        params,
    )
    .await
    {
        ApiResult::Success(_) => {}
        ApiResult::Error(err) => {
            panic!("classification detail must succeed: {}", err.error.message)
        }
    }
    // Label matching is case-insensitive with aliases, so an uppercase alias
    // resolves to the same bucket rather than being rejected.
    let params = Query(ClassificationQueryParams { limit: Some(50) });
    match entity::classification::get_relations_by_classification(
        State(env.state.clone()),
        Path((id, "STD".to_string())),
        params,
    )
    .await
    {
        ApiResult::Success(_) => {}
        ApiResult::Error(err) => {
            panic!("classification alias must succeed: {}", err.error.message)
        }
    }
    // An unrecognized label is rejected once the snapshot has resolved, which
    // isolates label validation from snapshot availability.
    let params = Query(ClassificationQueryParams { limit: Some(50) });
    match entity::classification::get_relations_by_classification(
        State(env.state),
        Path((id, "no-such-class".to_string())),
        params,
    )
    .await
    {
        ApiResult::Error(err) => assert_eq!(err.error.code, error_codes::INVALID_REQUEST),
        ApiResult::Success(_) => panic!("unknown classification must not succeed"),
    }
}

#[tokio::test]
async fn graph_traversal_succeeds_on_fixture_symbols() {
    let (env, _root, id) = seeded_project().await;
    match graph::handle_graph_ego(
        State(env.state.clone()),
        Path(id),
        query(json!({"entity_id": "fixture_caller"})),
    )
    .await
    {
        ApiResult::Success(response) => {
            assert!(response.success);
            assert!(!response.nodes.is_empty(), "ego must contain the seed");
        }
        ApiResult::Error(err) => panic!("ego must succeed: {}", err.error.message),
    }
    match graph::handle_graph_path(
        State(env.state.clone()),
        Path(id),
        query(json!({"start": "fixture_caller", "end": "fixture_callee"})),
    )
    .await
    {
        ApiResult::Success(response) => assert!(response.success),
        ApiResult::Error(err) => panic!("graph path must succeed: {}", err.error.message),
    }
    match graph::handle_graph_subgraph(
        State(env.state.clone()),
        Path(id),
        query(json!({"ids": "fixture_caller,fixture_callee"})),
    )
    .await
    {
        ApiResult::Success(response) => assert!(response.success),
        ApiResult::Error(err) => panic!("subgraph must succeed: {}", err.error.message),
    }
    match graph::handle_graph_components(State(env.state.clone()), Path(id), query(json!({}))).await
    {
        ApiResult::Success(response) => assert!(response.success),
        ApiResult::Error(err) => panic!("components must succeed: {}", err.error.message),
    }
    match graph::handle_graph_export(State(env.state), Path(id), query(json!({"limit": 100}))).await
    {
        ApiResult::Success(response) => assert!(response.success),
        ApiResult::Error(err) => panic!("export must succeed: {}", err.error.message),
    }
}

#[tokio::test]
async fn graph_analysis_succeeds_on_fixture_data() {
    let (env, root, id) = seeded_project().await;
    let file = root
        .path()
        .join("src")
        .join("calls.rs")
        .to_string_lossy()
        .to_string();
    match graph::handle_graph_impact(
        State(env.state.clone()),
        Path(id),
        query(json!({"file": file})),
    )
    .await
    {
        ApiResult::Success(response) => assert!(response.success),
        ApiResult::Error(err) => panic!("impact must succeed: {}", err.error.message),
    }
    match graph::handle_graph_entity_impact(
        State(env.state.clone()),
        Path(id),
        query(json!({"entity_id": "fixture_caller"})),
    )
    .await
    {
        ApiResult::Success(response) => assert!(response.success),
        ApiResult::Error(err) => panic!("entity impact must succeed: {}", err.error.message),
    }
    match graph::handle_graph_cycles(
        State(env.state.clone()),
        Path(id),
        query(json!({"level": "file", "limit": 10})),
    )
    .await
    {
        ApiResult::Success(response) => assert!(response.success),
        ApiResult::Error(err) => panic!("cycles must succeed: {}", err.error.message),
    }
    match graph::handle_graph_structural(
        State(env.state.clone()),
        Path(id),
        query(json!({"entity_id": "fixture_caller", "kind": "child_elements"})),
    )
    .await
    {
        ApiResult::Success(response) => assert!(response.success),
        ApiResult::Error(err) => panic!("structural must succeed: {}", err.error.message),
    }
    match graph::handle_graph_module(State(env.state), Path(id), query(json!({"file": file}))).await
    {
        ApiResult::Success(response) => assert!(response.success),
        ApiResult::Error(err) => panic!("module must succeed: {}", err.error.message),
    }
}

#[tokio::test]
async fn watch_start_status_stop_state_machine() {
    let (env, root, id) = seeded_project().await;
    match watch::handle_start_watch(
        State(env.state.clone()),
        Path(id),
        axum::Json(cce_api::models::StartWatchRequest {
            path: root.path().to_string_lossy().to_string(),
            extensions: Vec::new(),
            debounce_ms: 50,
        }),
    )
    .await
    {
        ApiResult::Success(response) => assert!(response.success),
        ApiResult::Error(err) => panic!("start watch must succeed: {}", err.error.message),
    }
    match watch::handle_watch_status(State(env.state.clone()), Path(id)).await {
        ApiResult::Success(response) => {
            assert!(response.success);
            assert!(response.status.active, "watch must be active after start");
        }
        ApiResult::Error(err) => panic!("watch status must succeed: {}", err.error.message),
    }
    match watch::handle_stop_watch(State(env.state.clone()), Path(id)).await {
        ApiResult::Success(response) => assert!(response.success),
        ApiResult::Error(err) => panic!("stop watch must succeed: {}", err.error.message),
    }
    match watch::handle_watch_status(State(env.state), Path(id)).await {
        ApiResult::Success(response) => {
            assert!(!response.status.active, "watch must be inactive after stop");
        }
        ApiResult::Error(err) => panic!("watch status must follow stop: {}", err.error.message),
    }
}

#[tokio::test]
async fn watch_start_rejects_path_outside_project() {
    let (env, _root, id) = seeded_project().await;
    let elsewhere = tempfile::TempDir::new().expect("outside root");
    match watch::handle_start_watch(
        State(env.state),
        Path(id),
        axum::Json(cce_api::models::StartWatchRequest {
            path: elsewhere.path().to_string_lossy().to_string(),
            extensions: Vec::new(),
            debounce_ms: 50,
        }),
    )
    .await
    {
        ApiResult::Error(err) => assert_eq!(err.error.code, error_codes::INVALID_REQUEST),
        ApiResult::Success(_) => panic!("outside watch path must be rejected"),
    }
}

#[tokio::test]
async fn qdrant_process_reports_not_implemented_without_handle() {
    let env = test_env().await;
    match qdrant_admin::handle_qdrant_process_status(State(env.state.clone())).await {
        ApiResult::Error(err) => assert_eq!(err.error.code, error_codes::NOT_IMPLEMENTED),
        ApiResult::Success(_) => panic!("status without handle must be unimplemented"),
    }
    match qdrant_admin::handle_qdrant_process_start(State(env.state.clone())).await {
        ApiResult::Error(err) => assert_eq!(err.error.code, error_codes::NOT_IMPLEMENTED),
        ApiResult::Success(_) => panic!("start without handle must be unimplemented"),
    }
    match qdrant_admin::handle_qdrant_process_stop(State(env.state.clone())).await {
        ApiResult::Error(err) => assert_eq!(err.error.code, error_codes::NOT_IMPLEMENTED),
        ApiResult::Success(_) => panic!("stop without handle must be unimplemented"),
    }
    match qdrant_admin::handle_qdrant_process_restart(State(env.state)).await {
        ApiResult::Error(err) => assert_eq!(err.error.code, error_codes::NOT_IMPLEMENTED),
        ApiResult::Success(_) => panic!("restart without handle must be unimplemented"),
    }
}
