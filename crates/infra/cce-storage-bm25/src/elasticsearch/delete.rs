//! Delete-by-query operations.

use cce_types::normalize_project_path;
use serde_json::{Value, json};

use crate::Bm25Error;

use super::ElasticsearchClient;

impl ElasticsearchClient {
    async fn delete_by_query(&self, query: &Value) -> Result<usize, Bm25Error> {
        self.check_breaker().await?;
        let body = json!({ "query": query });
        let result = self
            .apply_auth(
                self.http
                    .post(format!("{}/_delete_by_query", self.config.index_url())),
            )
            .json(&body)
            .send()
            .await
            .map_err(|e| Bm25Error::remote(format!("elasticsearch delete failed: {e}")));
        let result = match result {
            Ok(response) => {
                let status = response.status();
                let text = response.text().await.unwrap_or_default();
                if !status.is_success() {
                    Err(Self::classify_status(status, &text, "delete"))
                } else {
                    serde_json::from_str::<Value>(&text)
                        .map(|v| v.get("deleted").and_then(Value::as_u64).unwrap_or(0) as usize)
                        .map_err(|e| {
                            Bm25Error::remote(format!("elasticsearch delete parse failed: {e}"))
                        })
                }
            }
            Err(e) => Err(e),
        };
        match &result {
            Ok(_) => self.record_success().await,
            Err(err) => self.record_failure(err).await,
        }
        let deleted = result?;
        if let Some(metrics) = &self.metrics {
            metrics.record_delete(0.0, deleted, true);
        }
        Ok(deleted)
    }

    /// Delete documents for one file within one project.
    pub async fn delete_by_file_path_scoped(
        &self,
        index_name: &str,
        file_path: &str,
        project_id: i64,
    ) -> Result<usize, Bm25Error> {
        self.validate_index_name(index_name)?;
        let normalized = normalize_project_path(file_path);
        self.delete_by_query(&json!({
            "bool": { "filter": [
                { "term": { "file_path": normalized } },
                { "term": { "project_id": project_id.to_string() } },
            ]}
        }))
        .await
    }

    /// Delete documents for one file in one data epoch.
    pub async fn delete_by_file_path_scoped_epoch(
        &self,
        index_name: &str,
        file_path: &str,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, Bm25Error> {
        self.validate_index_name(index_name)?;
        let normalized = normalize_project_path(file_path);
        self.delete_by_query(&json!({
            "bool": { "filter": [
                { "term": { "file_path": normalized } },
                { "term": { "project_id": project_id.to_string() } },
                { "term": { "epoch": epoch } },
            ]}
        }))
        .await
    }

    /// Delete all documents for one project and data epoch.
    pub async fn delete_by_project_epoch(
        &self,
        index_name: &str,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, Bm25Error> {
        self.validate_index_name(index_name)?;
        self.delete_by_query(&json!({
            "bool": { "filter": [
                { "term": { "project_id": project_id.to_string() } },
                { "term": { "epoch": epoch } },
            ]}
        }))
        .await
    }

    /// Delete all documents for a project.
    pub async fn delete_all_project_docs(
        &self,
        index_name: &str,
        project_id: i64,
    ) -> Result<usize, Bm25Error> {
        self.validate_index_name(index_name)?;
        self.delete_by_query(&json!({
            "bool": { "filter": [
                { "term": { "project_id": project_id.to_string() } },
            ]}
        }))
        .await
    }
}
