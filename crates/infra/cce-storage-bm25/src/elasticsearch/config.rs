//! Elasticsearch connection and behavior configuration.

use std::time::Duration;

use cce_config::Validate;
use cce_config::modules::{Bm25Config, FulltextRemoteConfig};

use crate::Bm25Error;

/// Elasticsearch connection and behavior configuration.
#[derive(Debug, Clone)]
pub struct ElasticsearchConfig {
    /// Search-service base URL (no trailing slash).
    pub base_url: String,
    /// API key for `ApiKey` authorization, when set.
    pub api_key: Option<String>,
    /// Basic-auth user name, when set.
    pub username: Option<String>,
    /// Basic-auth password, when set.
    pub password: Option<String>,
    /// Target index name.
    pub index_name: String,
    /// Bulk batch size for write requests.
    pub bulk_size: usize,
    /// Per-request timeout.
    pub request_timeout: Duration,
    /// Desired index refresh interval (index setting value).
    pub refresh_interval: Option<String>,
    /// BM25 term-frequency saturation, mirrored into the index similarity.
    pub k1: f32,
    /// BM25 length normalization, mirrored into the index similarity.
    pub b: f32,
}

impl ElasticsearchConfig {
    /// Build the client configuration from the database configuration.
    ///
    /// Both the URL and the index name are required explicitly: the index
    /// name never falls back to the local `bm25.index_name`, so local and
    /// remote indexes cannot silently share a name. The BM25 algorithm
    /// parameters travel into the index similarity so remote scoring uses
    /// the same saturation and normalization as local scoring.
    pub fn from_remote(
        remote: &FulltextRemoteConfig,
        bm25: &Bm25Config,
    ) -> Result<Self, Bm25Error> {
        let url = remote
            .url
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| {
                Bm25Error::config("database.fulltext_remote.url must be set for the remote branch")
            })?;
        let index_name = remote.explicit_index_name().ok_or_else(|| {
            Bm25Error::config(
                "database.fulltext_remote.index_name must be set for the remote branch",
            )
        })?;
        remote
            .validate_structured()
            .map_err(|e| Bm25Error::config(format!("invalid fulltext remote config: {e}")))?;
        Ok(Self {
            base_url: url.trim_end_matches('/').to_string(),
            api_key: remote.api_key.clone(),
            username: remote.username.clone(),
            password: remote.password.clone(),
            index_name: index_name.to_string(),
            bulk_size: remote.bulk_size,
            request_timeout: Duration::from_millis(remote.request_timeout_ms),
            refresh_interval: remote.refresh_interval.clone(),
            k1: bm25.algorithm.k1,
            b: bm25.algorithm.b,
        })
    }

    pub(super) fn index_url(&self) -> String {
        format!("{}/{}", self.base_url, self.index_name)
    }
}
