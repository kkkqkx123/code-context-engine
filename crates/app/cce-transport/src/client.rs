//! Shared HTTP transport for remote clients.
//!
//! Moved from the wire contract crate so local execution never pulls in
//! network timeout, base URL, or token behavior. The gateway and the CLI
//! remote mode share this client; local mode does not depend on it.

use anyhow::{Context, Result};
use std::fmt;

/// Transport error carrying an optional HTTP status code.
///
/// Remote service mapping can use the status instead of parsing error text.
#[derive(Debug)]
pub struct HttpTransportError {
    /// HTTP status code when the failure came from a response.
    pub status: Option<u16>,
    /// Human-readable failure detail.
    pub message: String,
}

impl fmt::Display for HttpTransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.status {
            Some(status) => write!(f, "HTTP {status}: {}", self.message),
            None => write!(f, "{}", self.message),
        }
    }
}

impl std::error::Error for HttpTransportError {}

/// Timeout applied to every shared client request.
pub const SHARED_HTTP_TIMEOUT_SECS: u64 = 300;

/// Normalize a server base URL by dropping trailing slashes.
pub fn normalize_base_url(base_url: &str) -> String {
    base_url.trim_end_matches('/').to_string()
}

/// Build the shared request client with the common timeout.
pub fn build_client() -> Result<reqwest::Client> {
    build_client_with_timeout(std::time::Duration::from_secs(SHARED_HTTP_TIMEOUT_SECS))
}

/// Build the shared request client with an explicit timeout.
///
/// Per-command overrides use this so long-running commands do not inherit
/// the shared default.
pub fn build_client_with_timeout(timeout: std::time::Duration) -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(timeout)
        .build()
        .context("Failed to create HTTP client")
}

/// Map a transport failure to a user-facing error without string sniffing.
///
/// 404 becomes `not found`, 5xx becomes `server error`, anything else with
/// a status stays `transport error`. Non-HTTP failures pass through
/// unchanged so their anyhow chain is preserved.
pub fn map_transport_error(error: anyhow::Error) -> anyhow::Error {
    if let Some(http) = error.downcast_ref::<HttpTransportError>() {
        let text = http.to_string();
        return match http.status {
            Some(404) => anyhow::anyhow!("not found: {text}"),
            Some(status) if status >= 500 => anyhow::anyhow!("server error: {text}"),
            _ => anyhow::anyhow!("transport error: {text}"),
        };
    }
    error
}

/// Minimal shared transport carrying the admission token.
#[derive(Debug, Clone)]
pub struct SharedHttpClient {
    client: reqwest::Client,
    base_url: String,
    token: Option<String>,
}

impl SharedHttpClient {
    /// Build a transport for the given server base URL.
    pub fn new(base_url: &str) -> Result<Self> {
        Self::new_with_token(base_url, None)
    }

    /// Build a transport with an explicit token outranking the environment.
    pub fn new_with_token(base_url: &str, explicit_token: Option<String>) -> Result<Self> {
        Self::new_with_token_and_timeout(
            base_url,
            explicit_token,
            std::time::Duration::from_secs(SHARED_HTTP_TIMEOUT_SECS),
        )
    }

    /// Build a transport with an explicit token and a per-command timeout.
    pub fn new_with_token_and_timeout(
        base_url: &str,
        explicit_token: Option<String>,
        timeout: std::time::Duration,
    ) -> Result<Self> {
        Ok(Self {
            client: build_client_with_timeout(timeout)?,
            base_url: normalize_base_url(base_url),
            token: crate::resolve_gateway_token(explicit_token),
        })
    }

    /// Base URL without trailing slashes.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Attach the authorization header when a token is configured.
    fn authed(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.token {
            Some(token) => builder.bearer_auth(token),
            None => builder,
        }
    }

