//! Local vector store backed by simvec.

use std::collections::HashMap;
use std::sync::Arc;

use cce_config::modules::{DistanceMetric as CceDistance, LocalVectorConfig};
use cce_storage_common::{
    DenseSearchQuery, Payload, ScoredPoint, SearchFilter, VectorPoint, VectorStorage,
    payload_matches_filter, vector_collection_name,
};
use cce_types::{PointKind, StorageError};
use simvec::{
    CollectionConfig, DistanceMetric as SimDistance, FilterCondition, HnswConfig as SimHnsw,
    LocalVectorEngine, SearchQuery as SimQuery, VectorFilter as SimFilter, VectorPoint as SimPoint,
    error::VectorSearchError,
};

/// Embedded local vector store (single collection, group-isolated).
///
/// Resilience note: unlike the Qdrant branch this store performs in-process
/// calls with no network hop, so it carries no circuit breaker, retry loop,
/// or remote metrics. Validation failures (dimension, non-finite elements)
/// fail fast; I/O errors surface directly through `StorageError`.
#[derive(Clone)]
pub struct LocalVectorStore {
    engine: Arc<LocalVectorEngine>,
    collection: String,
    vector_size: usize,
}

impl std::fmt::Debug for LocalVectorStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalVectorStore")
            .field("collection", &self.collection)
            .field("vector_size", &self.vector_size)
            .finish()
    }
}

impl LocalVectorStore {
    /// Open (or create) the engine rooted at the resolved data dir and ensure
    /// the shared collection exists with the configured dimension and metric.
    pub fn open(config: &LocalVectorConfig, sqlite_path: &str) -> Result<Self, StorageError> {
        let data_dir = config.resolved_data_dir(sqlite_path);
        let engine = LocalVectorEngine::open(&data_dir).map_err(map_simvec_error)?;
        let collection = vector_collection_name();
        let distance = map_distance(config.distance_metric);
        if !engine.collection_exists(&collection) {
            let mut collection_config = CollectionConfig::new(config.vector_size, distance);
            if let Some(hnsw) = build_hnsw_config(config) {
                collection_config = collection_config.with_hnsw(hnsw);
            }
            engine
                .create_collection(&collection, &collection_config)
                .map_err(map_simvec_error)?;
        } else {
            let existing = engine
                .collection_config(&collection)
                .map_err(map_simvec_error)?
                .ok_or_else(|| StorageError::not_found(format!("collection {collection}")))?;
            if existing.vector_size != config.vector_size {
                return Err(StorageError::validation(format!(
                    "local vector dimension mismatch: collection has {}, config wants {} (rebuild required)",
                    existing.vector_size, config.vector_size
                )));
            }
        }
        Ok(Self {
            engine: Arc::new(engine),
            collection,
            vector_size: config.vector_size,
        })
    }

    /// Open with an explicit data dir (tests and tooling).
    pub fn open_at(
        data_dir: impl AsRef<std::path::Path>,
        config: &LocalVectorConfig,
    ) -> Result<Self, StorageError> {
        let engine = LocalVectorEngine::open(data_dir.as_ref()).map_err(map_simvec_error)?;
        let collection = vector_collection_name();
        let distance = map_distance(config.distance_metric);
        if !engine.collection_exists(&collection) {
            let mut collection_config = CollectionConfig::new(config.vector_size, distance);
            if let Some(hnsw) = build_hnsw_config(config) {
                collection_config = collection_config.with_hnsw(hnsw);
            }
            engine
                .create_collection(&collection, &collection_config)
                .map_err(map_simvec_error)?;
        } else {
            let existing = engine
                .collection_config(&collection)
                .map_err(map_simvec_error)?
                .ok_or_else(|| StorageError::not_found(format!("collection {collection}")))?;
            if existing.vector_size != config.vector_size {
                return Err(StorageError::validation(format!(
                    "local vector dimension mismatch: collection has {}, config wants {} (rebuild required)",
                    existing.vector_size, config.vector_size
                )));
            }
        }
        Ok(Self {
            engine: Arc::new(engine),
            collection,
            vector_size: config.vector_size,
        })
    }

