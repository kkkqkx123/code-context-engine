//! XML processing types
//!
//! This module provides types for XML document processing.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::common::{DocumentNode, GenericGroup};
use cce_types::Span;

/// XML node type
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum XmlNodeType {
    /// Root node (document root)
    Root,
    /// Element node
    Element {
        /// Tag name
        tag: String,
    },
    /// Text content node
    Text,
    /// Comment node
    Comment,
    /// Processing instruction
    ProcessingInstruction {
        /// Target name
        target: String,
    },
    /// CDATA section
    CData,
    /// Declaration (<?xml ...?>)
    Declaration,
}

impl XmlNodeType {
    /// Check if this is an element node
    pub fn is_element(&self) -> bool {
        matches!(self, XmlNodeType::Element { .. })
    }

    /// Check if this is a container node (element or root)
    pub fn is_container(&self) -> bool {
        matches!(self, XmlNodeType::Element { .. } | XmlNodeType::Root)
    }

    /// Check if this is a leaf node (text, comment, etc.)
    pub fn is_leaf(&self) -> bool {
        matches!(
            self,
            XmlNodeType::Text | XmlNodeType::Comment | XmlNodeType::CData
        )
    }

    /// Get the tag name if this is an Element node
    pub fn tag(&self) -> Option<&str> {
        match self {
            XmlNodeType::Element { tag } => Some(tag),
            _ => None,
        }
    }

    /// Infer semantic value type from tag name and content
    pub fn infer_value_type(&self, _text: Option<&str>) -> Option<String> {
        match self {
            XmlNodeType::Element { tag } => {
                let tag_lower = tag.to_lowercase();
                // Boolean inference
                if tag_lower.contains("enabled")
                    || tag_lower.contains("active")
                    || tag_lower.contains("visible")
                    || tag_lower.contains("optional")
                {
                    return Some("boolean".to_string());
                }
                // Numeric inference
                if tag_lower.contains("count")
                    || tag_lower.contains("size")
                    || tag_lower.contains("timeout")
                    || tag_lower.contains("port")
                    || tag_lower.contains("max")
                    || tag_lower.contains("min")
                {
                    return Some("number".to_string());
                }
                // URL/Path inference
                if tag_lower.contains("url")
                    || tag_lower.contains("path")
                    || tag_lower.contains("location")
                    || tag_lower.contains("href")
                {
                    return Some("url".to_string());
                }
                None
            }
            _ => None,
        }
    }
}

impl std::fmt::Display for XmlNodeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            XmlNodeType::Root => write!(f, "root"),
            XmlNodeType::Element { tag } => write!(f, "element({})", tag),
            XmlNodeType::Text => write!(f, "text"),
            XmlNodeType::Comment => write!(f, "comment"),
            XmlNodeType::ProcessingInstruction { target } => {
                write!(f, "pi({})", target)
            }
            XmlNodeType::CData => write!(f, "cdata"),
            XmlNodeType::Declaration => write!(f, "declaration"),
        }
    }
}

/// XML node
///
/// Represents a node in the XML tree structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XmlNode {
    /// Unique node ID
    pub id: String,
    /// Node type
    pub node_type: XmlNodeType,
    /// Tag name (for Element nodes)
    pub tag: Option<String>,
    /// Text content (for Text, Comment, CData nodes)
    pub text: Option<String>,
    /// Attributes (for Element nodes)
    pub attributes: HashMap<String, String>,
    /// Full path (e.g., "root.child.grandchild")
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

impl XmlNode {
    /// Create a new XML node
    pub fn new(id: String, node_type: XmlNodeType, path: String, span: Span) -> Self {
        let depth = path.split('.').filter(|s| !s.is_empty()).count();
        Self {
            id,
            node_type,
            tag: None,
            text: None,
            attributes: HashMap::new(),
            path,
            depth,
            parent_id: None,
            children: Vec::new(),
            span,
        }
    }

    /// Set the tag name
    pub fn with_tag(mut self, tag: String) -> Self {
        self.tag = Some(tag);
        self
    }

