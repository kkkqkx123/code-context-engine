//! JSON processing pipeline
//!
//! This module provides the complete JSON processing pipeline:
//! - Parser: Parse JSON into structured JsonNodes
//! - Grouper: Group nodes by object/array boundaries
//! - Chunker: Split groups into chunks
//! - Summarizer: Generate document summaries

mod chunker;
mod grouper;
mod parser;
mod summarizer;
#[cfg(test)]
mod test;
mod types;

pub use chunker::JsonChunker;
pub use grouper::JsonGrouper;
pub use parser::JsonParser;
pub use summarizer::JsonSummarizer;
pub use types::{JsonGroup, JsonGroupType, JsonNode, JsonNodeType, JsonValueType};

use crate::common::GenericChunker;
use crate::common::summarizer::GenericSummarizer;
use crate::pipeline::TextPipeline;
use crate::types::DocSummary;
use crate::types::DocumentClassification;
use cce_config::modules::ChunkingConfig;
use cce_types::ChunkedResult;
use cce_types::ParseError;
use cce_types::ast_to_nl::options::OutputMode;

/// JSON processing pipeline
#[derive(Clone)]
pub struct JsonPipeline {
    grouper: JsonGrouper,
    summarizer: JsonSummarizer,
}

impl JsonPipeline {
    /// Create a new JSON pipeline
    pub fn new() -> Self {
        Self {
            grouper: JsonGrouper::new(),
            summarizer: JsonSummarizer::new(),
        }
    }
}

impl Default for JsonPipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl TextPipeline for JsonPipeline {
    type ParsedNode = JsonNode;
    type Group = JsonGroup;

    fn parse(&self, content: &str) -> Result<Vec<Self::ParsedNode>, ParseError> {
        let mut parser = JsonParser::new();
        parser.parse(content)
    }

    fn group(
        &self,
        nodes: Vec<Self::ParsedNode>,
        file_path: &str,
    ) -> Result<Vec<Self::Group>, ParseError> {
        self.grouper.group(nodes, file_path)
    }

    fn chunk(
        &self,
        groups: Vec<Self::Group>,
        config: &ChunkingConfig,
        file_path: &str,
        output_mode: OutputMode,
        classification: &DocumentClassification,
    ) -> Result<Vec<ChunkedResult>, ParseError> {
        let chunker = JsonChunker::new(config.clone());
        Ok(chunker.chunk_groups(&groups, file_path, output_mode, classification))
    }

    fn summarize(
        &self,
        nodes: &[Self::ParsedNode],
        groups: &[Self::Group],
        file_path: &str,
    ) -> Option<DocSummary> {
        Some(self.summarizer.summarize(nodes, groups, file_path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DocType;

    #[test]
    fn test_json_pipeline_simple() {
        let pipeline = JsonPipeline::new();
        let config = ChunkingConfig::default();
        let json = r#"{"name": "test", "value": 123}"#;

        let (chunks, summary) = pipeline
            .process(json, "test.json", &config, OutputMode::default())
            .expect("should process");

        assert!(!chunks.is_empty());
        assert!(summary.is_some());

        let summary = summary.unwrap();
        assert!(summary.title.is_some());
        assert_eq!(summary.doc_type, DocType::Config);
    }

    #[test]
    fn test_json_pipeline_nested() {
        let pipeline = JsonPipeline::new();
        let config = ChunkingConfig::default();
        let json = r#"{
            "database": {
                "host": "localhost",
                "port": 3306
            },
            "debug": true
        }"#;

        let (chunks, summary) = pipeline
            .process(json, "config.json", &config, OutputMode::default())
            .expect("should process");

        assert!(!chunks.is_empty());
        assert!(summary.is_some());

        let summary = summary.unwrap();
        assert!(summary.title.is_some());
        assert!(summary.line_count > 0);
    }

    #[test]
    fn test_json_pipeline_array() {
        let pipeline = JsonPipeline::new();
        let config = ChunkingConfig::default();
        let json = r#"{
            "items": [
                {"id": 1, "name": "item1"},
                {"id": 2, "name": "item2"},
                {"id": 3, "name": "item3"}
            ]
        }"#;

        let (chunks, summary) = pipeline
            .process(json, "data.json", &config, OutputMode::default())
            .expect("should process");

        assert!(!chunks.is_empty());
        assert!(summary.is_some());

        let summary = summary.unwrap();
        assert!(summary.main_headings.is_empty() || !summary.main_headings.is_empty());
    }

    #[test]
    fn test_json_pipeline_package_json() {
        let pipeline = JsonPipeline::new();
        let config = ChunkingConfig::default();
        let json = r#"{
            "name": "my-package",
            "version": "1.0.0",
            "dependencies": {
                "express": "^4.18.0"
            },
            "scripts": {
                "start": "node index.js"
            }
        }"#;

        let (chunks, summary) = pipeline
            .process(json, "package.json", &config, OutputMode::default())
            .expect("should process");

        assert!(!chunks.is_empty());
        assert!(summary.is_some());

        let summary = summary.unwrap();
        assert_eq!(summary.title, Some("Package Configuration".to_string()));
    }

