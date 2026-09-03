//! JSON processing types
//!
//! This module provides types for JSON document processing.

use serde::{Deserialize, Serialize};

use crate::common::{DocumentNode, GenericGroup};
use cce_types::Span;

/// JSON node type
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum JsonNodeType {
    /// Root node
    Root,
    /// Object node (container)
    Object,
    /// Array node (container)
    Array,
    /// Primitive value node (string, number, boolean, null)
    Primitive(JsonValueType),
}

impl JsonNodeType {
    /// Check if this is a container node (object or array)
    pub fn is_container(&self) -> bool {
        matches!(
            self,
            JsonNodeType::Object | JsonNodeType::Array | JsonNodeType::Root
        )
    }

    /// Check if this is a leaf node (primitive value)
    pub fn is_leaf(&self) -> bool {
        matches!(self, JsonNodeType::Primitive(_))
    }

    /// Get the value type if this is a primitive node
    pub fn value_type(&self) -> Option<JsonValueType> {
        match self {
            JsonNodeType::Primitive(vt) => Some(*vt),
            _ => None,
        }
    }
}

impl std::fmt::Display for JsonNodeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JsonNodeType::Root => write!(f, "root"),
            JsonNodeType::Object => write!(f, "object"),
            JsonNodeType::Array => write!(f, "array"),
            JsonNodeType::Primitive(vt) => write!(f, "primitive({})", vt),
        }
    }
}

/// JSON value type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum JsonValueType {
    /// String value
    String,
    /// Number value
    Number,
    /// Boolean value
    Boolean,
    /// Null value
    Null,
    /// Object value
    Object,
    /// Array value
    Array,
}

impl std::fmt::Display for JsonValueType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JsonValueType::String => write!(f, "string"),
            JsonValueType::Number => write!(f, "number"),
            JsonValueType::Boolean => write!(f, "boolean"),
            JsonValueType::Null => write!(f, "null"),
            JsonValueType::Object => write!(f, "object"),
            JsonValueType::Array => write!(f, "array"),
        }
    }
}

impl JsonValueType {
    /// Check if this is a primitive type
    pub fn is_primitive(&self) -> bool {
        matches!(
            self,
            JsonValueType::String
                | JsonValueType::Number
                | JsonValueType::Boolean
                | JsonValueType::Null
        )
    }

    /// Convert from serde_json::Value
    pub fn from_json_value(value: &serde_json::Value) -> Self {
        match value {
            serde_json::Value::Null => JsonValueType::Null,
            serde_json::Value::Bool(_) => JsonValueType::Boolean,
            serde_json::Value::Number(_) => JsonValueType::Number,
            serde_json::Value::String(_) => JsonValueType::String,
            serde_json::Value::Array(_) => JsonValueType::Array,
            serde_json::Value::Object(_) => JsonValueType::Object,
        }
    }
}

/// JSON node
///
/// Represents a node in the JSON tree structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonNode {
    /// Unique node ID
    pub id: String,
    /// Node type
    pub node_type: JsonNodeType,
    /// Key name (for object members)
    pub key_name: Option<String>,
    /// Array index (for array elements)
    pub array_index: Option<usize>,
    /// Value content (for primitive nodes)
    pub value: Option<String>,
    /// Full path (e.g., "database.connection.host")
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

impl JsonNode {
    /// Create a new JSON node
    pub fn new(id: String, node_type: JsonNodeType, path: String, span: Span) -> Self {
        let depth = path.split('.').filter(|s| !s.is_empty()).count();
        Self {
            id,
            node_type,
            key_name: None,
            array_index: None,
            value: None,
            path,
            depth,
            parent_id: None,
            children: Vec::new(),
            span,
        }
    }

    /// Set the key name (for object members)
    pub fn with_key_name(mut self, key: String) -> Self {
        self.key_name = Some(key);
        self
    }

    /// Set the array index (for array elements)
    pub fn with_array_index(mut self, index: usize) -> Self {
        self.array_index = Some(index);
        self
    }

