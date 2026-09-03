//! YAML processing types
//!
//! This module provides types for YAML document processing.

use serde::{Deserialize, Serialize};

use crate::common::{DocumentNode, GenericGroup};
use cce_types::Span;

/// YAML node type
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum YamlNodeType {
    /// Root node
    Root,
    /// Mapping node (e.g., key-value pairs)
    Mapping {
        /// Mapping key name
        key: String,
    },
    /// Sequence node (e.g., list items)
    Sequence {
        /// Sequence key name (if named)
        key: Option<String>,
    },
    /// Key-value pair
    KeyValue {
        /// Key name
        key: String,
        /// Value type
        value_type: YamlValueType,
    },
    /// Sequence element
    SequenceElement {
        /// Element index
        index: usize,
        /// Value type
        value_type: YamlValueType,
    },
}

impl YamlNodeType {
    /// Check if this is a container node (mapping or sequence)
    pub fn is_container(&self) -> bool {
        matches!(
            self,
            YamlNodeType::Root | YamlNodeType::Mapping { .. } | YamlNodeType::Sequence { .. }
        )
    }

    /// Check if this is a leaf node (key-value or sequence element with primitive value)
    pub fn is_leaf(&self) -> bool {
        matches!(
            self,
            YamlNodeType::KeyValue { .. } | YamlNodeType::SequenceElement { .. }
        )
    }

    /// Get the key name if this is a KeyValue or Mapping node
    pub fn key(&self) -> Option<&str> {
        match self {
            YamlNodeType::KeyValue { key, .. } => Some(key),
            YamlNodeType::Mapping { key } => Some(key),
            YamlNodeType::Sequence { key, .. } => key.as_deref(),
            _ => None,
        }
    }
}

impl std::fmt::Display for YamlNodeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            YamlNodeType::Root => write!(f, "root"),
            YamlNodeType::Mapping { key } => write!(f, "mapping({})", key),
            YamlNodeType::Sequence { key, .. } => {
                if let Some(k) = key {
                    write!(f, "sequence({})", k)
                } else {
                    write!(f, "sequence")
                }
            }
            YamlNodeType::KeyValue { key, value_type } => {
                write!(f, "key_value({}: {})", key, value_type)
            }
            YamlNodeType::SequenceElement { index, value_type } => {
                write!(f, "sequence_element[{}]({})", index, value_type)
            }
        }
    }
}

/// YAML value type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum YamlValueType {
    /// String value
    String,
    /// Integer value
    Integer,
    /// Float value
    Float,
    /// Boolean value
    Boolean,
    /// Null value
    Null,
    /// Array value
    Array,
    /// Mapping value
    Mapping,
}

impl std::fmt::Display for YamlValueType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            YamlValueType::String => write!(f, "string"),
            YamlValueType::Integer => write!(f, "integer"),
            YamlValueType::Float => write!(f, "float"),
            YamlValueType::Boolean => write!(f, "boolean"),
            YamlValueType::Null => write!(f, "null"),
            YamlValueType::Array => write!(f, "array"),
            YamlValueType::Mapping => write!(f, "mapping"),
        }
    }
}

impl YamlValueType {
    /// Check if this is a primitive type
    pub fn is_primitive(&self) -> bool {
        matches!(
            self,
            YamlValueType::String
                | YamlValueType::Integer
                | YamlValueType::Float
                | YamlValueType::Boolean
                | YamlValueType::Null
        )
    }

    /// Convert from yaml-rust2 Yaml value
    pub fn from_yaml_value(value: &yaml_rust2::Yaml) -> Self {
        match value {
            yaml_rust2::Yaml::String(_) => YamlValueType::String,
            yaml_rust2::Yaml::Integer(_) => YamlValueType::Integer,
            yaml_rust2::Yaml::Real(_) => YamlValueType::Float,
            yaml_rust2::Yaml::Boolean(_) => YamlValueType::Boolean,
            yaml_rust2::Yaml::Null => YamlValueType::Null,
            yaml_rust2::Yaml::Array(_) => YamlValueType::Array,
            yaml_rust2::Yaml::Hash(_) => YamlValueType::Mapping,
            yaml_rust2::Yaml::Alias(_) => YamlValueType::String,
            yaml_rust2::Yaml::BadValue => YamlValueType::String,
        }
    }
}

