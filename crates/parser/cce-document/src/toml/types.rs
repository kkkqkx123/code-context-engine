//! TOML processing types
//!
//! This module provides types for TOML document processing.

use serde::{Deserialize, Serialize};

use crate::common::{DocumentNode, GenericGroup};
use cce_types::Span;

/// TOML node type
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TomlNodeType {
    /// Root node
    Root,
    /// Table node (e.g., [section])
    Table {
        /// Table name
        table_name: String,
    },
    /// Array table node (e.g., [[items]])
    ArrayTable {
        /// Table name
        table_name: String,
        /// Array index
        index: usize,
    },
    /// Key-value pair
    KeyValue {
        /// Key name
        key: String,
        /// Value type
        value_type: TomlValueType,
    },
    /// Array element
    ArrayElement {
        /// Element index
        index: usize,
        /// Value type
        value_type: TomlValueType,
    },
}

impl TomlNodeType {
    /// Check if this is a container node (table or array table)
    pub fn is_container(&self) -> bool {
        matches!(
            self,
            TomlNodeType::Table { .. } | TomlNodeType::ArrayTable { .. } | TomlNodeType::Root
        )
    }

    /// Check if this is a leaf node (key-value or array element with primitive value)
    pub fn is_leaf(&self) -> bool {
        matches!(
            self,
            TomlNodeType::KeyValue { .. } | TomlNodeType::ArrayElement { .. }
        )
    }

    /// Get the key name if this is a KeyValue node
    pub fn key(&self) -> Option<&str> {
        match self {
            TomlNodeType::KeyValue { key, .. } => Some(key),
            _ => None,
        }
    }

    /// Get the table name if this is a Table or ArrayTable node
    pub fn table_name(&self) -> Option<&str> {
        match self {
            TomlNodeType::Table { table_name } => Some(table_name),
            TomlNodeType::ArrayTable { table_name, .. } => Some(table_name),
            _ => None,
        }
    }
}

impl std::fmt::Display for TomlNodeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TomlNodeType::Root => write!(f, "root"),
            TomlNodeType::Table { table_name } => write!(f, "table({})", table_name),
            TomlNodeType::ArrayTable { table_name, index } => {
                write!(f, "array_table({}[{}])", table_name, index)
            }
            TomlNodeType::KeyValue { key, value_type } => {
                write!(f, "key_value({}: {})", key, value_type)
            }
            TomlNodeType::ArrayElement { index, value_type } => {
                write!(f, "array_element[{}]({})", index, value_type)
            }
        }
    }
}

/// TOML value type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TomlValueType {
    /// String value
    String,
    /// Integer value
    Integer,
    /// Float value
    Float,
    /// Boolean value
    Boolean,
    /// Date-time value
    DateTime,
    /// Array value
    Array,
    /// Inline table value
    InlineTable,
}

impl std::fmt::Display for TomlValueType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TomlValueType::String => write!(f, "string"),
            TomlValueType::Integer => write!(f, "integer"),
            TomlValueType::Float => write!(f, "float"),
            TomlValueType::Boolean => write!(f, "boolean"),
            TomlValueType::DateTime => write!(f, "datetime"),
            TomlValueType::Array => write!(f, "array"),
            TomlValueType::InlineTable => write!(f, "inline_table"),
        }
    }
}

impl TomlValueType {
    /// Check if this is a primitive type
    pub fn is_primitive(&self) -> bool {
        matches!(
            self,
            TomlValueType::String
                | TomlValueType::Integer
                | TomlValueType::Float
                | TomlValueType::Boolean
                | TomlValueType::DateTime
        )
    }

    /// Convert from toml::Value
    pub fn from_toml_value(value: &toml::Value) -> Self {
        match value {
            toml::Value::String(_) => TomlValueType::String,
            toml::Value::Integer(_) => TomlValueType::Integer,
            toml::Value::Float(_) => TomlValueType::Float,
            toml::Value::Boolean(_) => TomlValueType::Boolean,
            toml::Value::Datetime(_) => TomlValueType::DateTime,
            toml::Value::Array(_) => TomlValueType::Array,
            toml::Value::Table(_) => TomlValueType::InlineTable,
        }
    }
}

