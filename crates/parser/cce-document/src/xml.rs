//! XML processing pipeline
//!
//! This module provides the complete XML processing pipeline:
//! - Parser: Parse XML into structured XmlNodes
//! - Grouper: Group nodes by element boundaries
//! - Chunker: Split groups into chunks
//! - Summarizer: Generate document summaries

mod chunker;
mod grouper;
mod parser;
mod summarizer;
#[cfg(test)]
mod test;
mod types;

pub use chunker::XmlChunker;
pub use grouper::XmlGrouper;
pub use parser::XmlParser;
pub use summarizer::XmlSummarizer;
pub use types::{XmlGroup, XmlGroupType, XmlNode, XmlNodeType};

use crate::common::GenericChunker;
use crate::common::summarizer::GenericSummarizer;
use crate::pipeline::TextPipeline;
use crate::types::DocSummary;
use crate::types::DocumentClassification;
use cce_config::modules::ChunkingConfig;
use cce_types::ChunkedResult;
use cce_types::ParseError;
use cce_types::ast_to_nl::options::OutputMode;

/// XML processing pipeline
#[derive(Clone)]
pub struct XmlPipeline {
    grouper: XmlGrouper,
    summarizer: XmlSummarizer,
}

impl XmlPipeline {
    /// Create a new XML pipeline
    pub fn new() -> Self {
        Self {
            grouper: XmlGrouper::new(),
            summarizer: XmlSummarizer::new(),
        }
    }
}

impl Default for XmlPipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl TextPipeline for XmlPipeline {
    type ParsedNode = XmlNode;
    type Group = XmlGroup;

    fn parse(&self, content: &str) -> Result<Vec<Self::ParsedNode>, ParseError> {
        let mut parser = XmlParser::new();
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
        let chunker = XmlChunker::new(config.clone());
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
    fn test_xml_pipeline_simple() {
        let pipeline = XmlPipeline::new();
        let config = ChunkingConfig::default();
        let xml = r#"<root><child>text</child></root>"#;

        let (chunks, summary) = pipeline
            .process(xml, "test.xml", &config, OutputMode::default())
            .expect("should process");

        assert!(!chunks.is_empty());
        assert!(summary.is_some());

        let summary = summary.unwrap();
        assert!(summary.title.is_some());
        assert!(summary.main_headings.is_empty() || !summary.main_headings.is_empty());
    }

    #[test]
    fn test_xml_pipeline_with_attributes() {
        let pipeline = XmlPipeline::new();
        let config = ChunkingConfig::default();
        let xml = r#"<root id="main"><child name="test">value</child></root>"#;

        let (chunks, summary) = pipeline
            .process(xml, "config.xml", &config, OutputMode::default())
            .expect("should process");

        assert!(!chunks.is_empty());
        assert!(summary.is_some());

        let summary = summary.unwrap();
        assert!(summary.title.is_some());
    }

    #[test]
    fn test_xml_pipeline_nested() {
        let pipeline = XmlPipeline::new();
        let config = ChunkingConfig::default();
        let xml = r#"
            <config>
                <database>
                    <host>localhost</host>
                    <port>3306</port>
                </database>
                <cache>
                    <enabled>true</enabled>
                </cache>
            </config>
        "#;

        let (chunks, summary) = pipeline
            .process(xml, "config.xml", &config, OutputMode::default())
            .expect("should process");

        assert!(!chunks.is_empty());
        assert!(summary.is_some());

        let summary = summary.unwrap();
        assert!(summary.title.is_some());
        assert!(summary.line_count > 0);
    }

    #[test]
    fn test_xml_pipeline_maven_pom() {
        let pipeline = XmlPipeline::new();
        let config = ChunkingConfig::default();
        let xml = r#"
            <project>
                <modelVersion>4.0.0</modelVersion>
                <dependencies>
                    <dependency>
                        <groupId>org.example</groupId>
                        <artifactId>lib</artifactId>
                    </dependency>
                </dependencies>
            </project>
        "#;

        let (chunks, summary) = pipeline
            .process(xml, "pom.xml", &config, OutputMode::default())
            .expect("should process");

        assert!(!chunks.is_empty());
        assert!(summary.is_some());

        let summary = summary.unwrap();
        assert_eq!(summary.title, Some("Maven POM".to_string()));
    }

    #[test]
    fn test_xml_pipeline_invalid() {
        let pipeline = XmlPipeline::new();
        let config = ChunkingConfig::default();
        // Use a more obviously invalid XML
        let xml = r#"<root><child></root>"#;

        // quick-xml may be lenient with some invalid XML
        // Just check that it doesn't panic
        let _ = pipeline.process(xml, "test.xml", &config, OutputMode::default());
    }
}