    /// Set the text content
    pub fn with_text(mut self, text: String) -> Self {
        self.text = Some(text);
        self
    }

    /// Set attributes
    pub fn with_attributes(mut self, attributes: HashMap<String, String>) -> Self {
        self.attributes = attributes;
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

    /// Check if this node has attributes
    pub fn has_attributes(&self) -> bool {
        !self.attributes.is_empty()
    }

    /// Generate text for embedding (original XML format)
    pub fn to_embedding_text(&self) -> String {
        match &self.node_type {
            XmlNodeType::Root => "<root>".to_string(),
            XmlNodeType::Element { tag } => {
                if self.has_attributes() {
                    let attrs: Vec<String> = self
                        .attributes
                        .iter()
                        .map(|(k, v)| format!("{}=\"{}\"", k, v))
                        .take(3)
                        .collect();
                    format!("<{} {}>", tag, attrs.join(" "))
                } else {
                    format!("<{}>", tag)
                }
            }
            XmlNodeType::Text => {
                if let Some(ref text) = self.text {
                    let trimmed = text.trim();
                    if trimmed.len() > 100 {
                        format!("{}...", &trimmed[..100])
                    } else {
                        trimmed.to_string()
                    }
                } else {
                    String::new()
                }
            }
            XmlNodeType::Comment => {
                if let Some(ref text) = self.text {
                    format!("<!-- {} -->", text.trim())
                } else {
                    String::new()
                }
            }
            XmlNodeType::ProcessingInstruction { target } => {
                format!("<?{}?>", target)
            }
            XmlNodeType::CData => "CDATA".to_string(),
            XmlNodeType::Declaration => "<?xml?>".to_string(),
        }
    }

    /// Generate text for BM25 (retain structure with dual representation)
    pub fn to_bm25_text(&self) -> String {
        match &self.node_type {
            XmlNodeType::Root => String::new(),
            XmlNodeType::Element { tag } => {
                let mut parts = vec![format!("<{}>", tag)];
                if self.has_attributes() {
                    let attrs: Vec<String> = self
                        .attributes
                        .iter()
                        .map(|(k, v)| format!("{}=\"{}\"", k, v))
                        .collect();
                    parts.push(format!("attributes: {}", attrs.join(" ")));
                }
                parts.join(" ")
            }
            XmlNodeType::Text => {
                if let Some(ref text) = self.text {
                    text.trim().to_string()
                } else {
                    String::new()
                }
            }
            XmlNodeType::Comment => {
                if let Some(ref text) = self.text {
                    format!("<!-- {} -->", text)
                } else {
                    String::new()
                }
            }
            XmlNodeType::ProcessingInstruction { target } => {
                format!("<?{} ...?>", target)
            }
            XmlNodeType::CData => {
                if let Some(ref text) = self.text {
                    format!("<![CDATA[{}]]>", text)
                } else {
                    String::new()
                }
            }
            XmlNodeType::Declaration => "<?xml ...?>".to_string(),
        }
    }

    /// Generate dotted path representation for BM25 (e.g., "root.config.database.host")
    pub fn to_dotted_path(&self) -> String {
        self.path.clone()
    }

    /// Generate spaced path representation for BM25 (e.g., "root config database host")
    pub fn to_spaced_path(&self) -> String {
        self.path.replace('.', " ")
    }
}

/// XML group type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum XmlGroupType {
    /// Root element
    RootElement,
    /// Nested element
    NestedElement,
    /// Element with children
    ContainerElement,
    /// Leaf element (no children)
    LeafElement,
    /// Text content group
    TextGroup,
}

impl std::fmt::Display for XmlGroupType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            XmlGroupType::RootElement => write!(f, "root_element"),
            XmlGroupType::NestedElement => write!(f, "nested_element"),
            XmlGroupType::ContainerElement => write!(f, "container_element"),
            XmlGroupType::LeafElement => write!(f, "leaf_element"),
            XmlGroupType::TextGroup => write!(f, "text_group"),
        }
    }
}

