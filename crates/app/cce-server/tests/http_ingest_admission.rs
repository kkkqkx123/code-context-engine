//! Admission-gated push and guardrail tests.
//!
//! Compiled only under `--features admission`: the ingest entries and the
//! admission middlewares do not exist in default builds, so this file
//! doubles as the feature-matrix lock (it must not compile without the
//! feature). Handler tests reuse the shared engine harness; the middleware
//! matrix drives a real loopback socket with hand-rolled HTTP/1.1 so no
//! extra client dependency is needed.
#![cfg(feature = "admission")]

mod common;

use std::io::{BufRead, BufReader, Read, Write};
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Extension, Path, State};
use axum::{Json, Router, routing::get};
use base64::Engine as _;
use cce_admission::{
    AdmissionConfig, AdmissionContext, AdmissionGate, AdmissionMetrics, TokenEntry,
};
use cce_api::models::{MAX_INGEST_BATCH_FILES, error_codes};
use cce_server::api::handlers::ingest;
use cce_server::api::response::ApiResult;

use common::{create_project, seeded_project, test_env};

fn metrics() -> Extension<Arc<AdmissionMetrics>> {
    Extension(Arc::new(AdmissionMetrics::default()))
}

fn manifest_entry(path: &str) -> cce_api::models::IngestFileMeta {
    cce_api::models::IngestFileMeta {
        relative_path: path.to_string(),
        size: 32,
        modified_secs: 1,
        content_hash: Some("deadbeef".to_string()),
    }
}

#[tokio::test]
async fn manifest_lists_unknown_files_for_upload() {
    let env = test_env().await;
    let root = tempfile::TempDir::new().expect("project root");
    let id = create_project(&env.state, &root.path().to_string_lossy()).await;
    match ingest::manifest::handle_ingest_manifest(
        State(env.state),
        Path(id),
        Json(cce_api::models::IngestManifestRequest {
            files: vec![manifest_entry("src/a.rs"), manifest_entry("src/b.rs")],
            manifest_version: 0,
        }),
    )
    .await
    {
        ApiResult::Success(response) => {
            assert!(response.success);
            assert_eq!(response.upload.len(), 2);
            assert_eq!(response.unchanged, 0);
        }
        ApiResult::Error(err) => panic!("manifest must succeed: {}", err.error.message),
    }
}

#[tokio::test]
async fn manifest_versioned_request_lists_missing_chunks() {
    let env = test_env().await;
    let root = tempfile::TempDir::new().expect("project root");
    let id = create_project(&env.state, &root.path().to_string_lossy()).await;
    match ingest::manifest::handle_ingest_manifest(
        State(env.state),
        Path(id),
        Json(cce_api::models::IngestManifestRequest {
            files: vec![manifest_entry("src/a.rs")],
            manifest_version: 41,
        }),
    )
    .await
    {
        ApiResult::Success(response) => {
            assert!(response.success);
            assert!(
                !response.upload.is_empty() || !response.missing_chunks.is_empty(),
                "unstored file must be requested whole or by chunk"
            );
        }
        ApiResult::Error(err) => panic!("manifest must succeed: {}", err.error.message),
    }
}

#[tokio::test]
async fn batch_rejects_over_bound_before_touching_storage() {
    let env = test_env().await;
    let root = tempfile::TempDir::new().expect("project root");
    let id = create_project(&env.state, &root.path().to_string_lossy()).await;
    let files = (0..MAX_INGEST_BATCH_FILES + 1)
        .map(|n| cce_api::models::IngestedFile {
            relative_path: format!("src/f{n}.rs"),
            content_hash: None,
            content_base64: String::new(),
            chunk_index: 0,
            total_chunks: 1,
            chunk_hash: None,
            compressed: false,
        })
        .collect();
    match ingest::batch::handle_ingest_batch(
        State(env.state),
        Path(id),
        None,
        metrics(),
        Json(cce_api::models::IngestBatchRequest {
            files,
            manifest_version: 1,
        }),
    )
    .await
    {
        ApiResult::Error(err) => assert_eq!(err.error.code, error_codes::INVALID_REQUEST),
        ApiResult::Success(_) => panic!("over-bound batch must be rejected"),
    }
}

