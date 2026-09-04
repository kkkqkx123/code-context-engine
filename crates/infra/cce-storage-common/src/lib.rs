//! Vector retrieval query types (Qdrant-specific type layer)
//!
//! This module defines the query and result types used for dense vector
//! retrieval. Per the storage-module refactor , the multi-backend trait
//! abstraction was abandoned: Qdrant is the only vector backend, so this
//! module is deliberately the **Qdrant retrieval type layer** — it holds
//! `Payload`, `ScoredPoint`, `DenseSearchQuery` and `SearchFilter`, all of
//! which are Qdrant-shaped. No other storage backend is coupled to it.
//!
//! # Architecture
//!
//! ```text
//! Application Layer
//!     │
//!     └── DenseSearchQuery / SearchFilter / ScoredPoint / Payload
//!             │
//! Storage Layer
//!     └── Qdrant implementation (QdrantRetrieval::search_dense)
//! ```

use serde::{Deserialize, Serialize};

use cce_types::{FileCategory, PointKind, TestSource, normalize_project_path};

pub mod sqlite_store {
    //! Trait abstracting SQLite persistence operations needed by the metrics aggregator.

    use chrono::{DateTime, Utc};
    use serde::{Deserialize, Serialize};

    /// Trait abstracting SQLite persistence operations needed by the aggregator.
    pub trait SqliteStore: Send + Sync + 'static {
        type Error: std::error::Error + Send + Sync + 'static;

        fn execute_write(
            &self,
            sql: &str,
            params: &[&dyn rusqlite::ToSql],
        ) -> Result<usize, Self::Error>;
        fn query_rows(
            &self,
            sql: &str,
            params: &[&dyn rusqlite::ToSql],
            f: &mut dyn FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<AggregatedMetric>,
        ) -> Result<Vec<AggregatedMetric>, Self::Error>;

        /// Insert a batch of rows, one parameter set per row.
        ///
        /// Implementations should wrap the batch in a single transaction when
        /// possible. The default implementation falls back to repeated
        /// `execute_write` calls so in-memory fakes keep working.
        fn execute_write_batch(
            &self,
            sql: &str,
            batch: &[Vec<Box<dyn rusqlite::ToSql>>],
        ) -> Result<usize, Self::Error> {
            let mut inserted = 0;
            for params in batch {
                let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
                if self.execute_write(sql, &refs).is_ok() {
                    inserted += 1;
                }
            }
            Ok(inserted)
        }
    }

    /// Aggregated metric record for storage
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct AggregatedMetric {
        pub timestamp: DateTime<Utc>,
        pub metric_name: String,
        pub metric_type: String,
        pub labels_json: Option<String>,
        pub count: i64,
        pub avg: Option<f64>,
        pub median: Option<f64>,
        pub max: Option<f64>,
        pub p90: Option<f64>,
        pub p99: Option<f64>,
        pub project_id: Option<i64>,
        pub operation_type: Option<String>,
    }
}

pub use sqlite_store::{AggregatedMetric, SqliteStore};

/// Search filter options for vector retrieval.
///
/// Provides a backend-agnostic filter that each implementation
/// translates into its native query language (e.g., Qdrant filter JSON).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SearchFilter {
    /// Visible data generations, ascending (`[parent, own]` under
    /// inheritance; a single element for full generations). Empty disables
    /// epoch filtering entirely.
    pub epochs: Vec<i64>,
    /// Files whose parent-generation rows are hidden (replaced or deleted by
    /// the own generation). Combined with `epochs`, this excludes exactly the
    /// "parent rows of overridden files" from the visible view.
    pub excluded_files: Option<Vec<String>>,
    /// Project or tenant group identifier
    pub group_id: Option<String>,
    /// Point type filter (chunk or summary)
    pub point_type: Option<PointKind>,
    /// Directory prefix filter
    pub directory_prefix: Option<String>,
    /// Exclude test files
    pub exclude_test: bool,
    /// Include only specific categories
    pub include_categories: Option<Vec<FileCategory>>,
    /// Exclude specific categories
    pub exclude_categories: Option<Vec<FileCategory>>,
    /// Pre-built raw filter JSON (takes precedence over other fields when set)
    pub raw_filter: Option<serde_json::Value>,
}

/// Dense vector search query
#[derive(Debug, Clone)]
pub struct DenseSearchQuery {
    /// Dense embedding vector
    pub vector: Vec<f32>,
    /// Maximum number of results to return
    pub limit: usize,
    /// Optional score threshold for filtering results
    pub score_threshold: Option<f32>,
    /// HNSW ef parameter (backend-specific, affects search accuracy/speed trade-off)
    pub hnsw_ef: Option<u64>,
    /// Optional filter conditions
    pub filter: Option<SearchFilter>,
}

impl DenseSearchQuery {
    /// Create a new dense search query
    pub fn new(vector: Vec<f32>, limit: usize) -> Self {
        Self {
            vector,
            limit,
            score_threshold: None,
            hnsw_ef: None,
            filter: None,
        }
    }

    /// Set the minimum score threshold
    pub fn with_score_threshold(mut self, threshold: f32) -> Self {
        self.score_threshold = Some(threshold);
        self
    }

    /// Set the HNSW ef parameter
    pub fn with_hnsw_ef(mut self, ef: u64) -> Self {
        self.hnsw_ef = Some(ef);
        self
    }

    /// Set filter options
    pub fn with_filter(mut self, filter: SearchFilter) -> Self {
        self.filter = Some(filter);
        self
    }
}

