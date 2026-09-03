use crate::types::DocSummary;
use crate::types::DocumentClassification;
use cce_config::modules::ChunkingConfig;
use cce_types::ChunkedResult;
use cce_types::ast_to_nl::options::OutputMode;

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

    let (chunks, summary): (Vec<ChunkedResult>, Option<DocSummary>) = pipeline
        .process(markdown, "api.md", &config, OutputMode::default())
        .expect("should process");

    assert!(!chunks.is_empty());
    assert!(summary.is_some());

    let summary = summary.unwrap();
    assert!(summary.line_count > 0);
}

#[test]
fn test_markdown_with_nested_lists() {
    let pipeline = MarkdownPipeline::new();
    let config = ChunkingConfig::default();

    let markdown = r#"# Nested Lists

1. First item
   - Nested item A
   - Nested item B
2. Second item
   - Nested item C"#;

    let (chunks, _): (Vec<ChunkedResult>, Option<DocSummary>) = pipeline
        .process(markdown, "list.md", &config, OutputMode::default())
        .expect("should process");
    assert!(!chunks.is_empty());
}

#[test]
fn test_markdown_with_blockquotes() {
    let pipeline = MarkdownPipeline::new();
    let config = ChunkingConfig::default();

    let markdown = r#"# Quotes

> This is a blockquote.
> It spans multiple lines.

Normal paragraph."#;

    let (chunks, _): (Vec<ChunkedResult>, Option<DocSummary>) = pipeline
        .process(markdown, "blockquote.md", &config, OutputMode::default())
        .expect("should process");
    assert!(!chunks.is_empty());
}