#[tokio::test]
async fn batch_zero_quota_rejects_whole_batch() {
    let (env, _root, id) = seeded_project().await;
    let context = AdmissionContext::new("fp-zero-quota".to_string(), vec![id], Some(0));
    let files = vec![cce_api::models::IngestedFile {
        relative_path: "src/a.rs".to_string(),
        content_hash: None,
        content_base64: base64::engine::general_purpose::STANDARD.encode(b"fn a() {}"),
        chunk_index: 0,
        total_chunks: 1,
        chunk_hash: None,
        compressed: false,
    }];
    match ingest::batch::handle_ingest_batch(
        State(env.state),
        Path(id),
        Some(Extension(context)),
        metrics(),
        Json(cce_api::models::IngestBatchRequest {
            files,
            manifest_version: 1,
        }),
    )
    .await
    {
        ApiResult::Error(err) => {
            assert_eq!(err.error.code, error_codes::INVALID_REQUEST);
            assert!(
                err.error.message.contains("quota"),
                "unexpected: {}",
                err.error.message
            );
        }
        ApiResult::Success(_) => panic!("zero-quota batch must be rejected"),
    }
}

#[tokio::test]
async fn event_applies_created_file_and_rejects_over_bound() {
    let (env, _root, id) = seeded_project().await;
    let created = cce_api::models::IngestEvent {
        relative_path: "src/pushed_new.rs".to_string(),
        kind: cce_api::models::IngestEventKind::Created,
        content_hash: None,
        content_base64: Some(
            base64::engine::general_purpose::STANDARD.encode(b"pub fn pushed() {}\n"),
        ),
    };
    match ingest::event::handle_ingest_event(
        State(env.state.clone()),
        Path(id),
        Json(cce_api::models::IngestEventRequest {
            events: vec![created],
        }),
    )
    .await
    {
        ApiResult::Success(response) => {
            assert!(response.success, "errors: {:?}", response.errors);
            assert_eq!(response.applied, 1);
        }
        ApiResult::Error(err) => panic!("event must succeed: {}", err.error.message),
    }
    let oversized = (0..MAX_INGEST_BATCH_FILES + 1)
        .map(|n| cce_api::models::IngestEvent {
            relative_path: format!("src/x{n}.rs"),
            kind: cce_api::models::IngestEventKind::Deleted,
            content_hash: None,
            content_base64: None,
        })
        .collect();
    match ingest::event::handle_ingest_event(
        State(env.state),
        Path(id),
        Json(cce_api::models::IngestEventRequest { events: oversized }),
    )
    .await
    {
        ApiResult::Error(err) => assert_eq!(err.error.code, error_codes::INVALID_REQUEST),
        ApiResult::Success(_) => panic!("over-bound event must be rejected"),
    }
}

#[tokio::test]
async fn commit_on_real_project_succeeds() {
    let (env, _root, id) = seeded_project().await;
    match ingest::commit::handle_ingest_commit(State(env.state), Path(id)).await {
        ApiResult::Success(response) => {
            assert!(response.success);
            assert_eq!(response.project_id, id);
        }
        ApiResult::Error(err) => panic!("commit must succeed: {}", err.error.message),
    }
}

#[tokio::test]
async fn admission_stats_reports_process_counters() {
    let env = test_env().await;
    let gate_metrics = Arc::new(AdmissionMetrics::default());
    gate_metrics.record_admitted();
    let response =
        ingest::admission::handle_admission_stats(State(env.state), Extension(gate_metrics)).await;
    assert_eq!(response.0.global.admitted, 1);
    assert_eq!(response.0.global.auth_rejections, 0);
    let _ = response.0.tokens.len();
}

// ---- Middleware matrix over a loopback socket ----

fn gate_with(config: AdmissionConfig) -> Arc<AdmissionGate> {
    Arc::new(AdmissionGate::new(
        &config,
        Arc::new(AdmissionMetrics::default()),
    ))
}

fn matrix_config() -> AdmissionConfig {
    AdmissionConfig {
        enabled: true,
        tokens: vec![TokenEntry {
            token: "matrix-token-abcdef1234567890".to_string(),
            projects: vec![7],
            quota_bytes: None,
        }],
        public_path_prefixes: vec!["/api/health".to_string()],
        rate_limit_per_min: 600,
        max_body_bytes: 16,
        request_timeout_secs: 0,
        cors_allowed_origins: Vec::new(),
    }
}