/// Scored point result from vector search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoredPoint {
    /// Point ID
    pub id: String,
    /// Similarity score (higher is better)
    pub score: f32,
    /// Associated payload data
    pub payload: Payload,
}

/// Payload metadata for a vector point
///
/// Minimal payload design: only essential fields for filtering.
/// All other metadata is stored in SQLite and fetched on-demand.
///
/// Defined here (the vector retrieval type layer) rather than in the Qdrant
/// module so that retrieval types carry no dependency on the backend crate;
/// the Qdrant write path re-uses the same type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Payload {
    /// The original application-level point ID (e.g. chunk_id like `group_9_emb_0`)
    /// Stored alongside the Qdrant UUID point ID so that search results can
    /// be mapped back to the original chunk/entity.
    pub source_id: String,
    /// File path (normalized with forward slashes) - used for filtering
    pub file_path: String,
    /// Project or tenant group identifier used for logical isolation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
    /// Point type used for single-collection separation (chunk or summary)
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub r#type: Option<PointKind>,
    /// File category for category-aware retrieval (code, config, documentation,
    /// schema, other)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<FileCategory>,
    /// Test-code marker derived from TestInfo. New writes always populate the
    /// field; it stays an `Option` purely as read-side defense so a
    /// partially-written payload cannot fail deserialization.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub test: Option<bool>,
    /// Source of the test determination (ast/path/none) stored as u8 encoding.
    /// Always populated on new writes; `Option` is read-side defense only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub test_source: Option<TestSource>,
    /// Epoch/version for version-aware filtering
    #[serde(skip_serializing_if = "Option::is_none")]
    pub epoch: Option<i64>,
    /// Batch ID for per-epoch version tracking
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch_id: Option<i64>,
    /// All entity IDs associated with this chunk. Enables entity-level expansion
    /// of multi-entity chunks on the vector path (mirrors the BM25 index which
    /// stores the full comma-separated list). Empty for document/plain-text chunks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_ids: Option<Vec<i64>>,
    /// Segment ID for hybrid fusion alignment. Always populated.
    /// For code chunks: same as source_group_id.
    /// For document chunks: source_group_id identifying the logical section.
    /// Enables BM25 ↔ vector matching for non-entity chunks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub segment_id: Option<String>,
}

impl Payload {
    /// Create a new payload
    pub fn new(file_path: impl Into<String>) -> Self {
        Self {
            source_id: String::new(),
            file_path: normalize_project_path(&file_path.into()),
            group_id: None,
            r#type: None,
            category: None,
            test: None,
            test_source: None,
            epoch: None,
            batch_id: None,
            entity_ids: None,
            segment_id: None,
        }
    }

    /// Set the source ID (original application-level point ID)
    pub fn with_source_id(mut self, source_id: impl Into<String>) -> Self {
        self.source_id = source_id.into();
        self
    }

    /// Set the group ID
    pub fn with_group_id(mut self, group_id: impl Into<String>) -> Self {
        self.group_id = Some(group_id.into());
        self
    }

    /// Set the point type
    pub fn with_type(mut self, point_type: PointKind) -> Self {
        self.r#type = Some(point_type);
        self
    }

    /// Set the file category
    pub fn with_category(mut self, category: FileCategory) -> Self {
        self.category = Some(category);
        self
    }

    /// Set the test-code marker
    pub fn with_test(mut self, test: bool) -> Self {
        self.test = Some(test);
        self
    }

    /// Set the test determination source
    pub fn with_test_source(mut self, test_source: TestSource) -> Self {
        self.test_source = Some(test_source);
        self
    }

    /// Set the epoch
    pub fn with_epoch(mut self, epoch: i64) -> Self {
        self.epoch = Some(epoch);
        self
    }

    /// Set the batch ID
    pub fn with_batch_id(mut self, batch_id: i64) -> Self {
        self.batch_id = Some(batch_id);
        self
    }

    /// Set all entity IDs associated with this chunk
    pub fn with_entity_ids(mut self, entity_ids: Vec<i64>) -> Self {
        if entity_ids.is_empty() {
            self.entity_ids = None;
        } else {
            self.entity_ids = Some(entity_ids);
        }
        self
    }

    /// Set the segment ID for hybrid fusion alignment
    pub fn with_segment_id(mut self, segment_id: impl Into<String>) -> Self {
        self.segment_id = Some(segment_id.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::PointKind;

    #[test]
    fn test_payload_creation() {
        let payload = Payload::new("src/lib.rs");
        assert_eq!(payload.file_path, "src/lib.rs");
    }

    #[test]
    fn test_payload_path_normalization() {
        let payload = Payload::new("src\\lib\\test.rs");
        assert_eq!(payload.file_path, "src/lib/test.rs");
    }

    #[test]
    fn test_payload_validation() {
        // Test empty file path
        let invalid = Payload::new("");
        assert_eq!(invalid.file_path, "");

        // Test valid payload
        let valid = Payload::new("test.rs").with_type(PointKind::Chunk);
        assert_eq!(valid.file_path, "test.rs");
        assert_eq!(valid.r#type, Some(PointKind::Chunk));
    }

    #[test]
    fn test_payload_with_type() {
        let payload = Payload::new("src/main.rs").with_type(PointKind::Chunk);
        assert_eq!(payload.file_path, "src/main.rs");
        assert_eq!(payload.r#type, Some(PointKind::Chunk));
    }
}