/// YAML node
///
/// Represents a node in the YAML tree structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YamlNode {
    /// Unique node ID
    pub id: String,
    /// Node type
    pub node_type: YamlNodeType,
    /// Key name (for KeyValue nodes)
    pub key: Option<String>,
    /// Value content (for leaf nodes)
    pub value: Option<String>,
    /// Value type
    pub value_type: YamlValueType,
    /// Full path (e.g., "section.subsection.key")
    pub path: String,
    /// Depth level in tree
    pub depth: usize,
    /// Parent node ID
    pub parent_id: Option<String>,
    /// Children node IDs
    pub children: Vec<String>,
    /// Source span
    pub span: Span,
}

impl YamlNode {
    /// Create a new YAML node
    pub fn new(id: String, node_type: YamlNodeType, path: String, span: Span) -> Self {
        let depth = path.split('.').filter(|s| !s.is_empty()).count();
        Self {
            id,
            node_type,
            key: None,
            value: None,
            value_type: YamlValueType::String,
            path,
            depth,
            parent_id: None,
            children: Vec::new(),
            span,
        }
    }

    /// Set the key name
    pub fn with_key(mut self, key: String) -> Self {
        self.key = Some(key);
        self
    }

    /// Set the value
    pub fn with_value(mut self, value: String, value_type: YamlValueType) -> Self {
        self.value = Some(value);
        self.value_type = value_type;
        self
    }

    /// Set the parent ID
    pub fn with_parent(mut self, parent_id: String) -> Self {
        self.parent_id = Some(parent_id);
        self
    }

    /// Add a child node ID
    pub fn add_child(&mut self, child_id: String) {
        if !self.children.contains(&child_id) {
            self.children.push(child_id);
        }
    }

    /// Check if this node has children
    pub fn has_children(&self) -> bool {
        !self.children.is_empty()
    }

    /// Generate text for embedding (original key:value format)
    pub fn to_embedding_text(&self) -> String {
        match &self.node_type {
            YamlNodeType::Root => "YAML configuration document.".to_string(),
            YamlNodeType::Mapping { key } => {
                if self.path.is_empty() {
                    "{root}".to_string()
                } else {
                    format!("{}: {{mapping}}", key)
                }
            }
            YamlNodeType::Sequence { key, .. } => {
                if let Some(k) = key {
                    format!("{}: [sequence]", k)
                } else {
                    "[sequence]".to_string()
                }
            }
            YamlNodeType::KeyValue { key, value_type } => {
                if let Some(ref value) = self.value {
                    format!("{}: {}", key, format_value_inline_yaml(value, *value_type))
                } else {
                    match value_type {
                        YamlValueType::Mapping => format!("{}: {{mapping}}", key),
                        YamlValueType::Array => format!("{}: [sequence]", key),
                        _ => format!("{}: {{...}}", key),
                    }
                }
            }
            YamlNodeType::SequenceElement { index, .. } => {
                if let Some(ref value) = self.value {
                    format!(
                        "- [{}]: {}",
                        index,
                        format_value_inline_yaml(value, self.value_type)
                    )
                } else {
                    format!("- [{}]: {{...}}", index)
                }
            }
        }
    }

