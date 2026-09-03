//! JSON parser
//!
//! This module provides parsing functionality for JSON files using serde_json.
//! It extracts structural elements and builds a tree of JsonNodes.

use serde_json::Value;

use crate::json::types::{JsonNode, JsonNodeType, JsonValueType};
use cce_types::{ParseError, Span};

/// Context for building nodes
struct BuildNodesContext<'a> {
    value: &'a Value,
    path: &'a str,
    depth: usize,
    parent_id: &'a str,
    start_line: usize,
    nodes: &'a mut Vec<JsonNode>,
    key_name: Option<String>,
}

/// JSON parser
pub struct JsonParser {
    node_counter: usize,
}

impl JsonParser {
    /// Create a new JSON parser
    pub fn new() -> Self {
        Self { node_counter: 0 }
    }

    /// Generate a unique node ID
    fn next_id(&mut self) -> String {
        self.node_counter += 1;
        format!("json_node_{}", self.node_counter)
    }

    /// Parse JSON content into JSON nodes
    pub fn parse(&mut self, content: &str) -> Result<Vec<JsonNode>, ParseError> {
        // Parse JSON using serde_json
        let value: Value = serde_json::from_str(content)
            .map_err(|e| ParseError::json(format!("JSON parse error: {}", e)))?;

        let mut nodes = Vec::new();

        // Create root node
        let root_id = self.next_id();
        let root_node = JsonNode::new(
            root_id.clone(),
            JsonNodeType::Root,
            String::new(),
            Span::from_lines(0, 1),
        );
        nodes.push(root_node);

        // Build nodes recursively
        let build_ctx = BuildNodesContext {
            value: &value,
            path: "",
            depth: 0,
            parent_id: &root_id,
            start_line: 0,
            nodes: &mut nodes,
            key_name: None,
        };
        self.build_nodes(build_ctx)?;

        Ok(nodes)
    }

