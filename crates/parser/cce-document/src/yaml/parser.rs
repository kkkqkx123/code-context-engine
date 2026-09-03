//! YAML parser
//!
//! This module provides parsing functionality for YAML files using yaml-rust2 crate.
//! It extracts structural elements and builds a tree of YamlNodes.

use yaml_rust2::Yaml;

use crate::yaml::types::{YamlNode, YamlNodeType, YamlValueType};
use cce_types::{ParseError, Span};

/// YAML parser
pub struct YamlParser {
    node_counter: usize,
}

impl YamlParser {
    /// Create a new YAML parser
    pub fn new() -> Self {
        Self { node_counter: 0 }
    }

    /// Generate a unique node ID
    fn next_id(&mut self) -> String {
        self.node_counter += 1;
        format!("yaml_node_{}", self.node_counter)
    }

    /// Parse YAML content into YAML nodes
    pub fn parse(&mut self, content: &str) -> Result<Vec<YamlNode>, ParseError> {
        // Parse YAML using yaml-rust2 crate
        let docs = yaml_rust2::YamlLoader::load_from_str(content)
            .map_err(|e| ParseError::yaml(format!("YAML parse error: {}", e)))?;

        let mut nodes = Vec::new();

        // Create root node
        let root_id = self.next_id();
        let root_node = YamlNode::new(
            root_id.clone(),
            YamlNodeType::Root,
            String::new(),
            Span::from_lines(0, 1),
        );
        nodes.push(root_node);

        // Build nodes from each document in the YAML stream
        for doc in &docs {
            self.build_nodes_from_yaml(doc, "", 0, &root_id, 0, &mut nodes)?;
        }

        Ok(nodes)
    }

