mod common;

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::{Json, http::StatusCode};

use cce_api::models::{
    AggregatedSearchRequest, BatchDeleteRequest, ClearIndexRequest, EntitySearchRequest,
    ProjectConfigUpdateRequest, SearchRequest, StorageQuery, SubQueryRequest, SummaryRequest,
    error_codes,
};
use cce_server::api::handlers::project::indexing as project_indexing;
use cce_server::api::handlers::{
    entity_search, health, index, metrics, project, search, storage, summary, tools,
};
use cce_server::api::response::ApiResult;

use common::{
    create_project, create_project_named, index_fixture_project, seeded_project, test_env,
    write_code_fixture,
};

fn storage_query(project_id: i64) -> Query<StorageQuery> {
    Query(StorageQuery { project_id })
}

fn summary_request(files: Vec<String>) -> SummaryRequest {
    SummaryRequest {
        file_paths: files,
        directory_paths: Vec::new(),
        extensions: Vec::new(),
        exclude_dirs: Vec::new(),
        respect_gitignore: true,
        ignore_patterns: Vec::new(),
        recursive: true,
        max_files: 100,
    }
}

fn aggregated_request(sub_queries: Vec<SubQueryRequest>) -> AggregatedSearchRequest {
    AggregatedSearchRequest {
        project_id: Some(1),
        project_path: None,
        sub_queries,
        limit: 10,
        min_score: None,
        directory_prefix: None,
        exclude_content_types: Vec::new(),
        exclude_patterns: Vec::new(),
        include_patterns: Vec::new(),
        include_categories: Vec::new(),
        exclude_categories: Vec::new(),
        enable_rerank: None,
        rerank_max_candidates: None,
        query_intent: None,
        expand: None,
    }
}

#[tokio::test]
async fn clear_index_rejects_nonpositive_project() {
    let env = test_env().await;
    let request = ClearIndexRequest {
        project_id: 0,
        vectors: false,
        bm25: false,
        relations: false,
        cache: false,
    };
    match storage::handle_clear_index(State(env.state), Json(request)).await {
        ApiResult::Error(err) => assert_eq!(err.error.code, error_codes::INVALID_REQUEST),
        ApiResult::Success(_) => panic!("project_id 0 must be rejected"),
    }
}

#[tokio::test]
async fn delete_file_rejects_invalid_project() {
    let env = test_env().await;
    match storage::handle_delete_file(
        State(env.state),
        Path("src/main.rs".to_string()),
        storage_query(-3),
    )
    .await
    {
        ApiResult::Error(err) => assert_eq!(err.error.code, error_codes::INVALID_REQUEST),
        ApiResult::Success(_) => panic!("negative project must be rejected"),
    }
}

#[tokio::test]
async fn delete_entity_rejects_invalid_project() {
    let env = test_env().await;
    match storage::handle_delete_entity(State(env.state), Path(42u64), storage_query(0)).await {
        ApiResult::Error(err) => assert_eq!(err.error.code, error_codes::INVALID_REQUEST),
        ApiResult::Success(_) => panic!("project_id 0 must be rejected"),
    }
}

#[tokio::test]
async fn batch_delete_rejects_invalid_project() {
    let env = test_env().await;
    let request = BatchDeleteRequest {
        file_paths: vec!["src/a.rs".to_string()],
        entity_ids: Vec::new(),
    };
    match storage::handle_batch_delete(State(env.state), storage_query(0), Json(request)).await {
        ApiResult::Error(err) => assert_eq!(err.error.code, error_codes::INVALID_REQUEST),
        ApiResult::Success(_) => panic!("project_id 0 must be rejected"),
    }
}

#[tokio::test]
async fn index_stats_and_storage_status_succeed_on_fresh_project() {
    let env = test_env().await;
    let root = tempfile::TempDir::new().expect("project root");
    let id = create_project(&env.state, &root.path().to_string_lossy()).await;
    match storage::handle_index_stats(State(env.state.clone()), storage_query(id)).await {
        ApiResult::Success(response) => {
            assert!(response.success);
            let _ = (
                response.statistics.total_entities,
                response.statistics.total_files,
            );
        }
        ApiResult::Error(err) => panic!("stats must succeed: {}", err.error.message),
    }
    match storage::handle_storage_status(State(env.state)).await {
        ApiResult::Success(_) => {}
        ApiResult::Error(err) => panic!("storage status must succeed: {}", err.error.message),
    }
}

