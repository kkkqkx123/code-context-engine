//! TOML parser
//!
//! This module provides parsing functionality for TOML files using toml crate.
//! It extracts structural elements and builds a tree of TomlNodes.

use toml::Value;

use crate::toml::types::{TomlNode, TomlNodeType, TomlValueType};
use cce_types::{ParseError, Span};

/// TOML parser
pub struct TomlParser {
    node_counter: usize,
}

impl TomlParser {
    /// Create a new TOML parser
    pub fn new() -> Self {
        Self { node_counter: 0 }
    }

    /// Generate a unique node ID
    fn next_id(&mut self) -> String {
        self.node_counter += 1;
        format!("toml_node_{}", self.node_counter)
    }

    /// Parse TOML content into TOML nodes
    pub fn parse(&mut self, content: &str) -> Result<Vec<TomlNode>, ParseError> {
        // Parse TOML using toml crate
        let value: Value = toml::from_str(content)
            .map_err(|e| ParseError::toml(format!("TOML parse error: {}", e)))?;

        let mut nodes = Vec::new();

        // Create root node
        let root_id = self.next_id();
        let root_node = TomlNode::new(
            root_id.clone(),
            TomlNodeType::Root,
            String::new(),
            Span::from_lines(0, 1),
        );
        nodes.push(root_node);

        // Build nodes from table
        if let Value::Table(table) = value {
            self.build_nodes_from_table(&table, "", 0, &root_id, 0, &mut nodes)?;
        }

        Ok(nodes)
    }

    /// Build nodes from TOML table
    fn build_nodes_from_table(
        &mut self,
        table: &toml::value::Table,
        path: &str,
        _depth: usize,
        parent_id: &str,
        start_line: usize,
        nodes: &mut Vec<TomlNode>,
    ) -> Result<usize, ParseError> {
        let mut current_line = start_line;

        for (key, val) in table {
            let child_path = if path.is_empty() {
                key.clone()
            } else {
                format!("{}.{}", path, key)
            };

            let value_type = TomlValueType::from_toml_value(val);

            // Create key-value node
            let kv_id = self.next_id();
            let kv_node = TomlNode::new(
                kv_id.clone(),
                TomlNodeType::KeyValue {
                    key: key.clone(),
                    value_type,
                },
                child_path.clone(),
                Span::new(0, 0, current_line, 0, current_line + 1, 0),
            )
            .with_key(key.clone())
            .with_parent(parent_id.to_string());

            nodes.push(kv_node);

            // Update parent's children
            if let Some(parent) = nodes.iter_mut().find(|n| n.id == parent_id) {
                parent.add_child(kv_id.clone());
            }

            // If value is primitive, set it directly
            if value_type.is_primitive() {
                let value_str = self.value_to_string(val);
                if let Some(kv) = nodes.iter_mut().find(|n| n.id == kv_id) {
                    kv.value = Some(value_str);
                    kv.value_type = value_type;
                }
                current_line += 1;
            } else {
                // Handle complex values
                match val {
                    Value::Table(nested_table) => {
                        // This is an inline table, treat it as nested structure
                        let end = self.build_nodes_from_table(
                            nested_table,
                            &child_path,
                            _depth + 1,
                            &kv_id,
                            current_line,
                            nodes,
                        )?;
                        current_line = end + 1;
                    }
                    Value::Array(arr) => {
                        // Check if this is an array table (array of tables, like [[items]])
                        let is_array_table = arr.iter().all(|elem| matches!(elem, Value::Table(_)));

                        // Process each element
                        let mut element_line = current_line + 1;
                        for (index, elem) in arr.iter().enumerate() {
                            let elem_value_type = TomlValueType::from_toml_value(elem);
                            let elem_path = format!("{}[{}]", child_path, index);

                            // Determine node type based on whether it's an array table
                            let (node_type, node_key) = if is_array_table {
                                let table_name = key.clone();
                                (
                                    TomlNodeType::ArrayTable { table_name, index },
                                    Some(key.clone()),
                                )
                            } else {
                                (
                                    TomlNodeType::ArrayElement {
                                        index,
                                        value_type: elem_value_type,
                                    },
                                    None,
                                )
                            };

                            // Create array element node
                            let elem_id = self.next_id();
                            let elem_path_clone = elem_path.clone();
                            let mut elem_node = TomlNode::new(
                                elem_id.clone(),
                                node_type,
                                elem_path_clone,
                                Span::new(0, 0, element_line, 0, element_line + 1, 0),
                            )
                            .with_parent(kv_id.clone());

                            if let Some(k) = node_key {
                                elem_node = elem_node.with_key(k);
                            }

                            nodes.push(elem_node);

                            // Update kv node's children
                            if let Some(kv) = nodes.iter_mut().find(|n| n.id == kv_id) {
                                kv.add_child(elem_id.clone());
                            }

                            // If element is primitive, set it directly (only for non-array-table)
                            if !is_array_table && elem_value_type.is_primitive() {
                                let value_str = self.value_to_string(elem);
                                if let Some(elem_node) = nodes.iter_mut().find(|n| n.id == elem_id)
                                {
                                    elem_node.value = Some(value_str);
                                    elem_node.value_type = elem_value_type;
                                }
                                element_line += 1;
                            } else if let Value::Table(nested_table) = elem {
                                // Table inside array (array table structure)
                                let end = self.build_nodes_from_table(
                                    nested_table,
                                    &elem_path.clone(),
                                    _depth + 1,
                                    &elem_id,
                                    element_line,
                                    nodes,
                                )?;
                                element_line = end + 1;
                            }
                        }
                        current_line = element_line;
                    }
                    _ => {
                        current_line += 1;
                    }
                }
            }
        }

        Ok(current_line)
    }

