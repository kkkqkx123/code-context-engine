//! BM25 related type definitions

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub use cce_config::modules::search::TermOperator;

/// Document for BM25 indexing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bm25Document {
    /// Document ID
    pub document_id: String,

    /// Field values (title, content, etc.)
    pub fields: HashMap<String, String>,
}

impl Bm25Document {
    /// Create a new BM25 document
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

/// Search result from BM25
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bm25SearchResult {
    /// Document ID
    pub document_id: String,

    /// BM25 score
    pub score: f32,

    /// Field values
    pub fields: HashMap<String, String>,
}

impl Bm25SearchResult {
    /// Get the title field (entity/function name)
    pub fn title(&self) -> Option<&String> {
        self.fields.get("title")
    }

    /// Get the chunk_id field (for SQLite lookup)
    pub fn chunk_id(&self) -> Option<&String> {
        self.fields.get("chunk_id")
    }
}

/// Search options for BM25 retrieval (unified read-path entry)
#[derive(Debug, Clone)]
pub struct Bm25SearchOptions {
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
    pub include_categories: Vec<cce_types::FileCategory>,
    /// Exclude chunks whose category matches any of these values
    pub exclude_categories: Vec<cce_types::FileCategory>,
    /// Operator for combining multiple query terms (`or`/`and`)
    pub term_operator: TermOperator,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bm25_document_builder() {
        let doc = Bm25Document::new("test:1")
            .with_field("title", "Test Function")
            .with_field("content", "This is a test function");

        assert_eq!(doc.document_id, "test:1");
        assert_eq!(doc.get_field("title"), Some(&"Test Function".to_string()));
        assert!(doc.has_field("content"));
        assert!(!doc.has_field("nonexistent"));
    }

    #[test]
    fn test_bm25_document_chunk_shape() {
        // Production documents are built from chunked results (see the storage
        // coordinator mapping): per-chunk title, content text, and keywords.
        let doc = Bm25Document::new("1::1::group_1_bm25_0")
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
}
