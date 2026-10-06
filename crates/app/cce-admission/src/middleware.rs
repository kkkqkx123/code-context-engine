//! Axum admission middleware and its failures.
//!
//! The middleware authenticates every non-public request, binds the verified
//! token to its project scope, and applies per-token rate limiting. Project
//! scope is enforced uniformly from the route path, the query string, and
//! the top-level `project_id` of JSON bodies, so existing handlers keep
//! their data plane filtering untouched.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Json;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use tokio::sync::Mutex;

use crate::context::{AdmissionContext, is_public_path, project_id_from_path};
use crate::metrics::AdmissionMetrics;
use crate::token::TokenStore;

/// Fixed rate limit window shared by all tokens.
const RATE_WINDOW: Duration = Duration::from_secs(60);

/// Admission configuration or verification failure.
#[derive(Debug, thiserror::Error)]
pub enum AdmissionError {
    /// Invalid admission configuration.
    #[error("invalid admission configuration: {0}")]
    Config(String),
}

impl AdmissionError {
    /// Build a configuration failure.
    pub fn config(reason: impl Into<String>) -> Self {
        Self::Config(reason.into())
    }
}

/// One per-token rate limit bucket.
#[derive(Debug, Clone, Copy)]
struct RateBucket {
    window_start: Instant,
    count: u32,
}

/// Shared admission state installed once per router.
#[derive(Debug)]
pub struct AdmissionGate {
    store: TokenStore,
    public_prefixes: Vec<String>,
    rate_limit_per_min: u32,
    max_body_bytes: usize,
    buckets: Mutex<HashMap<[u8; 32], RateBucket>>,
    metrics: Arc<AdmissionMetrics>,
}

impl AdmissionGate {
    /// Build the gate from a validated configuration.
    pub fn new(config: &crate::config::AdmissionConfig, metrics: Arc<AdmissionMetrics>) -> Self {
        Self {
            store: TokenStore::from_config(config),
            public_prefixes: config.public_path_prefixes.clone(),
            rate_limit_per_min: config.rate_limit_per_min,
            max_body_bytes: config.max_body_bytes,
            buckets: Mutex::new(HashMap::new()),
            metrics,
        }
    }

    /// Admission counters backing this gate.
    pub fn metrics(&self) -> &Arc<AdmissionMetrics> {
        &self.metrics
    }

    /// Whether any token is configured.
    pub fn is_empty(&self) -> bool {
        self.store.is_empty()
    }

    async fn check_rate(&self, token_hash: &[u8; 32]) -> bool {
        if self.rate_limit_per_min == 0 {
            return true;
        }
        let mut buckets = self.buckets.lock().await;
        let now = Instant::now();
        let bucket = buckets.entry(*token_hash).or_insert(RateBucket {
            window_start: now,
            count: 0,
        });
        if now.duration_since(bucket.window_start) >= RATE_WINDOW {
            bucket.window_start = now;
            bucket.count = 0;
        }
        bucket.count = bucket.count.saturating_add(1);
        bucket.count <= self.rate_limit_per_min
    }
}

/// Rejection body sharing the shape of the service error responses.
#[derive(Debug, Serialize)]
struct Rejection {
    success: bool,
    error: RejectionDetail,
}

/// Rejection detail sharing the shape of the service error responses.
#[derive(Debug, Serialize)]
struct RejectionDetail {
    code: String,
    message: String,
}

fn reject(status: StatusCode, code: &str, message: &str) -> Response {
    let body = Rejection {
        success: false,
        error: RejectionDetail {
            code: code.to_string(),
            message: message.to_string(),
        },
    };
    (status, Json(body)).into_response()
}

/// Extract the presented token from the authorization headers.
fn presented_token(request: &Request) -> Option<String> {
    if let Some(value) = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    {
        if let Some((scheme, token)) = value.split_once(' ')
            && scheme.eq_ignore_ascii_case("bearer")
        {
            let token = token.trim();
            if !token.is_empty() {
                return Some(token.to_string());
            }
        }
    }
    request
        .headers()
        .get("x-api-token")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
}

/// Extract a `project_id` query argument without extra dependencies.
fn project_id_from_query(query: Option<&str>) -> Option<i64> {
    let query = query?;
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=')?;
        if key == "project_id" {
            return value.parse().ok();
        }
    }
    None
}

