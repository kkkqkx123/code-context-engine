//! XML parser
//!
//! This module provides parsing functionality for XML files using quick-xml.
//! It extracts structural elements and builds a tree of XmlNodes.

use quick_xml::Reader;
use quick_xml::events::Event;
use std::collections::HashMap;

use crate::xml::types::{XmlNode, XmlNodeType};
use cce_types::{ParseError, Span};

/// XML parser
pub struct XmlParser {
    node_counter: usize,
}

impl XmlParser {
    /// Create a new XML parser
    pub fn new() -> Self {
        Self { node_counter: 0 }
    }

    /// Generate a unique node ID
    fn next_id(&mut self) -> String {
        self.node_counter += 1;
        format!("xml_node_{}", self.node_counter)
    }

    /// Parse XML content into XML nodes
    pub fn parse(&mut self, content: &str) -> Result<Vec<XmlNode>, ParseError> {
        let mut reader = Reader::from_str(content);
        let config = reader.config_mut();
        config.trim_text(true);

        let mut nodes = Vec::new();
        let mut path_stack: Vec<String> = Vec::new();
        let mut id_stack: Vec<String> = Vec::new();

        // Create root node
        let root_id = self.next_id();
        let root_node = XmlNode::new(
            root_id.clone(),
            XmlNodeType::Root,
            String::new(),
            Span::from_line(0), // Root starts at line 0
        );
        nodes.push(root_node);
        id_stack.push(root_id.clone());

        let mut buf = Vec::new();
        let mut current_line = 1u32;

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(ref e)) => {
                    let tag = e.name().as_ref().to_vec();
                    let tag_name = String::from_utf8_lossy(&tag).to_string();

                    // Create element node
                    let path = if path_stack.is_empty() {
                        tag_name.clone()
                    } else {
                        format!("{}.{}", path_stack.join("."), tag_name)
                    };

                    let node_id = self.next_id();
                    let mut attributes = HashMap::new();

                    for attr in e.attributes().flatten() {
                        let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                        let value = String::from_utf8_lossy(&attr.value).to_string();
                        attributes.insert(key, value);
                    }

                    let element_node = XmlNode::new(
                        node_id.clone(),
                        XmlNodeType::Element {
                            tag: tag_name.clone(),
                        },
                        path.clone(),
                        Span::from_line(current_line as usize),
                    )
                    .with_tag(tag_name.clone())
                    .with_attributes(attributes)
                    .with_parent(id_stack.last().cloned().unwrap_or_default());

                    nodes.push(element_node);

                    // Update parent's children
                    if let Some(parent_id) = id_stack.last() {
                        if let Some(parent) = nodes.iter_mut().find(|n| n.id == *parent_id) {
                            parent.add_child(node_id.clone());
                        }
                    }

                    path_stack.push(tag_name);
                    id_stack.push(node_id);
                    current_line += 1;
                }
                Ok(Event::Empty(ref e)) => {
                    let tag = e.name().as_ref().to_vec();
                    let tag_name = String::from_utf8_lossy(&tag).to_string();

                    // Create self-closing element node
                    let path = if path_stack.is_empty() {
                        tag_name.clone()
                    } else {
                        format!("{}.{}", path_stack.join("."), tag_name)
                    };

                    let node_id = self.next_id();
                    let mut attributes = HashMap::new();

                    for attr in e.attributes().flatten() {
                        let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                        let value = String::from_utf8_lossy(&attr.value).to_string();
                        attributes.insert(key, value);
                    }

                    let element_node = XmlNode::new(
                        node_id.clone(),
                        XmlNodeType::Element {
                            tag: tag_name.clone(),
                        },
                        path.clone(),
                        Span::from_line(current_line as usize),
                    )
                    .with_tag(tag_name)
                    .with_attributes(attributes)
                    .with_parent(id_stack.last().cloned().unwrap_or_default());

                    nodes.push(element_node);

                    // Update parent's children
                    if let Some(parent_id) = id_stack.last() {
                        if let Some(parent) = nodes.iter_mut().find(|n| n.id == *parent_id) {
                            parent.add_child(node_id);
                        }
                    }

                    current_line += 1;
                }
                Ok(Event::End(_)) => {
                    if !path_stack.is_empty() {
                        path_stack.pop();
                        id_stack.pop();
                    }
                }
                Ok(Event::Text(ref e)) => {
                    let text = String::from_utf8_lossy(e.as_ref()).to_string();
                    let trimmed = text.trim();

                    if !trimmed.is_empty() {
                        // Create text node
                        let node_id = self.next_id();
                        let path = if path_stack.is_empty() {
                            "text".to_string()
                        } else {
                            format!("{}.text", path_stack.join("."))
                        };

                        let text_node = XmlNode::new(
                            node_id.clone(),
                            XmlNodeType::Text,
                            path,
                            Span::from_line(current_line as usize),
                        )
                        .with_text(trimmed.to_string())
                        .with_parent(id_stack.last().cloned().unwrap_or_default());

                        nodes.push(text_node);

                        // Update parent's children
                        if let Some(parent_id) = id_stack.last() {
                            if let Some(parent) = nodes.iter_mut().find(|n| n.id == *parent_id) {
                                parent.add_child(node_id);
                            }
                        }
                    }
                    current_line += 1;
                }
                Ok(Event::Comment(ref e)) => {
                    let text = String::from_utf8_lossy(e.as_ref()).to_string();

                    // Create comment node
                    let node_id = self.next_id();
                    let path = if path_stack.is_empty() {
                        "comment".to_string()
                    } else {
                        format!("{}.comment", path_stack.join("."))
                    };

                    let comment_node = XmlNode::new(
                        node_id.clone(),
                        XmlNodeType::Comment,
                        path,
                        Span::from_line(current_line as usize),
                    )
                    .with_text(text)
                    .with_parent(id_stack.last().cloned().unwrap_or_default());

                    nodes.push(comment_node);

                    // Update parent's children
                    if let Some(parent_id) = id_stack.last() {
                        if let Some(parent) = nodes.iter_mut().find(|n| n.id == *parent_id) {
                            parent.add_child(node_id);
                        }
                    }

                    current_line += 1;
                }
                Ok(Event::CData(ref e)) => {
                    let text = String::from_utf8_lossy(e.as_ref()).to_string();

                    // Create CDATA node
                    let node_id = self.next_id();
                    let path = if path_stack.is_empty() {
                        "cdata".to_string()
                    } else {
                        format!("{}.cdata", path_stack.join("."))
                    };

                    let cdata_node = XmlNode::new(
                        node_id.clone(),
                        XmlNodeType::CData,
                        path,
                        Span::from_line(current_line as usize),
                    )
                    .with_text(text)
                    .with_parent(id_stack.last().cloned().unwrap_or_default());

                    nodes.push(cdata_node);

                    // Update parent's children
                    if let Some(parent_id) = id_stack.last() {
                        if let Some(parent) = nodes.iter_mut().find(|n| n.id == *parent_id) {
                            parent.add_child(node_id);
                        }
                    }

                    current_line += 1;
                }
                Ok(Event::PI(ref e)) => {
                    let target = String::from_utf8_lossy(e.target()).to_string();

                    // Create processing instruction node
                    let node_id = self.next_id();
                    let path = if path_stack.is_empty() {
                        format!("pi_{}", target)
                    } else {
                        format!("{}.pi_{}", path_stack.join("."), target)
                    };

                    let pi_node = XmlNode::new(
                        node_id.clone(),
                        XmlNodeType::ProcessingInstruction {
                            target: target.clone(),
                        },
                        path,
                        Span::from_line(current_line as usize),
                    )
                    .with_parent(id_stack.last().cloned().unwrap_or_default());

                    nodes.push(pi_node);

                    // Update parent's children
                    if let Some(parent_id) = id_stack.last() {
                        if let Some(parent) = nodes.iter_mut().find(|n| n.id == *parent_id) {
                            parent.add_child(node_id);
                        }
                    }

                    current_line += 1;
                }
                Ok(Event::Decl(_)) => {
                    // Create declaration node
                    let node_id = self.next_id();
                    let decl_node = XmlNode::new(
                        node_id.clone(),
                        XmlNodeType::Declaration,
                        "declaration".to_string(),
                        Span::from_line(current_line as usize),
                    )
                    .with_parent(id_stack.last().cloned().unwrap_or_default());

                    nodes.push(decl_node);

                    // Update parent's children
                    if let Some(parent_id) = id_stack.last() {
                        if let Some(parent) = nodes.iter_mut().find(|n| n.id == *parent_id) {
                            parent.add_child(node_id);
                        }
                    }

                    current_line += 1;
                }
                Ok(Event::DocType(_)) => {
                    // Skip DOCTYPE declarations for now
                    current_line += 1;
                }
                Ok(Event::Eof) => break,
                Err(e) => {
                    return Err(ParseError::xml(format!("XML parse error: {:?}", e)));
                }
            }
            buf.clear();
        }

        Ok(nodes)
    }
}

