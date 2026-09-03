//! YAML summarizer
//!
//! Extracts only reliable structural metadata: title, root-level keys, line count.

use crate::common::summarizer::{GenericSummarizer, extract_root_keys, infer_title_from_filename};
use crate::types::{DocSummary, DocType};
use crate::yaml::types::{YamlGroup, YamlNode, YamlNodeType};

/// YAML summarizer
#[derive(Clone)]
pub struct YamlSummarizer;

impl YamlSummarizer {
    /// Create a new YAML summarizer
    pub fn new() -> Self {
        Self
    }

    /// Get key name from a YAML node
    fn get_key(node: &YamlNode) -> Option<&str> {
        if let Some(ref key) = node.key {
            Some(key.as_str())
        } else if let YamlNodeType::Mapping { key } = &node.node_type {
            Some(key.as_str())
        } else if let YamlNodeType::Sequence { key } = &node.node_type {
            key.as_deref()
        } else {
            None
        }
    }
}

impl Default for YamlSummarizer {
    fn default() -> Self {
        Self::new()
    }
}

impl GenericSummarizer<YamlNode, YamlGroup> for YamlSummarizer {
    fn doc_type(&self) -> DocType {
        DocType::Config
    }

    fn extract_title(&self, nodes: &[YamlNode], file_path: &str) -> Option<String> {
        // Try filename-based inference first
        if let Some(title) = infer_title_from_filename(file_path) {
            return Some(title);
        }

        // Try to find a "name" key at root level
        for node in nodes.iter().filter(|n| n.depth == 1) {
            if node.key.as_deref() == Some("name") && node.value.is_some() {
                return node.value.clone();
            }
        }

        // Fall back to filename stem
        std::path::Path::new(file_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .map(String::from)
    }

    fn extract_structural_entries(&self, nodes: &[YamlNode]) -> Vec<String> {
        extract_root_keys(nodes, Self::get_key)
    }

    fn count_lines(&self, nodes: &[YamlNode]) -> u32 {
        nodes.len() as u32
    }

    fn summarize(&self, nodes: &[YamlNode], groups: &[YamlGroup], file_path: &str) -> DocSummary {
        let mut summary = DocSummary::new(file_path.to_string(), self.doc_type());

        summary.title = self.extract_title(nodes, file_path);
        summary.main_headings = self.extract_structural_entries(nodes);
        summary.line_count = self.count_lines(nodes);

        let entry_count = summary.main_headings.len();
        summary.set_summary_text(Some(format!(
            "YAML config with {}{}",
            entry_count,
            if entry_count == 1 {
                " document".to_string()
            } else {
                " documents".to_string()
            }
        )));

        let _ = groups;
        summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::yaml::types::YamlValueType;
    use cce_types::Span;

    fn create_mock_node(id: &str, node_type: YamlNodeType, _path: &str) -> YamlNode {
        YamlNode::new(id.to_string(), node_type, String::new(), Span::default())
    }

    #[test]
    fn test_summarize_simple_yaml() {
        let summarizer = YamlSummarizer::new();

        let root = create_mock_node("root", YamlNodeType::Root, "");
        let kv1 = create_mock_node(
            "kv1",
            YamlNodeType::KeyValue {
                key: "name".to_string(),
                value_type: YamlValueType::String,
            },
            "name",
        )
        .with_key("name".to_string())
        .with_value("test".to_string(), YamlValueType::String);

        let kv2 = create_mock_node(
            "kv2",
            YamlNodeType::KeyValue {
                key: "version".to_string(),
                value_type: YamlValueType::String,
            },
            "version",
        )
        .with_key("version".to_string())
        .with_value("1.0.0".to_string(), YamlValueType::String);

        let nodes = vec![root, kv1, kv2];
        let groups = vec![];

        let summary = summarizer.summarize(&nodes, &groups, "config.yaml");

        assert!(summary.title.is_some());
        assert_eq!(summary.line_count, 3);
    }

    #[test]
    fn test_summarize_docker_compose() {
        let summarizer = YamlSummarizer::new();

        let root = create_mock_node("root", YamlNodeType::Root, "");
        let kv1 = create_mock_node(
            "kv1",
            YamlNodeType::KeyValue {
                key: "version".to_string(),
                value_type: YamlValueType::String,
            },
            "version",
        )
        .with_key("version".to_string())
        .with_value("3".to_string(), YamlValueType::String);

        let mapping1 = create_mock_node(
            "mapping1",
            YamlNodeType::Mapping {
                key: "services".to_string(),
            },
            "services",
        );

        let nodes = vec![root, kv1, mapping1];
        let groups = vec![];

        let summary = summarizer.summarize(&nodes, &groups, "docker-compose.yml");

        assert_eq!(
            summary.title,
            Some("Docker Compose Configuration".to_string())
        );
        assert_eq!(summary.line_count, 3);
    }
}
