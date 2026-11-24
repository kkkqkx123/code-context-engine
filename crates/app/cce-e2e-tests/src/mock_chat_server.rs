//! Deterministic mock chat server for self-contained e2e tests.
//!
//! Serves the OpenAI-compatible `/chat/completions` endpoint over plain TCP.
//! The response content is scripted per-instance. For rerank scenarios it
//! extracts the candidate IDs from the cross-encoder prompt and replies with
//! a reversed-score JSON array, so the rerank ordering is visibly different
//! from the input order.

use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, oneshot, watch};

/// A running mock chat server.
pub struct MockChatServer {
    /// Base URL to configure the LLM client with.
    pub base_url: String,
    /// Last received request body (for assertion on request payloads).
    pub last_request: Arc<Mutex<Option<String>>>,
    /// Shutdown signal for the background task.
    shutdown: oneshot::Sender<()>,
    /// Join handle of the server task.
    task: tokio::task::JoinHandle<()>,
}

impl MockChatServer {
    /// Spawn a rerank-oriented chat server on an ephemeral port.
    ///
    /// Every `/chat/completions` request is answered with the given JSON
    /// array as the message content. When `reverse_scores` is set the
    /// candidate IDs found in the prompt are scored in reverse order.
    pub async fn start_rerank(reverse_scores: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock chat server");
        let port = listener.local_addr().expect("local addr").port();
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();
        let (live_tx, mut live_rx) = watch::channel(false);
        let last_request: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

        let task_listener = Arc::new(listener);
        let last_for_task = last_request.clone();
        let task = tokio::spawn(async move {
            loop {
                let conn = tokio::select! {
                    conn = task_listener.accept() => conn,
                    _ = &mut shutdown_rx => break,
                };
                let (stream, _) = match conn {
                    Ok(conn) => conn,
                    Err(_) => break,
                };
                let last = last_for_task.clone();
                tokio::spawn(async move {
                    let _ = handle_connection(stream, last, reverse_scores).await;
                });
            }
        });

        live_tx.send_replace(true);
        while !*live_rx.borrow() {
            if live_rx.changed().await.is_err() {
                break;
            }
        }

        Self {
            base_url: format!("http://127.0.0.1:{port}"),
            last_request,
            shutdown: shutdown_tx,
            task,
        }
    }

    /// Shut the server down and await its task.
    pub async fn stop(self) {
        let _ = self.shutdown.send(());
        let _ = self.task.await;
    }
}

/// Read one HTTP request (headers + body), record it, and serve the chat
/// completion response.
async fn handle_connection(
    mut stream: TcpStream,
    last_request: Arc<Mutex<Option<String>>>,
    reverse_scores: bool,
) -> std::io::Result<()> {
    let mut buf = Vec::with_capacity(4096);
    let mut tmp = [0u8; 4096];

    loop {
        let read = stream.read(&mut tmp).await?;
        if read == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&tmp[..read]);
        if let Some((header_end, content_length)) = parse_headers(&buf) {
            let body_start = header_end + 4;
            if buf.len() >= body_start + content_length {
                let body = &buf[body_start..body_start + content_length];
                let request_body = String::from_utf8_lossy(body).to_string();
                if let Ok(mut guard) = last_request.try_lock() {
                    *guard = Some(request_body.clone());
                }
                let response = build_response(&request_body, reverse_scores);
                stream.write_all(&response).await?;
                return Ok(());
            }
        }
    }
}

/// Locate the header terminator (\r\n\r\n) and Content-Length value.
fn parse_headers(buf: &[u8]) -> Option<(usize, usize)> {
    let haystack = buf.windows(4).position(|w| w == b"\r\n\r\n")?;
    let header = String::from_utf8_lossy(&buf[..haystack]);
    let content_length = header
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.trim()
                .eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    Some((haystack, content_length))
}

/// Build the chat completion HTTP response.
///
/// The candidate IDs embedded in the cross-encoder prompt are scored in
/// reverse order (highest score for the last candidate) so reordering is
/// observable.
fn build_response(request_body: &str, reverse_scores: bool) -> Vec<u8> {
    let candidate_ids = extract_candidate_ids(request_body);

    let scores: Vec<(String, f32)> = if reverse_scores {
        candidate_ids
            .iter()
            .rev()
            .enumerate()
            .map(|(i, id)| (id.clone(), 0.95 - i as f32 * 0.2))
            .collect()
    } else {
        candidate_ids
            .iter()
            .enumerate()
            .map(|(i, id)| (id.clone(), 0.95 - i as f32 * 0.2))
            .collect()
    };

    let items: Vec<serde_json::Value> = scores
        .iter()
        .map(|(id, score)| {
            serde_json::json!({
                "id": id,
                "score": score,
                "reasoning": "mock relevance"
            })
        })
        .collect();
    let content = serde_json::to_string(&items).unwrap_or_else(|_| "[]".to_string());

    let response = serde_json::json!({
        "choices": [{"message": {"content": content}}],
        "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
    });
    let body = serde_json::to_string(&response).unwrap_or_default();

    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )
    .into_bytes()
}

/// Extract candidate IDs from the rerank prompt ("[N] ID: <id>" entries).
///
/// The prompt arrives JSON-escaped inside the request body (newlines are
/// literal `\n` sequences), so we scan the whole body for every occurrence
/// of the "ID:" marker instead of splitting on lines.
fn extract_candidate_ids(request_body: &str) -> Vec<String> {
    let marker = "ID:";
    let mut ids = Vec::new();
    let mut search_from = 0;
    while let Some(pos) = request_body[search_from..].find(marker) {
        let mut id_start = search_from + pos + marker.len();
        // Skip whitespace between the marker and the id.
        while id_start < request_body.len()
            && request_body[id_start..]
                .chars()
                .next()
                .is_some_and(char::is_whitespace)
        {
            id_start += 1;
        }
        let rest = &request_body[id_start..];
        let id: String = rest
            .chars()
            .take_while(|c| !c.is_whitespace() && !matches!(c, '\\' | '"' | ','))
            .collect();
        if !id.is_empty() {
            ids.push(id);
        }
        search_from = id_start;
    }
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_candidate_ids_from_escaped_prompt() {
        let body = r#"{"model":"m","messages":[{"role":"user","content":"Query: q\n\n[0] ID: merged_group_0_bm25_0\nFile: src/lib.rs\nType: chunk\nContent: fn main\n\n[1] ID: 1::0::summary::src/lib.rs\nFile: src/lib.rs\n"}]}"#;
        let ids = extract_candidate_ids(body);
        assert_eq!(
            ids,
            vec![
                "merged_group_0_bm25_0".to_string(),
                "1::0::summary::src/lib.rs".to_string()
            ]
        );
    }
}
