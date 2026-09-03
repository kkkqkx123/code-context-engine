//! Plain text pipeline
//!
//! Processing pipeline for plain text files (.txt, .log, .ini, etc.).
//!
//! # Design Trade-offs
//!
//! The four-stage trait (`parse` → `group` → `chunk` → `summarize`) is
//! formally over-specified for plain text, where `parse` and `group` carry
//! minimal structural weight.  However, the trait is kept as-is rather than
//! collapsing into a two-stage implementation because:
//!
//! - The [`TextPipeline`] uniform interface is depended on by
//!   [`PipelineRouter`](super::PipelineRouter) dispatch; special-casing the
//!   plain branch would increase branching complexity.
//! - The default `summarize_document` implementation relies on `parse` and
//!   `group` outputs, shared across all six pipelines.
//! - The empty-shell cost is negligible (one `String` clone per node).
//!
//! The real structural work happens in the **chunker** (`PlainTextChunker`),
//! which splits content by paragraphs, lines, INI sections, CSV batches, or
//! RST headings depending on the file kind.  `parse` produces paragraph-level
//! `DocNode`s so that `summarize` can report paragraph-granular `line_count`
//! rather than a single whole-file count.
//!
//! If a second "structureless" pipeline is needed in the future (e.g.
//! binary-extracted text), the `parse`/`group` default implementations may
//! be reconsidered at that point.

mod chunker;
#[cfg(test)]
mod test;

pub use chunker::{PlainTextChunker, PlainTextKind};

use crate::GenericGroup;
use crate::pipeline::TextPipeline;
use crate::types::{
    DocGroup, DocGroupType, DocNode, DocNodeType, DocSummary, DocType, DocumentClassification,
};
use cce_config::modules::ChunkingConfig;
use cce_types::ChunkedResult;
use cce_types::ast_to_nl::options::OutputMode;
use cce_types::{ParseError, Span};
use cce_utils::token_estimation::TokenEstimator;

/// Plain text processing pipeline
///
/// Simplified implementation that skips intermediate parsing/grouping
/// since plain text lacks semantic structure.
#[derive(Clone)]
pub struct PlainTextPipeline {
    estimator: TokenEstimator,
}

impl PlainTextPipeline {
    /// Create a new plain text pipeline
    pub fn new() -> Self {
        Self {
            estimator: TokenEstimator::default(),
        }
    }

    /// Get file kind from path (handles extensionless Makefile/Dockerfile)
    fn get_kind(file_path: &str) -> PlainTextKind {
        PlainTextKind::from_path(file_path)
    }
}

impl Default for PlainTextPipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl TextPipeline for PlainTextPipeline {
    type ParsedNode = DocNode;
    type Group = DocGroup;

    fn parse(&self, content: &str) -> Result<Vec<Self::ParsedNode>, ParseError> {
        // Split by blank lines into paragraph-level DocNodes so that
        // `summarize` can report paragraph-granular `line_count`.  The
        // chunker still receives the full text via `group.bm25_text` and
        // applies its own splitting independently.
        let mut nodes = Vec::new();
        for (i, paragraph) in content.split("\n\n").enumerate() {
            if paragraph.is_empty() {
                continue;
            }
            let byte_start = content.find(paragraph).unwrap_or(0);
            let byte_end = byte_start + paragraph.len();
            let span = Span::from_byte_range(content, byte_start, byte_end).unwrap_or_default();
            nodes.push(DocNode::new(
                format!("plain_p{i}"),
                DocNodeType::Paragraph,
                paragraph.to_string(),
                span,
            ));
        }
        Ok(nodes)
    }

    fn group(
        &self,
        nodes: Vec<Self::ParsedNode>,
        file_path: &str,
    ) -> Result<Vec<Self::Group>, ParseError> {
        // Simplified: return single group
        // Real chunking happens in the chunk() method
        let mut group = DocGroup::new(
            cce_types::path::group_id_base(file_path),
            DocGroupType::ParagraphGroup,
        );
        for node in nodes {
            group.add_member(node);
        }
        group.finalize(&self.estimator);
        Ok(vec![group])
    }

    fn chunk(
        &self,
        groups: Vec<Self::Group>,
        config: &ChunkingConfig,
        file_path: &str,
        _output_mode: OutputMode,
        classification: &DocumentClassification,
    ) -> Result<Vec<ChunkedResult>, ParseError> {
        // Direct chunking - the core logic for plain text. Use path-based
        // detection for extensionless build files (Makefile/Dockerfile) so
        // suffixed variants like `Dockerfile.dev` still get structured chunking;
        // fallback to the entry-passed classification for normal extensions.
        let kind = {
            let path_kind = PlainTextKind::from_path(file_path);
            if path_kind != PlainTextKind::Text {
                path_kind
            } else {
                PlainTextKind::from_language_info(classification.language_info())
            }
        };
        let chunker = PlainTextChunker::new(config.clone());

        // Extract content from the single group
        let content = groups.first().map(|g| g.bm25_text.as_str()).unwrap_or("");

        Ok(chunker.chunk(content, file_path, kind, classification))
    }

    fn summarize(
        &self,
        nodes: &[Self::ParsedNode],
        _groups: &[Self::Group],
        file_path: &str,
    ) -> Option<DocSummary> {
        let kind = Self::get_kind(file_path);
        let doc_type = match kind {
            PlainTextKind::Log => DocType::PlainText,
            PlainTextKind::Ini => DocType::Config,
            PlainTextKind::Make => DocType::Config,
            PlainTextKind::Docker => DocType::Config,
            PlainTextKind::Csv => DocType::PlainText,
            PlainTextKind::Rst => DocType::PlainText,
            PlainTextKind::Text => DocType::PlainText,
        };

        let mut summary = DocSummary::new(file_path.to_string(), doc_type);

        // Aggregate line counts across all paragraph nodes, adding 1 for
        // each blank-line separator between paragraphs so the total
        // reflects the full file including blank lines.
        let sep_count = if nodes.is_empty() {
            0
        } else {
            (nodes.len() - 1) as u32
        };
        summary.line_count = nodes
            .iter()
            .map(|n| n.content.lines().count() as u32)
            .sum::<u32>()
            + sep_count;

        Some(summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_text() {
        let pipeline = PlainTextPipeline::new();
        let config = ChunkingConfig::default();
        let text = "Hello world\n\nThis is a test.";

        let (chunks, summary) = pipeline
            .process(text, "test.txt", &config, OutputMode::default())
            .expect("should process");

        assert!(!chunks.is_empty());
        assert!(summary.is_some());
        let summary = summary.unwrap();
        assert_eq!(summary.line_count, 3);
    }

    #[test]
    fn test_pipeline_log() {
        let pipeline = PlainTextPipeline::new();
        let config = ChunkingConfig::default();
        let log = "2024-01-01 INFO Starting\n2024-01-01 DEBUG Running";

        let (chunks, summary) = pipeline
            .process(log, "app.log", &config, OutputMode::default())
            .expect("should process");

        assert!(!chunks.is_empty());
        let summary = summary.unwrap();
        assert_eq!(summary.doc_type, DocType::PlainText);
    }

    #[test]
    fn test_pipeline_ini() {
        let pipeline = PlainTextPipeline::new();
        let config = ChunkingConfig::default();
        let ini = "[section]\nkey=value";

        let (chunks, summary) = pipeline
            .process(ini, "config.ini", &config, OutputMode::default())
            .expect("should process");

        assert!(!chunks.is_empty());
        let summary = summary.unwrap();
        assert_eq!(summary.doc_type, DocType::Config);
    }
}