    /// Check a response status and read the body for error context.
    async fn checked(response: reqwest::Response) -> Result<reqwest::Response, HttpTransportError> {
        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(HttpTransportError {
                status: Some(status.as_u16()),
                message: text,
            });
        }
        Ok(response)
    }

    /// Make a GET request.
    pub async fn get<R: serde::de::DeserializeOwned>(&self, path: &str) -> Result<R> {
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .authed(self.client.get(&url))
            .send()
            .await
            .context(format!("Failed to GET {url}"))?;
        Self::checked(response)
            .await?
            .json()
            .await
            .context("Failed to parse response JSON")
    }

    /// Make a GET request returning raw text.
    pub async fn get_text(&self, path: &str) -> Result<String> {
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .authed(self.client.get(&url))
            .send()
            .await
            .context(format!("Failed to GET {url}"))?;
        Self::checked(response)
            .await?
            .text()
            .await
            .context("Failed to read response body")
    }

    /// Make a POST request.
    pub async fn post<T: serde::Serialize, R: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &T,
    ) -> Result<R> {
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .authed(self.client.post(&url).json(body))
            .send()
            .await
            .context(format!("Failed to POST {url}"))?;
        Self::checked(response)
            .await?
            .json()
            .await
            .context("Failed to parse response JSON")
    }

    /// Make a PUT request.
    pub async fn put<T: serde::Serialize, R: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &T,
    ) -> Result<R> {
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .authed(self.client.put(&url).json(body))
            .send()
            .await
            .context(format!("Failed to PUT {url}"))?;
        Self::checked(response)
            .await?
            .json()
            .await
            .context("Failed to parse response JSON")
    }

    /// Make a DELETE request.
    pub async fn delete<R: serde::de::DeserializeOwned>(&self, path: &str) -> Result<R> {
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .authed(self.client.delete(&url))
            .send()
            .await
            .context(format!("Failed to DELETE {url}"))?;
        Self::checked(response)
            .await?
            .json()
            .await
            .context("Failed to parse response JSON")
    }

    /// Make a DELETE request with body.
    pub async fn delete_with_body<T: serde::Serialize, R: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &T,
    ) -> Result<R> {
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .authed(self.client.delete(&url).json(body))
            .send()
            .await
            .context(format!("Failed to DELETE {url}"))?;
        Self::checked(response)
            .await?
            .json()
            .await
            .context("Failed to parse response JSON")
    }

    /// Push an ingest manifest for the given project.
    pub async fn post_ingest_manifest(
        &self,
        project_id: i64,
        body: &cce_api::models::IngestManifestRequest,
    ) -> Result<cce_api::models::IngestManifestResponse> {
        self.post(&cce_api::paths::ingest_manifest(project_id), body)
            .await
    }

    /// Push an ingest batch for the given project.
    pub async fn post_ingest_batch(
        &self,
        project_id: i64,
        body: &cce_api::models::IngestBatchRequest,
    ) -> Result<cce_api::models::IngestBatchResponse> {
        self.post(&cce_api::paths::ingest_batch(project_id), body)
            .await
    }

    /// Commit a staged ingest for the given project.
    pub async fn post_ingest_commit<T: serde::Serialize, R: serde::de::DeserializeOwned>(
        &self,
        project_id: i64,
        body: &T,
    ) -> Result<R> {
        self.post(&cce_api::paths::ingest_commit(project_id), body)
            .await
    }

    /// Push ingest events for the given project.
    pub async fn post_ingest_event(
        &self,
        project_id: i64,
        body: &cce_api::models::IngestEventRequest,
    ) -> Result<cce_api::models::IngestEventResponse> {
        self.post(&cce_api::paths::ingest_event(project_id), body)
            .await
    }

    /// Run an aggregated search across sub queries.
    pub async fn post_aggregated_search(
        &self,
        body: &cce_api::models::AggregatedSearchRequest,
    ) -> Result<cce_api::models::SearchResponse> {
        self.post(cce_api::paths::SEARCH_AGGREGATED, body).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_url_normalization_drops_trailing_slashes() {
        assert_eq!(
            normalize_base_url("http://127.0.0.1:9000///"),
            "http://127.0.0.1:9000"
        );
        assert_eq!(
            normalize_base_url("http://127.0.0.1:9000"),
            "http://127.0.0.1:9000"
        );
    }

    #[test]
    fn shared_timeout_matches_existing_clients() {
        assert_eq!(SHARED_HTTP_TIMEOUT_SECS, 300);
    }

    #[test]
    fn map_404_becomes_not_found() {
        let err = anyhow::anyhow!(HttpTransportError {
            status: Some(404),
            message: "missing".to_string(),
        });
        let mapped = map_transport_error(err);
        let text = format!("{mapped:#}");
        assert!(text.starts_with("not found:"), "unexpected: {text}");
        assert!(text.contains("404"));
    }

    #[test]
    fn map_5xx_becomes_server_error() {
        for status in [500, 502, 503] {
            let err = anyhow::anyhow!(HttpTransportError {
                status: Some(status),
                message: "boom".to_string(),
            });
            let mapped = map_transport_error(err);
            let text = format!("{mapped:#}");
            assert!(text.starts_with("server error:"), "unexpected: {text}");
        }
    }

    #[test]
    fn map_other_status_stays_transport_error() {
        for status in [Some(400), Some(429), None] {
            let err = anyhow::anyhow!(HttpTransportError {
                status,
                message: "denied".to_string(),
            });
            let mapped = map_transport_error(err);
            let text = format!("{mapped:#}");
            assert!(text.starts_with("transport error:"), "unexpected: {text}");
        }
    }

    #[test]
    fn map_non_http_error_passes_through() {
        let err = anyhow::anyhow!("plain failure without status wording");
        let text_before = format!("{err:#}");
        let mapped = map_transport_error(err);
        assert_eq!(format!("{mapped:#}"), text_before);
    }
}
