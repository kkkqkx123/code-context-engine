//! Backend-agnostic storage abstraction.
//!
//! This module holds the retrieval types (`Payload`, `ScoredPoint`,
//! `DenseSearchQuery`, `SearchFilter`, `VectorPoint`) with backend-neutral
//! semantics plus the operation contract every vector backend implements.
//! Backends translate the filter into their native form (local predicate or
//! Qdrant filter JSON). The `raw_filter` passthrough is a Qdrant-only escape
//! hatch: the Qdrant branch honors it, the local branch rejects queries that
//! set it instead of silently ignoring it.
//!
//! It also holds the relation storage contract (`RelationStorage`) shared by
//! the embedded SQLite branch and the remote PostgreSQL branch.
//!
//! # Architecture
//!
//! ```text
//! Application Layer
//!     └── DenseSearchQuery / SearchFilter / ScoredPoint / Payload / VectorPoint
//!             └── VectorStorage (operation contract)
//!                     ├── Local branch (embedded simvec)
//!                     └── Qdrant branch (remote service)
//!     └── RelationStorage (operation contract)
//!             ├── SQLite branch (embedded)
//!             └── PostgreSQL branch (remote)
//! ```
//!
//! Dispatch uses a backend enum at the assembly layer, never trait objects.

use serde::{Deserialize, Serialize};

use cce_types::{FileCategory, PointKind, TestSource, normalize_project_path};

pub mod relation;

pub use relation::{
    RelationChunk, RelationEntity, RelationFile, RelationStorage, assert_relation_storage,
};

/// Search filter options for vector retrieval.
///
/// Backend-agnostic: each implementation translates it into its native query
/// language (local predicate or Qdrant filter JSON).
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
    /// Exact file path match (normalized). Used by per-file deletes.
    pub file_path: Option<String>,
    /// Directory prefix filter
    pub directory_prefix: Option<String>,
    /// Exclude test files
    pub exclude_test: bool,
    /// Include only specific categories
    pub include_categories: Option<Vec<FileCategory>>,
    /// Exclude specific categories
    pub exclude_categories: Option<Vec<FileCategory>>,
    /// Pre-built raw filter JSON (Qdrant branch only; takes precedence over
    /// other fields when set. The local branch rejects queries that set it
    /// with a validation error rather than silently ignoring it).
    pub raw_filter: Option<serde_json::Value>,
}

impl SearchFilter {
    /// Exact file-path filter scoped to a group.
    pub fn file_scoped(file_path: impl Into<String>, group_id: impl Into<String>) -> Self {
        Self {
            file_path: Some(normalize_project_path(&file_path.into())),
            group_id: Some(group_id.into()),
            ..Default::default()
        }
    }
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

/// Vector point with payload (write path).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorPoint {
    /// Unique point ID (project-scoped, e.g. `group::epoch::chunk`)
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

/// Payload metadata for a vector point
///
/// Minimal payload design: only essential fields for filtering.
/// All other metadata is stored in SQLite and fetched on-demand.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Payload {
    /// The original application-level point ID (e.g. chunk_id like `group_9_emb_0`)
    /// Stored alongside the backend point ID so that search results can
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
    /// Marker that the stored text was token-budget truncated. Only chunk
    /// writes populate it; `Option` keeps non-chunk payloads free of the key.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncated: Option<bool>,
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
            truncated: None,
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

