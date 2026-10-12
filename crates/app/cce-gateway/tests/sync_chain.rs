//! Gateway full-chain tests against a loopback stub server.
//!
//! The stub speaks just enough HTTP/1.1 (request line, headers,
//! `Content-Length` bodies) to exercise the real `GatewayClient` transport:
//! ingest path templates, bearer forwarding, base64+zstd chunk round-trips,
//! `manifest_version` echo, missing-chunk resume and staged-only passes.
//! No external network, no new dependencies: the stub runs on
//! `std::net` in a background thread.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

/// Canned responder: path and raw body in, status and JSON out.
type Responder = dyn Fn(&str, &[u8]) -> (u16, serde_json::Value) + Send + Sync + 'static;

use base64::Engine as _;
use cce_gateway::{GatewayClient, SyncParams, sync_once};

struct Recorded {
    method: String,
    path: String,
    auth: Option<String>,
    body: Vec<u8>,
}

struct Stub {
    url: String,
    recorded: Arc<Mutex<Vec<Recorded>>>,
    shutdown: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Stub {
    fn spawn(
        responder: impl Fn(&str, &[u8]) -> (u16, serde_json::Value) + Send + Sync + 'static,
    ) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("stub binds loopback");
        let url = format!("http://{}", listener.local_addr().expect("stub addr"));
        let recorded = Arc::new(Mutex::new(Vec::new()));
        let shutdown = Arc::new(AtomicBool::new(false));
        let responder: Arc<Responder> = Arc::new(responder);
        listener
            .set_nonblocking(true)
            .expect("stub nonblocking accept");
        let accept_recorded = Arc::clone(&recorded);
        let accept_shutdown = Arc::clone(&shutdown);
        let handle = std::thread::spawn(move || {
            while !accept_shutdown.load(Ordering::Relaxed) {
                let (stream, _) = match listener.accept() {
                    Ok(pair) => pair,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(10));
                        continue;
                    }
                    Err(_) => break,
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .expect("stub read timeout");
                if let Err(e) = serve_one(stream, &responder, &accept_recorded) {
                    eprintln!("stub connection failed: {e:#}");
                }
            }
        });
        Self {
            url,
            recorded,
            shutdown,
            handle: Some(handle),
        }
    }

    fn requests(&self) -> Vec<(String, String)> {
        self.recorded
            .lock()
            .expect("recorded")
            .iter()
            .map(|r| (r.method.clone(), r.path.clone()))
            .collect()
    }

    fn auth_headers(&self) -> Vec<Option<String>> {
        self.recorded
            .lock()
            .expect("recorded")
            .iter()
            .map(|r| r.auth.clone())
            .collect()
    }

    fn bodies(&self, suffix: &str) -> Vec<serde_json::Value> {
        self.recorded
            .lock()
            .expect("recorded")
            .iter()
            .filter(|r| r.path.ends_with(suffix))
            .map(|r| serde_json::from_slice(&r.body).expect("stub body is json"))
            .collect()
    }
}

impl Drop for Stub {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn serve_one(
    stream: std::net::TcpStream,
    responder: &Arc<Responder>,
    recorded: &Arc<Mutex<Vec<Recorded>>>,
) -> anyhow::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("").to_string();
    let mut headers = HashMap::new();
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            let name = name.trim().to_ascii_lowercase();
            let value = value.trim().to_string();
            if name == "content-length" {
                content_length = value.parse().unwrap_or(0);
            }
            headers.insert(name, value);
        }
    }
    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body)?;
    }
    let auth = headers.get("authorization").cloned();
    recorded.lock().expect("recorded").push(Recorded {
        method,
        path: path.clone(),
        auth,
        body: body.clone(),
    });
    // The responder never fails in tests; fall back to an empty object.
    let (status, payload) = responder(&path, &body);
    let text = serde_json::to_string(&payload).expect("stub payload serializes");
    let reason = if status == 200 { "OK" } else { "Error" };
    let mut stream = reader.into_inner();
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
        text.len()
    )?;
    stream.flush()?;
    Ok(())
}

fn fixture_dir() -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().expect("fixture dir");
    std::fs::write(
        dir.path().join("a.rs"),
        "pub fn alpha() -> i32 {\n    1\n}\n",
    )
    .expect("write a.rs");
    std::fs::write(dir.path().join("notes.txt"), "hello gateway\n".repeat(200))
        .expect("write notes.txt");
    dir
}

fn params_for(project_id: i64, dir: &tempfile::TempDir, commit: bool) -> SyncParams {
    let mut params = SyncParams::with_defaults(project_id, dir.path().to_string_lossy());
    params.commit = commit;
    params.compress = true;
    params.gitignore = false;
    params.json_progress = false;
    params
}

fn manifest_response(project_id: i64, upload: Vec<&str>) -> serde_json::Value {
    serde_json::json!({
        "success": true,
        "project_id": project_id,
        "manifest_version": 41,
        "upload": upload,
        "missing_chunks": [],
        "unchanged": 0,
    })
}

