mod common;

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::{Json, http::StatusCode};

use cce_api::models::{UpdateProjectRequest, error_codes};
use cce_server::api::handlers::{config, health, project, watch};
use cce_server::api::response::ApiResult;

use common::{create_request, test_env};

#[tokio::test]
async fn health_returns_success_envelope() {
    let env = test_env().await;
    match health::handle_health(State(env.state)).await {
        ApiResult::Success(status) => {
            let _ = (status.healthy, status.qdrant, status.bm25, status.embedding);
        }
        ApiResult::Error(err) => panic!("health must succeed, got {}", err.error.message),
    }
}

#[tokio::test]
async fn health_maps_to_http_200() {
    let env = test_env().await;
    let response = health::handle_health(State(env.state))
        .await
        .into_response();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn create_project_rejects_empty_root() {
    let env = test_env().await;
    let mut request = create_request("/tmp/does-not-matter");
    request.root_path = String::new();
    match project::management::handle_create_project(State(env.state), Json(request)).await {
        ApiResult::Error(err) => assert_eq!(err.error.code, error_codes::INVALID_REQUEST),
        ApiResult::Success(_) => panic!("empty root path must be rejected"),
    }
}

#[tokio::test]
async fn create_project_rejects_missing_directory() {
    let env = test_env().await;
    let request = create_request("/tmp/cce-contract-missing-dir-zzz");
    let response = project::management::handle_create_project(State(env.state), Json(request))
        .await
        .into_response();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn get_missing_project_returns_not_found() {
    let env = test_env().await;
    match project::query::handle_get_project(State(env.state.clone()), Path(999_199)).await {
        ApiResult::Error(err) => assert_eq!(err.error.code, error_codes::ENTITY_NOT_FOUND),
        ApiResult::Success(_) => panic!("missing project must not succeed"),
    }
    let response = project::query::handle_get_project(State(env.state), Path(999_199))
        .await
        .into_response();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn project_crud_roundtrip() {
    let env = test_env().await;
    let root = tempfile::TempDir::new().expect("project root");
    let created = match project::management::handle_create_project(
        State(env.state.clone()),
        Json(create_request(&root.path().to_string_lossy())),
    )
    .await
    {
        ApiResult::Success(detail) => detail,
        ApiResult::Error(err) => panic!("create must succeed: {}", err.error.message),
    };
    let id: i64 = created.project.id.parse().expect("project id is numeric");

    match project::query::handle_get_project(State(env.state.clone()), Path(id)).await {
        ApiResult::Success(detail) => assert_eq!(detail.project.id, id.to_string()),
        ApiResult::Error(err) => panic!("get must succeed: {}", err.error.message),
    }

    match project::query::handle_list_projects(State(env.state.clone())).await {
        ApiResult::Success(list) => {
            assert!(list.projects.iter().any(|p| p.id == id.to_string()));
        }
        ApiResult::Error(err) => panic!("list must succeed: {}", err.error.message),
    }

    let update = UpdateProjectRequest {
        name: Some("renamed-probe".to_string()),
        ..UpdateProjectRequest::default()
    };
    match project::management::handle_update_project(
        State(env.state.clone()),
        Path(id.to_string()),
        Json(update),
    )
    .await
    {
        ApiResult::Success(detail) => assert_eq!(detail.project.name, "renamed-probe"),
        ApiResult::Error(err) => panic!("update must succeed: {}", err.error.message),
    }

    match project::management::handle_delete_project(State(env.state.clone()), Path(id)).await {
        ApiResult::Success(_) => {}
        ApiResult::Error(err) => panic!("delete must succeed: {}", err.error.message),
    }
    match project::query::handle_get_project(State(env.state), Path(id)).await {
        ApiResult::Error(err) => assert_eq!(err.error.code, error_codes::ENTITY_NOT_FOUND),
        ApiResult::Success(_) => panic!("deleted project must be gone"),
    }
}

#[tokio::test]
async fn update_rejects_malformed_id() {
    let env = test_env().await;
    let update = UpdateProjectRequest::default();
    match project::management::handle_update_project(
        State(env.state),
        Path("not-a-number".to_string()),
        Json(update),
    )
    .await
    {
        ApiResult::Error(err) => assert_eq!(err.error.code, error_codes::INVALID_INPUT),
        ApiResult::Success(_) => panic!("malformed id must be rejected"),
    }
}

#[tokio::test]
async fn watch_rejects_nonpositive_id() {
    let env = test_env().await;
    match watch::handle_watch_status(State(env.state), Path(0)).await {
        ApiResult::Error(err) => assert_eq!(err.error.code, error_codes::INVALID_REQUEST),
        ApiResult::Success(_) => panic!("project_id 0 must be rejected"),
    }
}

#[tokio::test]
async fn watch_status_unknown_project_returns_inactive_default() {
    let env = test_env().await;
    match watch::handle_watch_status(State(env.state), Path(999_199)).await {
        ApiResult::Success(response) => {
            assert!(response.success);
            assert!(!response.status.active);
        }
        ApiResult::Error(err) => panic!("unknown watch status must default: {}", err.error.message),
    }
}

#[tokio::test]
async fn config_info_and_validate_return_envelopes() {
    match config::handle_config_info().await {
        ApiResult::Success(_) => {}
        ApiResult::Error(err) => panic!("config info must succeed: {}", err.error.message),
    }
    let response = config::handle_config_validate().await.into_response();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn config_reload_requires_project_id_and_accepts_real_project() {
    use axum::extract::Query;
    let env = test_env().await;
    match config::handle_config_reload(
        State(env.state.clone()),
        Query(cce_api::models::ConfigReloadQuery { project_id: None }),
    )
    .await
    {
        ApiResult::Error(err) => assert_eq!(err.error.code, error_codes::INVALID_REQUEST),
        ApiResult::Success(_) => panic!("reload without project_id must be rejected"),
    }
    let root = tempfile::TempDir::new().expect("project root");
    let id = common::create_project(&env.state, &root.path().to_string_lossy()).await;
    match config::handle_config_reload(
        State(env.state),
        Query(cce_api::models::ConfigReloadQuery {
            project_id: Some(id),
        }),
    )
    .await
    {
        ApiResult::Success(response) => assert!(response.success),
        ApiResult::Error(err) => panic!("reload must succeed: {}", err.error.message),
    }
}

#[tokio::test]
async fn dead_letter_acknowledge_rejects_unknown_module() {
    use cce_server::api::handlers::project::indexing as project_indexing;
    let env = test_env().await;
    let root = tempfile::TempDir::new().expect("project root");
    let id = common::create_project(&env.state, &root.path().to_string_lossy()).await;
    match project_indexing::handle_dead_letter_acknowledge(
        State(env.state),
        Path(id),
        Json(cce_api::models::DeadLetterAcknowledgeRequest {
            file_path: "src/main.rs".to_string(),
            module: Some("no-such-module".to_string()),
        }),
    )
    .await
    {
        ApiResult::Error(err) => assert_eq!(err.error.code, error_codes::INVALID_REQUEST),
        ApiResult::Success(_) => panic!("unknown module must be rejected"),
    }
}

#[tokio::test]
async fn metrics_history_accepts_valid_range() {
    use cce_server::api::handlers::metrics;
    let env = test_env().await;
    match metrics::handle_get_metrics_history(
        State(env.state),
        axum::extract::Query(cce_api::models::MetricsHistoryQuery {
            from: "2020-01-01T00:00:00Z".to_string(),
            to: "2030-01-01T00:00:00Z".to_string(),
            metric: None,
            project_id: None,
            operation_type: None,
        }),
    )
    .await
    {
        ApiResult::Success(_) => {}
        ApiResult::Error(err) => panic!("history must succeed: {}", err.error.message),
    }
}
