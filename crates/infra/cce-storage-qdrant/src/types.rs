//! Data types for Qdrant vector storage
//!
//! This module defines data structures for vectors, payloads,
//! search results, and collection information.

pub use cce_storage_common::Payload;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Deterministic namespace UUID for generating point IDs from string IDs.
pub const POINT_ID_NAMESPACE: Uuid = Uuid::from_u128(0x6ba7b810_9dad_11d1_80b4_00c04fd430c8);

/// Convert a string-based point ID to a deterministic UUID v5.
pub fn to_qdrant_point_id(id: &str) -> Uuid {
    Uuid::new_v5(&POINT_ID_NAMESPACE, id.as_bytes())
}

/// Vector point with payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorPoint {
    /// Unique point ID
    pub id: String,
    /// Dense vector data
    pub vector: Vec<f32>,
    /// Payload metadata
    pub payload: Payload,
}

impl VectorPoint {
    /// Create a new vector point with dense vector only
    pub fn new(id: impl Into<String>, vector: Vec<f32>, payload: Payload) -> Self {
        Self {
            id: id.into(),
            vector,
            payload,
        }
    }

    /// Create a vector point with minimal payload
    pub fn with_file_path(
        id: impl Into<String>,
        vector: Vec<f32>,
        file_path: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            vector,
            payload: Payload::new(file_path),
        }
    }
}

/// Search query parameters
#[derive(Debug, Clone)]
pub struct SearchQuery {
    /// Query vector (dense)
    pub vector: Vec<f32>,
    /// Maximum number of results
    pub limit: usize,
    /// Minimum score threshold
    pub min_score: Option<f32>,
    /// Directory prefix filter
    pub directory_prefix: Option<String>,
    /// HNSW ef parameter for search
    pub hnsw_ef: Option<u32>,
}

impl SearchQuery {
    /// Create a new search query with dense vector only
    pub fn new(vector: Vec<f32>, limit: usize) -> Self {
        Self {
            vector,
            limit,
            min_score: None,
            directory_prefix: None,
            hnsw_ef: None,
        }
    }

    /// Set minimum score threshold
    pub fn with_min_score(mut self, score: f32) -> Self {
        self.min_score = Some(score);
        self
    }

    /// Set directory prefix filter
    pub fn with_directory_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.directory_prefix = Some(prefix.into());
        self
    }

    /// Set HNSW ef parameter
    pub fn with_hnsw_ef(mut self, ef: u32) -> Self {
        self.hnsw_ef = Some(ef);
        self
    }
}

/// Search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// Point ID
    pub id: String,
    /// Similarity score
    pub score: f32,
    /// Payload
    pub payload: Payload,
}

impl SearchResult {
    /// Create a new search result
    pub fn new(id: impl Into<String>, score: f32, payload: Payload) -> Self {
        Self {
            id: id.into(),
            score,
            payload,
        }
    }
}

/// Collection information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionInfo {
    /// Collection name
    pub name: String,
    /// Vector size
    pub vector_size: usize,
    /// Distance metric
    pub distance_metric: String,
    /// Total number of points
    pub points_count: u64,
    /// Number of indexed vectors
    pub indexed_vectors_count: u64,
    /// Number of segments
    pub segments_count: u64,
    /// Collection status
    pub status: CollectionStatus,
    /// HNSW config
    pub hnsw_config: Option<HnswConfigInfo>,
    /// Whether vectors are stored on disk
    pub vectors_on_disk: bool,
}

/// Collection status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CollectionStatus {
    /// Green - healthy
    Green,
    /// Yellow - optimization in progress
    Yellow,
    /// Red - error
    Red,
    /// Grey - initializing
    Grey,
}

/// HNSW configuration info from collection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HnswConfigInfo {
    /// M parameter
    pub m: u32,
    /// Ef construct parameter
    pub ef_construct: u32,
    /// Whether index is on disk
    pub on_disk: bool,
    /// Payload M parameter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload_m: Option<u32>,
}

/// Size estimation result
#[derive(Debug, Clone)]
pub struct SizeEstimation {
    /// Estimated vector count
    pub estimated_vector_count: usize,
    /// File count used for estimation
    pub file_count: usize,
    /// Average vectors per file
    pub avg_vectors_per_file: f32,
}

impl SizeEstimation {
    /// Estimate from file count
    pub fn from_file_count(file_count: usize, avg_vectors_per_file: f32) -> Self {
        Self {
            estimated_vector_count: (file_count as f32 * avg_vectors_per_file) as usize,
            file_count,
            avg_vectors_per_file,
        }
    }

    /// Get recommended preset for this size
    pub fn recommended_preset(&self) -> crate::config::CollectionPreset {
        crate::config::CollectionPreset::from_vector_count(self.estimated_vector_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vector_point() {
        let point = VectorPoint::with_file_path("point-1", vec![0.1, 0.2, 0.3], "src/main.rs");
        assert_eq!(point.id, "point-1");
        assert_eq!(point.vector.len(), 3);
        assert_eq!(point.payload.file_path, "src/main.rs");
    }

    #[test]
    fn test_search_query() {
        let query = SearchQuery::new(vec![0.1, 0.2], 10)
            .with_min_score(0.5)
            .with_directory_prefix("src/lib");

        assert_eq!(query.limit, 10);
        assert_eq!(query.min_score, Some(0.5));
        assert_eq!(query.directory_prefix, Some("src/lib".to_string()));
    }

    #[test]
    fn test_size_estimation() {
        let estimation = SizeEstimation::from_file_count(100, 10.0);
        assert_eq!(estimation.estimated_vector_count, 1000);
        assert_eq!(estimation.file_count, 100);
    }
}
