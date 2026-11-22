//! Batch write path and write-visibility refresh.

use std::time::Instant;

use serde_json::{Value, json};
use tracing::debug;

use cce_storage_common::FulltextDocument;

use crate::Bm25Error;

use super::ElasticsearchClient;

impl ElasticsearchClient {
    /// Index a batch of documents (idempotent per document id).
    pub async fn batch_index(
        &self,
        index_name: &str,
        documents: &[FulltextDocument],
    ) -> Result<usize, Bm25Error> {
        self.validate_index_name(index_name)?;
        if documents.is_empty() {
            return Ok(0);
        }
        self.check_breaker().await?;
        let start = Instant::now();
        let mut total = 0usize;
        let mut written_ids: Vec<String> = Vec::new();
        for chunk in documents.chunks(self.config.bulk_size.max(1)) {
            match self.bulk_once(chunk).await {
                Ok(count) => {
                    self.record_success().await;
                    total += count;
                    written_ids.extend(chunk.iter().map(|doc| doc.document_id.clone()));
                }
                Err(err) => {
                    self.record_failure(&err).await;
                    // The contract commits a batch as one atomic scope. Remove
                    // chunks already written before returning so a failure
                    // never leaves a partial prefix visible to readers.
                    if let Err(rollback_err) = self.rollback_ids(&written_ids).await {
                        tracing::warn!(
                            error = %rollback_err,
                            "elasticsearch batch rollback failed after write error"
                        );
                    }
                    return Err(err);
                }
            }
        }
        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
        if let Some(metrics) = &self.metrics {
            metrics.record_index(elapsed_ms, total, true);
        }
        debug!(count = total, "Elasticsearch bulk index completed");
        Ok(total)
    }

    /// Remove a batch's already-written documents after a later chunk fails,
    /// so the batch leaves no partial prefix. Best-effort: the caller keeps
    /// the original write error and only logs a rollback failure.
    async fn rollback_ids(&self, ids: &[String]) -> Result<usize, Bm25Error> {
        if ids.is_empty() {
            return Ok(0);
        }
        self.delete_by_query(&json!({ "ids": { "values": ids } }))
            .await
    }

    async fn bulk_once(&self, documents: &[FulltextDocument]) -> Result<usize, Bm25Error> {
        let body = self.bulk_body(documents);
        let response = self
            .apply_auth(
                self.http
                    .post(format!("{}/_bulk", self.config.index_url()))
                    .header("Content-Type", "application/x-ndjson")
                    .body(body),
            )
            .send()
            .await
            .map_err(|e| Bm25Error::remote(format!("elasticsearch bulk failed: {e}")))?;
        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|e| Bm25Error::remote(format!("elasticsearch bulk read failed: {e}")))?;
        if !status.is_success() {
            return Err(Self::classify_status(status, &text, "bulk index"));
        }
        let payload: Value = serde_json::from_str(&text)
            .map_err(|e| Bm25Error::remote(format!("elasticsearch bulk parse failed: {e}")))?;
        if payload
            .get("errors")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return Err(Bm25Error::remote(format!(
                "elasticsearch bulk reported item errors: {text}"
            )));
        }
        Ok(documents.len())
    }

    /// Refresh the index so recent writes become visible to counts and
    /// snapshot readbacks. Called after the last batch before a generation
    /// is marked ready or activated (local branch: no-op, its reader is
    /// reloaded per batch).
    pub async fn flush(&self) -> Result<(), Bm25Error> {
        self.check_breaker().await?;
        let result = self
            .apply_auth(
                self.http
                    .post(format!("{}/_refresh", self.config.index_url())),
            )
            .send()
            .await
            .map_err(|e| Bm25Error::remote(format!("elasticsearch refresh failed: {e}")));
        let result = match result {
            Ok(response) if response.status().is_success() => Ok(()),
            Ok(response) => {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                Err(Self::classify_status(status, &body, "refresh"))
            }
            Err(e) => Err(e),
        };
        match &result {
            Ok(_) => self.record_success().await,
            Err(err) => self.record_failure(err).await,
        }
        result
    }
}
