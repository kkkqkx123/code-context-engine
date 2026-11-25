//! Qdrant vector retrieval implementation

use cce_types::StorageError;
use reqwest::Client;

use crate::config::DistanceMetric;
use crate::types::Payload;
use cce_storage_common::{DenseSearchQuery, ScoredPoint, SearchFilter};

/// Qdrant-based vector retrieval implementation
pub struct QdrantRetrieval {
    /// HTTP client for making requests
    http_client: Client,
    /// Qdrant base URL (e.g., "http://localhost:6334")
    base_url: String,
    /// Collection name to search
    collection_name: String,
    /// Distance metric backing the collection, used to report scores in the
    /// crate-wide "higher is better" form.
    distance_metric: DistanceMetric,
}

impl QdrantRetrieval {
    /// Create a new Qdrant retrieval instance using the default Cosine metric.
    pub fn new(http_client: Client, base_url: String, collection_name: String) -> Self {
        Self::new_with_metric(
            http_client,
            base_url,
            collection_name,
            DistanceMetric::Cosine,
        )
    }

    /// Create a retrieval instance bound to a collection with an explicit
    /// distance metric.
    pub fn new_with_metric(
        http_client: Client,
        base_url: String,
        collection_name: String,
        distance_metric: DistanceMetric,
    ) -> Self {
        Self {
            http_client,
            base_url,
            collection_name,
            distance_metric,
        }
    }

    /// Build search filter from filter options
    fn build_filter(&self, filter: Option<&SearchFilter>) -> Option<serde_json::Value> {
        filter.and_then(|f| {
            if let Some(ref raw) = f.raw_filter {
                return Some(raw.clone());
            }

            let mut must_conditions: Vec<serde_json::Value> = Vec::new();
            let mut must_not_conditions: Vec<serde_json::Value> = Vec::new();

            if !f.epochs.is_empty() {
                must_conditions.push(Self::build_epoch_condition(&f.epochs));
            }

            let excluded = f.excluded_files.as_ref().filter(|files| !files.is_empty());
            if let Some(excluded) = excluded
                && f.epochs.len() > 1
            {
                must_not_conditions.push(Self::build_parent_exclusion_condition(
                    f.epochs[0],
                    excluded,
                ));
            }

            if let Some(ref group_id) = f.group_id {
                must_conditions.push(serde_json::json!({
                    "key": "group_id",
                    "match": { "value": group_id }
                }));
            }

            if let Some(point_type) = f.point_type {
                must_conditions.push(serde_json::json!({
                    "key": "type",
                    "match": { "value": point_type.as_u8() }
                }));
            }

            if let Some(ref file_path) = f.file_path {
                let normalized = cce_types::normalize_project_path(file_path);
                must_conditions.push(serde_json::json!({
                    "key": "file_path",
                    "match": { "value": normalized }
                }));
            }

            if let Some(ref categories) = f.include_categories
                && !categories.is_empty()
            {
                let or_conditions: Vec<serde_json::Value> = categories
                    .iter()
                    .map(|cat| {
                        serde_json::json!({
                            "key": "category",
                            "match": { "value": cat.as_u8() }
                        })
                    })
                    .collect();
                must_conditions.push(serde_json::json!({
                    "min_should": {
                        "conditions": or_conditions,
                        "min_count": 1
                    }
                }));
            }

            if let Some(ref categories) = f.exclude_categories {
                for cat in categories {
                    must_not_conditions.push(serde_json::json!({
                        "key": "category",
                        "match": { "value": cat.as_u8() }
                    }));
                }
            }

            if f.exclude_test {
                must_not_conditions.push(serde_json::json!({
                    "key": "test",
                    "match": { "value": true }
                }));
            }

            if f.exclude_generated {
                must_not_conditions.push(serde_json::json!({
                    "key": "generated",
                    "match": { "value": true }
                }));
            }

            if f.exclude_dependency {
                must_not_conditions.push(serde_json::json!({
                    "key": "dependency",
                    "match": { "value": true }
                }));
            }

            if must_conditions.is_empty() && must_not_conditions.is_empty() {
                None
            } else {
                let mut filter_json = serde_json::json!({});
                if !must_conditions.is_empty() {
                    filter_json["must"] = serde_json::json!(must_conditions);
                }
                if !must_not_conditions.is_empty() {
                    filter_json["must_not"] = serde_json::json!(must_not_conditions);
                }
                Some(filter_json)
            }
        })
    }

    fn build_epoch_condition(epochs: &[i64]) -> serde_json::Value {
        if epochs.len() == 1 {
            return serde_json::json!({
                "key": "epoch",
                "match": { "value": epochs[0] }
            });
        }
        let should_conditions: Vec<serde_json::Value> = epochs
            .iter()
            .map(|epoch| {
                serde_json::json!({
                    "key": "epoch",
                    "match": { "value": epoch }
                })
            })
            .collect();
        serde_json::json!({
            "min_should": {
                "conditions": should_conditions,
                "min_count": 1
            }
        })
    }

