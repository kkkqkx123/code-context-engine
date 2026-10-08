//! Client-side document serialization and pre-tokenization.

use serde_json::{Value, json};

use cce_storage_common::FulltextDocument;

use super::ElasticsearchClient;

impl ElasticsearchClient {
    /// Client-side pre-tokenization shared by writes and queries.
    pub(super) fn pretokenize(&self, text: &str) -> String {
        self.tokenizer.tokenize(text).join(" ")
    }

    /// Build the indexed source document for one neutral document.
    ///
    /// Raw body and keyword text never enter `_source`; only the
    /// pre-tokenized sidecars are indexed for those fields.
    pub fn index_source(&self, document: &FulltextDocument) -> Value {
        let field = |name: &str| document.get_field(name).cloned().unwrap_or_default();
        let title = field("title");
        let content = field("content");
        let keywords = field("keywords");
        let entity_ids: Vec<&str> = document
            .get_field("entity_id")
            .map(|s| {
                s.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        let parse_i64 = |name: &str| {
            document
                .get_field(name)
                .and_then(|v| v.parse::<i64>().ok())
                .unwrap_or(0)
        };
        json!({
            "document_id": document.document_id,
            "chunk_id": field("chunk_id"),
            "file_path": field("file_path"),
            "project_id": field("project_id"),
            "epoch": parse_i64("epoch"),
            "title": title,
            "title_tokens": self.pretokenize(&title),
            "content_tokens": self.pretokenize(&content),
            "keywords_tokens": self.pretokenize(&keywords),
            "entity_id": entity_ids,
            "segment_id": field("segment_id"),
            "test": parse_i64("test"),
            "category": parse_i64("category"),
        })
    }

    /// Bulk request body for one batch (newline-delimited action pairs).
    pub fn bulk_body(&self, documents: &[FulltextDocument]) -> String {
        let mut body = String::new();
        for document in documents {
            body.push_str(&json!({ "index": { "_id": document.document_id } }).to_string());
            body.push('\n');
            body.push_str(&self.index_source(document).to_string());
            body.push('\n');
        }
        body
    }
}