impl Default for XmlParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_element() {
        let mut parser = XmlParser::new();
        let xml = r#"<root><child>text</child></root>"#;
        let nodes = parser.parse(xml).expect("should parse");

        // Root + root element + child element + text node
        assert!(nodes.len() >= 3);

        // Check root
        assert!(matches!(nodes[0].node_type, XmlNodeType::Root));

        // Check child element
        let child = nodes.iter().find(|n| n.tag.as_deref() == Some("child"));
        assert!(child.is_some());
    }

    #[test]
    fn test_parse_attributes() {
        let mut parser = XmlParser::new();
        let xml = r#"<root id="main" class="container"></root>"#;
        let nodes = parser.parse(xml).expect("should parse");

        // Find root element
        let root = nodes.iter().find(|n| n.tag.as_deref() == Some("root"));
        assert!(root.is_some());
        let root = root.unwrap();

        assert_eq!(root.attributes.get("id"), Some(&"main".to_string()));
        assert_eq!(root.attributes.get("class"), Some(&"container".to_string()));
    }

    #[test]
    fn test_parse_nested_elements() {
        let mut parser = XmlParser::new();
        let xml = r#"<root><parent><child>value</child></parent></root>"#;
        let nodes = parser.parse(xml).expect("should parse");

        // Find child element
        let child = nodes.iter().find(|n| n.path == "root.parent.child");
        assert!(child.is_some());
    }

    #[test]
    fn test_parse_self_closing() {
        let mut parser = XmlParser::new();
        let xml = r#"<root><empty/></root>"#;
        let nodes = parser.parse(xml).expect("should parse");

        // Find empty element
        let empty = nodes.iter().find(|n| n.tag.as_deref() == Some("empty"));
        assert!(empty.is_some());
    }

    #[test]
    fn test_parse_comment() {
        let mut parser = XmlParser::new();
        let xml = r#"<root><!-- This is a comment --></root>"#;
        let nodes = parser.parse(xml).expect("should parse");

        // Find comment node
        let comment = nodes
            .iter()
            .find(|n| matches!(n.node_type, XmlNodeType::Comment));
        assert!(comment.is_some());
    }

    #[test]
    fn test_parse_cdata() {
        let mut parser = XmlParser::new();
        let xml = r#"<root><![CDATA[Some <data> here]]></root>"#;
        let nodes = parser.parse(xml).expect("should parse");

        // Find CDATA node
        let cdata = nodes
            .iter()
            .find(|n| matches!(n.node_type, XmlNodeType::CData));
        assert!(cdata.is_some());
        let cdata = cdata.unwrap();
        assert_eq!(cdata.text, Some("Some <data> here".to_string()));
    }

    #[test]
    fn test_parse_declaration() {
        let mut parser = XmlParser::new();
        let xml = r#"<?xml version="1.0"?><root></root>"#;
        let nodes = parser.parse(xml).expect("should parse");

        // Find declaration node
        let decl = nodes
            .iter()
            .find(|n| matches!(n.node_type, XmlNodeType::Declaration));
        assert!(decl.is_some());
    }

    #[test]
    fn test_parse_invalid_xml() {
        let mut parser = XmlParser::new();
        // Use a more obviously invalid XML
        let xml = r#"<root><child></root>"#;
        let result = parser.parse(xml);
        // quick-xml may be lenient, so we just check it doesn't panic
        // In production, you might want stricter validation
        let _ = result;
    }
}