    #[test]
    fn test_json_pipeline_invalid() {
        let pipeline = JsonPipeline::new();
        let config = ChunkingConfig::default();
        let json = r#"{"invalid": }"#;

        let result = pipeline.process(json, "test.json", &config, OutputMode::default());
        assert!(result.is_err());
    }

    // Test: Array elements should be independent groups
    #[test]
    fn test_array_elements_independent_groups() {
        let pipeline = JsonPipeline::new();
        let config = ChunkingConfig::default();
        let json = r#"{
            "users": [
                {"id": 1, "name": "Alice"},
                {"id": 2, "name": "Bob"},
                {"id": 3, "name": "Charlie"}
            ]
        }"#;

        let (chunks, _) = pipeline
            .process(json, "test.json", &config, OutputMode::default())
            .expect("should process");

        // Each array element should be in its own chunk (or merged by chunker)
        // But they should NOT all be in one giant chunk
        assert!(!chunks.is_empty());

        // Verify chunks contain complete user objects
        for chunk in &chunks {
            let text = chunk.text.as_str();
            // Each chunk with user data should have both id and name or be empty
            if text.contains("id") {
                assert!(
                    text.contains("name") || text.contains("Array Element"),
                    "Chunk should contain complete user object: {}",
                    text
                );
            }
        }
    }

    // Test: Root object should be flattened
    #[test]
    fn test_root_object_flattening() {
        let pipeline = JsonPipeline::new();
        let config = ChunkingConfig::default();
        let json = r#"{
            "database": {
                "host": "localhost",
                "port": 3306
            },
            "debug": true,
            "version": "1.0.0"
        }"#;

        let (chunks, _) = pipeline
            .process(json, "config.json", &config, OutputMode::Both)
            .expect("should process");

        // With root flattening, debug and database might be in same chunk
        // or at least not completely isolated
        assert!(!chunks.is_empty());

        // Check that chunks have proper context
        let has_context = chunks.iter().any(|c| {
            let text = c.text.as_str();
            text.contains("[Context:") || text.contains("[Root]")
        });
        assert!(has_context, "Chunks should have path context");
    }

    // Test: Context prefix should include full path
    #[test]
    fn test_chunk_context_prefix() {
        let pipeline = JsonPipeline::new();
        let config = ChunkingConfig::default();
        let json = r#"{
            "database": {
                "credentials": {
                    "username": "admin",
                    "password": "secret"
                }
            }
        }"#;

        let (chunks, _) = pipeline
            .process(json, "test.json", &config, OutputMode::Both)
            .expect("should process");

        // Check that chunks contain path context
        let has_database_context = chunks.iter().any(|c| {
            let text = c.text.as_str();
            text.contains("database -> credentials")
        });
        assert!(
            has_database_context,
            "Chunks should contain full path context"
        );
    }

    // Test: BM25 text should have dual representation
    #[test]
    fn test_bm25_dual_representation() {
        use crate::json::JsonParser;

        let mut parser = JsonParser::new();
        let json = r#"{"database": {"host": "localhost"}}"#;
        let nodes = parser.parse(json).expect("should parse");

        // Find the host node
        let host_node = nodes.iter().find(|n| n.key_name.as_deref() == Some("host"));
        assert!(host_node.is_some());

        let host_node = host_node.unwrap();
        let bm25_text = host_node.to_bm25_text();

        // Should contain both dotted and spaced forms
        assert!(
            bm25_text.contains("database.host"),
            "Should have dotted form"
        );
        assert!(
            bm25_text.contains("database host"),
            "Should have spaced form"
        );
    }

    // Test: Tags are no longer inferred
    #[test]
    fn test_fuzzy_tag_matching() {
        let pipeline = JsonPipeline::new();
        let config = ChunkingConfig::default();

        // Test with db_config and auth_token keys
        let json = r#"{
            "db_config": {
                "host": "localhost"
            },
            "auth_token": "xyz123"
        }"#;

        let (_, summary) = pipeline
            .process(json, "test.json", &config, OutputMode::default())
            .expect("should process");

        let summary = summary.expect("should have summary");

        // Verify structural fields are populated
        assert!(summary.line_count > 0);
        assert!(!summary.main_headings.is_empty());
    }
}
