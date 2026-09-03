//! Markdown document summarizer
//!
//! Extracts only reliable structural metadata from Markdown documents:
//! title, heading list, heading/block counts, line count.

use crate::types::{DocGroup, DocGroupType, DocNode, DocNodeType, DocSummary, DocType};

/// Document summarizer
#[derive(Clone)]
pub struct DocSummarizer;

impl DocSummarizer {
    /// Create a new document summarizer
    pub fn new() -> Self {
        Self
    }

    /// Generate a summary from parsed nodes and groups
    pub fn summarize(&self, nodes: &[DocNode], groups: &[DocGroup], file_path: &str) -> DocSummary {
        let mut summary = DocSummary::new(file_path.to_string(), DocType::Markdown);

        // Extract title (first level-1 heading)
        summary.title = self.extract_title(nodes);

        // Count headings and code blocks
        for node in nodes {
            match &node.node_type {
                DocNodeType::Heading { .. } => summary.heading_count += 1,
                DocNodeType::CodeBlock { .. } => summary.code_block_count += 1,
                _ => {}
            }
        }

        // Extract main headings
        summary.main_headings = self.extract_main_headings(groups);

        // Count lines
        summary.line_count = nodes
            .iter()
            .map(|n| {
                n.span
                    .end_position
                    .row
                    .saturating_sub(n.span.start_position.row)
            })
            .sum::<usize>() as u32;

        summary.set_summary_text(self.build_summary_text(nodes, &summary));

        summary
    }

    /// Extract document title (first level-1 heading)
    fn extract_title(&self, nodes: &[DocNode]) -> Option<String> {
        nodes
            .iter()
            .find(|n| matches!(n.node_type, DocNodeType::Heading { level: 1 }))
            .map(|n| n.content.clone())
    }

    /// Extract main headings (up to 10)
    fn extract_main_headings(&self, groups: &[DocGroup]) -> Vec<String> {
        groups
            .iter()
            .filter(|g| matches!(g.group_type, DocGroupType::Chapter | DocGroupType::Section))
            .filter_map(|g| g.header.as_ref())
            .map(|h| h.content.clone())
            .take(10)
            .collect()
    }

    fn build_summary_text(&self, nodes: &[DocNode], summary: &DocSummary) -> Option<String> {
        let first_paragraph = nodes
            .iter()
            .skip_while(|n| {
                matches!(
                    n.node_type,
                    DocNodeType::Heading { .. } | DocNodeType::CodeBlock { .. }
                )
            })
            .find(|n| matches!(n.node_type, DocNodeType::Paragraph | DocNodeType::Text))
            .map(|n| {
                let text = n.content.trim().replace('\n', " ");
                if text.len() > 200 {
                    format!("{}…", &text[..200])
                } else {
                    text
                }
            })
            .filter(|text| !text.is_empty());

        if let Some(text) = first_paragraph {
            return Some(text);
        }

        let heading_count = summary.heading_count;
        match (&summary.title, heading_count) {
            (Some(title), 0) => Some(title.clone()),
            (Some(title), count) => Some(format!("{} ({} headings)", title, count)),
            (None, count) if count > 0 => Some(format!("{} headings", count)),
            _ => None,
        }
    }
}

impl Default for DocSummarizer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::group::GenericGroup;
    use cce_types::Span;

    fn create_heading_node(level: usize, content: &str) -> DocNode {
        let mut node = DocNode::new(
            format!("h{}", level),
            DocNodeType::Heading { level },
            content.to_string(),
            Span::default(),
        );
        node.depth = level;
        node
    }

    fn create_code_block(language: Option<&str>, content: &str) -> DocNode {
        DocNode::new(
            "code".to_string(),
            DocNodeType::CodeBlock {
                language: language.map(|s| s.to_string()),
            },
            content.to_string(),
            Span::default(),
        )
    }

    #[test]
    fn test_extract_title() {
        let nodes = vec![
            create_heading_node(1, "Document Title"),
            create_heading_node(2, "Section"),
        ];

        let summarizer = DocSummarizer::new();
        let title = summarizer.extract_title(&nodes);

        assert_eq!(title, Some("Document Title".to_string()));
    }

    #[test]
    fn test_title_from_filename_when_no_h1() {
        let nodes = vec![create_heading_node(2, "Section")];
        let summarizer = DocSummarizer::new();
        let title = summarizer.extract_title(&nodes);

        assert_eq!(title, None);
    }

    #[test]
    fn test_extract_main_headings() {
        let mut chapter = DocGroup::new("g1".to_string(), DocGroupType::Chapter);
        chapter.set_header(create_heading_node(1, "Chapter 1"));

        let mut section = DocGroup::new("g2".to_string(), DocGroupType::Section);
        section.set_header(create_heading_node(2, "Section A"));

        let groups = vec![chapter, section];
        let summarizer = DocSummarizer::new();
        let headings = summarizer.extract_main_headings(&groups);

        assert_eq!(
            headings,
            vec!["Chapter 1".to_string(), "Section A".to_string()]
        );
    }

    #[test]
    fn test_count_elements() {
        let nodes = vec![
            create_heading_node(1, "Title"),
            create_code_block(Some("rust"), "fn main() {}"),
            create_heading_node(2, "Section"),
            create_code_block(None, "some text"),
        ];

        let summarizer = DocSummarizer::new();
        let groups = vec![];
        let summary = summarizer.summarize(&nodes, &groups, "test.md");

        assert_eq!(summary.heading_count, 2);
        assert_eq!(summary.code_block_count, 2);
        assert_eq!(summary.title, Some("Title".to_string()));
        // line_count uses Span positions; default spans have row=0
        assert!(summary.line_count <= 4);
    }
}
