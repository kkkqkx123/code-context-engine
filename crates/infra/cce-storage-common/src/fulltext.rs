//! Backend-neutral fulltext storage contract.
//!
//! Holds the fulltext document types plus the operation contract every
//! fulltext backend implements: batch writes, scoped deletes, keyword
//! retrieval, snapshot readback, generation enumeration, per-project counts,
//! and clear/rebuild. Batch writes are idempotent per document id
//! (delete-term then add), so replaying a batch after a transient failure
//! is safe. The body keyword field is indexed but not stored, so it never
//! participates in readback.
//!
//! Transaction boundary: every named operation is atomic on its own scope.
//! Batch writes commit once per batch; a transient failure retries the whole
//! batch, never a partial prefix.
//!
//! Writes take a shared reference: synchronization lives inside the branch
//! implementations, so holders share the client through a plain reference
//! count and no lock crosses module boundaries.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub use cce_config::modules::search::TermOperator;
use cce_types::FileCategory;

/// Backend-neutral fulltext document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FulltextDocument {
    /// Document ID
    pub document_id: String,

    /// Field values (title, content, etc.)
    pub fields: HashMap<String, String>,
}

impl FulltextDocument {
    /// Create a new fulltext document
    pub fn new(document_id: impl Into<String>) -> Self {
        Self {
            document_id: document_id.into(),
            fields: HashMap::new(),
        }
    }

    /// Add a field to the document
    pub fn with_field(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.fields.insert(name.into(), value.into());
        self
    }

    /// Get a field value
    pub fn get_field(&self, name: &str) -> Option<&String> {
        self.fields.get(name)
    }

    /// Check if document has a field
    pub fn has_field(&self, name: &str) -> bool {
        self.fields.contains_key(name)
    }
}

/// Backend-neutral fulltext retrieval hit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FulltextHit {
    /// Document ID
    pub document_id: String,

    /// Ranking score
    pub score: f32,

    /// Field values
    pub fields: HashMap<String, String>,
}

impl FulltextHit {
    /// Get the title field (entity/function name)
    pub fn title(&self) -> Option<&String> {
        self.fields.get("title")
    }

    /// Get the chunk_id field (for SQLite lookup)
    pub fn chunk_id(&self) -> Option<&String> {
        self.fields.get("chunk_id")
    }
}

/// Backend-neutral fulltext retrieval options (unified read-path entry).
#[derive(Debug, Clone)]
pub struct FulltextSearchOptions {
    /// Maximum number of results to return
    pub limit: usize,
    /// Number of top results to skip (for pagination)
    pub offset: usize,
    /// Field weights for ranking (title/content/keywords)
    pub field_weights: HashMap<String, f32>,
    /// Required project_id for multi-tenant isolation
    /// Only documents with this project_id will be returned
    pub project_id: i64,
    /// Visible data generations for version-aware filtering, ascending
    /// (`[parent, own]` under inheritance; a single element for full
    /// generations). Empty disables epoch filtering.
    pub epochs: Vec<i64>,
    /// Files whose parent-generation documents are hidden (replaced or
    /// deleted by the own generation). Only meaningful together with a
    /// two-element `epochs` chain.
    pub excluded_files: Option<Vec<String>>,
    /// Exclude test chunks (documents marked `test: "true"`)
    pub exclude_test: bool,
    /// Include only chunks whose category matches one of these values
    pub include_categories: Vec<FileCategory>,
    /// Exclude chunks whose category matches any of these values
    pub exclude_categories: Vec<FileCategory>,
    /// Operator for combining multiple query terms (`or`/`and`)
    pub term_operator: TermOperator,
}

/// Backend-neutral fulltext error (same classification as the branch error).
pub type FulltextError = cce_types::error::Bm25Error;

