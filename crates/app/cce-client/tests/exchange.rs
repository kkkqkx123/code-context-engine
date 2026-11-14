//! End-to-end checks of one client exchange against a loopback stub.
//!
//! These cover what unit tests cannot reach: the verb and path actually sent,
//! query encoding, credentials, decoding, and error classification.

use cce_api::models::{EgoQuery, IndexRequest};
use cce_client::{Client, ClientError};

#[path = "stub/mod.rs"]
mod stub;

use stub::{Reply, Stub};

fn ego_query() -> EgoQuery {
    EgoQuery {
        entity_id: "sym::foo/bar".to_string(),
        depth: 2,
        direction: "out".to_string(),
        offset: 0,
        limit: 10,
        domains: String::new(),
        relation_types: String::new(),
        include_external: true,
    }
}

fn subgraph_body() -> serde_json::Value {
    serde_json::json!({
        "success": true,
        "relation_epoch": 7,
        "nodes": [],
        "edges": []
    })
}

fn health_body() -> serde_json::Value {
    serde_json::json!({
        "healthy": true,
        "qdrant": {"reachable": true, "message": "ok"},
        "bm25": {"reachable": true, "message": "ok"},
        "embedding": {"reachable": true, "message": "ok"}
    })
}

fn index_body() -> serde_json::Value {
    serde_json::json!({
        "success": true,
        "message": "indexed",
        "files_scanned": 1,
        "files_indexed": 1,
        "failed_files": 0,
        "degraded_files": 0,
        "skipped_permanent": 0,
        "total_entities": 2,
        "total_relations": 3,
        "total_vectors": 4,
        "elapsed_ms": 5,
        "errors": []
    })
}

#[tokio::test]
async fn graph_query_reaches_contract_path_with_encoded_params() {
    let stub = Stub::spawn(vec![Reply::json(200, subgraph_body())]);
    let client = Client::new(stub.url()).expect("client builds");

    let _ = client
        .graph_ego(3, &ego_query())
        .await
        .expect("stub responds with a subgraph");

    let requests = stub.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "GET");
    assert!(
        requests[0].target.starts_with("/api/project/3/graph/ego?"),
        "unexpected target: {}",
        requests[0].target
    );
    assert!(
        requests[0].target.contains("entity_id=sym%3A%3Afoo%2Fbar"),
        "entity id not encoded: {}",
        requests[0].target
    );
    assert!(requests[0].target.contains("include_external=true"));
}

#[tokio::test]
async fn post_sends_the_wire_body_to_the_contract_path() {
    let stub = Stub::spawn(vec![Reply::json(200, index_body())]);
    let client = Client::new(stub.url()).expect("client builds");

    let request = IndexRequest {
        project_id: 9,
        path: "/tmp/repo".to_string(),
        extensions: vec!["rs".to_string()],
        exclude_dirs: Vec::new(),
        respect_gitignore: true,
        ignore_patterns: Vec::new(),
        custom_gitignore: None,
    };
    let response = client.index(&request).await.expect("index decodes");
    assert_eq!(response.total_entities, 2);

    let requests = stub.requests();
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].target, "/api/index");
    let body: serde_json::Value =
        serde_json::from_str(&requests[0].body).expect("index body is JSON");
    assert_eq!(body["project_id"], 9);
    assert_eq!(body["path"], "/tmp/repo");
}

#[tokio::test]
async fn explicit_token_travels_as_bearer() {
    let stub = Stub::spawn(vec![Reply::json(200, health_body())]);
    let client = Client::with_token(stub.url(), Some("token-1".to_string())).expect("client");
    let _ = client.health().await.expect("health decodes");
    assert_eq!(
        stub.requests()[0].authorization.as_deref(),
        Some("Bearer token-1")
    );
}

#[tokio::test]
async fn retryable_status_is_unavailable_with_status() {
    let stub = Stub::spawn(vec![Reply::json(
        503,
        serde_json::json!({
            "success": false,
            "error": {"code": "SERVICE_UNAVAILABLE", "message": "backend down"}
        }),
    )]);
    let client = Client::new(stub.url()).expect("client builds");
    let error = client
        .health()
        .await
        .expect_err("503 is not a health answer");
    assert!(matches!(error, ClientError::Unavailable { .. }));
    assert_eq!(error.status(), Some(503));
    let text = error.to_string();
    assert!(text.contains("SERVICE_UNAVAILABLE"), "code lost: {text}");
    assert!(text.contains("backend down"), "message lost: {text}");
}

#[tokio::test]
async fn client_error_status_is_rejected_with_code() {
    let stub = Stub::spawn(vec![Reply::json(
        404,
        serde_json::json!({
            "success": false,
            "error": {"code": "ENTITY_NOT_FOUND", "message": "no such project"}
        }),
    )]);
    let client = Client::new(stub.url()).expect("client builds");
    let error = client
        .get_project(7)
        .await
        .expect_err("404 rejects the call");
    assert!(matches!(error, ClientError::Rejected { .. }));
    assert_eq!(error.code(), Some("ENTITY_NOT_FOUND"));
    assert_eq!(error.status(), Some(404));
    assert_eq!(error.path(), "/api/project/7");
}

#[tokio::test]
async fn undecodable_success_body_is_a_protocol_error() {
    let stub = Stub::spawn(vec![Reply::json(200, serde_json::json!({"healthy": true}))]);
    let client = Client::new(stub.url()).expect("client builds");
    let error = client
        .health()
        .await
        .expect_err("missing services do not decode");
    assert!(matches!(error, ClientError::Protocol { .. }));
    let text = error.to_string();
    assert!(text.contains("qdrant"), "missing field not named: {text}");
}

#[tokio::test]
async fn text_endpoint_returns_the_raw_body() {
    let stub = Stub::spawn(vec![Reply::text(200, "# HELP cce_up\ncce_up 1\n")]);
    let client = Client::new(stub.url()).expect("client builds");
    let text = client.metrics_prometheus().await.expect("text decoded");
    assert!(text.contains("cce_up 1"));
    assert_eq!(stub.requests()[0].target, "/api/metrics");
}

#[tokio::test]
async fn unreachable_server_is_unavailable_without_status() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let addr = listener.local_addr().expect("addr");
    drop(listener);
    let client = Client::new(format!("http://{addr}")).expect("client builds");
    let error = client.health().await.expect_err("nothing listens");
    assert!(matches!(error, ClientError::Unavailable { .. }));
    assert_eq!(error.status(), None);
}

#[tokio::test]
async fn path_segments_are_percent_encoded() {
    let stub = Stub::spawn(vec![Reply::json(
        200,
        serde_json::json!({
            "success": true,
            "function": {
                "id": "entity:1",
                "name": "f",
                "signature": "fn f()",
                "file_path": "src/lib.rs",
                "start_line": 1,
                "end_line": 2,
                "parameters": []
            }
        }),
    )]);
    let client = Client::new(stub.url()).expect("client builds");
    let _ = client
        .function_detail(4, "sym::a/b")
        .await
        .expect("function detail decodes");
    assert_eq!(
        stub.requests()[0].target,
        "/api/project/4/function/sym%3A%3Aa%2Fb"
    );
}