    /// Generate text for BM25 (retain structure)
    pub fn to_bm25_text(&self) -> String {
        match &self.node_type {
            YamlNodeType::Root => String::new(),
            YamlNodeType::Mapping { .. } => String::new(),
            YamlNodeType::Sequence { .. } => String::new(),
            YamlNodeType::KeyValue { key, .. } => {
                if let Some(ref value) = self.value {
                    let parent_path = self
                        .path
                        .rsplit_once('.')
                        .map(|(p, _)| p.to_string())
                        .unwrap_or_default();
                    let dotted_form = if parent_path.is_empty() {
                        key.clone()
                    } else {
                        format!("{}.{}", parent_path, key)
                    };
                    let spaced_form = if parent_path.is_empty() {
                        key.clone()
                    } else {
                        format!("{} {}", parent_path, key)
                    };
                    format!(
                        "{} {} = {}",
                        dotted_form,
                        spaced_form,
                        format_value_for_bm25(value, self.value_type)
                    )
                } else {
                    key.clone()
                }
            }
            YamlNodeType::SequenceElement { index, .. } => {
                if let Some(ref value) = self.value {
                    let spaced_path = self.path.replace('[', " ").replace(']', "");
                    format!(
                        "{} {} = {}",
                        self.path,
                        spaced_path,
                        format_value_for_bm25(value, self.value_type)
                    )
                } else {
                    format!("{}[{}]", self.path, index)
                }
            }
        }
    }
}

/// Format value for BM25 text
fn format_value_for_bm25(value: &str, value_type: YamlValueType) -> String {
    match value_type {
        YamlValueType::String => format!("\"{}\"", value),
        _ => value.to_string(),
    }
}

/// Format value for inline YAML embedding text
fn format_value_inline_yaml(value: &str, value_type: YamlValueType) -> String {
    match value_type {
        YamlValueType::String => format!("\"{}\"", value),
        _ => value.to_string(),
    }
}

/// YAML group type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum YamlGroupType {
    /// Root mapping (flattened into KeyValueGroups)
    RootMapping,
    /// Named mapping
    NamedMapping,
    /// Sequence element (each element is a separate group)
    SequenceElement,
    /// Key-value group (sibling key-value pairs)
    KeyValueGroup,
}

impl std::fmt::Display for YamlGroupType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            YamlGroupType::RootMapping => write!(f, "root_mapping"),
            YamlGroupType::NamedMapping => write!(f, "named_mapping"),
            YamlGroupType::SequenceElement => write!(f, "sequence_element"),
            YamlGroupType::KeyValueGroup => write!(f, "key_value_group"),
        }
    }
}

impl YamlGroupType {
    /// Convert to DocGroupType for compatibility
    pub fn to_doc_group_type(&self) -> crate::types::DocGroupType {
        match self {
            YamlGroupType::RootMapping => crate::types::DocGroupType::Chapter,
            YamlGroupType::NamedMapping => crate::types::DocGroupType::Section,
            YamlGroupType::SequenceElement => crate::types::DocGroupType::ParagraphGroup,
            YamlGroupType::KeyValueGroup => crate::types::DocGroupType::ParagraphGroup,
        }
    }
}

/// YAML group
///
/// A group of related YAML nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YamlGroup {
    /// Group ID
    pub group_id: String,
    /// Group type
    pub group_type: YamlGroupType,
    /// Path prefix
    pub path_prefix: String,
    /// Header node (for mapping groups)
    pub header: Option<YamlNode>,
    /// Member nodes
    pub members: Vec<YamlNode>,
    /// Combined text for embedding
    pub embedding_text: String,
    /// Combined text for BM25
    pub bm25_text: String,
    /// Estimated token count
    pub token_count: usize,
    /// Source span
    pub span: Span,
}

impl YamlGroup {
    /// Create a new YAML group
    pub fn new(group_id: String, group_type: YamlGroupType, path_prefix: String) -> Self {
        Self {
            group_id,
            group_type,
            path_prefix,
            header: None,
            members: Vec::new(),
            embedding_text: String::new(),
            bm25_text: String::new(),
            token_count: 0,
            span: Span::default(),
        }
    }
}

impl GenericGroup<YamlNode> for YamlGroup {
    fn group_id(&self) -> &str {
        &self.group_id
    }

    fn group_id_mut(&mut self) -> &mut String {
        &mut self.group_id
    }

    fn header(&self) -> Option<&YamlNode> {
        self.header.as_ref()
    }

    fn header_mut(&mut self) -> &mut Option<YamlNode> {
        &mut self.header
    }

    fn members(&self) -> &[YamlNode] {
        &self.members
    }

    fn members_mut(&mut self) -> &mut Vec<YamlNode> {
        &mut self.members
    }