    /// Underlying engine root (diagnostics and tests).
    pub fn data_dir(&self) -> std::path::PathBuf {
        self.engine.root_dir().to_path_buf()
    }

    /// Collection name served by this store.
    pub fn collection_name(&self) -> &str {
        &self.collection
    }

    /// Fetch window for a filtered search: the requested limit plus a
    /// heuristic margin for post-filtered rows plus one slot per excluded
    /// file, bounded by a ceiling that grows with the exclusion load so a
    /// large override set cannot truncate the visible window.
    fn overfetch_limit(limit: usize, excluded_len: usize) -> usize {
        (limit * 5 + excluded_len + 20)
            .min(10_000 + excluded_len)
            .max(limit)
    }

    fn simvec_filter(filter: &SearchFilter) -> Option<SimFilter> {
        // `raw_filter` is a Qdrant-only escape hatch with no local meaning.
        // Queries that set it are rejected in `search_dense` before reaching
        // here; the debug log below is a second line of defense for any
        // future caller that bypasses that check.
        if filter.raw_filter.is_some() {
            tracing::debug!("local vector backend ignores raw_filter");
        }
        let mut sim = SimFilter::new();
        let mut has = false;
        if let Some(ref group_id) = filter.group_id {
            sim = sim.must(FilterCondition::match_value("group_id", group_id.as_str()));
            has = true;
        }
        if let Some(ref file_path) = filter.file_path {
            let normalized = cce_types::normalize_project_path(file_path);
            sim = sim.must(FilterCondition::match_value("file_path", normalized));
            has = true;
        }
        if let Some(point_type) = filter.point_type {
            sim = sim.must(FilterCondition::match_value(
                "type",
                point_type.as_u8().to_string(),
            ));
            has = true;
        }
        if !filter.epochs.is_empty() {
            if filter.epochs.len() == 1 {
                sim = sim.must(FilterCondition::match_value(
                    "epoch",
                    filter.epochs[0].to_string(),
                ));
            } else {
                for epoch in &filter.epochs {
                    sim = sim.should(FilterCondition::match_value("epoch", epoch.to_string()));
                }
            }
            has = true;
        }
        if let Some(ref categories) = filter.include_categories
            && !categories.is_empty()
        {
            for cat in categories {
                sim = sim.should(FilterCondition::match_value(
                    "category",
                    cat.as_u8().to_string(),
                ));
            }
            has = true;
        }
        if let Some(ref categories) = filter.exclude_categories {
            for cat in categories {
                sim = sim.must_not(FilterCondition::match_value(
                    "category",
                    cat.as_u8().to_string(),
                ));
                has = true;
            }
        }
        if filter.exclude_test {
            sim = sim.must_not(FilterCondition::match_value("test", "true"));
            has = true;
        }
        if has { Some(sim) } else { None }
    }

    fn delete_with_filter(&self, filter: &SimFilter) -> Result<(), StorageError> {
        self.engine
            .delete_by_filter(&self.collection, filter)
            .map_err(map_simvec_error)?;
        Ok(())
    }
}

/// Map cce distance metric to simvec metric.
fn map_distance(metric: CceDistance) -> SimDistance {
    match metric {
        CceDistance::Cosine => SimDistance::Cosine,
        CceDistance::Euclid => SimDistance::Euclid,
        CceDistance::Dot => SimDistance::Dot,
    }
}

fn build_hnsw_config(config: &LocalVectorConfig) -> Option<SimHnsw> {
    let m = config.hnsw_m.map(|v| v as usize);
    let ef_construct = config.hnsw_ef_construct.map(|v| v as usize);
    let ef_search = config.hnsw_ef_search;
    let threshold = config.full_scan_threshold;
    if m.is_none() && ef_construct.is_none() && ef_search.is_none() && threshold.is_none() {
        return None;
    }
    let mut hnsw = SimHnsw::default();
    if let Some(m) = m {
        hnsw.m = m;
    }
    if let Some(ef) = ef_construct {
        hnsw.ef_construct = ef;
    }
    if let Some(ef) = ef_search {
        hnsw.ef_search = ef;
    }
    if let Some(threshold) = threshold {
        hnsw.full_scan_threshold = Some(threshold);
    }
    if let Err(e) = hnsw.validate() {
        tracing::warn!(error = %e, "invalid local hnsw config, using engine default");
        return None;
    }
    Some(hnsw)
}