impl XmlGroupType {
    /// Convert to DocGroupType for compatibility
    pub fn to_doc_group_type(&self) -> crate::types::DocGroupType {
        match self {
            XmlGroupType::RootElement => crate::types::DocGroupType::Chapter,
            XmlGroupType::NestedElement => crate::types::DocGroupType::Section,
            XmlGroupType::ContainerElement => crate::types::DocGroupType::Section,
            XmlGroupType::LeafElement => crate::types::DocGroupType::StandaloneBlock,
            XmlGroupType::TextGroup => crate::types::DocGroupType::ParagraphGroup,
        }
    }
}

/// XML group
///
/// A group of related XML nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XmlGroup {
    /// Group ID
    pub group_id: String,
    /// Group type
    pub group_type: XmlGroupType,
    /// Path prefix
    pub path_prefix: String,
    /// Header node (for element groups)
    pub header: Option<XmlNode>,
    /// Member nodes
    pub members: Vec<XmlNode>,
    /// Combined text for embedding
    pub embedding_text: String,
    /// Combined text for BM25
    pub bm25_text: String,
    /// Estimated token count
    pub token_count: usize,
    /// Source span
    pub span: Span,
}

impl XmlGroup {
    /// Create a new XML group
    pub fn new(group_id: String, group_type: XmlGroupType, path_prefix: String) -> Self {
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

    /// Set the header node
    pub fn set_header(&mut self, node: XmlNode) {
        self.embedding_text = node.to_embedding_text();
        self.bm25_text = node.to_bm25_text();
        self.span = node.span;
        self.header = Some(node);
    }

    /// Add a member node
    pub fn add_member(&mut self, node: XmlNode) {
        self.members.push(node);
    }

    /// Finalize group (compute combined text and token count)
    pub fn finalize(&mut self, estimator: &cce_utils::token_estimation::TokenEstimator) {
        let mut embedding_parts = Vec::new();
        let mut bm25_parts = Vec::new();

        if let Some(ref header) = self.header {
            let emb = header.to_embedding_text();
            if !emb.is_empty() {
                embedding_parts.push(emb);
            }
            let bm = header.to_bm25_text();
            if !bm.is_empty() {
                bm25_parts.push(bm);
            }
        }

        // Track span
        let mut min_start = None;
        let mut max_end = None;

        for member in &self.members {
            let emb_text = member.to_embedding_text();
            let bm_text = member.to_bm25_text();

            if !emb_text.is_empty() {
                embedding_parts.push(emb_text);
            }
            if !bm_text.is_empty() {
                bm25_parts.push(bm_text);
            }

            // Update span
            if min_start.is_none() || member.span.start_position.row < min_start.unwrap() {
                min_start = Some(member.span.start_position.row);
            }
            if max_end.is_none() || member.span.end_position.row > max_end.unwrap() {
                max_end = Some(member.span.end_position.row);
            }
        }

        self.embedding_text = embedding_parts.join("\n");

        // Enhanced BM25 text with dual representation (dotted + spaced paths)
        let base_bm25 = bm25_parts.join("\n");
        let dotted_paths: Vec<String> = self
            .members
            .iter()
            .map(|m| m.to_dotted_path())
            .filter(|p| !p.is_empty())
            .collect();
        let spaced_paths: Vec<String> = self
            .members
            .iter()
            .map(|m| m.to_spaced_path())
            .filter(|p| !p.is_empty())
            .collect();

        // Combine all representations for better search quality
        let mut enhanced_bm25 = base_bm25.clone();
        if !dotted_paths.is_empty() {
            enhanced_bm25.push_str(&format!("\n{}", dotted_paths.join(" ")));
        }
        if !spaced_paths.is_empty() {
            enhanced_bm25.push_str(&format!("\n{}", spaced_paths.join(" ")));
        }
        self.bm25_text = enhanced_bm25;

        self.token_count = estimator.estimate_text(&self.bm25_text);

        // Update span if we have members
        if let (Some(start), Some(end)) = (min_start, max_end) {
            if self.span.start_position.row == 0 && self.span.end_position.row == 0 {
                self.span = Span::from_lines(start, end);
            } else {
                // Extend to include members
                self.span = Span::new(
                    0,
                    0,
                    self.span.start_position.row.min(start),
                    0,
                    self.span.end_position.row.max(end),
                    0,
                );
            }
        }
    }

    /// Check if group has header
    pub fn has_header(&self) -> bool {
        self.header.is_some()
    }

    /// Get all node IDs in this group
    pub fn all_node_ids(&self) -> Vec<String> {
        let mut ids = Vec::new();
        if let Some(ref header) = self.header {
            ids.push(header.id.clone());
        }
        for member in &self.members {
            ids.push(member.id.clone());
        }
        ids
    }
}

impl GenericGroup<XmlNode> for XmlGroup {
    fn group_id(&self) -> &str {
        &self.group_id
    }

