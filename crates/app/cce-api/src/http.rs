//! Shared HTTP transport for clients talking to the server.
//!
//! The CLI and the file supply gateway previously carried their own copies of
//! base URL handling, token resolution, timeout setup, and error reporting.
//! This module keeps that behavior in one place so both clients send the same
//! requests. Existing client types keep their public shape and delegate here.

use anyhow::{Context, Result};

/// Timeout applied to every shared client request.
pub const SHARED_HTTP_TIMEOUT_SECS: u64 = 300;

/// Normalize a server base URL by dropping trailing slashes.
pub fn normalize_base_url(base_url: &str) -> String {
    base_url.trim_end_matches('/').to_string()
}

/// Build the shared request client with the common timeout.
pub fn build_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(SHARED_HTTP_TIMEOUT_SECS))
        .build()
        .context("Failed to create HTTP client")
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
        Ok(Self {
            client: build_client()?,
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
    async fn checked(
        response: reqwest::Response,
        method: &str,
        url: &str,
    ) -> Result<reqwest::Response> {
        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("Request failed with status {status}: {text}");
        }
        let _ = method;
        let _ = url;
        // The caller holds the URL context in its own error wrapper.
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
        Self::checked(response, "GET", &url)
            .await?
            .json()
            .await
            .context("Failed to parse response JSON")
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
        Self::checked(response, "POST", &url)
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
        Self::checked(response, "PUT", &url)
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
        Self::checked(response, "DELETE", &url)
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
        Self::checked(response, "DELETE", &url)
            .await?
            .json()
            .await
            .context("Failed to parse response JSON")
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
}