    fn embedding_text(&self) -> &str {
        &self.embedding_text
    }

    fn embedding_text_mut(&mut self) -> &mut String {
        &mut self.embedding_text
    }

    fn bm25_text(&self) -> &str {
        &self.bm25_text
    }

    fn bm25_text_mut(&mut self) -> &mut String {
        &mut self.bm25_text
    }

    fn token_count(&self) -> usize {
        self.token_count
    }

    fn token_count_mut(&mut self) -> &mut usize {
        &mut self.token_count
    }

    fn span(&self) -> &Span {
        &self.span
    }

    fn span_mut(&mut self) -> &mut Span {
        &mut self.span
    }

    fn node_to_embedding_text(node: &YamlNode) -> String {
        node.to_embedding_text()
    }

    fn node_to_bm25_text(node: &YamlNode) -> String {
        node.to_bm25_text()
    }

    fn node_id(node: &YamlNode) -> &str {
        &node.id
    }

    fn node_span(node: &YamlNode) -> &Span {
        &node.span
    }
}

impl DocumentNode for YamlNode {
    fn span(&self) -> &Span {
        &self.span
    }

    fn depth(&self) -> usize {
        self.depth
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_yaml_value_type_from_yaml() {
        use yaml_rust2::Yaml;

        assert_eq!(
            YamlValueType::from_yaml_value(&Yaml::String("test".to_string())),
            YamlValueType::String
        );
        assert_eq!(
            YamlValueType::from_yaml_value(&Yaml::Integer(123)),
            YamlValueType::Integer
        );
        assert_eq!(
            YamlValueType::from_yaml_value(&Yaml::Real("1.5".to_string())),
            YamlValueType::Float
        );
        assert_eq!(
            YamlValueType::from_yaml_value(&Yaml::Boolean(true)),
            YamlValueType::Boolean
        );
        assert_eq!(
            YamlValueType::from_yaml_value(&Yaml::Null),
            YamlValueType::Null
        );
        assert_eq!(
            YamlValueType::from_yaml_value(&Yaml::Array(vec![])),
            YamlValueType::Array
        );
        assert_eq!(
            YamlValueType::from_yaml_value(&Yaml::Hash(yaml_rust2::yaml::Hash::new())),
            YamlValueType::Mapping
        );
    }

    #[test]
    fn test_yaml_node_embedding_text() {
        let node = YamlNode::new(
            "test".to_string(),
            YamlNodeType::KeyValue {
                key: "host".to_string(),
                value_type: YamlValueType::String,
            },
            "database.host".to_string(),
            Span::default(),
        )
        .with_key("host".to_string())
        .with_value("localhost".to_string(), YamlValueType::String);

        let text = node.to_embedding_text();
        assert!(text.contains("host"));
        assert!(text.contains("localhost"));
        assert!(text.contains(':'));
    }

    #[test]
    fn test_yaml_node_mapping_text() {
        let node = YamlNode::new(
            "test".to_string(),
            YamlNodeType::Mapping {
                key: "database".to_string(),
            },
            "database".to_string(),
            Span::default(),
        );

        let text = node.to_embedding_text();
        assert!(text.contains("database"));
        assert!(text.contains("{mapping}"));
    }

    #[test]
    fn test_yaml_node_key_value_without_value() {
        let node = YamlNode::new(
            "test".to_string(),
            YamlNodeType::KeyValue {
                key: "config".to_string(),
                value_type: YamlValueType::Mapping,
            },
            "config".to_string(),
            Span::default(),
        )
        .with_key("config".to_string());

        let text = node.to_embedding_text();
        assert!(text.contains("config"));
        assert!(text.contains("{mapping}"));
    }

    #[test]
    fn test_yaml_node_sequence_text() {
        let node = YamlNode::new(
            "test".to_string(),
            YamlNodeType::Sequence {
                key: Some("items".to_string()),
            },
            "items".to_string(),
            Span::default(),
        );

        let text = node.to_embedding_text();
        assert!(text.contains("items"));
        assert!(text.contains("[sequence]"));
    }
}
