//! Index mapping, creation, health probe, and lifecycle.

use reqwest::StatusCode;
use serde_json::{Value, json};
use tracing::info;

use crate::Bm25Error;

use super::ElasticsearchClient;

impl ElasticsearchClient {
    /// Index mapping for the single shared index.
    ///
    /// `title` keeps the standard analyzer plus a whitespace-analyzed
    /// pre-tokenized sidecar; body and keyword fields only exist as
    /// pre-tokenized sidecars excluded from `_source` (indexed, not stored).
    /// The BM25 similarity carries the local `k1`/`b` parameters.
    pub fn index_mapping(&self) -> Value {
        let similarity = json!({
            "type": "BM25",
            "k1": self.config.k1,
            "b": self.config.b,
        });
        let mut settings = json!({
            "number_of_shards": 1,
            "number_of_replicas": 0,
            "similarity": { "code_bm25": similarity },
        });
        if let Some(ref interval) = self.config.refresh_interval {
            settings["refresh_interval"] = json!(interval);
        }
        json!({
            "settings": settings,
            "mappings": {
                "_source": { "excludes": ["content", "keywords", "content_tokens", "keywords_tokens", "title_tokens"] },
                "properties": {
                    "document_id": { "type": "keyword", "store": true },
                    "chunk_id": { "type": "keyword", "store": true },
                    "file_path": { "type": "keyword" },
                    "project_id": { "type": "keyword" },
                    "epoch": { "type": "long" },
                    "title": { "type": "text", "similarity": "code_bm25", "store": true },
                    "title_tokens": { "type": "text", "analyzer": "whitespace", "similarity": "code_bm25" },
                    "content_tokens": { "type": "text", "analyzer": "whitespace", "similarity": "code_bm25" },
                    "keywords_tokens": { "type": "text", "analyzer": "whitespace", "similarity": "code_bm25" },
                    "entity_id": { "type": "keyword", "index": false },
                    "segment_id": { "type": "keyword", "index": false },
                    "test": { "type": "integer" },
                    "category": { "type": "integer" },
                }
            }
        })
    }

    /// Create the index when it does not exist yet.
    pub async fn ensure_index(&self) -> Result<bool, Bm25Error> {
        self.check_breaker().await?;
        let head = self
            .apply_auth(self.http.head(self.config.index_url()))
            .send()
            .await
            .map_err(|e| Bm25Error::remote(format!("elasticsearch index probe failed: {e}")))?;
        if head.status().is_success() {
            self.record_success().await;
            return Ok(false);
        }
        let response = self
            .apply_auth(self.http.put(self.config.index_url()))
            .json(&self.index_mapping())
            .send()
            .await
            .map_err(|e| Bm25Error::remote(format!("elasticsearch index create failed: {e}")))?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if status.is_success() {
            info!(index = %self.config.index_name, "Elasticsearch index created");
            self.record_success().await;
            return Ok(true);
        }
        if status == StatusCode::BAD_REQUEST && body.contains("resource_already_exists") {
            self.record_success().await;
            return Ok(false);
        }
        let err = Self::classify_status(status, &body, "index create");
        self.record_failure(&err).await;
        Err(err)
    }

    /// Lightweight reachability probe.
    pub async fn health(&self) -> Result<bool, Bm25Error> {
        self.check_breaker().await?;
        let result = self
            .apply_auth(self.http.get(format!(
                "{}/_cluster/health/{}",
                self.config.base_url, self.config.index_name
            )))
            .send()
            .await
            .map_err(|e| Bm25Error::remote(format!("elasticsearch health probe failed: {e}")));
        let ok = match result {
            Ok(response) => response.status().is_success(),
            Err(e) => {
                self.record_failure(&e).await;
                return Err(e);
            }
        };
        self.record_success().await;
        Ok(ok)
    }

    /// Recreate the index from scratch (generation rebuild/cleanup).
    pub async fn clear_index(&self, index_name: &str) -> Result<usize, Bm25Error> {
        self.validate_index_name(index_name)?;
        let before = self.document_count().await.unwrap_or(0);
        self.check_breaker().await?;
        let response = self
            .apply_auth(self.http.delete(self.config.index_url()))
            .send()
            .await
            .map_err(|e| Bm25Error::remote(format!("elasticsearch index delete failed: {e}")))?;
        let status = response.status();
        if !status.is_success() && status != StatusCode::NOT_FOUND {
            let body = response.text().await.unwrap_or_default();
            let err = Self::classify_status(status, &body, "index delete");
            self.record_failure(&err).await;
            return Err(err);
        }
        self.record_success().await;
        self.ensure_index().await?;
        Ok(before)
    }
}