    /// Build nodes from YAML value
    fn build_nodes_from_yaml(
        &mut self,
        value: &Yaml,
        path: &str,
        _depth: usize,
        parent_id: &str,
        start_line: usize,
        nodes: &mut Vec<YamlNode>,
    ) -> Result<usize, ParseError> {
        let mut current_line = start_line;

        match value {
            Yaml::Hash(hash) => {
                for (key, val) in hash {
                    let key_str = match key {
                        Yaml::String(s) => s.clone(),
                        Yaml::Integer(i) => i.to_string(),
                        Yaml::Boolean(b) => b.to_string(),
                        _ => key.as_str().unwrap_or("unknown").to_string(),
                    };

                    let child_path = if path.is_empty() {
                        key_str.clone()
                    } else {
                        format!("{}.{}", path, key_str)
                    };

                    let value_type = YamlValueType::from_yaml_value(val);

                    // Create key-value node
                    let kv_id = self.next_id();
                    let kv_node = YamlNode::new(
                        kv_id.clone(),
                        YamlNodeType::KeyValue {
                            key: key_str.clone(),
                            value_type,
                        },
                        child_path.clone(),
                        Span::new(0, 0, current_line, 0, current_line + 1, 0),
                    )
                    .with_key(key_str.clone())
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
                            Yaml::Hash(nested_hash) => {
                                let end = self.build_nodes_from_yaml(
                                    &Yaml::Hash(nested_hash.clone()),
                                    &child_path,
                                    _depth + 1,
                                    &kv_id,
                                    current_line,
                                    nodes,
                                )?;
                                current_line = end + 1;
                            }
                            Yaml::Array(arr) => {
                                let is_mapping_array =
                                    arr.iter().all(|elem| matches!(elem, Yaml::Hash(_)));

                                let mut element_line = current_line + 1;
                                for (index, elem) in arr.iter().enumerate() {
                                    let elem_value_type = YamlValueType::from_yaml_value(elem);
                                    let elem_path = format!("{}[{}]", child_path, index);

                                    let (node_type, node_key) = if is_mapping_array {
                                        (
                                            YamlNodeType::SequenceElement {
                                                index,
                                                value_type: elem_value_type,
                                            },
                                            Some(key_str.clone()),
                                        )
                                    } else {
                                        (
                                            YamlNodeType::SequenceElement {
                                                index,
                                                value_type: elem_value_type,
                                            },
                                            None,
                                        )
                                    };

                                    let elem_id = self.next_id();
                                    let elem_path_clone = elem_path.clone();
                                    let mut elem_node = YamlNode::new(
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

                                    if !is_mapping_array && elem_value_type.is_primitive() {
                                        let value_str = self.value_to_string(elem);
                                        if let Some(elem_node) =
                                            nodes.iter_mut().find(|n| n.id == elem_id)
                                        {
                                            elem_node.value = Some(value_str);
                                            elem_node.value_type = elem_value_type;
                                        }
                                        element_line += 1;
                                    } else if let Yaml::Hash(nested_hash) = elem {
                                        let end = self.build_nodes_from_yaml(
                                            &Yaml::Hash(nested_hash.clone()),
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
            }
            Yaml::Array(arr) => {
                for (index, elem) in arr.iter().enumerate() {
                    let elem_value_type = YamlValueType::from_yaml_value(elem);
                    let elem_path = if path.is_empty() {
                        format!("[{}]", index)
                    } else {
                        format!("{}[{}]", path, index)
                    };

                    let elem_id = self.next_id();
                    let elem_path_clone = elem_path.clone();
                    let elem_node = YamlNode::new(
                        elem_id.clone(),
                        YamlNodeType::SequenceElement {
                            index,
                            value_type: elem_value_type,
                        },
                        elem_path_clone,
                        Span::new(0, 0, current_line, 0, current_line + 1, 0),
                    )
                    .with_parent(parent_id.to_string());

                    nodes.push(elem_node);

                    if let Some(parent) = nodes.iter_mut().find(|n| n.id == parent_id) {
                        parent.add_child(elem_id.clone());
                    }

                    if elem_value_type.is_primitive() {
                        let value_str = self.value_to_string(elem);
                        if let Some(elem_node) = nodes.iter_mut().find(|n| n.id == elem_id) {
                            elem_node.value = Some(value_str);
                            elem_node.value_type = elem_value_type;
                        }
                        current_line += 1;
                    } else if let Yaml::Hash(nested_hash) = elem {
                        let end = self.build_nodes_from_yaml(
                            &Yaml::Hash(nested_hash.clone()),
                            &elem_path,
                            _depth + 1,
                            &elem_id,
                            current_line,
                            nodes,
                        )?;
                        current_line = end + 1;
                    }
                }
            }
            // Primitive values at root level (uncommon but possible)
            _ => {
                let value_type = YamlValueType::from_yaml_value(value);
                let kv_id = self.next_id();
                let kv_node = YamlNode::new(
                    kv_id.clone(),
                    YamlNodeType::KeyValue {
                        key: "value".to_string(),
                        value_type,
                    },
                    "value".to_string(),
                    Span::new(0, 0, current_line, 0, current_line + 1, 0),
                )
                .with_key("value".to_string())
                .with_parent(parent_id.to_string())
                .with_value(self.value_to_string(value), value_type);

                nodes.push(kv_node);

                if let Some(parent) = nodes.iter_mut().find(|n| n.id == parent_id) {
                    parent.add_child(kv_id);
                }
                current_line += 1;
            }
        }

        Ok(current_line)
    }

    /// Convert YAML value to string representation
    fn value_to_string(&self, value: &Yaml) -> String {
        match value {
            Yaml::String(s) => s.clone(),
            Yaml::Integer(i) => i.to_string(),
            Yaml::Real(r) => r.clone(),
            Yaml::Boolean(b) => b.to_string(),
            Yaml::Null => "null".to_string(),
            Yaml::Array(_) => "[...]".to_string(),
            Yaml::Hash(_) => "{...}".to_string(),
            Yaml::Alias(_) => "[alias]".to_string(),
            Yaml::BadValue => "[bad value]".to_string(),
        }
    }
}

impl Default for YamlParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_mapping() {
        let mut parser = YamlParser::new();
        let yaml = r#"name: test
value: 123"#;
        let nodes = parser.parse(yaml).expect("should parse");

        // Root + 2 key-value pairs
        assert_eq!(nodes.len(), 3);

        // Check root
        assert!(matches!(nodes[0].node_type, YamlNodeType::Root));

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
    fn test_parse_nested_mapping() {
        let mut parser = YamlParser::new();
        let yaml = r#"database:
  host: localhost
  port: 3306"#;
        let nodes = parser.parse(yaml).expect("should parse");

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
        let mut parser = YamlParser::new();
        let yaml = r#"items:
  - 1
  - 2
  - 3"#;
        let nodes = parser.parse(yaml).expect("should parse");

        // Find array elements
        let elements: Vec<_> = nodes
            .iter()
            .filter(|n| matches!(n.node_type, YamlNodeType::SequenceElement { .. }))
            .collect();
        assert_eq!(elements.len(), 3);
    }

    #[test]
    fn test_parse_boolean_and_float() {
        let mut parser = YamlParser::new();
        let yaml = r#"enabled: true
disabled: false
pi: 3.14"#;
        let nodes = parser.parse(yaml).expect("should parse");

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
        let mut parser = YamlParser::new();
        let yaml = r#"config:
  database:
    host: localhost
    port: 3306
  servers:
    - name: web
      port: 8080
    - name: api
      port: 3000"#;
        let nodes = parser.parse(yaml).expect("should parse");

        // Check nested path
        let host = nodes.iter().find(|n| n.path == "config.database.host");
        assert!(host.is_some());
        assert_eq!(host.unwrap().value, Some("localhost".to_string()));

        // Check array of mappings
        let server_names: Vec<_> = nodes
            .iter()
            .filter(|n| n.key.as_deref() == Some("name"))
            .collect();
        assert_eq!(server_names.len(), 2);
    }

    #[test]
    fn test_parse_invalid_yaml() {
        let mut parser = YamlParser::new();
        let _yaml = r#"name:
  - invalid
    indentation"#;
        // This may or may not be invalid depending on YAML parsing rules
        // Let's test with truly invalid YAML
        let yaml2 = r#"name: [invalid"#;
        let result = parser.parse(yaml2);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_null_value() {
        let mut parser = YamlParser::new();
        let yaml = r#"value: null"#;
        let nodes = parser.parse(yaml).expect("should parse");

        let value = nodes.iter().find(|n| n.key.as_deref() == Some("value"));
        assert!(value.is_some());
        assert_eq!(value.unwrap().value_type, YamlValueType::Null);
    }

    #[test]
    fn test_parse_inline_array() {
        let mut parser = YamlParser::new();
        let yaml = r#"items: [1, 2, 3]"#;
        let nodes = parser.parse(yaml).expect("should parse");

        // Find array elements
        let elements: Vec<_> = nodes
            .iter()
            .filter(|n| matches!(n.node_type, YamlNodeType::SequenceElement { .. }))
            .collect();
        assert_eq!(elements.len(), 3);
    }
}
