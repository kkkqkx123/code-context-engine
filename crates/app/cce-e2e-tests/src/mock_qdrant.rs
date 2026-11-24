//! Capturing in-process Qdrant stand-in for drift-sweep tests.
//!
//! Answers every request with a successful empty JSON response (like the real
//! server would for writes) and additionally records every upserted point
//! (UUID point id, vector, raw payload) so tests can assert exactly what the
//! storage layer wrote — without a network dependency or a real Qdrant.

use std::sync::{Arc, Mutex};

/// One upserted Qdrant point as captured off the wire.
#[derive(Debug, Clone)]
pub struct CapturedPoint {
    /// UUID v5 derived from the logical point id (`to_qdrant_point_id`).
    pub point_id: String,
    pub vector: Vec<f32>,
    /// Raw payload object as serialized by the client.
    pub payload: serde_json::Value,
}

/// Handle to a running capturing mock: the base URL plus the shared capture
/// buffer.
#[derive(Clone)]
pub struct CapturingMockQdrant {
    url: String,
    points: Arc<Mutex<Vec<CapturedPoint>>>,
}

impl CapturingMockQdrant {
    /// Bind an ephemeral port and start serving until dropped.
    pub async fn spawn() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock qdrant port");
        let addr = listener.local_addr().expect("local addr");
        let points: Arc<Mutex<Vec<CapturedPoint>>> = Arc::new(Mutex::new(Vec::new()));
        let worker_points = Arc::clone(&points);
        tokio::spawn(async move {
            loop {
                let Ok((socket, _)) = listener.accept().await else {
                    break;
                };
                let buffer = Arc::clone(&worker_points);
                tokio::spawn(async move {
                    serve_connection(socket, buffer).await;
                });
            }
        });
        Self {
            url: format!("http://{addr}"),
            points,
        }
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    /// Snapshot of every point captured so far, in arrival order.
    pub fn captured(&self) -> Vec<CapturedPoint> {
        self.points
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Distinct captured point ids (UUID strings).
    pub fn captured_point_ids(&self) -> std::collections::HashSet<String> {
        self.captured()
            .into_iter()
            .map(|point| point.point_id)
            .collect()
    }

    /// Drop everything captured so far (used to separate test phases).
    pub fn clear(&self) {
        self.points
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
    }
}

/// Serve HTTP requests on one keep-alive connection until EOF.
///
/// Only `PUT …/points` bodies are inspected; every request gets a minimal
/// successful JSON response, which satisfies all write/delete/count paths of
/// [`QdrantClient`](cce_storage_vector_qdrant::QdrantClient) used by
/// these tests.
async fn serve_connection(
    mut socket: tokio::net::TcpStream,
    points: Arc<Mutex<Vec<CapturedPoint>>>,
) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let body = br#"{"result":{"operation_id":1,"status":"ok"}}"#;
    let header = format!(
        "HTTP/1.1 200 OK\r\ncontent-length: {}\r\ncontent-type: application/json\r\n\r\n",
        body.len()
    );
    let response = [header.as_bytes(), body].concat();
    // Count endpoints must answer with an actual count, not a write ack.
    let count_body = br#"{"result":{"count":0}}"#;
    let count_header = format!(
        "HTTP/1.1 200 OK\r\ncontent-length: {}\r\ncontent-type: application/json\r\n\r\n",
        count_body.len()
    );
    let count_response = [count_header.as_bytes(), count_body.as_slice()].concat();
    let mut pending: Vec<u8> = Vec::new();
    loop {
        // Accumulate until the full header block is buffered.
        let header_end = loop {
            if let Some(position) = find_subslice(&pending, b"\r\n\r\n") {
                break position;
            }
            let mut chunk = [0u8; 8192];
            match socket.read(&mut chunk).await {
                Ok(0) | Err(_) => return,
                Ok(read) => pending.extend_from_slice(&chunk[..read]),
            }
        };

        let head = String::from_utf8_lossy(&pending[..header_end]).to_string();
        let content_length = head
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.trim()
                    .eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())?
            })
            .unwrap_or(0);

        let body_start = header_end + 4;
        while pending.len() < body_start + content_length {
            let mut chunk = [0u8; 8192];
            match socket.read(&mut chunk).await {
                Ok(0) | Err(_) => return,
                Ok(read) => pending.extend_from_slice(&chunk[..read]),
            }
        }
        let body = pending[body_start..body_start + content_length].to_vec();
        pending.drain(..body_start + content_length);

        let is_upsert = head.starts_with("PUT ") && head.contains("/points");
        if is_upsert
            && let Ok(document) = serde_json::from_slice::<serde_json::Value>(&body)
            && let Some(upserted) = document["points"].as_array()
        {
            let mut batch = Vec::new();
            for entry in upserted {
                let Some(point_id) = entry["id"].as_str().map(str::to_string) else {
                    continue;
                };
                let vector = entry["vector"]
                    .as_array()
                    .map(|values| {
                        values
                            .iter()
                            .filter_map(serde_json::Value::as_f64)
                            .map(|value| value as f32)
                            .collect()
                    })
                    .unwrap_or_default();
                batch.push(CapturedPoint {
                    point_id,
                    vector,
                    payload: entry["payload"].clone(),
                });
            }
            points
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .extend(batch);
        }

        // `POST …/points/count` reads a count; everything else is a write.
        let is_count = head.contains("/points/count");
        if socket
            .write_all(if is_count { &count_response } else { &response })
            .await
            .is_err()
        {
            return;
        }
    }
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}
