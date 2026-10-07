//! HTTP client for communicating with CCE server

use anyhow::Result;
use serde::{de::DeserializeOwned, Serialize};

use cce_api::http::SharedHttpClient;

/// API client
pub struct ApiClient {
    inner: SharedHttpClient,
}

impl ApiClient {
    /// Create a new API client
    pub fn new(base_url: &str) -> Result<Self> {
        Self::new_with_token(base_url, None)
    }

    /// Create a client with an explicit token outranking the environment.
    pub fn new_with_token(base_url: &str, explicit_token: Option<String>) -> Result<Self> {
        Ok(Self {
            inner: SharedHttpClient::new_with_token(base_url, explicit_token)?,
        })
    }

    /// Make a GET request
    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.inner.get(path).await
    }

    /// Make a POST request
    pub async fn post<T: Serialize, R: DeserializeOwned>(&self, path: &str, body: &T) -> Result<R> {
        self.inner.post(path, body).await
    }

    /// Make a DELETE request
    pub async fn delete<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.inner.delete(path).await
    }

    /// Make a PUT request
    pub async fn put<T: Serialize, R: DeserializeOwned>(&self, path: &str, body: &T) -> Result<R> {
        self.inner.put(path, body).await
    }

    /// Make a DELETE request with body
    pub async fn delete_with_body<T: Serialize, R: DeserializeOwned>(
        &self,
        path: &str,
        body: &T,
    ) -> Result<R> {
        self.inner.delete_with_body(path, body).await
    }

    /// Execute aggregated search
    pub async fn search_aggregated(
        &self,
        request: &cce_api::models::AggregatedSearchRequest,
    ) -> Result<cce_api::models::SearchResponse> {
        self.post("/api/search/aggregated", request).await
    }
}