    fn build_parent_exclusion_condition(
        parent_epoch: i64,
        excluded_files: &[String],
    ) -> serde_json::Value {
        let path_matches: Vec<serde_json::Value> = excluded_files
            .iter()
            .map(|path| {
                serde_json::json!({
                    "key": "file_path",
                    "match": { "value": path }
                })
            })
            .collect();
        serde_json::json!({
            "must": [
                { "key": "epoch", "match": { "value": parent_epoch } }
            ],
            "min_should": {
                "conditions": path_matches,
                "min_count": 1
            }
        })
    }

    async fn execute_search(
        &self,
        url: &str,
        body: serde_json::Value,
    ) -> Result<Vec<ScoredPoint>, StorageError> {
        let response = self
            .http_client
            .post(url)
            .json(&body)
            .send()
            .await
            .map_err(|e| StorageError::Connection(format!("Failed to send request: {}", e)))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(StorageError::Query(format!(
                "Search failed with status {}: {}",
                status, error_text
            )));
        }

        let json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| StorageError::Query(format!("Failed to parse response: {}", e)))?;

        let results = parse_search_response(json, self.distance_metric);

        tracing::trace!("Search returned {} results", results.len());
        Ok(results)
    }
}

/// Normalize a raw Qdrant score into the contract's "higher is better" form.
///
/// Qdrant returns cosine similarity and dot product with the contract
/// direction already, but returns Euclidean distance where smaller is nearer.
/// For distance metrics we map `d` to `1/(1+d)` — identical to the local
/// engine's `to_score` — so callers see one consistent score semantic across
/// backends. Non-finite or negative distances collapse to zero similarity.
fn normalize_score(metric: DistanceMetric, raw: f32) -> f32 {
    match metric {
        DistanceMetric::Cosine | DistanceMetric::Dot => raw,
        DistanceMetric::Euclid => {
            if raw.is_finite() && raw >= 0.0 {
                1.0 / (1.0 + raw)
            } else {
                0.0
            }
        }
    }
}

fn parse_search_response(json: serde_json::Value, metric: DistanceMetric) -> Vec<ScoredPoint> {
    let points = match json.get("result") {
        Some(serde_json::Value::Array(arr)) => arr,
        Some(other) => {
            let Some(points) = other.get("points").and_then(|p| p.as_array()) else {
                tracing::warn!("Unexpected result shape, no points array found");
                return Vec::new();
            };
            points
        }
        None => return Vec::new(),
    };

    points
        .iter()
        .filter_map(|item| {
            let raw_score = item.get("score")?.as_f64()? as f32;
            let score = normalize_score(metric, raw_score);
            let payload_json = item.get("payload")?;
            let payload = match serde_json::from_value::<Payload>(payload_json.clone()) {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!("Failed to parse payload: {}", e);
                    return None;
                }
            };
            let id = if !payload.source_id.is_empty() {
                payload.source_id.clone()
            } else {
                item.get("id")?.as_str()?.to_string()
            };
            Some(ScoredPoint { id, score, payload })
        })
        .collect()
}