fn matrix_router(gate: Arc<AdmissionGate>) -> Router {
    Router::new()
        .route("/api/health", get(|| async { "healthy" }))
        .route("/api/project/{id}/probe", get(|| async { "probe" }))
        .route("/api/search", get(|| async { "search" }))
        .layer(axum::middleware::from_fn_with_state(
            Arc::clone(&gate),
            cce_admission::admission_middleware,
        ))
        .with_state(gate)
}

fn raw_request(addr: &std::net::SocketAddr, head: &str, body: Option<&[u8]>) -> (u16, String) {
    let mut stream = std::net::TcpStream::connect(addr).expect("connect stub");
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
    let mut request = head.to_string();
    if let Some(body) = body {
        request.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    request.push_str("Connection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).expect("write head");
    if let Some(body) = body {
        stream.write_all(body).expect("write body");
    }
    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    reader.read_line(&mut status_line).expect("status line");
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("0")
        .parse()
        .unwrap_or(0);
    let mut rest = String::new();
    let _ = reader.read_to_string(&mut rest);
    (status, rest)
}

// The loopback matrix drives blocking raw TCP requests against a server
// task spawned on the same runtime, so it needs a multi-thread runtime:
// a single-threaded runtime would park the server behind the blocking read.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn admission_middleware_matrix() {
    let gate = gate_with(matrix_config());
    let app = matrix_router(gate);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind loopback");
    let addr = listener.local_addr().expect("loopback addr");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve matrix");
    });

    let get = |path: &str, token: Option<&str>| {
        let mut head = format!("GET {path} HTTP/1.1\r\nHost: x\r\n");
        if let Some(token) = token {
            head.push_str(&format!("Authorization: Bearer {token}\r\n"));
        }
        raw_request(&addr, &head, None)
    };

    let (public, _) = get("/api/health", None);
    assert_eq!(public, 200, "public path bypasses admission");

    let (missing, _) = get("/api/project/7/probe", None);
    assert_eq!(missing, 401, "missing token is unauthorized");

    let (unknown, _) = get("/api/project/7/probe", Some("no-such-token-000000000000"));
    assert_eq!(unknown, 401, "unknown token is unauthorized");

    let (scoped, _) = get(
        "/api/project/7/probe",
        Some("matrix-token-abcdef1234567890"),
    );
    assert_eq!(scoped, 200, "bound project is admitted");

    let (forbidden, _) = get(
        "/api/project/8/probe",
        Some("matrix-token-abcdef1234567890"),
    );
    assert_eq!(forbidden, 403, "out-of-scope project is forbidden");

    let big = vec![b'x'; 64];
    let (oversize, _) = raw_request(
        &addr,
        "POST /api/search HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\nAuthorization: Bearer matrix-token-abcdef1234567890\r\n",
        Some(&big),
    );
    assert_eq!(oversize, 413, "body over the admission bound is rejected");

    let scoped_body = serde_json::json!({"project_id": 8})
        .to_string()
        .into_bytes();
    assert!(
        scoped_body.len() <= 16,
        "scoped probe must fit the tiny bound"
    );
    let (body_scope, _) = raw_request(
        &addr,
        "POST /api/search HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\nAuthorization: Bearer matrix-token-abcdef1234567890\r\n",
        Some(&scoped_body),
    );
    assert_eq!(
        body_scope, 403,
        "body-embedded out-of-scope project is forbidden"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn admission_rate_burst_is_rejected() {
    let mut config = matrix_config();
    config.rate_limit_per_min = 2;
    let gate = gate_with(config);
    let app = matrix_router(gate);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind loopback");
    let addr = listener.local_addr().expect("loopback addr");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve rate");
    });

    let mut statuses = Vec::new();
    for _ in 0..3 {
        let (status, _) = raw_request(
            &addr,
            "GET /api/project/7/probe HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer matrix-token-abcdef1234567890\r\n",
            None,
        );
        statuses.push(status);
    }
    assert_eq!(statuses[0], 200);
    assert_eq!(statuses[1], 200);
    assert_eq!(
        statuses[2], 429,
        "third request in the burst is rate limited"
    );
}

#[tokio::test]
async fn admission_stats_shape_is_stable() {
    let snapshot = metrics().0.snapshot();
    assert_eq!(snapshot.admitted, 0);
    assert_eq!(snapshot.auth_rejections, 0);
    assert_eq!(snapshot.scope_rejections, 0);
    assert_eq!(snapshot.rate_rejections, 0);
    assert_eq!(snapshot.body_rejections, 0);
    assert_eq!(snapshot.quota_rejections, 0);
}