    /// Build nodes recursively from JSON value
    fn build_nodes(&mut self, ctx: BuildNodesContext) -> Result<usize, ParseError> {
        match ctx.value {
            Value::Object(map) => {
                // Create object node (unless it's the root)
                let (node_id, end_line) = if ctx.path.is_empty() && ctx.depth == 0 {
                    // This is the root object, update the root node
                    (ctx.parent_id.to_string(), ctx.start_line + map.len())
                } else {
                    // Create a new object node
                    let node_id = self.next_id();
                    let mut object_node = JsonNode::new(
                        node_id.clone(),
                        JsonNodeType::Object,
                        ctx.path.to_string(),
                        Span::from_line(ctx.start_line),
                    )
                    .with_parent(ctx.parent_id.to_string());

                    // Set key_name if provided
                    if let Some(key) = ctx.key_name {
                        object_node = object_node.with_key_name(key);
                    }

                    ctx.nodes.push(object_node);

                    // Update parent's children
                    if let Some(parent) = ctx.nodes.iter_mut().find(|n| n.id == ctx.parent_id) {
                        parent.add_child(node_id.clone());
                    }

                    (node_id, ctx.start_line + map.len())
                };

                // Process each key-value pair
                let mut current_line = ctx.start_line + 1;
                for (key, val) in map {
                    let child_path = if ctx.path.is_empty() {
                        key.clone()
                    } else {
                        format!("{}.{}", ctx.path, key)
                    };

                    let value_type = JsonValueType::from_json_value(val);

                    // If value is primitive, create a Primitive node directly
                    if value_type.is_primitive() {
                        let prim_id = self.next_id();
                        let value_str = self.value_to_string(val);
                        let prim_node = JsonNode::new(
                            prim_id.clone(),
                            JsonNodeType::Primitive(value_type),
                            child_path.clone(),
                            Span::from_line(current_line),
                        )
                        .with_key_name(key.clone())
                        .with_value(value_str)
                        .with_parent(node_id.clone()); // Parent is the object node

                        ctx.nodes.push(prim_node);

                        // Update object's children
                        if let Some(obj) = ctx.nodes.iter_mut().find(|n| n.id == node_id) {
                            obj.add_child(prim_id);
                        }
                        current_line += 1;
                    } else {
                        // For complex values (Object/Array), recursively process first
                        // The recursive call will create the container node
                        let end = self.build_nodes(BuildNodesContext {
                            value: val,
                            path: &child_path,
                            depth: ctx.depth + 1,
                            parent_id: &node_id,
                            start_line: current_line,
                            nodes: ctx.nodes,
                            key_name: Some(key.clone()), // Pass key_name for container nodes
                        })?;

                        // Now find the created container node and update relationships
                        if let Some(container_id) = ctx
                            .nodes
                            .iter()
                            .find(|n| {
                                n.path == child_path && n.parent_id.as_deref() == Some(&node_id)
                            })
                            .map(|n| n.id.clone())
                        {
                            // Container already has correct parent set during its creation
                            // Just ensure the parent knows about this child
                            if let Some(parent) = ctx.nodes.iter_mut().find(|n| n.id == node_id) {
                                if !parent.children.contains(&container_id) {
                                    parent.add_child(container_id);
                                }
                            }
                        }

                        current_line = end + 1;
                    }
                }

                Ok(end_line)
            }
            Value::Array(arr) => {
                // Create array node
                let node_id = self.next_id();
                let mut array_node = JsonNode::new(
                    node_id.clone(),
                    JsonNodeType::Array,
                    ctx.path.to_string(),
                    Span::from_lines(ctx.start_line, ctx.start_line + arr.len()),
                )
                .with_parent(ctx.parent_id.to_string());

                // Set key_name if provided (for nested arrays)
                if let Some(key) = ctx.key_name {
                    array_node = array_node.with_key_name(key);
                }

                ctx.nodes.push(array_node);

                // Update parent's children
                if let Some(parent) = ctx.nodes.iter_mut().find(|n| n.id == ctx.parent_id) {
                    parent.add_child(node_id.clone());
                }

                // Process each element
                let mut current_line = ctx.start_line + 1;
                for (index, elem) in arr.iter().enumerate() {
                    let child_path = format!("{}[{}]", ctx.path, index);
                    let value_type = JsonValueType::from_json_value(elem);

                    // If element is primitive, create a Primitive node directly
                    if value_type.is_primitive() {
                        let prim_id = self.next_id();
                        let value_str = self.value_to_string(elem);
                        let prim_node = JsonNode::new(
                            prim_id.clone(),
                            JsonNodeType::Primitive(value_type),
                            child_path.clone(),
                            Span::from_line(current_line),
                        )
                        .with_array_index(index)
                        .with_value(value_str)
                        .with_parent(node_id.clone()); // Parent is the array node

                        ctx.nodes.push(prim_node);

                        // Update array's children
                        if let Some(arr_node) = ctx.nodes.iter_mut().find(|n| n.id == node_id) {
                            arr_node.add_child(prim_id);
                        }
                        current_line += 1;
                    } else {
                        // For complex elements (Object/Array), recursively process first
                        let end = self.build_nodes(BuildNodesContext {
                            value: elem,
                            path: &child_path,
                            depth: ctx.depth + 1,
                            parent_id: &node_id,
                            start_line: current_line,
                            nodes: ctx.nodes,
                            key_name: Some(format!("{}", index)), // Pass array index as key_name
                        })?;

                        // Find the created container node and update relationships
                        if let Some(container_id) = ctx
                            .nodes
                            .iter()
                            .find(|n| {
                                n.path == child_path && n.parent_id.as_deref() == Some(&node_id)
                            })
                            .map(|n| n.id.clone())
                        {
                            if let Some(arr_node) = ctx.nodes.iter_mut().find(|n| n.id == node_id) {
                                if !arr_node.children.contains(&container_id) {
                                    arr_node.add_child(container_id);
                                }
                            }
                        }

                        current_line = end + 1;
                    }
                }

                Ok(ctx.start_line + arr.len())
            }
            // Primitive values are handled by the parent
            _ => Ok(ctx.start_line),
        }
    }

    /// Convert JSON value to string representation
    fn value_to_string(&self, value: &Value) -> String {
        match value {
            Value::Null => "null".to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Number(n) => n.to_string(),
            Value::String(s) => s.clone(),
            Value::Array(_) | Value::Object(_) => String::new(),
        }
    }
}

impl Default for JsonParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_object() {
        let mut parser = JsonParser::new();
        let json = r#"{"name": "test", "value": 123}"#;
        let nodes = parser.parse(json).expect("should parse");

        // Root + 2 key-value pairs
        assert_eq!(nodes.len(), 3);

        // Check root
        assert!(matches!(nodes[0].node_type, JsonNodeType::Root));

        // Check primitive nodes
        let name_node = nodes.iter().find(|n| n.key_name.as_deref() == Some("name"));
        assert!(name_node.is_some());
        let name_node = name_node.unwrap();
        assert_eq!(name_node.value, Some("test".to_string()));
        assert_eq!(name_node.path, "name");

