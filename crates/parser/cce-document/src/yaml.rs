//! YAML processing pipeline
//!
//! This module provides the complete YAML processing pipeline:
//! - Parser: Parse YAML into structured YamlNodes
//! - Grouper: Group nodes by mapping boundaries
//! - Chunker: Split groups into chunks
//! - Summarizer: Generate document summaries

mod chunker;
mod grouper;
mod parser;
mod summarizer;
#[cfg(test)]
mod test;
mod types;

pub use chunker::YamlChunker;
pub use grouper::YamlGrouper;
pub use parser::YamlParser;
pub use summarizer::YamlSummarizer;
pub use types::{YamlGroup, YamlGroupType, YamlNode, YamlNodeType, YamlValueType};

use crate::common::chunker::GenericChunker;
use crate::common::summarizer::GenericSummarizer;
use crate::pipeline::TextPipeline;
use crate::types::DocSummary;
use crate::types::DocumentClassification;
use cce_config::modules::ChunkingConfig;
use cce_types::ChunkedResult;
use cce_types::ParseError;
use cce_types::ast_to_nl::options::OutputMode;

/// YAML processing pipeline
#[derive(Clone)]
pub struct YamlPipeline {
    summarizer: YamlSummarizer,
}

impl YamlPipeline {
    /// Create a new YAML pipeline
    pub fn new() -> Self {
        Self {
            summarizer: YamlSummarizer::new(),
        }
    }
}

impl Default for YamlPipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl TextPipeline for YamlPipeline {
    type ParsedNode = YamlNode;
    type Group = YamlGroup;

    fn parse(&self, content: &str) -> Result<Vec<Self::ParsedNode>, ParseError> {
        let mut parser = YamlParser::new();
        parser.parse(content)
    }

    fn group(
        &self,
        nodes: Vec<Self::ParsedNode>,
        file_path: &str,
    ) -> Result<Vec<Self::Group>, ParseError> {
        let mut grouper = YamlGrouper::new();
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
        let chunker = YamlChunker::new(config.clone());
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
    fn test_yaml_pipeline_simple() {
        let pipeline = YamlPipeline::new();
        let config = ChunkingConfig::default();
        let yaml = r#"name: test
value: 123"#;

        let (chunks, summary) = pipeline
            .process(yaml, "test.yaml", &config, OutputMode::default())
            .expect("should process");

        assert!(!chunks.is_empty());
        assert!(summary.is_some());

        let summary = summary.unwrap();
        assert!(summary.title.is_some());
        assert!(summary.line_count >= 2);
    }

    #[test]
    fn test_yaml_pipeline_with_mapping() {
        let pipeline = YamlPipeline::new();
        let config = ChunkingConfig::default();
        let yaml = r#"database:
  host: localhost
  port: 3306"#;

        let (chunks, summary) = pipeline
            .process(yaml, "config.yaml", &config, OutputMode::default())
            .expect("should process");

        assert!(!chunks.is_empty());
        assert!(summary.is_some());

        let summary = summary.unwrap();
        assert!(summary.line_count > 0);
    }
}
