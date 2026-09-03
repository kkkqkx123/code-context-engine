//! Qdrant HTTP API operation handlers

use reqwest::Client;
use serde_json::json;

use cce_types::PointKind;

use crate::config::{HnswConfig, QdrantConfig, QuantizationConfig, VectorStorageConfig, WalConfig};
use crate::error::QdrantError;
use crate::types::{
    CollectionInfo, CollectionStatus, Payload, SearchQuery, SearchResult, VectorPoint,
    to_qdrant_point_id,
};

/// Collection-level operations (create, info, delete)
pub struct CollectionOperations {
    http_client: Client,
    collection_name: String,
    base_url: String,
    config: QdrantConfig,
}

impl CollectionOperations {
    pub fn new(
        config: QdrantConfig,
        http_client: Client,
        collection_name: String,
        base_url: String,
    ) -> Self {
        Self {
            config,
            http_client,
            collection_name,
            base_url,
        }
    }

    pub async fn get_info(&self) -> Result<CollectionInfo, QdrantError> {
        let url = format!("{}/collections/{}", self.base_url, self.collection_name);

        let response = self
            .http_client
            .get(&url)
            .send()
            .await
            .map_err(|e| QdrantError::request(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            if status == reqwest::StatusCode::NOT_FOUND {
                return Err(QdrantError::CollectionNotFound(
                    cce_types::error::NotFoundError::new(&self.collection_name),
                ));
            }
            return Err(QdrantError::api(format!(
                "Failed to get collection info: {} - {}",
                status, error_text
            )));
        }

        let json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| QdrantError::ResponseParse(e.to_string()))?;

        let result = json.get("result").ok_or_else(|| {
            QdrantError::ResponseParse("Missing result field in collection info".to_string())
        })?;

        let name = result
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let status_str = result
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("green");

        let points_count = result
            .pointer("/points_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let indexed_vectors_count = result
            .pointer("/indexed_vectors_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let segments_count = result
            .pointer("/segments_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let config = result.get("config").and_then(|c| c.get("hnsw_config"));
        let hnsw_config = config.map(|c| crate::types::HnswConfigInfo {
            m: c.get("m").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            ef_construct: c.get("ef_construct").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            on_disk: c.get("on_disk").and_then(|v| v.as_bool()).unwrap_or(false),
            payload_m: c
                .get("payload_m")
                .and_then(|v| v.as_u64())
                .map(|v| v as u32),
        });

        Ok(CollectionInfo {
            name,
            vector_size: self.config.vector_size,
            distance_metric: self.config.distance_metric.as_str().to_string(),
            points_count,
            indexed_vectors_count,
            segments_count,
            status: parse_collection_status(status_str),
            hnsw_config,
            vectors_on_disk: false,
        })
    }

    pub async fn exists(&self) -> Result<bool, QdrantError> {
        let url = format!("{}/collections/{}", self.base_url, self.collection_name);

        let response = self
            .http_client
            .get(&url)
            .send()
            .await
            .map_err(|e| QdrantError::request(e.to_string()))?;

        Ok(response.status().is_success())
    }

    pub async fn create_with_config(
        &self,
        hnsw: Option<HnswConfig>,
        quantization: Option<QuantizationConfig>,
        wal: Option<WalConfig>,
        vector_storage: Option<VectorStorageConfig>,
    ) -> Result<(), QdrantError> {
        let url = format!("{}/collections/{}", self.base_url, self.collection_name);

        let mut body = json!({
            "vectors": {
                "size": self.config.vector_size,
                "distance": self.config.distance_metric.as_str(),
            }
        });

        if let Some(hnsw) = hnsw {
            let mut hnsw_body = json!({
                "m": hnsw.m,
                "ef_construct": hnsw.ef_construct,
                "on_disk": hnsw.on_disk,
                "payload_m": hnsw.payload_m.unwrap_or(hnsw.m),
            });
            if let Some(inline_storage) = hnsw.inline_storage {
                hnsw_body["inline_storage"] = json!(inline_storage);
            }
            body["hnsw_config"] = hnsw_body;
        }

        if let Some(quant) = quantization {
            body["quantization_config"] = match quant {
                QuantizationConfig::Scalar(config) => json!({
                    "scalar": {
                        "type": config.quant_type,
                        "quantile": config.quantile,
                        "always_ram": config.always_ram,
                    }
                }),
                QuantizationConfig::Product(config) => json!({
                    "product": {
                        "compression": config.compression,
                        "always_ram": config.always_ram,
                    }
                }),
                QuantizationConfig::Disabled => json!(null),
            };
        }

        if let Some(wal) = wal {
            body["wal_config"] = json!({
                "wal_capacity_mb": wal.capacity_mb,
                "wal_segments_ahead": wal.segments,
            });
        }

        if let Some(vector_storage) = vector_storage {
            body["vectors"]["on_disk"] = json!(vector_storage.on_disk);
        }

        let response = self
            .http_client
            .put(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| QdrantError::request(e.to_string()))?;

        let status = response.status();
        if status != reqwest::StatusCode::OK && status != reqwest::StatusCode::CREATED {
            let error_text = response.text().await.unwrap_or_default();
            return Err(QdrantError::api(format!(
                "Failed to create collection: {} - {}",
                status, error_text
            )));
        }

        Ok(())
    }

    pub async fn delete(&self) -> Result<(), QdrantError> {
        let url = format!("{}/collections/{}", self.base_url, self.collection_name);

        let response = self
            .http_client
            .delete(&url)
            .send()
            .await
            .map_err(|e| QdrantError::request(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(QdrantError::api(format!(
                "Failed to delete collection: {} - {}",
                status, error_text
            )));
        }

        Ok(())
    }

    pub async fn clear(&self) -> Result<(), QdrantError> {
        let url = format!(
            "{}/collections/{}/points/delete",
            self.base_url, self.collection_name
        );

        let body = json!({
            "filter": {}
        });

        let response = self
            .http_client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| QdrantError::request(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(QdrantError::api(format!(
                "Failed to clear collection: {} - {}",
                status, error_text
            )));
        }

        Ok(())
    }
}

fn parse_collection_status(s: &str) -> CollectionStatus {
    match s.to_lowercase().as_str() {
        "green" => CollectionStatus::Green,
        "yellow" => CollectionStatus::Yellow,
        "red" => CollectionStatus::Red,
        "grey" => CollectionStatus::Grey,
        _ => CollectionStatus::Green,
    }
}

/// Point-level operations (upsert, delete)
pub struct PointOperations {
    http_client: Client,
    collection_name: String,
    base_url: String,
}

fn serialize_point(p: &VectorPoint) -> serde_json::Value {
    let qdrant_id = to_qdrant_point_id(&p.id);

    let mut payload = p.payload.clone();
    if payload.source_id.is_empty() {
        payload.source_id = p.id.clone();
    }
    if payload.r#type.is_none() {
        payload.r#type = Some(PointKind::Chunk);
    }
    let payload = match serde_json::to_value(payload) {
        Ok(value) => value,
        Err(e) => {
            tracing::warn!(
                error = %e,
                "Failed to serialize payload, writing minimal payload"
            );
            json!({
                "source_id": p.payload.source_id,
                "file_path": p.payload.file_path,
                "type": PointKind::Chunk.as_u8(),
            })
        }
    };
    json!({
        "id": qdrant_id,
        "vector": p.vector,
        "payload": payload
    })
}

impl PointOperations {
    pub fn new(http_client: Client, collection_name: String, base_url: String) -> Self {
        Self {
            http_client,
            collection_name,
            base_url,
        }
    }

    pub async fn upsert(&self, points: &[VectorPoint]) -> Result<(), QdrantError> {
        let url = format!(
            "{}/collections/{}/points",
            self.base_url, self.collection_name
        );

        let points_json: Vec<serde_json::Value> = points.iter().map(serialize_point).collect();

        let body = json!({
            "points": points_json
        });

        let response = self
            .http_client
            .put(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| QdrantError::request(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(QdrantError::api(format!(
                "Failed to upsert points: {} - {}",
                status, error_text
            )));
        }

        Ok(())
    }

    pub async fn delete_by_file_path_scoped(
        &self,
        file_path: &str,
        group_id: Option<&str>,
        point_type: Option<PointKind>,
    ) -> Result<(), QdrantError> {
        let mut must = vec![json!({
            "key": "file_path",
            "match": { "value": file_path }
        })];

        if let Some(group_id) = group_id {
            must.push(json!({
                "key": "group_id",
                "match": { "value": group_id }
            }));
        }

        if let Some(point_type) = point_type {
            must.push(json!({
                "key": "type",
                "match": { "value": point_type.as_u8() }
            }));
        }

        self.delete_by_filter(json!({ "must": must })).await
    }

    pub async fn delete_by_file_path_scoped_epoch(
        &self,
        file_path: &str,
        group_id: &str,
        epoch: i64,
    ) -> Result<(), QdrantError> {
        self.delete_by_filter(json!({
            "must": [
                { "key": "file_path", "match": { "value": file_path } },
                { "key": "group_id", "match": { "value": group_id } },
                { "key": "epoch", "match": { "value": epoch } }
            ]
        }))
        .await
    }

    pub async fn delete_by_group_epoch(
        &self,
        group_id: &str,
        epoch: i64,
    ) -> Result<(), QdrantError> {
        self.delete_by_filter(json!({
            "must": [
                { "key": "group_id", "match": { "value": group_id } },
                { "key": "epoch", "match": { "value": epoch } }
            ]
        }))
        .await
    }

    pub async fn delete_by_group(&self, group_id: &str) -> Result<(), QdrantError> {
        let filter = json!({
            "must": [{
                "key": "group_id",
                "match": { "value": group_id }
            }]
        });
        self.delete_by_filter(filter).await
    }

    pub async fn count_all_points(&self) -> Result<usize, QdrantError> {
        let url = format!(
            "{}/collections/{}/points/count",
            self.base_url, self.collection_name
        );

        let body = json!({
            "exact": true
        });

        let response = self
            .http_client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| QdrantError::request(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(QdrantError::api(format!(
                "Failed to count all points: {} - {}",
                status, error_text
            )));
        }

        let resp_json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| QdrantError::request(format!("Failed to parse count response: {}", e)))?;

        let count = resp_json["result"]["count"].as_u64().unwrap_or(0) as usize;
        Ok(count)
    }

    pub async fn count_by_group(&self, group_id: &str) -> Result<usize, QdrantError> {
        let url = format!(
            "{}/collections/{}/points/count",
            self.base_url, self.collection_name
        );

        let body = json!({
            "filter": {
                "must": [{
                    "key": "group_id",
                    "match": { "value": group_id }
                }]
            },
            "exact": true
        });

        let response = self
            .http_client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| QdrantError::request(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(QdrantError::api(format!(
                "Failed to count points by group: {} - {}",
                status, error_text
            )));
        }

        let resp_json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| QdrantError::request(format!("Failed to parse count response: {}", e)))?;

        let count = resp_json["result"]["count"].as_u64().unwrap_or(0) as usize;
        Ok(count)
    }

    pub async fn delete_by_file_paths_scoped(
        &self,
        file_paths: &[&str],
        group_id: Option<&str>,
        point_type: Option<PointKind>,
    ) -> Result<(), QdrantError> {
        let should = file_paths
            .iter()
            .map(|fp| {
                let mut must = vec![json!({
                    "key": "file_path",
                    "match": { "value": fp }
                })];

                if let Some(group_id) = group_id {
                    must.push(json!({
                        "key": "group_id",
                        "match": { "value": group_id }
                    }));
                }

                if let Some(point_type) = point_type {
                    must.push(json!({
                        "key": "type",
                        "match": { "value": point_type.as_u8() }
                    }));
                }

                json!({ "must": must })
            })
            .collect::<Vec<_>>();

        self.delete_by_filter(json!({
            "should": should,
            "min_should": 1
        }))
        .await
    }

    pub async fn scroll_all(&self) -> Result<Vec<VectorPoint>, QdrantError> {
        const PAGE_SIZE: usize = 5000;

        let url = format!(
            "{}/collections/{}/points/scroll",
            self.base_url, self.collection_name
        );

        let mut all_points = Vec::new();
        let mut next_page: Option<serde_json::Value> = None;

        loop {
            let mut body = json!({
                "limit": PAGE_SIZE,
                "with_payload": true,
                "with_vector": true,
            });
            if let Some(ref off) = next_page {
                body["offset"] = off.clone();
            }

            let response = self
                .http_client
                .post(&url)
                .json(&body)
                .send()
                .await
                .map_err(|e| QdrantError::request(e.to_string()))?;

            let status = response.status();
            if !status.is_success() {
                let error_text = response.text().await.unwrap_or_default();
                return Err(QdrantError::api(format!(
                    "Failed to scroll points: {} - {}",
                    status, error_text
                )));
            }

            let scroll_resp: serde_json::Value = response.json().await.map_err(|e| {
                QdrantError::request(format!("Failed to parse scroll response: {}", e))
            })?;

            let result = &scroll_resp["result"];
            let points = &result["points"];

            if let Some(arr) = points.as_array() {
                for point in arr {
                    let payload: Payload = serde_json::from_value(point["payload"].clone())
                        .map_err(|e| {
                            QdrantError::request(format!(
                                "Failed to parse scroll point payload: {}",
                                e
                            ))
                        })?;

                    let vector: Vec<f32> = serde_json::from_value(point["vector"].clone())
                        .map_err(|e| {
                            QdrantError::request(format!(
                                "Failed to parse scroll point vector: {}",
                                e
                            ))
                        })?;

                    let id = payload.source_id.clone();

                    all_points.push(VectorPoint {
                        id,
                        vector,
                        payload,
                    });
                }
            }

            match result["next_page_offset"].clone() {
                serde_json::Value::Null => break,
                val => next_page = Some(val),
            }
        }

        Ok(all_points)
    }

    async fn delete_by_filter(&self, filter: serde_json::Value) -> Result<(), QdrantError> {
        let url = format!(
            "{}/collections/{}/points/delete",
            self.base_url, self.collection_name
        );

        let body = json!({
            "filter": filter
        });

        let response = self
            .http_client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| QdrantError::request(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(QdrantError::api(format!(
                "Failed to delete points: {} - {}",
                status, error_text
            )));
        }

        Ok(())
    }
}

impl Clone for PointOperations {
    fn clone(&self) -> Self {
        Self {
            http_client: self.http_client.clone(),
            collection_name: self.collection_name.clone(),
            base_url: self.base_url.clone(),
        }
    }
}

/// Search operations
pub struct SearchOperations {
    http_client: Client,
    collection_name: String,
    base_url: String,
}

impl SearchOperations {
    pub fn new(http_client: Client, collection_name: String, base_url: String) -> Self {
        Self {
            http_client,
            collection_name,
            base_url,
        }
    }

    pub async fn search(&self, query: SearchQuery) -> Result<Vec<SearchResult>, QdrantError> {
        let url = format!(
            "{}/collections/{}/points/search",
            self.base_url, self.collection_name
        );

        let mut body = json!({
            "vector": query.vector,
            "limit": query.limit,
            "with_payload": true
        });

        if let Some(min_score) = query.min_score {
            body["score_threshold"] = json!(min_score);
        }

        if let Some(hnsw_ef) = query.hnsw_ef {
            body["params"] = json!({ "hnsw_ef": hnsw_ef });
        }

        if let Some(prefix) = &query.directory_prefix {
            let normalized = cce_types::normalize_project_path(prefix);
            let trimmed = normalized.trim_end_matches('/');
            let wildcard = if trimmed.is_empty() {
                "*".to_string()
            } else {
                format!("{trimmed}/*")
            };
            body["filter"] = json!({
                "must": [{
                    "key": "file_path",
                    "wildcard": wildcard
                }]
            });
        }

        let response = self
            .http_client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| QdrantError::request(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(QdrantError::api(format!(
                "Search failed: {} - {}",
                status, error_text
            )));
        }

        let json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| QdrantError::ResponseParse(e.to_string()))?;

        let results: Vec<SearchResult> = json
            .get("result")
            .and_then(|r| r.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| {
                        let score = item.get("score")?.as_f64()? as f32;
                        let payload: Payload =
                            serde_json::from_value(item.get("payload")?.clone()).ok()?;
                        let id = if !payload.source_id.is_empty() {
                            payload.source_id.clone()
                        } else {
                            item.get("id")?.as_str()?.to_string()
                        };
                        Some(SearchResult::new(id, score, payload))
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(results)
    }
}

impl Clone for SearchOperations {
    fn clone(&self) -> Self {
        Self {
            http_client: self.http_client.clone(),
            collection_name: self.collection_name.clone(),
            base_url: self.base_url.clone(),
        }
    }
}

impl Clone for CollectionOperations {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            http_client: self.http_client.clone(),
            collection_name: self.collection_name.clone(),
            base_url: self.base_url.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::{FileCategory, PointKind, TestSource};

    #[test]
    fn test_serialize_point_keeps_full_payload() {
        let payload = Payload::new("src/main.rs")
            .with_source_id("group_1_emb_0")
            .with_group_id("group-1")
            .with_type(PointKind::Chunk)
            .with_category(FileCategory::Code)
            .with_epoch(3)
            .with_batch_id(7)
            .with_entity_ids(vec![42, 43])
            .with_segment_id("group_1")
            .with_test(false)
            .with_test_source(TestSource::Ast);

        let point = VectorPoint::new("group_1_emb_0".to_string(), vec![0.1, 0.2], payload);
        let json = serialize_point(&point);

        let p = &json["payload"];
        assert_eq!(p["source_id"], "group_1_emb_0");
        assert_eq!(p["file_path"], "src/main.rs");
        assert_eq!(p["group_id"], "group-1");
        assert_eq!(p["type"], PointKind::Chunk.as_u8());
        assert_eq!(p["category"], FileCategory::Code.as_u8());
        assert_eq!(p["epoch"], 3);
        assert_eq!(p["batch_id"], 7);
        assert_eq!(p["entity_ids"], serde_json::json!([42, 43]));
        assert_eq!(p["segment_id"], "group_1");
        assert_eq!(p["test"], false);
        assert_eq!(p["test_source"], TestSource::Ast.as_u8());
    }

    #[test]
    fn test_serialize_point_falls_back_to_point_id_and_chunk_type() {
        let payload = Payload::new("src/lib.rs");
        let point = VectorPoint::new("legacy_point".to_string(), vec![0.5], payload);
        let json = serialize_point(&point);

        assert_eq!(json["payload"]["source_id"], "legacy_point");
        assert_eq!(json["payload"]["type"], PointKind::Chunk.as_u8());
    }

    #[test]
    fn test_serialize_point_omits_unset_optional_fields() {
        let payload = Payload::new("src/lib.rs");
        let point = VectorPoint::new("p1".to_string(), vec![0.5], payload);
        let json = serialize_point(&point);

        assert!(json["payload"].get("entity_ids").is_none());
        assert!(json["payload"].get("segment_id").is_none());
        assert!(json["payload"].get("epoch").is_none());
    }
}
