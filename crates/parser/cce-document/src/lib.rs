//! Document processing module
//!
//! This module provides functionality for processing document-based files
//! that don't require AST parsing, such as:
//! - Document files (Markdown, plain text)
//! - Configuration files (TOML, YAML, JSON)
//! - Markup files (XML, HTML)
//!
//! It provides a dedicated pipeline for each document type that:
//! 1. Parses the content into structured nodes
//! 2. Groups nodes by semantic relationships
//! 3. Chunks groups for embedding and storage
//! 4. Generates document summaries for high-level retrieval
//!
//! # Usage
//!
//! Use `PipelineRouter::global()` to access the shared pipeline router:
//!
//! ```ignore
//! use cce_parser::document::PipelineRouter;
//!
//! let router = PipelineRouter::global();
//! // let (chunks, summary) = router.process(content, "file.md", &config)?;
//! ```

mod common;
mod json;
mod markdown;
mod pipeline;
mod plain;
mod plugin;
mod toml;
mod types;
mod xml;
mod yaml;

pub use common::{GenericChunker, GenericGroup};
pub use json::{
    JsonChunker, JsonGroup, JsonGroupType, JsonGrouper, JsonNode, JsonNodeType, JsonParser,
    JsonPipeline, JsonSummarizer, JsonValueType,
};
pub use markdown::{DocChunker, DocGrouper, DocSummarizer, MarkdownParser, MarkdownPipeline};
pub use pipeline::{PipelineRouter, TextPipeline};
pub use plain::PlainTextPipeline;
pub use plugin::PluginDocumentPipeline;
pub use toml::{
    TomlChunker, TomlGroup, TomlGroupType, TomlGrouper, TomlNode, TomlNodeType, TomlParser,
    TomlPipeline, TomlSummarizer, TomlValueType,
};
pub use types::{
    CodeSpan, DocGroup, DocGroupType, DocNode, DocNodeMeta, DocNodeType, DocSummary, DocType,
    DocumentClassification, LinkInfo,
};
pub use xml::{
    XmlChunker, XmlGroup, XmlGroupType, XmlGrouper, XmlNode, XmlNodeType, XmlParser, XmlPipeline,
    XmlSummarizer,
};
pub use yaml::{
    YamlChunker, YamlGroup, YamlGroupType, YamlGrouper, YamlNode, YamlNodeType, YamlParser,
    YamlPipeline, YamlSummarizer, YamlValueType,
};
