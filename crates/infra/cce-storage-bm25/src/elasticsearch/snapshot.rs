//! Snapshot readback for generation copy.

use serde_json::{Value, json};

use cce_storage_common::FulltextDocument;

use crate::Bm25Error;

use super::ElasticsearchClient;

/// Page size for snapshot readback pagination.
const SNAPSHOT_PAGE_SIZE: usize = 1000;

impl ElasticsearchClient {
    /// Read back the stored fields needed to copy an epoch into a candidate
    /// generation (paginated cursor over the project and generation filter).
    pub async fn snapshot_documents(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Vec<FulltextDocument>, Bm25Error> {
        self.check_breaker().await?;
        let mut documents = Vec::new();
        let mut search_after: Option<Value> = None;
        loop {
            let mut body = json!({
                "query": { "bool": { "filter": Self::scope_filter(project_id, &[epoch]) } },
                "size": SNAPSHOT_PAGE_SIZE,
                "sort": [{ "document_id": "asc" }],
                "_source": ["document_id", "chunk_id", "file_path", "title", "project_id", "epoch", "entity_id", "segment_id", "test", "category"],
            });
            if let Some(after) = search_after.as_ref() {
                body["search_after"] = json!([after]);
            }
            let response = self
                .apply_auth(
                    self.http
                        .post(format!("{}/_search", self.config.index_url())),
                )
                .json(&body)
                .send()
                .await
                .map_err(|e| {
                    Bm25Error::remote(format!("elasticsearch snapshot read failed: {e}"))
                })?;
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            if !status.is_success() {
                let err = Self::classify_status(status, &text, "snapshot read");
                self.record_failure(&err).await;
                return Err(err);
            }
            let payload: Value = serde_json::from_str(&text).map_err(|e| {
                Bm25Error::remote(format!("elasticsearch snapshot parse failed: {e}"))
            })?;
            let empty = Vec::new();
            let entries = payload
                .pointer("/hits/hits")
                .and_then(Value::as_array)
                .unwrap_or(&empty);
            if entries.is_empty() {
                break;
            }
            for entry in entries {
                let source = entry.get("_source").cloned().unwrap_or(Value::Null);
                let document_id = entry
                    .get("_id")
                    .and_then(Value::as_str)
                    .or_else(|| source.get("document_id").and_then(Value::as_str))
                    .unwrap_or_default()
                    .to_string();
                let mut fields = std::collections::HashMap::new();
                for name in ["chunk_id", "file_path", "title", "project_id", "segment_id"] {
                    if let Some(value) = source.get(name).and_then(Value::as_str) {
                        fields.insert(name.to_string(), value.to_string());
                    }
                }
                for name in ["epoch", "test", "category"] {
                    if let Some(value) = source.get(name).and_then(Value::as_i64) {
                        fields.insert(name.to_string(), value.to_string());
                    }
                }
                if let Some(ids) = source.get("entity_id") {
                    let joined = match ids {
                        Value::Array(items) => items
                            .iter()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join(","),
                        Value::String(s) => s.clone(),
                        _ => String::new(),
                    };
                    if !joined.is_empty() {
                        fields.insert("entity_id".to_string(), joined);
                    }
                }
                search_after = entry.get("sort").and_then(|s| s.get(0)).cloned();
                documents.push(FulltextDocument {
                    document_id,
                    fields,
                });
            }
            if entries.len() < SNAPSHOT_PAGE_SIZE {
                break;
            }
            if search_after.is_none() {
                break;
            }
        }
        self.record_success().await;
        Ok(documents)
    }
}