    /// Set the token-budget truncation marker
    pub fn with_truncated(mut self, truncated: bool) -> Self {
        self.truncated = Some(truncated);
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

/// Whether a stored payload is visible under a search filter.
///
/// Shared by the local backend predicate and the dual-backend contract tests
/// so generation, group, type, directory, test and category semantics stay
/// identical. `raw_filter` is Qdrant-only and never reaches this predicate:
/// the local backend rejects it before searching.
pub fn payload_matches_filter(payload: &Payload, filter: &SearchFilter) -> bool {
    if let Some(ref group_id) = filter.group_id
        && payload.group_id.as_deref() != Some(group_id.as_str())
    {
        return false;
    }
    if let Some(ref file_path) = filter.file_path {
        let normalized = normalize_project_path(file_path);
        if payload.file_path != normalized {
            return false;
        }
    }
    if !filter.epochs.is_empty() {
        let epoch = payload.epoch.unwrap_or(i64::MIN);
        if !filter.epochs.contains(&epoch) {
            return false;
        }
        if let Some(ref excluded) = filter.excluded_files
            && filter.epochs.len() > 1
            && epoch == filter.epochs[0]
            && excluded.contains(&payload.file_path)
        {
            return false;
        }
    }
    if let Some(point_type) = filter.point_type
        && payload.r#type != Some(point_type)
    {
        return false;
    }
    if let Some(ref prefix) = filter.directory_prefix {
        let normalized = normalize_project_path(prefix);
        let trimmed = normalized.trim_end_matches('/');
        if trimmed.is_empty() {
            // Root prefix matches everything.
        } else if !(payload.file_path == trimmed
            || payload.file_path.starts_with(&format!("{trimmed}/")))
        {
            return false;
        }
    }
    if let Some(ref categories) = filter.include_categories
        && !categories.is_empty()
    {
        let Some(category) = payload.category else {
            return false;
        };
        if !categories.contains(&category) {
            return false;
        }
    }
    if let Some(ref categories) = filter.exclude_categories {
        if let Some(category) = payload.category
            && categories.contains(&category)
        {
            return false;
        }
    }
    if filter.exclude_test && payload.test == Some(true) {
        return false;
    }
    true
}

/// Deterministic group ID derived from a workspace path.
pub fn generate_group_id(workspace_path: &str) -> String {
    let hash = cce_utils::hash::calculate_hash(workspace_path.as_bytes());
    format!("proj_{}", &hash[..12])
}

/// Stable namespace for one logical project.
pub fn generate_project_group_id(project_id: i64, workspace_path: &str) -> String {
    format!("project-{project_id}-{}", generate_group_id(workspace_path))
}

/// Single collection name shared by both backends.
pub fn vector_collection_name() -> String {
    format!("cce_vectors-i{}", cce_types::INDEX_FORMAT_VERSION)
}

/// Backend-agnostic vector storage contract.
///
/// Covers writes, vector search, id/file deletes, counting and collection
/// management. Filter semantics stay backend-agnostic; each backend
/// translates `SearchFilter` into its native predicate.
#[async_trait::async_trait]
pub trait VectorStorage: Send + Sync {
    /// Backend name for logging and diagnostics (`local` or `qdrant`).
    fn backend_name(&self) -> &'static str;

    /// Ensure the collection exists (create when missing).
    /// Returns true when the collection was created.
    async fn ensure_collection(&self) -> Result<bool, cce_types::StorageError>;

    /// Whether the collection exists.
    async fn collection_exists(&self) -> Result<bool, cce_types::StorageError>;

    /// Delete the whole collection.
    async fn delete_collection(&self) -> Result<(), cce_types::StorageError>;

    /// Remove all points from the collection.
    async fn clear_collection(&self) -> Result<(), cce_types::StorageError>;

    /// Upsert vector points (insert or overwrite by point ID).
    async fn upsert_points(&self, points: &[VectorPoint]) -> Result<(), cce_types::StorageError>;

    /// Dense vector similarity search (higher score is better).
    async fn search_dense(
        &self,
        query: DenseSearchQuery,
    ) -> Result<Vec<ScoredPoint>, cce_types::StorageError>;

    /// Delete one file's points inside a group.
    async fn delete_by_file_path_scoped(
        &self,
        file_path: &str,
        group_id: &str,
        point_type: Option<PointKind>,
    ) -> Result<(), cce_types::StorageError>;

    /// Delete one file's points inside a group and epoch.
    async fn delete_by_file_path_scoped_epoch(
        &self,
        file_path: &str,
        group_id: &str,
        epoch: i64,
    ) -> Result<(), cce_types::StorageError>;

    /// Delete all points of a group and epoch.
    async fn delete_by_group_epoch(
        &self,
        group_id: &str,
        epoch: i64,
    ) -> Result<(), cce_types::StorageError>;

    /// Delete all points of a group.
    async fn delete_by_group(&self, group_id: &str) -> Result<(), cce_types::StorageError>;

    /// Delete several files' points inside a group.
    async fn delete_by_file_paths_scoped(
        &self,
        file_paths: &[&str],
        group_id: &str,
        point_type: Option<PointKind>,
    ) -> Result<(), cce_types::StorageError> {
        for path in file_paths {
            self.delete_by_file_path_scoped(path, group_id, point_type)
                .await?;
        }
        Ok(())
    }

    /// List all points (used by generation GC and compaction).
    async fn scroll_all_points(&self) -> Result<Vec<VectorPoint>, cce_types::StorageError>;

    /// Count points of a group.
    async fn count_points_by_group(&self, group_id: &str)
    -> Result<usize, cce_types::StorageError>;

    /// Count all points in the collection.
    async fn count_all_points(&self) -> Result<usize, cce_types::StorageError>;

    /// Liveness probe (local always true when the engine is open).
    async fn health(&self) -> Result<bool, cce_types::StorageError>;
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

    #[test]
    fn test_filter_matches_epochs_and_excluded_files() {
        let payload = Payload::new("src/a.rs").with_group_id("g").with_epoch(4);
        let filter = SearchFilter {
            epochs: vec![4, 5],
            excluded_files: Some(vec!["src/a.rs".to_string()]),
            group_id: Some("g".to_string()),
            ..Default::default()
        };
        assert!(!payload_matches_filter(&payload, &filter));

        let own = Payload::new("src/a.rs").with_group_id("g").with_epoch(5);
        assert!(payload_matches_filter(&own, &filter));
    }

    #[test]
    fn test_filter_matches_file_and_directory() {
        let payload = Payload::new("src/lib/a.rs").with_group_id("g");
        let file_filter = SearchFilter::file_scoped("src/lib/a.rs", "g");
        assert!(payload_matches_filter(&payload, &file_filter));

        let dir_filter = SearchFilter {
            directory_prefix: Some("src/lib".to_string()),
            ..Default::default()
        };
        assert!(payload_matches_filter(&payload, &dir_filter));

        let miss = SearchFilter {
            directory_prefix: Some("src/other".to_string()),
            ..Default::default()
        };
        assert!(!payload_matches_filter(&payload, &miss));
    }

    #[test]
    fn test_filter_matches_test_and_category() {
        use cce_types::FileCategory;
        let payload = Payload::new("src/a.rs")
            .with_test(true)
            .with_category(FileCategory::Code);
        let filter = SearchFilter {
            exclude_test: true,
            ..Default::default()
        };
        assert!(!payload_matches_filter(&payload, &filter));

        let filter = SearchFilter {
            include_categories: Some(vec![FileCategory::Config]),
            ..Default::default()
        };
        assert!(!payload_matches_filter(&payload, &filter));
    }

    #[test]
    fn test_group_id_helpers() {
        let gid = generate_group_id("/tmp/ws");
        assert!(gid.starts_with("proj_"));
        let pgid = generate_project_group_id(7, "/tmp/ws");
        assert!(pgid.starts_with("project-7-proj_"));
        assert!(vector_collection_name().starts_with("cce_vectors-i"));
    }
}