        let value_node = nodes
            .iter()
            .find(|n| n.key_name.as_deref() == Some("value"));
        assert!(value_node.is_some());
        let value_node = value_node.unwrap();
        assert_eq!(value_node.value, Some("123".to_string()));
        assert_eq!(value_node.path, "value");
    }

    #[test]
    fn test_parse_nested_object() {
        let mut parser = JsonParser::new();
        let json = r#"{"database": {"host": "localhost", "port": 3306}}"#;
        let nodes = parser.parse(json).expect("should parse");

        // Find the host node
        let host_node = nodes.iter().find(|n| n.path == "database.host");
        assert!(host_node.is_some());
        let host_node = host_node.unwrap();
        assert_eq!(host_node.value, Some("localhost".to_string()));

        // Find the port node
        let port_node = nodes.iter().find(|n| n.path == "database.port");
        assert!(port_node.is_some());
        let port_node = port_node.unwrap();
        assert_eq!(port_node.value, Some("3306".to_string()));
    }

    #[test]
    fn test_parse_array() {
        let mut parser = JsonParser::new();
        let json = r#"{"items": [1, 2, 3]}"#;
        let nodes = parser.parse(json).expect("should parse");

        // Find array node
        let array_node = nodes
            .iter()
            .find(|n| matches!(n.node_type, JsonNodeType::Array));
        assert!(array_node.is_some());

        // Find array elements (Primitive nodes with array_index)
        let elements: Vec<_> = nodes.iter().filter(|n| n.array_index.is_some()).collect();
        assert_eq!(elements.len(), 3);
    }

    #[test]
    fn test_parse_boolean_and_null() {
        let mut parser = JsonParser::new();
        let json = r#"{"enabled": true, "disabled": false, "empty": null}"#;
        let nodes = parser.parse(json).expect("should parse");

        let enabled = nodes
            .iter()
            .find(|n| n.key_name.as_deref() == Some("enabled"));
        assert!(enabled.is_some());
        assert_eq!(enabled.unwrap().value, Some("true".to_string()));

        let disabled = nodes
            .iter()
            .find(|n| n.key_name.as_deref() == Some("disabled"));
        assert!(disabled.is_some());
        assert_eq!(disabled.unwrap().value, Some("false".to_string()));

        let empty = nodes
            .iter()
            .find(|n| n.key_name.as_deref() == Some("empty"));
        assert!(empty.is_some());
        assert_eq!(empty.unwrap().value, Some("null".to_string()));
    }

    #[test]
    fn test_parse_complex_structure() {
        let mut parser = JsonParser::new();
        let json = r#"{
            "config": {
                "database": {
                    "host": "localhost",
                    "port": 3306
                },
                "servers": [
                    {"name": "web", "port": 8080},
                    {"name": "api", "port": 3000}
                ]
            }
        }"#;
        let nodes = parser.parse(json).expect("should parse");

        // Check nested path
        let host = nodes.iter().find(|n| n.path == "config.database.host");
        assert!(host.is_some());
        assert_eq!(host.unwrap().value, Some("localhost".to_string()));

        // Check array of objects
        let server_names: Vec<_> = nodes
            .iter()
            .filter(|n| n.key_name.as_deref() == Some("name"))
            .collect();
        assert_eq!(server_names.len(), 2);
    }

    #[test]
    fn test_parse_invalid_json() {
        let mut parser = JsonParser::new();
        let json = r#"{"invalid": }"#;
        let result = parser.parse(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_hierarchy_relationships() {
        let mut parser = JsonParser::new();
        let json = r#"{"parent": {"child": "value"}}"#;
        let nodes = parser.parse(json).expect("should parse");

        // Find parent object (Object type)
        let parent_obj = nodes
            .iter()
            .find(|n| n.path == "parent" && matches!(n.node_type, JsonNodeType::Object));
        assert!(parent_obj.is_some());
        let parent_obj = parent_obj.unwrap();
        assert!(parent_obj.has_children());

        // Parent object should have key_name "parent"
        assert_eq!(parent_obj.key_name, Some("parent".to_string()));

        // Find child primitive node
        let child_prim = nodes.iter().find(|n| {
            matches!(&n.node_type, JsonNodeType::Primitive(_))
                && n.key_name.as_deref() == Some("child")
        });
        assert!(child_prim.is_some());
        let child_prim = child_prim.unwrap();

        // Child prim should have path "parent.child"
        assert_eq!(child_prim.path, "parent.child");

        // Child prim should have the value
        assert_eq!(child_prim.value, Some("value".to_string()));

        // Child prim should be child of parent object
        // In the flattened structure, the primitive node's parent is the object it belongs to.
        // Verify the relationship by checking if the parent object contains the child's ID.
        assert!(
            parent_obj.children.contains(&child_prim.id),
            "Parent object children {:?} should contain child id {}. Nodes: {:?}",
            parent_obj.children,
            child_prim.id,
            nodes.iter().map(|n| (&n.id, &n.path)).collect::<Vec<_>>()
        );
    }
}
