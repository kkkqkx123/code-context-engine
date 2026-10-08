//! Count and aggregation reads.

use serde_json::{Value, json};

use crate::Bm25Error;

use super::ElasticsearchClient;

impl ElasticsearchClient {
    async fn count_with_query(&self, query: &Value) -> Result<usize, Bm25Error> {
        self.check_breaker().await?;
        let body = json!({ "query": query });
        let result = self
            .apply_auth(
                self.http
                    .post(format!("{}/_count", self.config.index_url())),
            )
            .json(&body)
            .send()
            .await
            .map_err(|e| Bm25Error::remote(format!("elasticsearch count failed: {e}")));
        let result = match result {
            Ok(response) => {
                let status = response.status();
                let text = response.text().await.unwrap_or_default();
                if !status.is_success() {
                    Err(Self::classify_status(status, &text, "count"))
                } else {
                    serde_json::from_str::<Value>(&text)
                        .map(|v| v.get("count").and_then(Value::as_u64).unwrap_or(0) as usize)
                        .map_err(|e| {
                            Bm25Error::remote(format!("elasticsearch count parse failed: {e}"))
                        })
                }
            }
            Err(e) => Err(e),
        };
        match &result {
            Ok(_) => self.record_success().await,
            Err(err) => self.record_failure(err).await,
        }
        result
    }

    /// Count all documents in the index.
    pub async fn document_count(&self) -> Result<usize, Bm25Error> {
        self.count_with_query(&json!({ "match_all": {} })).await
    }

    /// Count documents belonging to one project.
    pub async fn document_count_by_project(&self, project_id: i64) -> Result<usize, Bm25Error> {
        self.count_with_query(&json!({
            "bool": { "filter": [{ "term": { "project_id": project_id.to_string() } }] }
        }))
        .await
    }

    /// List data epochs currently present for a project.
    pub async fn epochs_by_project(&self, project_id: i64) -> Result<Vec<i64>, Bm25Error> {
        self.check_breaker().await?;
        let body = json!({
            "size": 0,
            "query": { "bool": { "filter": [{ "term": { "project_id": project_id.to_string() } }] } },
            "aggs": { "epochs": { "terms": { "field": "epoch", "size": 10000 } } },
        });
        let result = self
            .apply_auth(
                self.http
                    .post(format!("{}/_search", self.config.index_url())),
            )
            .json(&body)
            .send()
            .await
            .map_err(|e| Bm25Error::remote(format!("elasticsearch epochs read failed: {e}")));
        let result = match result {
            Ok(response) => {
                let status = response.status();
                let text = response.text().await.unwrap_or_default();
                if !status.is_success() {
                    Err(Self::classify_status(status, &text, "epochs"))
                } else {
                    serde_json::from_str::<Value>(&text)
                        .map(|v| {
                            v.pointer("/aggregations/epochs/buckets")
                                .and_then(Value::as_array)
                                .map(|buckets| {
                                    buckets
                                        .iter()
                                        .filter_map(|b| b.get("key").and_then(Value::as_i64))
                                        .collect()
                                })
                                .unwrap_or_default()
                        })
                        .map_err(|e| {
                            Bm25Error::remote(format!("elasticsearch epochs parse failed: {e}"))
                        })
                }
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