/// Convert shared payload to simvec payload map.
pub fn payload_to_map(payload: &Payload) -> HashMap<String, serde_json::Value> {
    match serde_json::to_value(payload) {
        Ok(serde_json::Value::Object(map)) => map.into_iter().collect(),
        _ => HashMap::new(),
    }
}

fn map_to_payload(map: &HashMap<String, serde_json::Value>) -> Result<Payload, StorageError> {
    let value = serde_json::Value::Object(map.clone().into_iter().collect());
    serde_json::from_value(value)
        .map_err(|e| StorageError::query(format!("invalid local payload: {e}")))
}

fn to_simvec_point(point: &VectorPoint) -> Result<SimPoint, StorageError> {
    if point.vector.iter().any(|v| !v.is_finite()) {
        return Err(StorageError::validation("non-finite vector element"));
    }
    let mut sim = SimPoint::new(point.id.clone(), point.vector.clone());
    sim.payload = Some(payload_to_map(&point.payload));
    Ok(sim)
}

fn from_simvec_point(
    id: String,
    vector: Vec<f32>,
    payload_map: Option<HashMap<String, serde_json::Value>>,
) -> Result<VectorPoint, StorageError> {
    let map = payload_map.unwrap_or_default();
    let payload = map_to_payload(&map)?;
    Ok(VectorPoint {
        id,
        vector,
        payload,
    })
}

/// Map simvec errors to storage errors.
pub fn map_simvec_error(err: VectorSearchError) -> StorageError {
    match err {
        VectorSearchError::CollectionNotFound(name) => StorageError::not_found(name),
        VectorSearchError::InvalidVectorDimension { expected, actual } => StorageError::validation(
            format!("invalid vector dimension: expected {expected}, got {actual}"),
        ),
        VectorSearchError::InvalidConfig(msg) | VectorSearchError::InvalidPointId(msg) => {
            StorageError::validation(msg)
        }
        VectorSearchError::NonFiniteElement(index) => {
            StorageError::validation(format!("non-finite vector element at index {index}"))
        }
        VectorSearchError::Io(e) => {
            StorageError::from(std::io::Error::new(e.kind(), e.to_string()))
        }
        VectorSearchError::Json(e) => StorageError::query(format!("local payload error: {e}")),
        VectorSearchError::Serialization(e) => {
            StorageError::query(format!("local storage error: {e}"))
        }
        other => StorageError::query(other.to_string()),
    }
}