/// Admission middleware installed on the remote router.
///
/// Public paths pass through untouched. Every other request must carry a
/// known token, stay inside the token project binding, and respect the
/// per-token rate budget.
pub async fn admission_middleware(
    State(gate): State<Arc<AdmissionGate>>,
    mut request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path().to_string();
    if is_public_path(&path, &gate.public_prefixes) {
        return next.run(request).await;
    }

    let Some(token) = presented_token(&request) else {
        gate.metrics.record_auth_rejection();
        tracing::warn!(path = %path, "admission rejected request without token");
        return reject(
            StatusCode::UNAUTHORIZED,
            "AUTH_REQUIRED",
            "missing or malformed authorization header",
        );
    };

    let Some(entry) = gate.store.authenticate(&token) else {
        gate.metrics.record_auth_rejection();
        tracing::warn!(path = %path, "admission rejected request with unknown token");
        return reject(StatusCode::UNAUTHORIZED, "AUTH_REQUIRED", "unknown token");
    };
    let token_hash = entry.hash;
    let context = AdmissionContext::new(entry.fingerprint.clone(), entry.projects.clone());

    if !gate.check_rate(&token_hash).await {
        gate.metrics.record_rate_rejection();
        tracing::warn!(
            fingerprint = %context.fingerprint,
            path = %path,
            "admission rejected request over the token rate budget"
        );
        return reject(
            StatusCode::TOO_MANY_REQUESTS,
            "RATE_LIMITED",
            "token rate budget exhausted",
        );
    }

    let mut candidates = Vec::new();
    if let Some(id) = project_id_from_path(&path) {
        candidates.push(id);
    }
    if let Some(id) = project_id_from_query(request.uri().query()) {
        candidates.push(id);
    }
    if let Some(content_type) = request
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        && content_type.starts_with("application/json")
        && request.method() != axum::http::Method::GET
    {
        let (parts, body) = request.into_parts();
        match axum::body::to_bytes(body, gate.max_body_bytes).await {
            Ok(bytes) => {
                if !bytes.is_empty()
                    && let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes)
                    && let Some(id) = value.get("project_id").and_then(serde_json::Value::as_i64)
                {
                    candidates.push(id);
                }
                request = Request::from_parts(parts, Body::from(bytes));
            }
            Err(_) => {
                gate.metrics.record_body_rejection();
                tracing::warn!(
                    fingerprint = %context.fingerprint,
                    path = %path,
                    "admission rejected request with oversized body"
                );
                return reject(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "BODY_TOO_LARGE",
                    "request body exceeds the admission limit",
                );
            }
        }
    }

    for project_id in candidates {
        if !context.allows_project(project_id) {
            gate.metrics.record_scope_rejection();
            tracing::warn!(
                fingerprint = %context.fingerprint,
                path = %path,
                project_id,
                "admission rejected request outside the token project binding"
            );
            return reject(
                StatusCode::FORBIDDEN,
                "PROJECT_FORBIDDEN",
                "token is not authorized for this project",
            );
        }
    }

    gate.metrics.record_admitted();
    request.extensions_mut().insert(context);
    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gate_for_tests() -> Arc<AdmissionGate> {
        let config = crate::config::AdmissionConfig {
            enabled: true,
            tokens: vec![crate::config::TokenEntry {
                token: "alpha-token-value-1".to_string(),
                projects: vec![1, 2],
            }],
            ..crate::config::AdmissionConfig::default()
        };
        Arc::new(AdmissionGate::new(
            &config,
            Arc::new(AdmissionMetrics::default()),
        ))
    }

    #[test]
    fn bearer_header_wins_over_fallback() {
        let request = Request::builder()
            .uri("/api/search")
            .header(
                axum::http::header::AUTHORIZATION,
                "Bearer alpha-token-value-1",
            )
            .header("x-api-token", "other-token-value-00000")
            .body(Body::empty())
            .expect("request builds");
        assert_eq!(
            presented_token(&request).as_deref(),
            Some("alpha-token-value-1")
        );
    }

    #[test]
    fn fallback_header_supplies_token() {
        let request = Request::builder()
            .uri("/api/search")
            .header("x-api-token", "alpha-token-value-1")
            .body(Body::empty())
            .expect("request builds");
        assert_eq!(
            presented_token(&request).as_deref(),
            Some("alpha-token-value-1")
        );
    }

    #[test]
    fn query_project_extraction_reads_single_key() {
        assert_eq!(
            project_id_from_query(Some("project_id=12&limit=10")),
            Some(12)
        );
        assert_eq!(project_id_from_query(Some("limit=10")), None);
        assert_eq!(project_id_from_query(None), None);
    }

    #[tokio::test]
    async fn rate_budget_rejects_bursts() {
        let gate = gate_for_tests();
        let hash = crate::token::hash_token("alpha-token-value-1");
        for _ in 0..gate.rate_limit_per_min {
            assert!(gate.check_rate(&hash).await);
        }
        assert!(!gate.check_rate(&hash).await);
    }
}