/// TOML node
///
/// Represents a node in the TOML tree structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TomlNode {
    /// Unique node ID
    pub id: String,
    /// Node type
    pub node_type: TomlNodeType,
    /// Key name (for KeyValue nodes)
    pub key: Option<String>,
    /// Value content (for leaf nodes)
    pub value: Option<String>,
    /// Value type
    pub value_type: TomlValueType,
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

impl TomlNode {
    /// Create a new TOML node
    pub fn new(id: String, node_type: TomlNodeType, path: String, span: Span) -> Self {
        let depth = path.split('.').filter(|s| !s.is_empty()).count();
        Self {
            id,
            node_type,
            key: None,
            value: None,
            value_type: TomlValueType::String,
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
    pub fn with_value(mut self, value: String, value_type: TomlValueType) -> Self {
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

    /// Generate text for embedding (original key=value format)
    pub fn to_embedding_text(&self) -> String {
        match &self.node_type {
            TomlNodeType::Root => "TOML configuration document.".to_string(),
            TomlNodeType::Table { table_name } => {
                if self.path.is_empty() {
                    "[root]".to_string()
                } else {
                    format!("[{}]", table_name)
                }
            }
            TomlNodeType::ArrayTable { table_name, index } => {
                format!("[[{}]] # entry {}", table_name, index)
            }
            TomlNodeType::KeyValue { key, value_type } => {
                if let Some(ref value) = self.value {
                    format!("{} = {}", key, format_value_inline_toml(value, *value_type))
                } else {
                    match value_type {
                        TomlValueType::Array => format!("{} = [array]", key),
                        TomlValueType::InlineTable => format!("{} = {{inline_table}}", key),
                        _ => format!("{} = {{...}}", key),
                    }
                }
            }
            TomlNodeType::ArrayElement { index, .. } => {
                if let Some(ref value) = self.value {
                    format!(
                        "[{}] = {}",
                        index,
                        format_value_inline_toml(value, self.value_type)
                    )
                } else {
                    format!("[{}] = {{...}}", index)
                }
            }
        }
    }

    /// Generate text for BM25 (retain structure)
    pub fn to_bm25_text(&self) -> String {
        match &self.node_type {
            TomlNodeType::Root => String::new(),
            TomlNodeType::Table { .. } => String::new(),
            TomlNodeType::ArrayTable { .. } => String::new(),
            TomlNodeType::KeyValue { key, .. } => {
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
            TomlNodeType::ArrayElement { index, .. } => {
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
fn format_value_for_bm25(value: &str, value_type: TomlValueType) -> String {
    match value_type {
        TomlValueType::String => format!("\"{}\"", value),
        _ => value.to_string(),
    }
}

/// Format value for inline TOML embedding text
fn format_value_inline_toml(value: &str, value_type: TomlValueType) -> String {
    match value_type {
        TomlValueType::String => format!("\"{}\"", value),
        _ => value.to_string(),
    }
}

/// TOML group type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TomlGroupType {
    /// Root table (flattened into KeyValueGroups)
    RootTable,
    /// Named table
    NamedTable,
    /// Array table element (each element is a separate group)
    ArrayTableElement,
    /// Key-value group (sibling key-value pairs)
    KeyValueGroup,
}

impl std::fmt::Display for TomlGroupType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TomlGroupType::RootTable => write!(f, "root_table"),
            TomlGroupType::NamedTable => write!(f, "named_table"),
            TomlGroupType::ArrayTableElement => write!(f, "array_table_element"),
            TomlGroupType::KeyValueGroup => write!(f, "key_value_group"),
        }
    }
}

impl TomlGroupType {
    /// Convert to DocGroupType for compatibility
    pub fn to_doc_group_type(&self) -> crate::types::DocGroupType {
        match self {
            TomlGroupType::RootTable => crate::types::DocGroupType::Chapter,
            TomlGroupType::NamedTable => crate::types::DocGroupType::Section,
            TomlGroupType::ArrayTableElement => crate::types::DocGroupType::ParagraphGroup,
            TomlGroupType::KeyValueGroup => crate::types::DocGroupType::ParagraphGroup,
        }
    }
}

/// TOML group
///
/// A group of related TOML nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TomlGroup {
    /// Group ID
    pub group_id: String,
    /// Group type
    pub group_type: TomlGroupType,
    /// Path prefix
    pub path_prefix: String,
    /// Header node (for table groups)
    pub header: Option<TomlNode>,
    /// Member nodes
    pub members: Vec<TomlNode>,
    /// Combined text for embedding
    pub embedding_text: String,
    /// Combined text for BM25
    pub bm25_text: String,
    /// Estimated token count
    pub token_count: usize,
    /// Source span
    pub span: Span,
}

impl TomlGroup {
    /// Create a new TOML group
    pub fn new(group_id: String, group_type: TomlGroupType, path_prefix: String) -> Self {
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

impl GenericGroup<TomlNode> for TomlGroup {
    fn group_id(&self) -> &str {
        &self.group_id
    }

    fn group_id_mut(&mut self) -> &mut String {
        &mut self.group_id
    }

    fn header(&self) -> Option<&TomlNode> {
        self.header.as_ref()
    }

    fn header_mut(&mut self) -> &mut Option<TomlNode> {
        &mut self.header
    }

    fn members(&self) -> &[TomlNode] {
        &self.members
    }

    fn members_mut(&mut self) -> &mut Vec<TomlNode> {
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

    fn node_to_embedding_text(node: &TomlNode) -> String {
        node.to_embedding_text()
    }

    fn node_to_bm25_text(node: &TomlNode) -> String {
        node.to_bm25_text()
    }

    fn node_id(node: &TomlNode) -> &str {
        &node.id
    }

    fn node_span(node: &TomlNode) -> &Span {
        &node.span
    }
}

impl DocumentNode for TomlNode {
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
    fn test_toml_value_type_from_toml() {
        use toml::Value;

        assert_eq!(
            TomlValueType::from_toml_value(&Value::String("test".to_string())),
            TomlValueType::String
        );
        assert_eq!(
            TomlValueType::from_toml_value(&Value::Integer(123)),
            TomlValueType::Integer
        );
        assert_eq!(
            TomlValueType::from_toml_value(&Value::Float(1.5)),
            TomlValueType::Float
        );
        assert_eq!(
            TomlValueType::from_toml_value(&Value::Boolean(true)),
            TomlValueType::Boolean
        );
        assert_eq!(
            TomlValueType::from_toml_value(&Value::Array(vec![])),
            TomlValueType::Array
        );
        assert_eq!(
            TomlValueType::from_toml_value(&Value::Table(toml::map::Map::new())),
            TomlValueType::InlineTable
        );
    }

    #[test]
    fn test_toml_node_embedding_text() {
        let node = TomlNode::new(
            "test".to_string(),
            TomlNodeType::KeyValue {
                key: "host".to_string(),
                value_type: TomlValueType::String,
            },
            "database.host".to_string(),
            Span::default(),
        )
        .with_key("host".to_string())
        .with_value("localhost".to_string(), TomlValueType::String);

        let text = node.to_embedding_text();
        assert!(text.contains("host"));
        assert!(text.contains("localhost"));
        assert!(text.contains('='));
    }

    #[test]
    fn test_toml_node_table_text() {
        let node = TomlNode::new(
            "test".to_string(),
            TomlNodeType::Table {
                table_name: "project".to_string(),
            },
            "project".to_string(),
            Span::default(),
        );

        let text = node.to_embedding_text();
        assert!(text.contains("project"));
        assert!(text.contains('['));
    }

    #[test]
    fn test_toml_node_key_value_without_value() {
        let node = TomlNode::new(
            "test".to_string(),
            TomlNodeType::KeyValue {
                key: "items".to_string(),
                value_type: TomlValueType::Array,
            },
            "items".to_string(),
            Span::default(),
        )
        .with_key("items".to_string());

        let text = node.to_embedding_text();
        assert!(text.contains("items"));
        assert!(text.contains("[array]"));
    }

    #[test]
    fn test_toml_node_array_table_text() {
        let node = TomlNode::new(
            "test".to_string(),
            TomlNodeType::ArrayTable {
                table_name: "products".to_string(),
                index: 0,
            },
            "products".to_string(),
            Span::default(),
        );

        let text = node.to_embedding_text();
        assert!(text.contains("products"));
        assert!(text.contains("[["));
        assert!(text.contains("entry 0"));
    }
}
