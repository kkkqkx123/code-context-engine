//! HTTP transport for the remote ingest entries.

use anyhow::Result;

use cce_api::http::SharedHttpClient;

/// Minimal HTTP client carrying the admission token from the environment.
#[derive(Debug, Clone)]
pub struct GatewayClient {
    inner: SharedHttpClient,
}

impl GatewayClient {
    /// Build a client for the given server base URL.
    pub fn new(base_url: &str) -> Result<Self> {
        Self::new_with_token(base_url, None)
    }

    /// Build a client with an explicit token outranking the environment.
    pub fn new_with_token(base_url: &str, explicit_token: Option<String>) -> Result<Self> {
        Ok(Self {
            inner: SharedHttpClient::new_with_token(base_url, explicit_token)?,
        })
    }

    /// POST a JSON body and decode the JSON response.
    pub async fn post<T: serde::Serialize, R: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &T,
    ) -> Result<R> {
        self.inner.post(path, body).await
    }
}
