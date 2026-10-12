use std::collections::HashMap;

use cce_config::AppConfig;
use cce_config::modules::{EmbeddingModelConfig, ProviderConfig};
use cce_server::api::state::AppState;
use cce_server::engine::CodeContextEngine;

pub struct TestEnv {
    pub state: AppState,
    pub _data_dir: tempfile::TempDir,
}

static ENV_SERIAL: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();

/// Deterministic offline stand-in for an OpenAI-compatible provider.
///
/// Spawns a loopback HTTP/1.1 server that answers `/v1/embeddings` with
/// stable vectors derived from the input texts and `/v1/chat/completions`
/// with a canned completion. Indexing then runs its real end-to-end path
/// (vectors, BM25, summaries, relation snapshot publish) without external
/// network access, so success-path tests assert against fully landed data.
#[allow(dead_code)]
pub fn spawn_llm_stub(dimension: usize) -> String {
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").expect("loopback bind for llm stub");
    let port = listener
        .local_addr()
        .expect("loopback addr for llm stub")
        .port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            std::thread::spawn(move || serve_stub_connection(stream, dimension));
        }
    });
    format!("http://127.0.0.1:{port}/v1")
}

/// Answer one stub connection: read the full request, route by path, and
/// close. Runs on its own thread so concurrent index batches never queue
/// behind a slow peer.
fn serve_stub_connection(mut stream: std::net::TcpStream, dimension: usize) {
    use std::io::{Read, Write};

    let mut request = Vec::new();
    let mut header_end = None;
    let mut content_length = 0usize;
    loop {
        let mut buf = [0u8; 4096];
        let Ok(read) = stream.read(&mut buf) else {
            break;
        };
        if read == 0 {
            break;
        }
        request.extend_from_slice(&buf[..read]);
        if header_end.is_none()
            && let Some(pos) = find_subsequence(&request, b"\r\n\r\n")
        {
            header_end = Some(pos + 4);
            let head = String::from_utf8_lossy(&request[..pos]).to_lowercase();
            for line in head.lines() {
                if let Some(value) = line.strip_prefix("content-length:") {
                    content_length = value.trim().parse().unwrap_or(0);
                }
            }
        }
        if let Some(head_len) = header_end
            && request.len() >= head_len + content_length
        {
            break;
        }
    }
    let body = request
        .get(header_end.unwrap_or(0)..)
        .unwrap_or_default()
        .to_vec();
    let head = String::from_utf8_lossy(request.get(..header_end.unwrap_or(0)).unwrap_or_default())
        .to_lowercase();
    let body_json: serde_json::Value =
        serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);

    let response_body = if head.contains("/embeddings") {
        let inputs = body_json
            .get("input")
            .and_then(|value| value.as_array())
            .cloned()
            .unwrap_or_default();
        let data: Vec<serde_json::Value> = inputs
            .iter()
            .enumerate()
            .map(|(index, input)| {
                serde_json::json!({
                    "index": index,
                    "embedding": stub_vector(input.as_str().unwrap_or(""), index, dimension),
                })
            })
            .collect();
        serde_json::json!({
            "object": "list",
            "data": data,
            "model": "test-model",
            "usage": {"prompt_tokens": 1, "total_tokens": 1},
        })
        .to_string()
    } else if head.contains("/chat/completions") {
        serde_json::json!({
            "id": "chatcmpl-stub",
            "object": "chat.completion",
            "model": "test-model",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "stub summary"},
                "finish_reason": "stop",
            }],
            "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2},
        })
        .to_string()
    } else {
        r#"{"error":{"message":"stub: unsupported path"}}"#.to_string()
    };
    let status_line = if head.contains("/embeddings") || head.contains("/chat/completions") {
        "HTTP/1.1 200 OK"
    } else {
        "HTTP/1.1 404 Not Found"
    };
    let response = format!(
        "{status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response_body}",
        response_body.len()
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

/// Stable unit-length vector for one stub input: seeded from the text and
/// its position so repeated runs embed identical content identically.
fn stub_vector(text: &str, index: usize, dimension: usize) -> Vec<f32> {
    let mut seed: u64 = 0xcbf2_9ce4_8422_2325 ^ (index as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    for byte in text.as_bytes() {
        seed ^= u64::from(*byte);
        seed = seed.wrapping_mul(0x100_0000_01b3);
    }
    let mut vec = Vec::with_capacity(dimension);
    let mut state = seed | 1;
    for _ in 0..dimension {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let sample = ((state >> 32) as u32 as f64 / u32::MAX as f64) * 2.0 - 1.0;
        vec.push(sample as f32);
    }
    let norm = vec.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm > 0.0 {
        vec.iter().map(|v| v / norm).collect()
    } else {
        vec
    }
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[allow(dead_code)]
pub async fn test_env() -> TestEnv {
    let _guard = ENV_SERIAL
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await;
    let data_dir = tempfile::TempDir::new().expect("temp data dir");
    let mut config = AppConfig::default();
    config.database.sqlite.path = data_dir
        .path()
        .join("cce_http_contract.db")
        .to_string_lossy()
        .to_string();

    let mut providers = HashMap::new();
    providers.insert(
        "test-provider".to_string(),
        ProviderConfig {
            id: "test-provider".to_string(),
            name: "Test Provider".to_string(),
            base_url: spawn_llm_stub(384),
            api_keys: vec!["test-key".to_string()],
            ..ProviderConfig::default()
        },
    );
    config.llm.providers = providers;
    let mut models = HashMap::new();
    models.insert(
        "test-model".to_string(),
        EmbeddingModelConfig {
            provider_id: "test-provider".to_string(),
            model: "test-model".to_string(),
            vector_dimension: 384,
            ..EmbeddingModelConfig::default()
        },
    );
    config.llm.embedding_models = models;
    config.embedder.default_model = "test-model".to_string();
    // Keep the local vector collection dimension in sync with the mock
    // embedder's declared dimension, otherwise vector storage rejects
    // every batch and a batch-level error skips relation publication.
    config.database.vector_local.vector_size = 384;
    // BM25 defaults to disabled; the embedded Tantivy index is what the
    // keyword-search success paths assert against, so point it at the
    // per-test temp dir and enable it.
    config.database.bm25.enabled = true;
    config.database.bm25.index_path =
        Some(data_dir.path().join("bm25").to_string_lossy().to_string());

    let engine = CodeContextEngine::builder()
        .config(config)
        .build()
        .await
        .expect("engine builds with temp sqlite and mock provider");
    let state = AppState::from_engine(&engine, None).await;
    TestEnv {
        state,
        _data_dir: data_dir,
    }
}

#[allow(dead_code)]
pub fn create_request(root: &str) -> cce_api::models::CreateProjectRequest {
    create_request_named(root, "contract-probe")
}

/// Same as [`create_request`] with an explicit project name, so tests that
/// create several projects in one engine avoid the duplicate-name rejection.
#[allow(dead_code)]
pub fn create_request_named(root: &str, name: &str) -> cce_api::models::CreateProjectRequest {
    cce_api::models::CreateProjectRequest {
        name: Some(name.to_string()),
        root_path: root.to_string(),
        extensions: Vec::new(),
        exclude_dirs: Vec::new(),
        respect_gitignore: true,
        ignore_patterns: Vec::new(),
        supply_mode: None,
    }
}

#[allow(dead_code)]
pub async fn create_project(state: &AppState, root: &str) -> i64 {
    create_project_named(state, root, "contract-probe").await
}

#[allow(dead_code)]
pub async fn create_project_named(state: &AppState, root: &str, name: &str) -> i64 {
    use axum::Json;
    use axum::extract::State;
    use cce_server::api::handlers::project;
    use cce_server::api::response::ApiResult;
    match project::management::handle_create_project(
        State(state.clone()),
        Json(create_request_named(root, name)),
    )
    .await
    {
        ApiResult::Success(detail) => detail.project.id.parse().expect("project id is numeric"),
        ApiResult::Error(err) => panic!("create must succeed: {}", err.error.message),
    }
}

/// Minimal deterministic code fixture: a Rust caller/callee pair plus one
/// Python helper, so parse, search, entity, graph and summary paths all have
/// real data without external networks or large inputs.
#[allow(dead_code)]
pub fn write_code_fixture(root: &std::path::Path) {
    let src = root.join("src");
    std::fs::create_dir_all(&src).expect("fixture src dir");
    std::fs::write(
        src.join("calls.rs"),
        "pub fn fixture_callee() -> i32 {\n    41\n}\n\npub fn fixture_caller() -> i32 {\n    fixture_callee() + 1\n}\n",
    )
    .expect("fixture calls.rs");
    std::fs::write(
        src.join("extra.py"),
        "def fixture_helper():\n    return \"hi\"\n",
    )
    .expect("fixture extra.py");
}

/// Run a full index over a fixture project. The loopback LLM stub answers
/// embeddings and chat, so every stage (parse, chunks, vectors, BM25,
/// summaries, relation publish) executes its real path and the response
/// must come back clean; callers assert that through `seeded_project`.
#[allow(dead_code)]
pub async fn index_fixture_project(
    state: &AppState,
    project_id: i64,
    root: &std::path::Path,
) -> cce_api::models::IndexResponse {
    use axum::Json;
    use axum::extract::State;
    use cce_server::api::handlers::index;
    use cce_server::api::response::ApiResult;
    let request = cce_api::models::IndexRequest {
        project_id,
        path: root.to_string_lossy().to_string(),
        extensions: vec!["rs".to_string(), "py".to_string()],
        exclude_dirs: Vec::new(),
        respect_gitignore: false,
        ignore_patterns: Vec::new(),
        custom_gitignore: None,
    };
    match index::execute::handle_index(State(state.clone()), Json(request)).await {
        ApiResult::Success(response) => response,
        ApiResult::Error(err) => panic!("index must dispatch: {}", err.error.message),
    }
}

/// Create a project over a fresh fixture and index it. Returns the
/// environment, the project root (kept alive by the caller) and the id.
///
/// The seeded project is the substrate every success-path assertion in the
/// suite reads from, so the index must land cleanly end-to-end: files,
/// entities, chunks, vectors, BM25 documents and the published relation
/// snapshot. A single stage error here would surface much later as an
/// opaque "relation index not available", so the errors are asserted empty.
#[allow(dead_code)]
pub async fn seeded_project() -> (TestEnv, tempfile::TempDir, i64) {
    let env = test_env().await;
    let root = tempfile::TempDir::new().expect("fixture root");
    write_code_fixture(root.path());
    let id = create_project(&env.state, &root.path().to_string_lossy()).await;
    let outcome = index_fixture_project(&env.state, id, root.path()).await;
    assert!(
        outcome.files_indexed >= 2,
        "fixture index must land every file, got {} scanned / {} indexed (errors: {:?})",
        outcome.files_scanned,
        outcome.files_indexed,
        outcome.errors
    );
    assert!(
        outcome.errors.is_empty(),
        "fixture index must complete without stage errors, got {:?}",
        outcome.errors
    );
    assert!(
        outcome.total_relations >= 1,
        "fixture index must publish the caller/callee relation, got {}",
        outcome.total_relations
    );
    (env, root, id)
}