/// Fulltext storage contract implemented by the local Tantivy branch and
/// the remote search-service branch.
///
/// Callers hold the backend enum and call through this contract; no caller
/// touches a concrete client or index manager.
///
/// Frozen semantics: batch writes are idempotent per document id; deletes
/// report the number of removed documents; `clear_index` removes all
/// documents and reports how many were removed; counts and snapshots only
/// observe flushed writes, so callers must invoke `flush` after the last
/// batch before marking a generation ready. File paths are compared after
/// normalization on every branch.
pub trait FulltextStorage: Clone + Send + Sync + 'static {
    /// Index a batch of documents into the configured index.
    async fn batch_index(
        &self,
        index_name: &str,
        documents: &[FulltextDocument],
    ) -> Result<usize, FulltextError>;

    /// Run a keyword retrieval with project and generation filters.
    ///
    /// The output carries only the stored readback fields (document and
    /// chunk ids, title, file path, alignment ids); the indexed-only body
    /// and keyword fields never participate in readback on any branch.
    async fn search(
        &self,
        query: &str,
        options: &FulltextSearchOptions,
    ) -> Result<Vec<FulltextHit>, FulltextError>;

    /// Delete documents for one file within one project.
    async fn delete_by_file_path_scoped(
        &self,
        index_name: &str,
        file_path: &str,
        project_id: i64,
    ) -> Result<usize, FulltextError>;

    /// Delete documents for one file in one data epoch.
    async fn delete_by_file_path_scoped_epoch(
        &self,
        index_name: &str,
        file_path: &str,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, FulltextError>;

    /// Delete all documents for one project and data epoch.
    async fn delete_by_project_epoch(
        &self,
        index_name: &str,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, FulltextError>;

    /// Delete all documents for a project.
    async fn delete_all_project_docs(
        &self,
        index_name: &str,
        project_id: i64,
    ) -> Result<usize, FulltextError>;

    /// Read back the stored fields needed to copy an epoch into a
    /// candidate generation.
    async fn snapshot_documents(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Vec<FulltextDocument>, FulltextError>;

    /// Count all documents in the index.
    async fn document_count(&self) -> Result<usize, FulltextError>;

    /// Count documents belonging to one project.
    async fn document_count_by_project(&self, project_id: i64) -> Result<usize, FulltextError>;

    /// List data epochs currently present for a project.
    async fn epochs_by_project(&self, project_id: i64) -> Result<Vec<i64>, FulltextError>;

    /// Recreate the index from scratch (generation rebuild/cleanup).
    async fn clear_index(&self, index_name: &str) -> Result<usize, FulltextError>;

    /// Make recent writes visible to counts and snapshot readbacks.
    ///
    /// The local branch reloads its reader per batch, so this is a no-op
    /// there; the remote branch refreshes the index. Call it after the last
    /// batch before a generation is marked ready or activated.
    async fn flush(&self) -> Result<(), FulltextError>;

    /// Whether the branch is enabled and connected.
    fn is_enabled(&self) -> bool;

    /// Backend name for logging (`local` for the embedded branch).
    fn backend_name(&self) -> &'static str {
        "local"
    }
}

/// Compile-time assertion that a type implements the contract.
pub fn assert_fulltext_storage<T: FulltextStorage>() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_builder_round_trip() {
        let doc = FulltextDocument::new("test:1")
            .with_field("title", "Test Function")
            .with_field("content", "This is a test function");

        assert_eq!(doc.document_id, "test:1");
        assert_eq!(doc.get_field("title"), Some(&"Test Function".to_string()));
        assert!(doc.has_field("content"));
        assert!(!doc.has_field("nonexistent"));
    }

    #[test]
    fn document_chunk_shape() {
        // Production documents are built from chunked results (see the storage
        // coordinator mapping): per-chunk title, content text, and keywords.
        let doc = FulltextDocument::new("1::1::group_1_bm25_0")
            .with_field("chunk_id", "group_1_bm25_0")
            .with_field("title", "calculator.calculate_total")
            .with_field(
                "content",
                "calculator.calculate_total (function).\nfn calculate_total()",
            )
            .with_field("keywords", "calculate_total calculator")
            .with_field("file_path", "calculator.rs");

        assert_eq!(
            doc.get_field("title"),
            Some(&"calculator.calculate_total".to_string())
        );
        assert_eq!(
            doc.get_field("keywords"),
            Some(&"calculate_total calculator".to_string())
        );
        assert!(doc.has_field("content"));
    }

    #[test]
    fn hit_readback_helpers() {
        let hit = FulltextHit {
            document_id: "d".to_string(),
            score: 1.0,
            fields: HashMap::from([
                ("title".to_string(), "t".to_string()),
                ("chunk_id".to_string(), "c".to_string()),
            ]),
        };
        assert_eq!(hit.score, 1.0);
        assert_eq!(hit.title().map(String::as_str), Some("t"));
        assert_eq!(hit.chunk_id().map(String::as_str), Some("c"));
    }
}
