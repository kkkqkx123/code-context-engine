//! JSON document summarizer
//!
//! Extracts only reliable structural metadata: title, root-level keys, line count.

use crate::common::summarizer::{GenericSummarizer, extract_root_keys, infer_title_from_filename};
use crate::json::types::{JsonGroup, JsonNode};
use crate::types::{DocSummary, DocType};

/// JSON document summarizer
#[derive(Clone)]
pub struct JsonSummarizer;

impl JsonSummarizer {
    /// Create a new JSON summarizer
    pub fn new() -> Self {
        Self
    }

    /// Get key name from a JSON node
    fn get_key(node: &JsonNode) -> Option<&str> {
        node.key_name.as_deref()
    }
}

impl Default for JsonSummarizer {
    fn default() -> Self {
        Self::new()
    }
}

impl GenericSummarizer<JsonNode, JsonGroup> for JsonSummarizer {
    fn doc_type(&self) -> DocType {
        DocType::Config
    }

    fn extract_title(&self, nodes: &[JsonNode], file_path: &str) -> Option<String> {
        // Try filename-based inference first
        if let Some(title) = infer_title_from_filename(file_path) {
            return Some(title);
        }

        // Try to find a "name" key at root level
        for node in nodes.iter().filter(|n| n.depth == 1) {
            if node.key_name.as_deref() == Some("name") && node.value.is_some() {
                return node.value.clone();
            }
        }

        // Fall back to filename stem
        std::path::Path::new(file_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .map(String::from)
    }

    fn extract_structural_entries(&self, nodes: &[JsonNode]) -> Vec<String> {
        extract_root_keys(nodes, Self::get_key)
    }

    fn count_lines(&self, nodes: &[JsonNode]) -> u32 {
        nodes.len() as u32
    }

    fn summarize(&self, nodes: &[JsonNode], groups: &[JsonGroup], file_path: &str) -> DocSummary {
        let mut summary = DocSummary::new(file_path.to_string(), self.doc_type());

        summary.title = self.extract_title(nodes, file_path);
        summary.main_headings = self.extract_structural_entries(nodes);
        summary.line_count = self.count_lines(nodes);

        let entry_count = summary.main_headings.len();
        summary.set_summary_text(Some(format!(
            "JSON config with {}{}",
            entry_count,
            if entry_count == 1 {
                " key".to_string()
            } else {
                " keys".to_string()
            }
        )));

        let _ = groups;
        summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::types::{JsonGroupType, JsonNodeType, JsonValueType};
    use cce_types::Span;

    fn create_kv_node(key: &str, value: &str, depth: usize) -> JsonNode {
        let mut node = JsonNode::new(
            format!("node_{}", key),
            JsonNodeType::Primitive(JsonValueType::String),
            key.to_string(),
            Span::default(),
        );
        node.depth = depth;
        node.key_name = Some(key.to_string());
        node.value = Some(value.to_string());
        node
    }

    #[test]
    fn test_extract_root_keys() {
        let mut node1 = create_kv_node("name", "test", 1);
        node1.depth = 1;
        let mut node2 = create_kv_node("version", "1.0.0", 1);
        node2.depth = 1;
        let mut node3 = create_kv_node("nested", "", 2);
        node3.depth = 2;

        let nodes = vec![node1, node2, node3];

        let keys = extract_root_keys(&nodes, JsonSummarizer::get_key);

        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&"name".to_string()));
        assert!(keys.contains(&"version".to_string()));
    }

    #[test]
    fn test_summarize() {
        let nodes = vec![
            create_kv_node("name", "test-project", 1),
            create_kv_node("version", "1.0.0", 1),
        ];

        let groups = vec![JsonGroup::new(
            "test_group".to_string(),
            JsonGroupType::RootObject,
            String::new(),
        )];

        let summarizer = JsonSummarizer::new();
        let summary = summarizer.summarize(&nodes, &groups, "test.json");

        assert!(summary.title.is_some());
        assert!(!summary.main_headings.is_empty());
        assert_eq!(summary.line_count, 2);
    }

    #[test]
    fn test_infer_title() {
        let nodes = vec![create_kv_node("name", "my-package", 1)];

        let summarizer = JsonSummarizer::new();
        let title = summarizer.extract_title(&nodes, "package.json");
        assert_eq!(title, Some("Package Configuration".to_string()));

        let title = summarizer.extract_title(&nodes, "my-config.json");
        assert_eq!(title, Some("my-package".to_string()));
    }
}