#[tokio::test]
async fn summary_rejects_empty_inputs() {
    match summary::handle_summary(Json(summary_request(Vec::new()))).await {
        ApiResult::Error(err) => assert_eq!(err.error.code, error_codes::INVALID_REQUEST),
        ApiResult::Success(_) => panic!("empty summary inputs must be rejected"),
    }
}

#[tokio::test]
async fn summary_warns_on_missing_file_without_error() {
    match summary::handle_summary(Json(summary_request(vec![
        "/tmp/cce-summary-missing-zzz.rs".to_string(),
    ])))
    .await
    {
        ApiResult::Success(response) => {
            assert!(!response.warnings.is_empty());
        }
        ApiResult::Error(err) => panic!("missing file warns, not errors: {}", err.error.message),
    }
}

fn search_request(query: &str) -> SearchRequest {
    SearchRequest {
        project_id: Some(1),
        project_path: None,
        query: query.to_string(),
        query_type: "hybrid".to_string(),
        limit: 10,
        ..SearchRequest::default()
    }
}

#[tokio::test]
async fn search_rejects_blank_query() {
    let env = test_env().await;
    let root = tempfile::TempDir::new().expect("project root");
    let id = create_project(&env.state, &root.path().to_string_lossy()).await;
    let mut request = search_request("   ");
    request.project_id = Some(id);
    match search::handle_search(State(env.state), Json(request)).await {
        ApiResult::Error(err) => assert_eq!(err.error.code, error_codes::INVALID_REQUEST),
        ApiResult::Success(_) => panic!("blank query must be rejected"),
    }
}

#[tokio::test]
async fn search_rejects_missing_project_spec() {
    let env = test_env().await;
    let mut request = search_request("fold");
    request.project_id = None;
    request.project_path = None;
    match search::handle_search(State(env.state), Json(request)).await {
        ApiResult::Error(err) => assert_eq!(err.error.code, error_codes::INVALID_INPUT),
        ApiResult::Success(_) => panic!("missing project spec must be rejected"),
    }
}

