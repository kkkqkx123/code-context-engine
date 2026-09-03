//! XML document summarizer
//!
//! Extracts only reliable structural metadata: title, root-level tags, line count.

use crate::common::summarizer::{GenericSummarizer, extract_root_keys, infer_title_from_filename};
use crate::types::{DocSummary, DocType};
use crate::xml::types::{XmlGroup, XmlNode};

/// XML document summarizer
#[derive(Clone)]
pub struct XmlSummarizer;

impl XmlSummarizer {
    /// Create a new XML summarizer
    pub fn new() -> Self {
        Self
    }

    /// Get tag name from an XML node
    fn get_key(node: &XmlNode) -> Option<&str> {
        node.tag.as_deref()
    }
}

impl Default for XmlSummarizer {
    fn default() -> Self {
        Self::new()
    }
}

impl GenericSummarizer<XmlNode, XmlGroup> for XmlSummarizer {
    fn doc_type(&self) -> DocType {
        DocType::Xml
    }

    fn extract_title(&self, nodes: &[XmlNode], file_path: &str) -> Option<String> {
        // Try filename-based inference first
        if let Some(title) = infer_title_from_filename(file_path) {
            return Some(title);
        }

        // Try to infer from root tag
        let root_tag = nodes
            .iter()
            .filter(|n| n.depth == 1 && n.tag.is_some())
            .filter_map(|n| n.tag.as_deref())
            .next();

        if let Some(tag) = root_tag {
            return Some(tag.to_string());
        }

        // Fall back to filename stem
        std::path::Path::new(file_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .map(String::from)
    }

    fn extract_structural_entries(&self, nodes: &[XmlNode]) -> Vec<String> {
        extract_root_keys(nodes, Self::get_key)
    }

    fn count_lines(&self, nodes: &[XmlNode]) -> u32 {
        nodes.len() as u32
    }

    fn summarize(&self, nodes: &[XmlNode], groups: &[XmlGroup], file_path: &str) -> DocSummary {
        let mut summary = DocSummary::new(file_path.to_string(), self.doc_type());

        summary.title = self.extract_title(nodes, file_path);
        summary.main_headings = self.extract_structural_entries(nodes);
        summary.line_count = self.count_lines(nodes);

        let entry_count = summary.main_headings.len();
        summary.set_summary_text(Some(format!(
            "XML document with {} root element{}",
            entry_count,
            if entry_count == 1 { "" } else { "s" }
        )));

        let _ = groups;
        summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml::types::{XmlGroupType, XmlNodeType};
    use cce_types::Span;

    fn create_element_node(tag: &str, depth: usize) -> XmlNode {
        let mut node = XmlNode::new(
            format!("node_{}", tag),
            XmlNodeType::Element {
                tag: tag.to_string(),
            },
            tag.to_string(),
            Span::default(),
        );
        node.depth = depth;
        node.tag = Some(tag.to_string());
        node
    }

    #[test]
    fn test_extract_root_tags() {
        let mut node1 = create_element_node("root", 1);
        node1.depth = 1;
        let mut node2 = create_element_node("child", 2);
        node2.depth = 2;

        let nodes = vec![node1, node2];

        let tags = extract_root_keys(&nodes, XmlSummarizer::get_key);

        assert_eq!(tags.len(), 1);
        assert!(tags.contains(&"root".to_string()));
    }

    #[test]
    fn test_summarize() {
        let mut node1 = create_element_node("config", 1);
        node1.tag = Some("config".to_string());
        let mut node2 = create_element_node("database", 2);
        node2.tag = Some("database".to_string());

        let nodes = vec![node1, node2];
        let groups = vec![XmlGroup::new(
            "test_group".to_string(),
            XmlGroupType::RootElement,
            String::new(),
        )];

        let summarizer = XmlSummarizer::new();
        let summary = summarizer.summarize(&nodes, &groups, "config.xml");

        assert!(summary.title.is_some());
        assert!(!summary.main_headings.is_empty());
        assert_eq!(summary.line_count, 2);
    }

    #[test]
    fn test_infer_title() {
        let mut node = create_element_node("root", 1);
        node.depth = 1;
        node.tag = Some("root".to_string());
        let nodes = vec![node];

        let summarizer = XmlSummarizer::new();
        let title = summarizer.extract_title(&nodes, "pom.xml");
        assert_eq!(title, Some("Maven POM".to_string()));

        let title = summarizer.extract_title(&nodes, "my-config.xml");
        assert_eq!(title, Some("root".to_string()));
    }
}