#[tokio::test]
async fn sync_once_runs_manifest_batch_commit_in_order() {
    let dir = fixture_dir();
    let stub = Stub::spawn(|path, _body| {
        if path.ends_with("/ingest/manifest") {
            (200, manifest_response(7, vec!["a.rs", "notes.txt"]))
        } else if path.ends_with("/ingest/batch") {
            (
                200,
                serde_json::json!({
                    "success": true, "project_id": 7,
                    "manifest_version": 41, "staged": 2,
                    "staged_chunks": 2, "errors": [],
                }),
            )
        } else if path.ends_with("/ingest/commit") {
            (
                200,
                serde_json::json!({
                    "success": true, "project_id": 7, "project_name": "stub",
                    "indexed_files": 2, "total_entities": 3,
                    "total_vectors": 0, "elapsed_ms": 1,
                }),
            )
        } else {
            (404, serde_json::json!({"error": path}))
        }
    });

    let client = GatewayClient::new_with_token(&stub.url, Some("test-token-xyz".to_string()))
        .expect("client");
    let outcome = sync_once(&client, &params_for(7, &dir, true), false)
        .await
        .expect("sync completes");

    assert_eq!(outcome.snapshot.files.len(), 2);
    let sequence = stub.requests();
    assert_eq!(
        sequence,
        vec![
            (
                "POST".to_string(),
                "/api/project/7/ingest/manifest".to_string()
            ),
            (
                "POST".to_string(),
                "/api/project/7/ingest/batch".to_string()
            ),
            (
                "POST".to_string(),
                "/api/project/7/ingest/commit".to_string()
            ),
        ]
    );
    for auth in stub.auth_headers() {
        assert_eq!(auth.as_deref(), Some("Bearer test-token-xyz"));
    }

    let manifests = stub.bodies("/ingest/manifest");
    assert_eq!(manifests.len(), 1);
    let files: Vec<String> = serde_json::from_value(manifests[0]["files"].clone())
        .map(|files: Vec<serde_json::Value>| {
            files
                .iter()
                .map(|f| f["relative_path"].as_str().unwrap_or("").to_string())
                .collect()
        })
        .expect("manifest files decode");
    assert!(files.contains(&"a.rs".to_string()));
    assert!(files.contains(&"notes.txt".to_string()));

    let batches = stub.bodies("/ingest/batch");
    assert_eq!(batches.len(), 1);
    // The batch carries the effective version: the server's manifest
    // version raised to at least the locally computed snapshot hash.
    let manifest_version = manifests[0]["manifest_version"]
        .as_u64()
        .expect("manifest version in request");
    assert_eq!(
        batches[0]["manifest_version"].as_u64(),
        Some(manifest_version.max(41)),
        "batch must carry the effective manifest version"
    );
    let batch_files = batches[0]["files"].as_array().expect("batch files array");
    assert_eq!(batch_files.len(), 2);
    for entry in batch_files {
        let rel = entry["relative_path"].as_str().expect("relative path");
        let expected = std::fs::read(dir.path().join(rel)).expect("fixture bytes");
        let payload = base64::engine::general_purpose::STANDARD
            .decode(entry["content_base64"].as_str().expect("base64"))
            .expect("base64 decodes");
        let raw = if entry["compressed"].as_bool().unwrap_or(false) {
            zstd::decode_all(payload.as_slice()).expect("zstd round-trips")
        } else {
            payload
        };
        assert_eq!(raw, expected, "chunk bytes round-trip for {rel}");
    }
}

#[tokio::test]
async fn sync_once_without_commit_stages_only() {
    let dir = fixture_dir();
    let stub = Stub::spawn(|path, _body| {
        if path.ends_with("/ingest/manifest") {
            (200, manifest_response(9, vec!["a.rs"]))
        } else if path.ends_with("/ingest/batch") {
            (
                200,
                serde_json::json!({
                    "success": true, "project_id": 9,
                    "manifest_version": 41, "staged": 1,
                    "staged_chunks": 1, "errors": [],
                }),
            )
        } else {
            (404, serde_json::json!({"error": path}))
        }
    });

    let client =
        GatewayClient::new_with_token(&stub.url, Some("stage-only".to_string())).expect("client");
    sync_once(&client, &params_for(9, &dir, false), false)
        .await
        .expect("staged-only sync completes");

    let sequence = stub.requests();
    assert!(
        sequence
            .iter()
            .all(|(_, path)| !path.ends_with("/ingest/commit")),
        "staged-only pass must not commit: {sequence:?}"
    );
    assert!(
        sequence
            .iter()
            .any(|(_, path)| path.ends_with("/ingest/manifest")),
        "manifest must still run: {sequence:?}"
    );
}

#[tokio::test]
async fn missing_chunks_resume_uploads_only_the_missing_chunk() {
    let dir = fixture_dir();
    let stub = Stub::spawn(|path, _body| {
        if path.ends_with("/ingest/manifest") {
            (
                200,
                serde_json::json!({
                    "success": true, "project_id": 11,
                    "manifest_version": 41, "upload": [],
                    "missing_chunks": [
                        {"relative_path": "a.rs", "chunk_index": 0, "total_chunks": 1},
                    ],
                    "unchanged": 1,
                }),
            )
        } else if path.ends_with("/ingest/batch") {
            (
                200,
                serde_json::json!({
                    "success": true, "project_id": 11,
                    "manifest_version": 41, "staged": 1,
                    "staged_chunks": 1, "errors": [],
                }),
            )
        } else if path.ends_with("/ingest/commit") {
            (
                200,
                serde_json::json!({
                    "success": true, "project_id": 11, "project_name": "stub",
                    "indexed_files": 1, "total_entities": 1,
                    "total_vectors": 0, "elapsed_ms": 1,
                }),
            )
        } else {
            (404, serde_json::json!({"error": path}))
        }
    });

    let client =
        GatewayClient::new_with_token(&stub.url, Some("resume-token".to_string())).expect("client");
    sync_once(&client, &params_for(11, &dir, true), false)
        .await
        .expect("resume sync completes");

    let batches = stub.bodies("/ingest/batch");
    assert_eq!(batches.len(), 1);
    let files = batches[0]["files"].as_array().expect("batch files array");
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["relative_path"], "a.rs");
    assert_eq!(files[0]["chunk_index"], 0);
}
