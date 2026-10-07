//! HTTP transport for the remote ingest entries.

use anyhow::{Context, Result};

/// Minimal HTTP client carrying the admission token from the environment.
#[derive(Debug, Clone)]
pub struct GatewayClient {
    client: reqwest::Client,
    base_url: String,
    token: Option<String>,
}

impl GatewayClient {
    /// Build a client for the given server base URL.
    pub fn new(base_url: &str) -> Result<Self> {
        Self::new_with_token(base_url, None)
    }

    /// Build a client with an explicit token outranking the environment.
    pub fn new_with_token(base_url: &str, explicit_token: Option<String>) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .context("Failed to create gateway HTTP client")?;
        let token = cce_api::resolve_gateway_token(explicit_token);
        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            token,
        })
    }

    /// POST a JSON body and decode the JSON response.
    pub async fn post<T: serde::Serialize, R: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &T,
    ) -> Result<R> {
        let url = format!("{}{}", self.base_url, path);
        let mut request = self.client.post(&url).json(body);
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        let response = request
            .send()
            .await
            .context(format!("Failed to POST {url}"))?;
        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("Request failed with status {status}: {text}");
        }
        response
            .json()
            .await
            .context("Failed to parse response JSON")
    }
}
