//! TOML summarizer
//!
//! Extracts only reliable structural metadata: title, root-level keys, line count.

use crate::common::summarizer::{GenericSummarizer, extract_root_keys, infer_title_from_filename};
use crate::toml::types::{TomlGroup, TomlNode, TomlNodeType};
use crate::types::{DocSummary, DocType};

/// TOML summarizer
#[derive(Clone)]
pub struct TomlSummarizer;

impl TomlSummarizer {
    /// Create a new TOML summarizer
    pub fn new() -> Self {
        Self
    }

    /// Get key name from a TOML node
    fn get_key(node: &TomlNode) -> Option<&str> {
        if let Some(ref key) = node.key {
            Some(key.as_str())
        } else if let TomlNodeType::Table { table_name } = &node.node_type {
            Some(table_name.as_str())
        } else if let TomlNodeType::ArrayTable { table_name, .. } = &node.node_type {
            Some(table_name.as_str())
        } else {
            None
        }
    }
}

impl Default for TomlSummarizer {
    fn default() -> Self {
        Self::new()
    }
}

impl GenericSummarizer<TomlNode, TomlGroup> for TomlSummarizer {
    fn doc_type(&self) -> DocType {
        DocType::Config
    }

    fn extract_title(&self, nodes: &[TomlNode], file_path: &str) -> Option<String> {
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

    fn extract_structural_entries(&self, nodes: &[TomlNode]) -> Vec<String> {
        extract_root_keys(nodes, Self::get_key)
    }

    fn count_lines(&self, nodes: &[TomlNode]) -> u32 {
        nodes.len() as u32
    }

    fn summarize(&self, nodes: &[TomlNode], groups: &[TomlGroup], file_path: &str) -> DocSummary {
        let mut summary = DocSummary::new(file_path.to_string(), self.doc_type());

        summary.title = self.extract_title(nodes, file_path);
        summary.main_headings = self.extract_structural_entries(nodes);
        summary.line_count = self.count_lines(nodes);

        let entry_count = summary.main_headings.len();
        summary.set_summary_text(Some(format!(
            "TOML config with {}{}",
            entry_count,
            if entry_count == 1 {
                " section".to_string()
            } else {
                " sections".to_string()
            }
        )));

        let _ = groups;
        summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toml::types::TomlValueType;
    use cce_types::Span;

    fn create_mock_node(id: &str, node_type: TomlNodeType, _path: &str) -> TomlNode {
        TomlNode::new(id.to_string(), node_type, String::new(), Span::default())
    }

    #[test]
    fn test_summarize_simple_toml() {
        let summarizer = TomlSummarizer::new();

        let root = create_mock_node("root", TomlNodeType::Root, "");
        let kv1 = create_mock_node(
            "kv1",
            TomlNodeType::KeyValue {
                key: "name".to_string(),
                value_type: TomlValueType::String,
            },
            "name",
        )
        .with_key("name".to_string())
        .with_value("test".to_string(), TomlValueType::String);

        let kv2 = create_mock_node(
            "kv2",
            TomlNodeType::KeyValue {
                key: "version".to_string(),
                value_type: TomlValueType::String,
            },
            "version",
        )
        .with_key("version".to_string())
        .with_value("1.0.0".to_string(), TomlValueType::String);

        let nodes = vec![root, kv1, kv2];
        let groups = vec![];

        let summary = summarizer.summarize(&nodes, &groups, "config.toml");

        assert!(summary.title.is_some());
        assert_eq!(summary.line_count, 3);
    }

    #[test]
    fn test_summarize_cargo_toml() {
        let summarizer = TomlSummarizer::new();

        let root = create_mock_node("root", TomlNodeType::Root, "");
        let table1 = create_mock_node(
            "table1",
            TomlNodeType::Table {
                table_name: "package".to_string(),
            },
            "package",
        );

        let kv1 = create_mock_node(
            "kv1",
            TomlNodeType::KeyValue {
                key: "name".to_string(),
                value_type: TomlValueType::String,
            },
            "package.name",
        )
        .with_key("name".to_string())
        .with_value("my-package".to_string(), TomlValueType::String);

        let table2 = create_mock_node(
            "table2",
            TomlNodeType::Table {
                table_name: "dependencies".to_string(),
            },
            "dependencies",
        );

        let nodes = vec![root, table1, kv1, table2];
        let groups = vec![];

        let summary = summarizer.summarize(&nodes, &groups, "Cargo.toml");

        assert_eq!(summary.title, Some("Cargo Configuration".to_string()));
        assert_eq!(summary.line_count, 4);
    }
}
