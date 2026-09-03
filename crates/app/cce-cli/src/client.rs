//! HTTP client for communicating with CCE server

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{de::DeserializeOwned, Serialize};

/// API client
pub struct ApiClient {
    client: Client,
    base_url: String,
}

impl ApiClient {
    /// Create a new API client
    pub fn new(base_url: &str) -> Result<Self> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
        })
    }

    /// Make a GET request
    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context(format!("Failed to GET {}", url))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Request failed with status {}: {}", status, error_text);
        }

        response
            .json()
            .await
            .context("Failed to parse response JSON")
    }

    /// Make a POST request
    pub async fn post<T: Serialize, R: DeserializeOwned>(&self, path: &str, body: &T) -> Result<R> {
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .client
            .post(&url)
            .json(body)
            .send()
            .await
            .context(format!("Failed to POST {}", url))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Request failed with status {}: {}", status, error_text);
        }

        response
            .json()
            .await
            .context("Failed to parse response JSON")
    }

    /// Make a DELETE request
    pub async fn delete<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .client
            .delete(&url)
            .send()
            .await
            .context(format!("Failed to DELETE {}", url))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Request failed with status {}: {}", status, error_text);
        }

        response
            .json()
            .await
            .context("Failed to parse response JSON")
    }

    /// Make a PUT request
    pub async fn put<T: Serialize, R: DeserializeOwned>(&self, path: &str, body: &T) -> Result<R> {
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .client
            .put(&url)
            .json(body)
            .send()
            .await
            .context(format!("Failed to PUT {}", url))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Request failed with status {}: {}", status, error_text);
        }

        response
            .json()
            .await
            .context("Failed to parse response JSON")
    }

    /// Make a DELETE request with body
    pub async fn delete_with_body<T: Serialize, R: DeserializeOwned>(
        &self,
        path: &str,
        body: &T,
    ) -> Result<R> {
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .client
            .delete(&url)
            .json(body)
            .send()
            .await
            .context(format!("Failed to DELETE {}", url))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Request failed with status {}: {}", status, error_text);
        }

        response
            .json()
            .await
            .context("Failed to parse response JSON")
    }

    /// Execute aggregated search
    pub async fn search_aggregated(
        &self,
        request: &cce_api::models::AggregatedSearchRequest,
    ) -> Result<cce_api::models::AggregatedSearchResponse> {
        let url = format!("{}/api/search/aggregated", self.base_url);

        let response = self
            .client
            .post(&url)
            .json(request)
            .send()
            .await
            .context(format!("Failed to POST {}", url))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Aggregated search failed: {}", error_text);
        }

        response
            .json()
            .await
            .context("Failed to parse aggregated search response JSON")
    }
}