#[tokio::test]
async fn search_rejects_oversize_limit() {
    let env = test_env().await;
    let root = tempfile::TempDir::new().expect("project root");
    let id = create_project(&env.state, &root.path().to_string_lossy()).await;
    let mut request = search_request("fold");
    request.project_id = Some(id);
    request.limit = 10_000;
    let response = search::handle_search(State(env.state), Json(request))
        .await
        .into_response();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn aggregated_search_rejects_empty_subqueries() {
    let env = test_env().await;
    let root = tempfile::TempDir::new().expect("project root");
    let id = create_project(&env.state, &root.path().to_string_lossy()).await;
    let mut request = aggregated_request(Vec::new());
    request.project_id = Some(id);
    match search::handle_aggregated_search(State(env.state), Json(request)).await {
        ApiResult::Error(err) => assert_eq!(err.error.code, error_codes::INVALID_INPUT),
        ApiResult::Success(_) => panic!("empty sub-queries must be rejected"),
    }
}

#[tokio::test]
async fn aggregated_search_rejects_unknown_subquery_type() {
    let env = test_env().await;
    let root = tempfile::TempDir::new().expect("project root");
    let id = create_project(&env.state, &root.path().to_string_lossy()).await;
    let mut request = aggregated_request(vec![SubQueryRequest {
        text: "fold".to_string(),
        query_type: "no-such-source".to_string(),
        weight: 1.0,
    }]);
    request.project_id = Some(id);
    match search::handle_aggregated_search(State(env.state), Json(request)).await {
        ApiResult::Error(_) => {}
        ApiResult::Success(_) => panic!("unknown sub-query type must be rejected"),
    }
}

#[tokio::test]
async fn entity_search_rejects_empty_query() {
    let env = test_env().await;
    let request = EntitySearchRequest {
        query: String::new(),
        project_id: Some(1),
        project_path: None,
        limit: 10,
        kind_filter: None,
    };
    match entity_search::handle_entity_search(State(env.state), Json(request)).await {
        ApiResult::Error(err) => assert_eq!(err.error.code, error_codes::INVALID_REQUEST),
        ApiResult::Success(_) => panic!("empty entity query must be rejected"),
    }
}

#[tokio::test]
async fn project_config_missing_project_returns_not_found() {
    let env = test_env().await;
    match project::config::handle_get_project_config(State(env.state.clone()), Path(999_199)).await
    {
        ApiResult::Error(err) => assert_eq!(err.error.code, error_codes::ENTITY_NOT_FOUND),
        ApiResult::Success(_) => panic!("missing project config must not succeed"),
    }
    match project::config::handle_reload_project_config(State(env.state), Path(999_199)).await {
        ApiResult::Error(err) => assert_eq!(err.error.code, error_codes::ENTITY_NOT_FOUND),
        ApiResult::Success(_) => panic!("missing project reload must not succeed"),
    }
}

#[tokio::test]
async fn project_config_rejects_malformed_payload() {
    let env = test_env().await;
    let root = tempfile::TempDir::new().expect("project root");
    let id = create_project(&env.state, &root.path().to_string_lossy()).await;
    let payload = ProjectConfigUpdateRequest {
        config: serde_json::json!({"scanner": {"max_file_size": "not-a-number"}}),
    };
    match project::config::handle_update_project_config(State(env.state), Path(id), Json(payload))
        .await
    {
        ApiResult::Error(err) => assert_eq!(err.error.code, error_codes::INVALID_INPUT),
        ApiResult::Success(_) => panic!("malformed config must be rejected"),
    }
}

#[tokio::test]
async fn project_config_roundtrip_on_real_project() {
    let env = test_env().await;
    let root = tempfile::TempDir::new().expect("project root");
    let id = create_project(&env.state, &root.path().to_string_lossy()).await;
    match project::config::handle_get_project_config(State(env.state.clone()), Path(id)).await {
        ApiResult::Success(response) => {
            assert!(response.success);
            assert_eq!(response.project_id, id);
        }
        ApiResult::Error(err) => panic!("get config must succeed: {}", err.error.message),
    }
    match project::config::handle_reload_project_config(State(env.state), Path(id)).await {
        ApiResult::Success(response) => assert_eq!(response.project_id, id),
        ApiResult::Error(err) => panic!("reload must succeed: {}", err.error.message),
    }
}

#[tokio::test]
async fn health_subdivisions_return_envelopes() {
    let env = test_env().await;
    match health::handle_qdrant_health(State(env.state.clone())).await {
        ApiResult::Success(_) => {}
        ApiResult::Error(err) => panic!("qdrant health must succeed: {}", err.error.message),
    }
    match health::handle_embedding_health(State(env.state.clone())).await {
        ApiResult::Success(_) => {}
        ApiResult::Error(err) => panic!("embedding health must succeed: {}", err.error.message),
    }
    match health::handle_bm25_health(State(env.state.clone())).await {
        ApiResult::Success(_) => {}
        ApiResult::Error(err) => panic!("bm25 health must succeed: {}", err.error.message),
    }
    match health::handle_retry_queue_status(
        State(env.state.clone()),
        Query(cce_api::models::RetryQueueQuery { project_id: None }),
    )
    .await
    {
        ApiResult::Success(_) => {}
        ApiResult::Error(err) => panic!("retry status must succeed: {}", err.error.message),
    }
    match health::handle_retry_queue_dead_list(
        State(env.state),
        Query(cce_api::models::RetryQueueQuery { project_id: None }),
    )
    .await
    {
        ApiResult::Success(_) => {}
        ApiResult::Error(err) => panic!("dead list must succeed: {}", err.error.message),
    }
}

#[tokio::test]
async fn retry_queue_clear_is_idempotent() {
    let env = test_env().await;
    let first = match health::handle_retry_queue_clear(
        State(env.state.clone()),
        Query(cce_api::models::RetryQueueQuery { project_id: None }),
    )
    .await
    {
        ApiResult::Success(response) => response.cleared,
        ApiResult::Error(err) => panic!("clear must succeed: {}", err.error.message),
    };
    let second = match health::handle_retry_queue_clear(
        State(env.state.clone()),
        Query(cce_api::models::RetryQueueQuery { project_id: None }),
    )
    .await
    {
        ApiResult::Success(response) => response.cleared,
        ApiResult::Error(err) => panic!("second clear must succeed: {}", err.error.message),
    };
    assert_eq!(
        second, 0,
        "second clear drains nothing, first cleared {first}"
    );
}

#[tokio::test]
async fn stateless_tools_degrade_without_error() {
    let fold = tools::fold::handle_fold(Json(cce_api::models::FoldRequest {
        text: "fn main() {}".to_string(),
        language: Some("no-such-language".to_string()),
        file_name: None,
        max_tokens: None,
        mode: None,
    }))
    .await;
    assert!(fold.0.success);
    assert!(!fold.0.structure_known);

    let diagnose = tools::diagnosis::handle_diagnose(Json(cce_api::models::DiagnoseRequest {
        code: "fn main() {}".to_string(),
        language: Some("rust".to_string()),
        file_name: None,
        include_ast: false,
    }))
    .await;
    assert!(diagnose.0.success);
}

// ---- Indexed success paths (fixture-backed) ----

#[tokio::test]
async fn index_succeeds_on_small_code_fixture() {
    let env = test_env().await;
    let root = tempfile::TempDir::new().expect("fixture root");
    write_code_fixture(root.path());
    let id = create_project(&env.state, &root.path().to_string_lossy()).await;
    let outcome = index_fixture_project(&env.state, id, root.path()).await;
    assert!(
        outcome.files_scanned >= 2,
        "scanned {}",
        outcome.files_scanned
    );
    assert!(
        outcome.files_indexed >= 1,
        "indexed {}",
        outcome.files_indexed
    );
}

#[tokio::test]
async fn incremental_reindexes_touched_file() {
    let (env, root, id) = seeded_project().await;
    let touched = root.path().join("src").join("extra.py");
    let response = index::incremental::handle_incremental(
        State(env.state),
        Json(cce_api::models::IncrementalIndexRequest {
            project_id: id,
            files_to_index: vec![touched.to_string_lossy().to_string()],
            files_to_remove: Vec::new(),
            force_reindex: true,
        }),
    )
    .await
    .into_response();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn parse_reports_entities_for_fixture_file() {
    let (_env, root, _id) = seeded_project().await;
    let file = root.path().join("src").join("calls.rs");
    match index::parse::handle_parse(Json(cce_api::models::ParseRequest {
        file_path: file.to_string_lossy().to_string(),
    }))
    .await
    {
        ApiResult::Success(response) => {
            assert!(response.success);
            assert!(
                response.entities.iter().any(|e| e.name == "fixture_caller"),
                "caller entity must be parsed"
            );
        }
        ApiResult::Error(err) => panic!("parse must succeed: {}", err.error.message),
    }
}

#[tokio::test]
async fn bm25_search_succeeds_on_indexed_project() {
    let (env, _root, id) = seeded_project().await;
    let request = search_request("fixture_caller");
    let request = SearchRequest {
        project_id: Some(id),
        query_type: "bm25".to_string(),
        ..request
    };
    match search::handle_search(State(env.state), Json(request)).await {
        ApiResult::Success(response) => {
            assert!(response.success);
            assert!(response.total >= 1, "bm25 must hit the fixture");
        }
        ApiResult::Error(err) => panic!("bm25 search must succeed: {}", err.error.message),
    }
}

#[tokio::test]
async fn aggregated_bm25_search_succeeds_on_indexed_project() {
    let (env, _root, id) = seeded_project().await;
    let mut request = aggregated_request(vec![SubQueryRequest {
        text: "fixture_caller".to_string(),
        query_type: "bm25".to_string(),
        weight: 1.0,
    }]);
    request.project_id = Some(id);
    match search::handle_aggregated_search(State(env.state), Json(request)).await {
        ApiResult::Success(response) => assert!(response.success),
        ApiResult::Error(err) => panic!("aggregated search must succeed: {}", err.error.message),
    }
}

#[tokio::test]
async fn entity_search_finds_fixture_symbol() {
    let (env, _root, id) = seeded_project().await;
    match entity_search::handle_entity_search(
        State(env.state),
        Json(EntitySearchRequest {
            query: "fixture_caller".to_string(),
            project_id: Some(id),
            project_path: None,
            limit: 10,
            kind_filter: None,
        }),
    )
    .await
    {
        ApiResult::Success(response) => {
            assert!(response.success);
            assert!(
                response
                    .items
                    .iter()
                    .any(|item| item.name == "fixture_caller"),
                "entity index must contain the caller"
            );
        }
        ApiResult::Error(err) => panic!("entity search must succeed: {}", err.error.message),
    }
}

#[tokio::test]
async fn summary_succeeds_on_fixture_file() {
    let (_env, root, _id) = seeded_project().await;
    let file = root.path().join("src").join("calls.rs");
    match summary::handle_summary(Json(summary_request(vec![
        file.to_string_lossy().to_string(),
    ])))
    .await
    {
        ApiResult::Success(response) => {
            assert!(response.success);
            assert_eq!(response.total_files, 1);
            assert_eq!(response.success_count, 1);
        }
        ApiResult::Error(err) => panic!("summary must succeed: {}", err.error.message),
    }
}

#[tokio::test]
async fn storage_delete_file_succeeds_and_stats_follow() {
    let (env, root, id) = seeded_project().await;
    let file = root.path().join("src").join("extra.py");
    match storage::handle_delete_file(
        State(env.state.clone()),
        Path(file.to_string_lossy().to_string()),
        storage_query(id),
    )
    .await
    {
        ApiResult::Success(response) => assert!(response.success),
        ApiResult::Error(err) => panic!("delete file must succeed: {}", err.error.message),
    }
    match storage::handle_index_stats(State(env.state), storage_query(id)).await {
        ApiResult::Success(response) => assert!(response.success),
        ApiResult::Error(err) => panic!("stats must follow delete: {}", err.error.message),
    }
}

#[tokio::test]
async fn two_projects_stay_isolated() {
    let env = test_env().await;
    let root_a = tempfile::TempDir::new().expect("project root a");
    write_code_fixture(root_a.path());
    let id_a = create_project_named(&env.state, &root_a.path().to_string_lossy(), "probe-a").await;
    index_fixture_project(&env.state, id_a, root_a.path()).await;
    let root_b = tempfile::TempDir::new().expect("project root b");
    let id_b = create_project_named(&env.state, &root_b.path().to_string_lossy(), "probe-b").await;
    match storage::handle_index_stats(State(env.state.clone()), storage_query(id_b)).await {
        ApiResult::Success(response) => {
            assert_eq!(
                response.statistics.total_entities, 0,
                "unindexed project must report no entities"
            );
        }
        ApiResult::Error(err) => panic!("stats must succeed: {}", err.error.message),
    }
    match storage::handle_index_stats(State(env.state), storage_query(id_a)).await {
        ApiResult::Success(response) => {
            assert!(
                response.statistics.total_entities >= 2,
                "indexed project keeps its entities, got {}",
                response.statistics.total_entities
            );
        }
        ApiResult::Error(err) => panic!("stats must succeed: {}", err.error.message),
    }
}

#[tokio::test]
async fn project_config_update_accepts_empty_object() {
    let env = test_env().await;
    let root = tempfile::TempDir::new().expect("project root");
    let id = create_project(&env.state, &root.path().to_string_lossy()).await;
    match project::config::handle_update_project_config(
        State(env.state.clone()),
        Path(id),
        Json(ProjectConfigUpdateRequest {
            config: serde_json::json!({}),
        }),
    )
    .await
    {
        ApiResult::Success(response) => assert!(response.success),
        ApiResult::Error(err) => panic!("empty config update must succeed: {}", err.error.message),
    }
    match project::config::handle_get_project_config(State(env.state), Path(id)).await {
        ApiResult::Success(response) => assert_eq!(response.project_id, id),
        ApiResult::Error(err) => panic!("get config must follow update: {}", err.error.message),
    }
}

#[tokio::test]
async fn project_config_rejects_global_sections() {
    let env = test_env().await;
    let root = tempfile::TempDir::new().expect("project root");
    let id = create_project(&env.state, &root.path().to_string_lossy()).await;
    match project::config::handle_update_project_config(
        State(env.state),
        Path(id),
        Json(ProjectConfigUpdateRequest {
            config: serde_json::json!({"server": {"host": "0.0.0.0"}}),
        }),
    )
    .await
    {
        ApiResult::Error(err) => assert_eq!(err.error.code, error_codes::INVALID_INPUT),
        ApiResult::Success(_) => panic!("global section must be rejected"),
    }
}

#[tokio::test]
async fn project_profile_persists_across_cache_invalidation() {
    let env = test_env().await;
    let root = tempfile::TempDir::new().expect("project root");
    let id = create_project(&env.state, &root.path().to_string_lossy()).await;
    let profile = serde_json::json!({
        "scanner": {"respect_gitignore": false, "gitignore_patterns": ["target/"]},
        "orchestrator": {"indexer": {"extensions": ["rs"], "exclude_dirs": ["target"]}},
    });
    let version = match project::config::handle_update_project_config(
        State(env.state.clone()),
        Path(id),
        Json(ProjectConfigUpdateRequest {
            config: profile.clone(),
        }),
    )
    .await
    {
        ApiResult::Success(response) => {
            assert!(response.success);
            response.config_version
        }
        ApiResult::Error(err) => panic!("profile update must succeed: {}", err.error.message),
    };
    let stored = match project::config::handle_get_project_config(
        State(env.state.clone()),
        Path(id),
    )
    .await
    {
        ApiResult::Success(response) => {
            assert_eq!(response.project_id, id);
            assert_eq!(response.config_version, version);
            assert!(version > 0, "profile write must bump version");
            response.config
        }
        ApiResult::Error(err) => panic!("get profile must succeed: {}", err.error.message),
    };
    assert_eq!(
        stored["scanner"]["respect_gitignore"],
        serde_json::json!(false)
    );
    assert_eq!(
        stored["orchestrator"]["indexer"]["extensions"],
        serde_json::json!(["rs"])
    );
    env.state
        .engine
        .project_registry()
        .invalidate_cache(Some(id))
        .await
        .expect("cache invalidation must succeed");
    match project::config::handle_get_project_config(State(env.state), Path(id)).await {
        ApiResult::Success(response) => {
            assert_eq!(response.config_version, version);
            assert_eq!(response.config, stored);
        }
        ApiResult::Error(err) => panic!("profile must survive reload: {}", err.error.message),
    }
}

#[tokio::test]
async fn stateless_compress_and_batch_succeed_on_real_files() {
    let (_env, root, _id) = seeded_project().await;
    let file = root
        .path()
        .join("src")
        .join("calls.rs")
        .to_string_lossy()
        .to_string();
    let single = tools::compression::handle_compress(Json(cce_api::models::CompressRequest {
        file_path: file.clone(),
        include_entities: true,
        include_groups: true,
    }))
    .await;
    assert!(
        single.0.success,
        "compress must succeed, got {:?}",
        single.0.error
    );
    assert!(single.0.result.is_some());

    let batch =
        tools::compression::handle_compress_batch(Json(cce_api::models::BatchCompressRequest {
            file_paths: vec![file],
            include_entities: Some(true),
            include_groups: None,
            max_concurrency: 2,
        }))
        .await;
    assert_eq!(batch.0.successes.len(), 1);
    assert!(batch.0.failures.is_empty());
}

#[tokio::test]
async fn fold_batch_succeeds_on_raw_text() {
    match tools::fold::handle_fold_batch(Json(cce_api::models::BatchFoldRequest {
        items: vec![cce_api::models::BatchFoldItem {
            id: "a".to_string(),
            text: "fn main() {}".to_string(),
            language: Some("rust".to_string()),
            file_name: None,
            max_tokens: None,
            mode: None,
        }],
        language: None,
        max_tokens: None,
        mode: None,
        max_concurrency: None,
    }))
    .await
    {
        ApiResult::Success(response) => assert_eq!(response.results.len(), 1),
        ApiResult::Error(err) => panic!("fold batch must succeed: {}", err.error.message),
    }
}

#[tokio::test]
async fn keyword_search_succeeds_on_indexed_project() {
    let (env, _root, id) = seeded_project().await;
    let response = tools::keyword::handle_keyword_search(
        State(env.state),
        Json(cce_api::models::KeywordSearchRequest {
            query: "fixture_caller".to_string(),
            top_n: 10,
            project_id: id,
            epoch: None,
            offset: 0,
            term_operator: cce_api::models::KeywordTermOperator::Or,
        }),
    )
    .await;
    assert!(
        response.0.success,
        "keyword search must succeed, got {:?}",
        response.0.error
    );
}

#[tokio::test]
async fn get_symbols_succeeds_on_indexed_project() {
    let (env, root, id) = seeded_project().await;
    let file = root
        .path()
        .join("src")
        .join("calls.rs")
        .to_string_lossy()
        .to_string();
    let response = tools::symbol::handle_get_symbols(
        State(env.state),
        Json(cce_api::models::GetSymbolsRequest {
            project_id: id,
            paths: vec![file],
        }),
    )
    .await;
    assert!(
        response.0.success,
        "symbols must succeed, got {:?}",
        response.0.error
    );
}

#[tokio::test]
async fn metrics_text_and_cleanup_succeed() {
    let env = test_env().await;
    let response = metrics::handle_get_metrics(State(env.state.clone()))
        .await
        .into_response();
    assert_eq!(response.status(), StatusCode::OK);
    let json = metrics::handle_get_metrics_json(State(env.state.clone()))
        .await
        .into_response();
    assert_eq!(json.status(), StatusCode::OK);
    match metrics::handle_cleanup_metrics(
        State(env.state),
        Query(cce_api::models::MetricsCleanupQuery {
            all: true,
            before: None,
        }),
    )
    .await
    {
        ApiResult::Success(_) => {}
        ApiResult::Error(err) => panic!("metrics cleanup must succeed: {}", err.error.message),
    }
}

#[tokio::test]
async fn retry_process_and_dead_clear_succeed_on_idle_engine() {
    let env = test_env().await;
    match health::handle_retry_queue_process(
        State(env.state.clone()),
        Query(cce_api::models::RetryQueueQuery { project_id: None }),
    )
    .await
    {
        ApiResult::Success(_) => {}
        ApiResult::Error(err) => panic!("retry process must succeed: {}", err.error.message),
    }
    match health::handle_retry_queue_dead_clear(
        State(env.state),
        Query(cce_api::models::RetryQueueQuery { project_id: None }),
    )
    .await
    {
        ApiResult::Success(_) => {}
        ApiResult::Error(err) => panic!("dead clear must succeed: {}", err.error.message),
    }
}

#[tokio::test]
async fn dead_letter_list_and_retry_succeed_on_indexed_project() {
    let (env, _root, id) = seeded_project().await;
    match project_indexing::handle_dead_letter_list(State(env.state.clone()), Path(id)).await {
        ApiResult::Success(response) => {
            let _ = response.files.len();
        }
        ApiResult::Error(err) => panic!("dead-letter list must succeed: {}", err.error.message),
    }
    match project_indexing::handle_dead_letter_files_retry(
        State(env.state),
        Path(id),
        Json(cce_api::models::DeadLetterRetryRequest { files: Vec::new() }),
    )
    .await
    {
        ApiResult::Success(response) => assert!(response.success),
        ApiResult::Error(err) => panic!("dead-letter retry must succeed: {}", err.error.message),
    }
}