    fn group_id_mut(&mut self) -> &mut String {
        &mut self.group_id
    }

    fn header(&self) -> Option<&XmlNode> {
        self.header.as_ref()
    }

    fn header_mut(&mut self) -> &mut Option<XmlNode> {
        &mut self.header
    }

    fn members(&self) -> &[XmlNode] {
        &self.members
    }

    fn members_mut(&mut self) -> &mut Vec<XmlNode> {
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

    fn node_to_embedding_text(node: &XmlNode) -> String {
        node.to_embedding_text()
    }

    fn node_to_bm25_text(node: &XmlNode) -> String {
        node.to_bm25_text()
    }

    fn node_id(node: &XmlNode) -> &str {
        &node.id
    }

    fn node_span(node: &XmlNode) -> &Span {
        &node.span
    }
}

impl DocumentNode for XmlNode {
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
    fn test_xml_node_element() {
        let node = XmlNode::new(
            "test".to_string(),
            XmlNodeType::Element {
                tag: "config".to_string(),
            },
            "root.config".to_string(),
            Span::default(),
        )
        .with_tag("config".to_string());

        let text = node.to_embedding_text();
        assert!(text.contains("<config>"));
    }

    #[test]
    fn test_xml_node_with_attributes() {
        let mut attrs = HashMap::new();
        attrs.insert("id".to_string(), "main".to_string());
        attrs.insert("class".to_string(), "container".to_string());

        let node = XmlNode::new(
            "test".to_string(),
            XmlNodeType::Element {
                tag: "div".to_string(),
            },
            "div".to_string(),
            Span::default(),
        )
        .with_tag("div".to_string())
        .with_attributes(attrs);

        assert!(node.has_attributes());
        let text = node.to_embedding_text();
        assert!(text.contains("<div"));
        assert!(text.contains("id=\"main\""));
        assert!(text.contains("class=\"container\""));
    }

    #[test]
    fn test_xml_node_text() {
        let node = XmlNode::new(
            "test".to_string(),
            XmlNodeType::Text,
            "div.text".to_string(),
            Span::default(),
        )
        .with_text("Hello, World!".to_string());

        let text = node.to_embedding_text();
        assert!(text.contains("Hello"));
        assert!(!text.contains("Text:"));
    }

    #[test]
    fn test_xml_node_comment() {
        let node = XmlNode::new(
            "test".to_string(),
            XmlNodeType::Comment,
            "div.comment".to_string(),
            Span::default(),
        )
        .with_text("This is a comment".to_string());

        let text = node.to_embedding_text();
        assert!(text.contains("<!-- This is a comment -->"));
    }

    #[test]
    fn test_xml_group() {
        let mut group = XmlGroup::new(
            "group1".to_string(),
            XmlGroupType::RootElement,
            String::new(),
        );

        let header = XmlNode::new(
            "root".to_string(),
            XmlNodeType::Element {
                tag: "root".to_string(),
            },
            String::new(),
            Span::default(),
        );
        group.set_header(header);

        assert!(group.has_header());
    }
}