    /// Convert TOML value to string representation
    fn value_to_string(&self, value: &Value) -> String {
        match value {
            Value::String(s) => s.clone(),
            Value::Integer(i) => i.to_string(),
            Value::Float(f) => f.to_string(),
            Value::Boolean(b) => b.to_string(),
            Value::Datetime(dt) => dt.to_string(),
            Value::Array(_) => "[...]".to_string(),
            Value::Table(_) => "{...}".to_string(),
        }
    }
}

impl Default for TomlParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_table() {
        let mut parser = TomlParser::new();
        let toml = r#"name = "test"
value = 123"#;
        let nodes = parser.parse(toml).expect("should parse");

        // Root + 2 key-value pairs
        assert_eq!(nodes.len(), 3);

        // Check root
        assert!(matches!(nodes[0].node_type, TomlNodeType::Root));

        // Check key-value nodes
        let name_node = nodes.iter().find(|n| n.key.as_deref() == Some("name"));
        assert!(name_node.is_some());
        let name_node = name_node.unwrap();
        assert_eq!(name_node.value, Some("test".to_string()));
        assert_eq!(name_node.path, "name");

        let value_node = nodes.iter().find(|n| n.key.as_deref() == Some("value"));
        assert!(value_node.is_some());
        let value_node = value_node.unwrap();
        assert_eq!(value_node.value, Some("123".to_string()));
        assert_eq!(value_node.path, "value");
    }

    #[test]
    fn test_parse_nested_table() {
        let mut parser = TomlParser::new();
        let toml = r#"[database]
host = "localhost"
port = 3306"#;
        let nodes = parser.parse(toml).expect("should parse");

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
        let mut parser = TomlParser::new();
        let toml = r#"items = [1, 2, 3]"#;
        let nodes = parser.parse(toml).expect("should parse");

        // Find array elements
        let elements: Vec<_> = nodes
            .iter()
            .filter(|n| matches!(n.node_type, TomlNodeType::ArrayElement { .. }))
            .collect();
        assert_eq!(elements.len(), 3);
    }

    #[test]
    fn test_parse_boolean_and_float() {
        let mut parser = TomlParser::new();
        let toml = r#"enabled = true
disabled = false
pi = 3.14"#;
        let nodes = parser.parse(toml).expect("should parse");

        let enabled = nodes.iter().find(|n| n.key.as_deref() == Some("enabled"));
        assert!(enabled.is_some());
        assert_eq!(enabled.unwrap().value, Some("true".to_string()));

        let disabled = nodes.iter().find(|n| n.key.as_deref() == Some("disabled"));
        assert!(disabled.is_some());
        assert_eq!(disabled.unwrap().value, Some("false".to_string()));

        let pi = nodes.iter().find(|n| n.key.as_deref() == Some("pi"));
        assert!(pi.is_some());
        assert_eq!(pi.unwrap().value, Some("3.14".to_string()));
    }

    #[test]
    fn test_parse_complex_structure() {
        let mut parser = TomlParser::new();
        let toml = r#"[config.database]
host = "localhost"
port = 3306

[[config.servers]]
name = "web"
port = 8080

[[config.servers]]
name = "api"
port = 3000"#;
        let nodes = parser.parse(toml).expect("should parse");

        // Check nested path
        let host = nodes.iter().find(|n| n.path == "config.database.host");
        assert!(host.is_some());
        assert_eq!(host.unwrap().value, Some("localhost".to_string()));

        // Check array of tables
        let server_names: Vec<_> = nodes
            .iter()
            .filter(|n| n.key.as_deref() == Some("name"))
            .collect();
        assert_eq!(server_names.len(), 2);
    }

    #[test]
    fn test_parse_invalid_toml() {
        let mut parser = TomlParser::new();
        let toml = r#"name = "#;
        let result = parser.parse(toml);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_datetime() {
        let mut parser = TomlParser::new();
        let toml = r#"created = 2024-01-01T00:00:00Z"#;
        let nodes = parser.parse(toml).expect("should parse");

        let created = nodes.iter().find(|n| n.key.as_deref() == Some("created"));
        assert!(created.is_some());
        assert_eq!(created.unwrap().value_type, TomlValueType::DateTime);
    }

    #[test]
    fn test_parse_inline_table() {
        let mut parser = TomlParser::new();
        let toml = r#"inline = { name = "test", value = 42 }"#;
        let nodes = parser.parse(toml).expect("should parse");

        // Find the inline table's children
        let name = nodes.iter().find(|n| n.path == "inline.name");
        assert!(name.is_some());
        assert_eq!(name.unwrap().value, Some("test".to_string()));

        let value = nodes.iter().find(|n| n.path == "inline.value");
        assert!(value.is_some());
        assert_eq!(value.unwrap().value, Some("42".to_string()));
    }
}
