//! BM25 related type definitions

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub use cce_config::modules::search::TermOperator;

/// Matched term information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchedTerm {
    /// The matched term text
    pub term: String,
    /// The field where the term was matched
    pub field: String,
    /// Number of occurrences in the field
    pub count: usize,
}

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

    /// Highlighted snippets (if requested)
    pub highlights: HashMap<String, String>,

    /// Matched terms across all fields
    pub matched_terms: Vec<MatchedTerm>,
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
    /// Whether to generate highlighted snippets
    pub highlight: bool,
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

/// Conversion from ConversionResult to Bm25Document
impl From<&cce_types::ConversionResult> for Bm25Document {
    fn from(result: &cce_types::ConversionResult) -> Self {
        let mut fields = HashMap::new();

        // Title field (high weight) - entity/function name for ranking
        fields.insert("title".to_string(), result.name.clone());

        // Content field (normal weight) - BM25 text
        if let Some(ref bm25_text) = result.bm25_text {
            fields.insert("content".to_string(), bm25_text.clone());
        }

        // Keywords field (for keyword search boosting)
        if !result.keywords.is_empty() {
            fields.insert("keywords".to_string(), result.keywords.join(" "));
        }

        // File path field (for path-based filtering)
        fields.insert("file_path".to_string(), result.file_path.clone());

        // Document ID: kind:name format (legacy, for backward compatibility)
        let document_id = format!("{}:{}", result.kind, result.name);

        Self {
            document_id,
            fields,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::ConversionResult;
    use cce_types::{EntityId, EntityKind};

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
    fn test_conversion_result_to_bm25_document() {
        let result = ConversionResult {
            entity_id: EntityId(1),
            kind: EntityKind::Function,
            name: "test_func".to_string(),
            file_path: "test.rs".to_string(),
            bm25_text: Some("test content".to_string()),
            embedding_text: None,
            keywords: vec!["test".to_string(), "func".to_string()],
            ..Default::default()
        };

        let doc = Bm25Document::from(&result);
        assert_eq!(doc.document_id, "function:test_func");
        // Title field is stored for ranking (high weight in BM25 search)
        assert_eq!(doc.fields.get("title"), Some(&"test_func".to_string()));
        assert_eq!(doc.fields.get("content"), Some(&"test content".to_string()));
        assert_eq!(doc.fields.get("keywords"), Some(&"test func".to_string()));
    }
}
