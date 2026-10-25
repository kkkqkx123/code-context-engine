//! HTTP transport for the remote ingest entries.

use anyhow::Result;

use cce_api::models::{
    IngestBatchRequest, IngestBatchResponse, IngestEventRequest, IngestEventResponse,
    IngestManifestRequest, IngestManifestResponse,
};
use cce_transport::SharedHttpClient;

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

    /// Push an ingest manifest for the given project.
    pub async fn post_manifest(
        &self,
        project_id: i64,
        body: &IngestManifestRequest,
    ) -> Result<IngestManifestResponse> {
        self.inner.post_ingest_manifest(project_id, body).await
    }

    /// Push an ingest batch for the given project.
    pub async fn post_batch(
        &self,
        project_id: i64,
        body: &IngestBatchRequest,
    ) -> Result<IngestBatchResponse> {
        self.inner.post_ingest_batch(project_id, body).await
    }

    /// Commit a staged ingest for the given project.
    pub async fn post_commit<R: serde::de::DeserializeOwned>(
        &self,
        project_id: i64,
        body: &serde_json::Value,
    ) -> Result<R> {
        self.inner.post_ingest_commit(project_id, body).await
    }

    /// Push ingest events for the given project.
    pub async fn post_event(
        &self,
        project_id: i64,
        body: &IngestEventRequest,
    ) -> Result<IngestEventResponse> {
        self.inner.post_ingest_event(project_id, body).await
    }
}