    /// Set the value (for primitive nodes)
    pub fn with_value(mut self, value: String) -> Self {
        self.value = Some(value);
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
            JsonNodeType::Root => "JSON root document".to_string(),
            JsonNodeType::Object => {
                if self.path.is_empty() {
                    "{root}".to_string()
                } else {
                    format!("\"{}\": {{object}}", self.path)
                }
            }
            JsonNodeType::Array => {
                if self.path.is_empty() {
                    "[root]".to_string()
                } else {
                    format!("\"{}\": [array]", self.path)
                }
            }
            JsonNodeType::Primitive(_) => {
                let value_type = self.node_type.value_type().unwrap_or(JsonValueType::String);
                if let Some(ref key) = self.key_name {
                    if let Some(ref value) = self.value {
                        format!("\"{}\": {}", key, format_value_inline(value, value_type))
                    } else {
                        match value_type {
                            JsonValueType::Object => format!("\"{}\": {{object}}", key),
                            JsonValueType::Array => format!("\"{}\": [array]", key),
                            _ => format!("\"{}\": {{...}}", key),
                        }
                    }
                } else if let Some(idx) = self.array_index {
                    if let Some(ref value) = self.value {
                        format!("[{}]: {}", idx, format_value_inline(value, value_type))
                    } else {
                        format!("[{}]: {{...}}", idx)
                    }
                } else {
                    self.value.clone().unwrap_or_default()
                }
            }
        }
    }

    /// Generate text for BM25 (retain structure)
    pub fn to_bm25_text(&self) -> String {
        match &self.node_type {
            JsonNodeType::Root | JsonNodeType::Object | JsonNodeType::Array => String::new(),
            JsonNodeType::Primitive(_) => {
                if let Some(ref key) = self.key_name {
                    if let Some(ref value) = self.value {
                        // Dual representation for better search
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
                        let value_form = format_value_for_bm25(
                            value,
                            self.node_type.value_type().unwrap_or(JsonValueType::String),
                        );

                        format!("{} {} = {}", dotted_form, spaced_form, value_form)
                    } else {
                        key.clone()
                    }
                } else if let Some(_idx) = self.array_index {
                    if let Some(ref value) = self.value {
                        // Dual representation for array elements
                        let bracketed = self.path.clone();
                        let spaced = self.path.replace('[', " ").replace(']', "");
                        let value_form = format_value_for_bm25(
                            value,
                            self.node_type.value_type().unwrap_or(JsonValueType::String),
                        );

                        format!("{} {} = {}", bracketed, spaced, value_form)
                    } else {
                        self.path.clone()
                    }
                } else {
                    self.value.clone().unwrap_or_default()
                }
            }
        }
    }
}

/// Format value for BM25 text
fn format_value_for_bm25(value: &str, value_type: JsonValueType) -> String {
    match value_type {
        JsonValueType::String => format!("\"{}\"", value),
        _ => value.to_string(),
    }
}

/// Format value for inline embedding text (without quotes for non-strings)
fn format_value_inline(value: &str, value_type: JsonValueType) -> String {
    match value_type {
        JsonValueType::String => format!("\"{}\"", value),
        _ => value.to_string(),
    }
}

/// JSON group type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum JsonGroupType {
    /// Root object
    RootObject,
    /// Nested object
    NestedObject,
    /// Array (legacy - kept for backward compatibility)
    Array,
    /// Array element (each element is a separate group)
    ArrayElement,
    /// Key-value group (sibling key-value pairs)
    KeyValueGroup,
}

impl std::fmt::Display for JsonGroupType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JsonGroupType::RootObject => write!(f, "root_object"),
            JsonGroupType::NestedObject => write!(f, "nested_object"),
            JsonGroupType::Array => write!(f, "array"),
            JsonGroupType::ArrayElement => write!(f, "array_element"),
            JsonGroupType::KeyValueGroup => write!(f, "key_value_group"),
        }
    }
}

impl JsonGroupType {
    /// Convert to DocGroupType for compatibility
    pub fn to_doc_group_type(&self) -> crate::types::DocGroupType {
        match self {
            JsonGroupType::RootObject => crate::types::DocGroupType::Chapter,
            JsonGroupType::NestedObject => crate::types::DocGroupType::Section,
            JsonGroupType::Array => crate::types::DocGroupType::StandaloneBlock,
            JsonGroupType::ArrayElement => crate::types::DocGroupType::ParagraphGroup,
            JsonGroupType::KeyValueGroup => crate::types::DocGroupType::ParagraphGroup,
        }
    }
}

