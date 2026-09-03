//! TOML processing pipeline
//!
//! This module provides the complete TOML processing pipeline:
//! - Parser: Parse TOML into structured TomlNodes
//! - Grouper: Group nodes by table boundaries
//! - Chunker: Split groups into chunks
//! - Summarizer: Generate document summaries

mod chunker;
mod grouper;
mod parser;
mod summarizer;
#[cfg(test)]
mod test;
mod types;

pub use chunker::TomlChunker;
pub use grouper::TomlGrouper;
pub use parser::TomlParser;
pub use summarizer::TomlSummarizer;
pub use types::{TomlGroup, TomlGroupType, TomlNode, TomlNodeType, TomlValueType};

use crate::common::chunker::GenericChunker;
use crate::common::summarizer::GenericSummarizer;
use crate::pipeline::TextPipeline;
use crate::types::DocSummary;
use crate::types::DocumentClassification;
use cce_config::modules::ChunkingConfig;
use cce_types::ChunkedResult;
use cce_types::ParseError;
use cce_types::ast_to_nl::options::OutputMode;

/// TOML processing pipeline
#[derive(Clone)]
pub struct TomlPipeline {
    summarizer: TomlSummarizer,
}

impl TomlPipeline {
    /// Create a new TOML pipeline
    pub fn new() -> Self {
        Self {
            summarizer: TomlSummarizer::new(),
        }
    }
}

impl Default for TomlPipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl TextPipeline for TomlPipeline {
    type ParsedNode = TomlNode;
    type Group = TomlGroup;

    fn parse(&self, content: &str) -> Result<Vec<Self::ParsedNode>, ParseError> {
        let mut parser = TomlParser::new();
        parser.parse(content)
    }

    fn group(
        &self,
        nodes: Vec<Self::ParsedNode>,
        file_path: &str,
    ) -> Result<Vec<Self::Group>, ParseError> {
        let mut grouper = TomlGrouper::new();
        grouper.group(nodes, file_path)
    }

    fn chunk(
        &self,
        groups: Vec<Self::Group>,
        config: &ChunkingConfig,
        file_path: &str,
        output_mode: OutputMode,
        classification: &DocumentClassification,
    ) -> Result<Vec<ChunkedResult>, ParseError> {
        let chunker = TomlChunker::new(config.clone());
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

    #[test]
    fn test_toml_pipeline_simple() {
        let pipeline = TomlPipeline::new();
        let config = ChunkingConfig::default();
        let toml = r#"name = "test"
value = 123"#;

        let (chunks, summary) = pipeline
            .process(toml, "test.toml", &config, OutputMode::default())
            .expect("should process");

        assert!(!chunks.is_empty());
        assert!(summary.is_some());

        let summary = summary.unwrap();
        assert!(summary.title.is_some());
        assert!(summary.line_count >= 2);
    }

    #[test]
    fn test_toml_pipeline_with_table() {
        let pipeline = TomlPipeline::new();
        let config = ChunkingConfig::default();
        let toml = r#"[database]
host = "localhost"
port = 3306"#;

        let (chunks, summary) = pipeline
            .process(toml, "config.toml", &config, OutputMode::default())
            .expect("should process");

        assert!(!chunks.is_empty());
        assert!(summary.is_some());

        let summary = summary.unwrap();
        assert!(summary.line_count > 0);
    }
}