impl QdrantRetrieval {
    /// Perform dense vector similarity search against Qdrant.
    pub async fn search_dense(
        &self,
        query: DenseSearchQuery,
    ) -> Result<Vec<ScoredPoint>, StorageError> {
        let url = format!(
            "{}/collections/{}/points/search",
            self.base_url, self.collection_name
        );

        // Qdrant keyword payloads have no wildcard predicate. Directory
        // scoping is applied to the returned payload instead, so widen the
        // candidate set and trim it back to the requested limit afterwards.
        let directory_prefix = query
            .filter
            .as_ref()
            .and_then(|filter| filter.directory_prefix.as_ref())
            .map(|prefix| {
                cce_types::normalize_project_path(prefix)
                    .trim_end_matches('/')
                    .to_string()
            });
        let candidate_limit = if directory_prefix.is_some() {
            query.limit.saturating_mul(8).min(1000)
        } else {
            query.limit
        };

        let filter = self.build_filter(query.filter.as_ref());

        let mut body = serde_json::json!({
            "vector": query.vector,
            "limit": candidate_limit,
            "with_payload": true
        });

        if let Some(filter) = filter {
            body["filter"] = filter;
        }

        // With Euclid, Qdrant treats score_threshold as a raw distance bound
        // (smaller is nearer), the opposite of the contract's similarity
        // lower bound. Push it down only for similarity metrics; for Euclid
        // we filter locally after normalizing the score below.
        let euclid_min_score = if self.distance_metric == DistanceMetric::Euclid {
            query.score_threshold
        } else {
            if let Some(score_threshold) = query.score_threshold {
                body["score_threshold"] = serde_json::json!(score_threshold);
            }
            None
        };

        if let Some(hnsw_ef) = query.hnsw_ef {
            body["params"] = serde_json::json!({
                "hnsw_ef": hnsw_ef
            });
        }

        tracing::trace!(
            collection = self.collection_name,
            limit = query.limit,
            "Executing dense vector search"
        );

        let mut hits = self.execute_search(&url, body).await?;
        if let Some(min_score) = euclid_min_score {
            hits.retain(|hit| hit.score >= min_score);
        }
        if let Some(prefix) = directory_prefix {
            hits.retain(|hit| {
                hit.payload.file_path == prefix
                    || hit.payload.file_path.starts_with(&format!("{prefix}/"))
            });
            hits.truncate(query.limit);
        }
        Ok(hits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_score_contract_direction() {
        // Similarity metrics already satisfy higher-is-better.
        assert_eq!(normalize_score(DistanceMetric::Cosine, 0.85), 0.85);
        assert_eq!(normalize_score(DistanceMetric::Dot, 2.5), 2.5);
        // Euclid distance is inverted to (0, 1]; smaller distance -> higher score.
        assert!((normalize_score(DistanceMetric::Euclid, 0.0) - 1.0).abs() < 1e-6);
        assert!((normalize_score(DistanceMetric::Euclid, 1.0) - 0.5).abs() < 1e-6);
        assert!(normalize_score(DistanceMetric::Euclid, 2.0) < 0.5);
        // Invalid distances collapse to zero, never NaN.
        assert_eq!(normalize_score(DistanceMetric::Euclid, -1.0), 0.0);
        assert_eq!(normalize_score(DistanceMetric::Euclid, f32::NAN), 0.0);
    }

    #[test]
    fn test_parse_search_response_normalizes_euclid() {
        let json = serde_json::json!({
            "result": [point_json("near", 0.0, Some(vec![1])), point_json("far", 3.0, None)]
        });
        let results = parse_search_response(json, DistanceMetric::Euclid);
        assert_eq!(results.len(), 2);
        assert!((results[0].score - 1.0).abs() < 1e-6);
        assert!((results[1].score - 0.25).abs() < 1e-6);
    }

    #[test]
    fn test_build_filter_with_epoch() {
        let client = QdrantRetrieval::new(
            Client::new(),
            "http://localhost:6334".to_string(),
            "test_collection".to_string(),
        );

        let filter = SearchFilter {
            epochs: vec![5],
            ..Default::default()
        };

        let result = client.build_filter(Some(&filter));
        assert!(result.is_some());

        let json = result.unwrap();
        assert_eq!(json["must"][0]["key"], "epoch");
        assert_eq!(json["must"][0]["match"]["value"], 5);
    }

    #[test]
    fn test_build_filter_with_inherited_epochs_is_or_combined() {
        let client = QdrantRetrieval::new(
            Client::new(),
            "http://localhost:6334".to_string(),
            "test_collection".to_string(),
        );

        let filter = SearchFilter {
            epochs: vec![4, 5],
            ..Default::default()
        };

        let json = client.build_filter(Some(&filter)).expect("filter built");
        let epoch_condition = &json["must"][0];
        let values: Vec<i64> = epoch_condition["min_should"]["conditions"]
            .as_array()
            .expect("min_should conditions")
            .iter()
            .map(|c| c["match"]["value"].as_i64().expect("epoch value"))
            .collect();
        assert_eq!(epoch_condition["min_should"]["min_count"], 1);
        assert_eq!(values, vec![4, 5]);
        assert!(
            json.get("must_not").is_none(),
            "no exclusion without overrides"
        );
    }

    #[test]
    fn test_build_filter_with_excluded_files_hides_parent_rows() {
        let client = QdrantRetrieval::new(
            Client::new(),
            "http://localhost:6334".to_string(),
            "test_collection".to_string(),
        );

        let filter = SearchFilter {
            epochs: vec![4, 5],
            excluded_files: Some(vec!["src/a.rs".to_string()]),
            ..Default::default()
        };

        let json = client.build_filter(Some(&filter)).expect("filter built");
        let must_not = json["must_not"].as_array().expect("must_not array");
        let exclusion = &must_not[0];
        assert_eq!(
            exclusion["must"][0]["key"], "epoch",
            "exclusion must be scoped to the parent epoch"
        );
        assert_eq!(exclusion["must"][0]["match"]["value"], 4);
        let paths: Vec<&str> = exclusion["min_should"]["conditions"]
            .as_array()
            .expect("min_should conditions")
            .iter()
            .map(|c| c["match"]["value"].as_str().expect("path"))
            .collect();
        assert_eq!(exclusion["min_should"]["min_count"], 1);
        assert_eq!(paths, vec!["src/a.rs"]);
    }

    #[test]
    fn test_build_filter_with_directory_prefix() {
        let client = QdrantRetrieval::new(
            Client::new(),
            "http://localhost:6334".to_string(),
            "test_collection".to_string(),
        );

        let filter = SearchFilter {
            directory_prefix: Some("/src/main".to_string()),
            ..Default::default()
        };

        // Directory prefixes are applied to returned payloads by the search
        // path: a keyword payload cannot serve a wildcard predicate on Qdrant.
        assert!(client.build_filter(Some(&filter)).is_none());
    }

    #[test]
    fn test_build_filter_without_conditions() {
        let client = QdrantRetrieval::new(
            Client::new(),
            "http://localhost:6334".to_string(),
            "test_collection".to_string(),
        );

        let filter = SearchFilter::default();

        let result = client.build_filter(Some(&filter));
        assert!(result.is_none());
    }

    #[test]
    fn test_build_filter_with_exclude_categories() {
        use cce_types::FileCategory;
        let client = QdrantRetrieval::new(
            Client::new(),
            "http://localhost:6334".to_string(),
            "test_collection".to_string(),
        );

        let filter = SearchFilter {
            exclude_categories: Some(vec![
                FileCategory::Config,
                FileCategory::Schema,
                FileCategory::Other,
            ]),
            ..Default::default()
        };

        let result = client.build_filter(Some(&filter));
        assert!(result.is_some());

        let json = result.unwrap();
        assert!(json.get("must_not").is_some());
    }

    #[test]
    fn test_build_filter_exclude_test_covers_flag_only() {
        let client = QdrantRetrieval::new(
            Client::new(),
            "http://localhost:6334".to_string(),
            "test_collection".to_string(),
        );

        let filter = SearchFilter {
            exclude_test: true,
            ..Default::default()
        };

        let json = client.build_filter(Some(&filter)).expect("filter built");
        let must_not = json["must_not"].as_array().expect("must_not array");
        let key = must_not[0]["key"].as_str().expect("key");
        assert_eq!(key, "test");
    }

    fn point_json(id: &str, score: f64, entity_ids: Option<Vec<i64>>) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "version": 0,
            "score": score,
            "payload": {
                "source_id": id,
                "file_path": "src/lib.rs",
                "entity_ids": entity_ids,
                "segment_id": "group_9"
            }
        })
    }

    #[test]
    fn test_parse_search_response_array_shape() {
        let json = serde_json::json!({
            "result": [
                point_json("group_1_emb_0", 0.85, Some(vec![1])),
                point_json("group_2_emb_0", 0.42, Some(vec![2])),
                point_json("group_3_emb_0", 0.31, None)
            ],
            "status": "ok",
            "time": 0.001
        });

        let results = parse_search_response(json, DistanceMetric::Cosine);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].id, "group_1_emb_0");
        assert_eq!(results[0].score, 0.85);
        assert_eq!(results[0].payload.segment_id.as_deref(), Some("group_9"));
        assert_eq!(results[2].payload.entity_ids, None);
    }

    #[test]
    fn test_parse_search_response_wrapped_points_shape() {
        let json = serde_json::json!({
            "result": {
                "points": [
                    point_json("group_1_emb_0", 0.9, Some(vec![7]))
                ]
            }
        });

        let results = parse_search_response(json, DistanceMetric::Cosine);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "group_1_emb_0");
        assert_eq!(results[0].payload.entity_ids, Some(vec![7]));
    }

    #[test]
    fn test_parse_search_response_empty_result() {
        let results =
            parse_search_response(serde_json::json!({ "result": [] }), DistanceMetric::Cosine);
        assert!(results.is_empty());
    }

    #[test]
    fn test_parse_search_response_missing_result() {
        let results = parse_search_response(
            serde_json::json!({ "error": "boom" }),
            DistanceMetric::Cosine,
        );
        assert!(results.is_empty());
    }

    #[test]
    fn test_parse_search_response_falls_back_to_point_id() {
        let json = serde_json::json!({
            "result": [{
                "id": "6f6a4a8a-6a2a-4a2a-9a2a-000000000000",
                "version": 0,
                "score": 0.5,
                "payload": { "file_path": "x.rs", "source_id": "" }
            }]
        });

        let results = parse_search_response(json, DistanceMetric::Cosine);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "6f6a4a8a-6a2a-4a2a-9a2a-000000000000");
    }

    #[test]
    fn test_parse_search_response_skips_malformed_point() {
        let json = serde_json::json!({
            "result": [
                point_json("group_1_emb_0", 0.7, Some(vec![1])),
                { "id": "broken", "version": 0, "score": 0.5 }
            ]
        });

        let results = parse_search_response(json, DistanceMetric::Cosine);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "group_1_emb_0");
    }
}