/// JSON group
///
/// A group of related JSON nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonGroup {
    /// Group ID
    pub group_id: String,
    /// Group type
    pub group_type: JsonGroupType,
    /// Path prefix
    pub path_prefix: String,
    /// Header node (for object/array groups)
    pub header: Option<JsonNode>,
    /// Member nodes
    pub members: Vec<JsonNode>,
    /// Combined text for embedding
    pub embedding_text: String,
    /// Combined text for BM25
    pub bm25_text: String,
    /// Estimated token count
    pub token_count: usize,
    /// Source span
    pub span: Span,
}

impl JsonGroup {
    /// Create a new JSON group
    pub fn new(group_id: String, group_type: JsonGroupType, path_prefix: String) -> Self {
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

impl GenericGroup<JsonNode> for JsonGroup {
    fn group_id(&self) -> &str {
        &self.group_id
    }

    fn group_id_mut(&mut self) -> &mut String {
        &mut self.group_id
    }

    fn header(&self) -> Option<&JsonNode> {
        self.header.as_ref()
    }

    fn header_mut(&mut self) -> &mut Option<JsonNode> {
        &mut self.header
    }

    fn members(&self) -> &[JsonNode] {
        &self.members
    }

    fn members_mut(&mut self) -> &mut Vec<JsonNode> {
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

    fn node_to_embedding_text(node: &JsonNode) -> String {
        node.to_embedding_text()
    }

    fn node_to_bm25_text(node: &JsonNode) -> String {
        node.to_bm25_text()
    }

    fn node_id(node: &JsonNode) -> &str {
        &node.id
    }

    fn node_span(node: &JsonNode) -> &Span {
        &node.span
    }
}

impl DocumentNode for JsonNode {
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
    fn test_json_value_type_from_json() {
        use serde_json::json;

        assert_eq!(
            JsonValueType::from_json_value(&json!("test")),
            JsonValueType::String
        );
        assert_eq!(
            JsonValueType::from_json_value(&json!(123)),
            JsonValueType::Number
        );
        assert_eq!(
            JsonValueType::from_json_value(&json!(true)),
            JsonValueType::Boolean
        );
        assert_eq!(
            JsonValueType::from_json_value(&json!(null)),
            JsonValueType::Null
        );
        assert_eq!(
            JsonValueType::from_json_value(&json!({})),
            JsonValueType::Object
        );
        assert_eq!(
            JsonValueType::from_json_value(&json!([])),
            JsonValueType::Array
        );
    }

    #[test]
    fn test_json_node_embedding_text() {
        let node = JsonNode::new(
            "test".to_string(),
            JsonNodeType::Primitive(JsonValueType::String),
            "database.host".to_string(),
            Span::default(),
        )
        .with_key_name("host".to_string())
        .with_value("localhost".to_string());

        let text = node.to_embedding_text();
        assert!(text.contains("\"host\""));
        assert!(text.contains("localhost"));
        assert!(text.contains(':'));
    }

    #[test]
    fn test_json_node_object_text() {
        let node = JsonNode::new(
            "test".to_string(),
            JsonNodeType::Object,
            "database.credentials".to_string(),
            Span::default(),
        );

        let text = node.to_embedding_text();
        assert!(text.contains("database.credentials"));
        assert!(text.contains("{object}"));
    }

    #[test]
    fn test_json_node_array_text() {
        let mut node = JsonNode::new(
            "test".to_string(),
            JsonNodeType::Array,
            "servers".to_string(),
            Span::default(),
        );
        node.add_child("s1".to_string());
        node.add_child("s2".to_string());

        let text = node.to_embedding_text();
        assert!(text.contains("servers"));
        assert!(text.contains("[array]"));
    }

    #[test]
    fn test_json_node_primitive_without_value() {
        let node = JsonNode::new(
            "test".to_string(),
            JsonNodeType::Primitive(JsonValueType::Object),
            "config".to_string(),
            Span::default(),
        )
        .with_key_name("database".to_string());

        let text = node.to_embedding_text();
        assert!(text.contains("\"database\""));
        assert!(text.contains("{object}"));
    }

    #[test]
    fn test_json_node_array_element() {
        let node = JsonNode::new(
            "test".to_string(),
            JsonNodeType::Primitive(JsonValueType::String),
            "items[0]".to_string(),
            Span::default(),
        )
        .with_array_index(0)
        .with_value("first".to_string());

        let text = node.to_embedding_text();
        assert!(text.contains("[0]"));
        assert!(text.contains("first"));
    }
}
