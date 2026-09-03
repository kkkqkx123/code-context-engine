//! Markdown processing pipeline
//!
//! This module provides the complete Markdown processing pipeline:
//! - Parser: Parse Markdown into structured DocNodes
//! - Grouper: Group nodes by heading hierarchy
//! - Chunker: Split groups into chunks
//! - Summarizer: Generate document summaries

mod chunker;
mod grouper;
mod parser;
mod summarizer;
#[cfg(test)]
mod test;

pub use chunker::DocChunker;
pub use grouper::DocGrouper;
pub use parser::MarkdownParser;
pub use summarizer::DocSummarizer;

use crate::pipeline::TextPipeline;
use crate::types::DocSummary;
use crate::types::DocumentClassification;
use cce_config::modules::ChunkingConfig;
use cce_types::ChunkedResult;
use cce_types::ParseError;
use cce_types::ast_to_nl::options::OutputMode;

/// Markdown processing pipeline
#[derive(Clone)]
pub struct MarkdownPipeline {
    grouper: DocGrouper,
    summarizer: DocSummarizer,
}

impl MarkdownPipeline {
    /// Create a new Markdown pipeline
    pub fn new() -> Self {
        Self {
            grouper: DocGrouper::new(),
            summarizer: DocSummarizer::new(),
        }
    }
}

impl Default for MarkdownPipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl TextPipeline for MarkdownPipeline {
    type ParsedNode = crate::types::DocNode;
    type Group = crate::types::DocGroup;

    fn parse(&self, content: &str) -> Result<Vec<Self::ParsedNode>, ParseError> {
        let mut parser = MarkdownParser::new();
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
        let chunker = DocChunker::new(config.clone())
            .with_smart_merging(true)
            .with_min_chunk_tokens(20);
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
mod integration_tests {
    use super::*;

    #[test]
    fn test_markdown_pipeline_simple_document() {
        let pipeline = MarkdownPipeline::new();
        let config = ChunkingConfig::default();

        let markdown = r#"# Introduction

This is a simple document.

## Getting Started

Follow these steps:

1. Install the package
2. Configure settings

```rust
fn main() {
    println!("Hello");
}
```"#;

        let nodes = pipeline.parse(markdown).expect("should parse");
        assert!(!nodes.is_empty());

        let groups = pipeline
            .group(nodes.clone(), "test.md")
            .expect("should group");
        assert!(!groups.is_empty());

        let chunks = pipeline
            .chunk(
                groups.clone(),
                &config,
                "test.md",
                OutputMode::default(),
                &DocumentClassification::detect("test.md"),
            )
            .expect("should chunk");
        assert!(!chunks.is_empty());

        let summary = pipeline.summarize(&nodes, &groups, "test.md");
        assert!(summary.is_some());

        let summary = summary.unwrap();
        assert_eq!(summary.title, Some("Introduction".to_string()));
    }

    #[test]
    fn test_markdown_pipeline_complex_document() {
        let pipeline = MarkdownPipeline::new();
        let config = ChunkingConfig::default();

        let markdown = r#"# API Documentation

Complete API reference.

## Authentication

Use OAuth 2.0 for authentication.

```python
import requests
token = get_token()
```

## Endpoints

### GET /users

Returns a list of users.

**Parameters:**
- `limit`: Maximum number of results
- `offset`: Pagination offset

**Response:**
```json
{
  "users": [],
  "total": 100
}
```

### POST /users

Create a new user.

See [authentication](#authentication) for details."#;

        let (chunks, summary) = pipeline
            .process(markdown, "api.md", &config, OutputMode::default())
            .expect("should process");

        assert!(!chunks.is_empty());
        assert!(summary.is_some());

        let summary = summary.unwrap();
        assert!(summary.line_count > 0);
    }
}