impl VectorStorage for LocalVectorStore {
    fn backend_name(&self) -> &'static str {
        "local"
    }

    async fn ensure_collection(&self) -> Result<bool, StorageError> {
        if self.engine.collection_exists(&self.collection) {
            return Ok(false);
        }
        // Open-time creation already ensures the collection; reaching here
        // means it was deleted out-of-band, so recreate with stored size.
        let config = CollectionConfig::new(self.vector_size, SimDistance::Cosine);
        self.engine
            .create_collection(&self.collection, &config)
            .map_err(map_simvec_error)?;
        Ok(true)
    }

    async fn collection_exists(&self) -> Result<bool, StorageError> {
        Ok(self.engine.collection_exists(&self.collection))
    }

    async fn delete_collection(&self) -> Result<(), StorageError> {
        self.engine
            .delete_collection(&self.collection)
            .map_err(map_simvec_error)
    }

    async fn clear_collection(&self) -> Result<(), StorageError> {
        // Physical clear is delete plus recreate so dimension and metric stay.
        let config = self
            .engine
            .collection_config(&self.collection)
            .map_err(map_simvec_error)?
            .ok_or_else(|| StorageError::not_found(format!("collection {}", self.collection)))?;
        self.engine
            .delete_collection(&self.collection)
            .map_err(map_simvec_error)?;
        self.engine
            .create_collection(&self.collection, &config)
            .map_err(map_simvec_error)?;
        Ok(())
    }

    async fn upsert_points(&self, points: &[VectorPoint]) -> Result<(), StorageError> {
        if points.is_empty() {
            return Ok(());
        }
        for point in points {
            if point.vector.len() != self.vector_size {
                return Err(StorageError::validation(format!(
                    "invalid vector dimension: expected {}, got {}",
                    self.vector_size,
                    point.vector.len()
                )));
            }
        }
        let engine = Arc::clone(&self.engine);
        let collection = self.collection.clone();
        let sim_points: Vec<SimPoint> = points
            .iter()
            .map(to_simvec_point)
            .collect::<Result<Vec<_>, _>>()?;
        tokio::task::spawn_blocking(move || {
            engine
                .upsert_batch(&collection, &sim_points)
                .map_err(map_simvec_error)
        })
        .await
        .map_err(|e| StorageError::query(format!("local upsert join failed: {e}")))?
    }

    async fn search_dense(
        &self,
        query: DenseSearchQuery,
    ) -> Result<Vec<ScoredPoint>, StorageError> {
        if query.vector.iter().any(|v| !v.is_finite()) {
            return Err(StorageError::validation("non-finite query vector"));
        }
        if query.vector.len() != self.vector_size {
            return Err(StorageError::validation(format!(
                "invalid query dimension: expected {}, got {}",
                self.vector_size,
                query.vector.len()
            )));
        }
        if query
            .filter
            .as_ref()
            .is_some_and(|filter| filter.raw_filter.is_some())
        {
            return Err(StorageError::validation(
                "raw_filter is Qdrant-only and not supported by the local vector backend",
            ));
        }
        let limit = query.limit.max(1);
        let excluded_len = query
            .filter
            .as_ref()
            .and_then(|f| f.excluded_files.as_ref().map(|v| v.len()))
            .unwrap_or(0);
        // Over-fetch so generation exclusion and directory post-filtering can
        // still fill the requested limit. The ceiling grows with the
        // exclusion load: excluded parent rows can occupy at most
        // `excluded_len` fetch slots, so a fixed ceiling would truncate the
        // visible window when the override set is large.
        let fetch_limit = Self::overfetch_limit(limit, excluded_len);
        let mut sim_query = SimQuery::new(query.vector.clone(), fetch_limit);
        if let Some(threshold) = query.score_threshold {
            sim_query = sim_query.with_score_threshold(threshold);
        }
        if let Some(ef) = query.hnsw_ef {
            sim_query = sim_query.with_knn(fetch_limit, Some(ef as usize));
        }
        if let Some(ref filter) = query.filter {
            if let Some(sim_filter) = Self::simvec_filter(filter) {
                sim_query = sim_query.with_filter(sim_filter);
            }
        }
        let engine = Arc::clone(&self.engine);
        let collection = self.collection.clone();
        let results = tokio::task::spawn_blocking(move || {
            engine
                .search(&collection, &sim_query)
                .map_err(map_simvec_error)
        })
        .await
        .map_err(|e| StorageError::query(format!("local search join failed: {e}")))??;
        let mut scored: Vec<ScoredPoint> = Vec::new();
        for item in results {
            let payload_map = item.payload.clone().unwrap_or_default();
            let payload = match map_to_payload(&payload_map) {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!(error = %e, id = %item.id, "skipping local hit with invalid payload");
                    continue;
                }
            };
            if let Some(ref filter) = query.filter {
                if !payload_matches_filter(&payload, filter) {
                    continue;
                }
            }
            if let Some(threshold) = query.score_threshold {
                if item.score < threshold {
                    continue;
                }
            }
            let id = if !payload.source_id.is_empty() {
                payload.source_id.clone()
            } else {
                item.id.to_string()
            };
            scored.push(ScoredPoint {
                id,
                score: item.score,
                payload,
            });
        }
        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(limit);
        Ok(scored)
    }

    async fn delete_by_file_path_scoped(
        &self,
        file_path: &str,
        group_id: &str,
        point_type: Option<PointKind>,
    ) -> Result<(), StorageError> {
        let normalized = cce_types::normalize_project_path(file_path);
        let mut filter = SimFilter::new()
            .must(FilterCondition::match_value("group_id", group_id))
            .must(FilterCondition::match_value("file_path", normalized));
        if let Some(kind) = point_type {
            filter = filter.must(FilterCondition::match_value(
                "type",
                kind.as_u8().to_string(),
            ));
        }
        let engine = Arc::clone(&self.engine);
        let collection = self.collection.clone();
        tokio::task::spawn_blocking(move || {
            engine
                .delete_by_filter(&collection, &filter)
                .map(|_| ())
                .map_err(map_simvec_error)
        })
        .await
        .map_err(|e| StorageError::query(format!("local delete join failed: {e}")))?
    }

    async fn delete_by_file_path_scoped_epoch(
        &self,
        file_path: &str,
        group_id: &str,
        epoch: i64,
    ) -> Result<(), StorageError> {
        let normalized = cce_types::normalize_project_path(file_path);
        let filter = SimFilter::new()
            .must(FilterCondition::match_value("group_id", group_id))
            .must(FilterCondition::match_value("file_path", normalized))
            .must(FilterCondition::match_value("epoch", epoch.to_string()));
        self.delete_with_filter(&filter)
    }

    async fn delete_by_group_epoch(&self, group_id: &str, epoch: i64) -> Result<(), StorageError> {
        let filter = SimFilter::new()
            .must(FilterCondition::match_value("group_id", group_id))
            .must(FilterCondition::match_value("epoch", epoch.to_string()));
        self.delete_with_filter(&filter)
    }

    async fn delete_by_group(&self, group_id: &str) -> Result<(), StorageError> {
        let filter = SimFilter::new().must(FilterCondition::match_value("group_id", group_id));
        self.delete_with_filter(&filter)
    }

    async fn scroll_all_points(&self) -> Result<Vec<VectorPoint>, StorageError> {
        const PAGE: usize = 5000;
        let mut out = Vec::new();
        let mut offset: Option<String> = None;
        loop {
            let engine = Arc::clone(&self.engine);
            let collection = self.collection.clone();
            let current = offset.clone();
            let (points, next) = tokio::task::spawn_blocking(move || {
                engine
                    .scroll(
                        &collection,
                        PAGE,
                        current.as_deref(),
                        Some(true),
                        Some(true),
                    )
                    .map_err(map_simvec_error)
            })
            .await
            .map_err(|e| StorageError::query(format!("local scroll join failed: {e}")))??;
            for sim in points {
                let id = sim.id.to_string();
                let vector = sim.vector.clone();
                match from_simvec_point(id, vector, sim.payload.clone()) {
                    Ok(point) => out.push(point),
                    Err(e) => {
                        tracing::warn!(error = %e, "skipping local point with invalid payload");
                    }
                }
            }
            match next {
                Some(next_offset) => offset = Some(next_offset),
                None => break,
            }
        }
        Ok(out)
    }

    async fn count_points_by_group(&self, group_id: &str) -> Result<usize, StorageError> {
        // Stream pages and count from payloads only. Vectors are never
        // fetched, so peak memory stays at one page instead of O(N*d).
        const PAGE: usize = 5000;
        let mut count = 0;
        let mut offset: Option<String> = None;
        loop {
            let engine = Arc::clone(&self.engine);
            let collection = self.collection.clone();
            let current = offset.clone();
            let (points, next) = tokio::task::spawn_blocking(move || {
                engine
                    .scroll(
                        &collection,
                        PAGE,
                        current.as_deref(),
                        Some(true),
                        Some(false),
                    )
                    .map_err(map_simvec_error)
            })
            .await
            .map_err(|e| StorageError::query(format!("local count join failed: {e}")))??;
            for sim in points {
                let Some(map) = sim.payload else {
                    continue;
                };
                match map_to_payload(&map) {
                    Ok(payload) => {
                        if payload.group_id.as_deref() == Some(group_id) {
                            count += 1;
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "skipping local point with invalid payload");
                    }
                }
            }
            match next {
                Some(next_offset) => offset = Some(next_offset),
                None => break,
            }
        }
        Ok(count)
    }

    async fn count_all_points(&self) -> Result<usize, StorageError> {
        self.engine
            .count(&self.collection)
            .map(|v| v as usize)
            .map_err(map_simvec_error)
    }

    async fn health(&self) -> Result<bool, StorageError> {
        Ok(self.engine.collection_exists(&self.collection))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_config::modules::LocalVectorConfig;
    use cce_storage_common::Payload;
    use cce_types::{FileCategory, PointKind};

    fn test_config(dim: usize) -> LocalVectorConfig {
        LocalVectorConfig {
            data_dir: None,
            vector_size: dim,
            distance_metric: CceDistance::Cosine,
            hnsw_m: None,
            hnsw_ef_construct: None,
            hnsw_ef_search: None,
            full_scan_threshold: None,
        }
    }

    fn point(id: &str, vector: Vec<f32>, file: &str, group: &str, epoch: i64) -> VectorPoint {
        VectorPoint::new(
            id.to_string(),
            vector,
            Payload::new(file)
                .with_source_id(id.to_string())
                .with_group_id(group)
                .with_type(PointKind::Chunk)
                .with_category(FileCategory::Code)
                .with_epoch(epoch)
                .with_test(false),
        )
    }

    #[tokio::test]
    async fn upsert_search_and_delete_roundtrip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = LocalVectorStore::open_at(dir.path(), &test_config(3)).expect("open store");
        store.ensure_collection().await.expect("ensure collection");

        let points = vec![
            point("p1", vec![1.0, 0.0, 0.0], "src/a.rs", "g1", 1),
            point("p2", vec![0.0, 1.0, 0.0], "src/b.rs", "g1", 1),
        ];
        store.upsert_points(&points).await.expect("upsert");

        let filter = SearchFilter {
            group_id: Some("g1".to_string()),
            epochs: vec![1],
            ..Default::default()
        };
        let results = store
            .search_dense(DenseSearchQuery::new(vec![1.0, 0.0, 0.0], 10).with_filter(filter))
            .await
            .expect("search");
        assert!(!results.is_empty());
        assert_eq!(results[0].id, "p1");

        store
            .delete_by_file_path_scoped("src/a.rs", "g1", None)
            .await
            .expect("delete file");
        assert_eq!(store.count_points_by_group("g1").await.expect("count"), 1);

        store.delete_by_group("g1").await.expect("delete group");
        assert_eq!(store.count_all_points().await.expect("count all"), 0);
    }

    #[tokio::test]
    async fn search_respects_generation_exclusion() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = LocalVectorStore::open_at(dir.path(), &test_config(2)).expect("open store");
        let points = vec![
            point("parent", vec![1.0, 0.0], "src/a.rs", "g1", 4),
            point("own", vec![1.0, 0.1], "src/a.rs", "g1", 5),
        ];
        store.upsert_points(&points).await.expect("upsert");
        let filter = SearchFilter {
            epochs: vec![4, 5],
            excluded_files: Some(vec!["src/a.rs".to_string()]),
            group_id: Some("g1".to_string()),
            ..Default::default()
        };
        let results = store
            .search_dense(DenseSearchQuery::new(vec![1.0, 0.0], 10).with_filter(filter))
            .await
            .expect("search");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "own");
    }

    #[tokio::test]
    async fn rejects_dimension_mismatch() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = LocalVectorStore::open_at(dir.path(), &test_config(2)).expect("open store");
        let err = store
            .upsert_points(&[point("bad", vec![1.0], "src/a.rs", "g1", 1)])
            .await
            .expect_err("dimension mismatch must fail");
        assert!(err.to_string().contains("dimension"));
    }

    #[test]
    fn overfetch_ceiling_scales_with_exclusions() {
        // Small loads keep the historical window.
        assert_eq!(LocalVectorStore::overfetch_limit(10, 0), 70);
        // The fixed ceiling still applies without exclusions.
        assert_eq!(LocalVectorStore::overfetch_limit(5000, 0), 10_000);
        // A large override set lifts the ceiling one-for-one so excluded
        // parent rows cannot truncate the visible window.
        assert_eq!(LocalVectorStore::overfetch_limit(10, 50_000), 50_070);
        // The requested limit is always a floor.
        assert_eq!(LocalVectorStore::overfetch_limit(20_000, 0), 20_000);
    }
}
