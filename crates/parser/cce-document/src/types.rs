//! Document types for structured text processing
//!
//! This module provides types for processing document files (Markdown, XML, etc.)
//! that preserve hierarchical structure and metadata.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::common::{DocumentNode, GenericGroup, code_block_embedding};
use cce_types::FileCategory;
use cce_types::ast_to_nl::ChunkContentType;
use cce_types::{LanguageInfo, Span};

/// Classification derived once at the document-pipeline entry.
///
/// Pairs the chunk payload type with the business category so every
/// downstream stage (chunkers of all six formats, plain-text sub-kinds)
/// reuses the same labels instead of re-deriving them from the file path.
/// The pair is always produced through the unified
/// [`LanguageInfo`] derivation chain.
#[derive(Debug, Clone)]
pub struct DocumentClassification {
    language_info: LanguageInfo,
    payload: ChunkContentType,
    category: FileCategory,
}

impl DocumentClassification {
    /// Detect from a file path (single detection entry).
    pub fn detect(file_path: &str) -> Self {
        Self::from_language_info(LanguageInfo::detect_from_path(file_path), file_path)
    }

    /// Build from already-detected routing information.
    pub fn from_language_info(info: LanguageInfo, file_path: &str) -> Self {
        let payload = info.chunk_content_type_for_path(file_path);
        let category = info.file_category();
        debug_assert!(
            payload.matches_category(category),
            "payload {payload:?} must match category {category:?}"
        );
        Self {
            language_info: info,
            payload,
            category,
        }
    }

    /// Detected routing information (language, extensions, file type).
    pub fn language_info(&self) -> &LanguageInfo {
        &self.language_info
    }

    /// Chunk payload type for this file.
    pub fn payload(&self) -> &ChunkContentType {
        &self.payload
    }

    /// Business-layer category for this file.
    pub fn category(&self) -> FileCategory {
        self.category
    }
}

/// Document type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DocType {
    /// Markdown document
    Markdown,
    /// XML document
    Xml,
    /// Configuration file (TOML, YAML, JSON)
    Config,
    /// Plain text file
    PlainText,
}

impl std::fmt::Display for DocType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DocType::Markdown => write!(f, "markdown"),
            DocType::Xml => write!(f, "xml"),
            DocType::Config => write!(f, "config"),
            DocType::PlainText => write!(f, "plaintext"),
        }
    }
}

/// Document node type
///
/// Represents different types of nodes in a document tree.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DocNodeType {
    /// Heading with level (1-6)
    Heading { level: usize },
    /// Paragraph
    Paragraph,
    /// Code block with optional language
    CodeBlock { language: Option<String> },
    /// List (ordered or unordered)
    List { ordered: bool },
    /// List item
    ListItem,
    /// Blockquote
    Blockquote,
    /// Table
    Table,
    /// Table row
    TableRow,
    /// Thematic break (horizontal rule)
    ThematicBreak,
    /// Image
    Image { alt: String, url: String },
    /// Text node
    Text,
}

impl DocNodeType {
    /// Check if this is a heading node
    pub fn is_heading(&self) -> bool {
        matches!(self, DocNodeType::Heading { .. })
    }

    /// Get heading level if this is a heading
    pub fn heading_level(&self) -> Option<usize> {
        match self {
            DocNodeType::Heading { level } => Some(*level),
            _ => None,
        }
    }

    /// Check if this is a block-level element
    pub fn is_block(&self) -> bool {
        matches!(
            self,
            DocNodeType::Heading { .. }
                | DocNodeType::Paragraph
                | DocNodeType::CodeBlock { .. }
                | DocNodeType::List { .. }
                | DocNodeType::Blockquote
                | DocNodeType::Table
                | DocNodeType::ThematicBreak
        )
    }
}

impl std::fmt::Display for DocNodeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DocNodeType::Heading { level } => write!(f, "heading_{}", level),
            DocNodeType::Paragraph => write!(f, "paragraph"),
            DocNodeType::CodeBlock { language } => {
                if let Some(lang) = language {
                    write!(f, "code_block_{}", lang)
                } else {
                    write!(f, "code_block")
                }
            }
            DocNodeType::List { ordered } => {
                if *ordered {
                    write!(f, "ordered_list")
                } else {
                    write!(f, "unordered_list")
                }
            }
            DocNodeType::ListItem => write!(f, "list_item"),
            DocNodeType::Blockquote => write!(f, "blockquote"),
            DocNodeType::Table => write!(f, "table"),
            DocNodeType::TableRow => write!(f, "table_row"),
            DocNodeType::ThematicBreak => write!(f, "thematic_break"),
            DocNodeType::Image { alt, .. } => write!(f, "image_{}", alt),
            DocNodeType::Text => write!(f, "text"),
        }
    }
}

/// Link information
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkInfo {
    /// Link text
    pub text: String,
    /// Link URL
    pub url: String,
    /// Whether it's an internal link
    pub is_internal: bool,
}

/// Code span (inline code)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeSpan {
    /// Code content
    pub content: String,
    /// Start position in text
    pub start: usize,
    /// End position in text
    pub end: usize,
}

/// Document node metadata
///
/// Stores inline formatting information for a document node.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DocNodeMeta {
    /// Bold text spans (start, end)
    pub bold_spans: Vec<(usize, usize)>,
    /// Italic text spans
    pub italic_spans: Vec<(usize, usize)>,
    /// Links
    pub links: Vec<LinkInfo>,
    /// Inline code spans
    pub inline_code: Vec<CodeSpan>,
    /// Custom attributes (for XML/HTML)
    pub attributes: HashMap<String, String>,
}

impl DocNodeMeta {
    /// Create empty metadata
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if metadata has any content
    pub fn is_empty(&self) -> bool {
        self.bold_spans.is_empty()
            && self.italic_spans.is_empty()
            && self.links.is_empty()
            && self.inline_code.is_empty()
            && self.attributes.is_empty()
    }

    /// Add a link
    pub fn add_link(&mut self, text: String, url: String) {
        let is_internal = url.starts_with('#') || url.starts_with("./") || url.starts_with("../");
        self.links.push(LinkInfo {
            text,
            url,
            is_internal,
        });
    }

    /// Add inline code
    pub fn add_inline_code(&mut self, content: String, start: usize, end: usize) {
        self.inline_code.push(CodeSpan {
            content,
            start,
            end,
        });
    }
}

/// Document node
///
/// Represents a node in the document tree with hierarchical structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocNode {
    /// Unique node ID
    pub id: String,
    /// Node type
    pub node_type: DocNodeType,
    /// Content text
    pub content: String,
    /// Depth level in tree (heading level for headings, parent depth for others)
    pub depth: usize,
    /// Parent node ID
    pub parent_id: Option<String>,
    /// Children node IDs
    pub children: Vec<String>,
    /// Line number range
    pub span: Span,
    /// Metadata (inline formatting, etc.)
    pub metadata: DocNodeMeta,
}

impl DocNode {
    /// Create a new document node
    pub fn new(id: String, node_type: DocNodeType, content: String, span: Span) -> Self {
        let depth = node_type.heading_level().unwrap_or(0);
        Self {
            id,
            node_type,
            content,
            depth,
            parent_id: None,
            children: Vec::new(),
            span,
            metadata: DocNodeMeta::new(),
        }
    }

    /// Set parent
    pub fn with_parent(mut self, parent_id: String) -> Self {
        self.parent_id = Some(parent_id);
        self
    }

    /// Add a child
    pub fn add_child(&mut self, child_id: String) {
        if !self.children.contains(&child_id) {
            self.children.push(child_id);
        }
    }

    /// Check if this node has children
    pub fn has_children(&self) -> bool {
        !self.children.is_empty()
    }

    /// Generate text for embedding (semantic description)
    pub fn to_embedding_text(&self) -> String {
        match &self.node_type {
            DocNodeType::Heading { level } => match level {
                1 => format!("Chapter: {}", self.content.trim()),
                2 => format!("Section: {}", self.content.trim()),
                _ => format!("Subsection (level {}): {}", level, self.content.trim()),
            },
            DocNodeType::CodeBlock { language } => {
                code_block_embedding(&self.content, language.as_deref(), 0)
            }
            DocNodeType::List { ordered } => {
                if *ordered {
                    format!("Ordered entries: {}", self.content.trim())
                } else {
                    format!("List item: {}", self.content.trim())
                }
            }
            DocNodeType::ListItem => format!("List item: {}", self.content.trim()),
            DocNodeType::Blockquote => format!("Quote: {}", self.content.trim()),
            DocNodeType::Table | DocNodeType::TableRow => "Table content".to_string(),
            DocNodeType::ThematicBreak => "divider".to_string(),
            DocNodeType::Image { alt, .. } => format!("Picture: {}", alt),
            DocNodeType::Paragraph | DocNodeType::Text => self.content.trim().to_string(),
        }
    }

    /// Generate text for BM25 (retain structure markers)
    pub fn to_bm25_text(&self) -> String {
        match &self.node_type {
            DocNodeType::Heading { level } => {
                format!("{} {}", "#".repeat(*level), self.content)
            }
            DocNodeType::CodeBlock { language } => {
                let lang = language.as_deref().unwrap_or("");
                format!("```{}\n{}\n```", lang, self.content)
            }
            DocNodeType::List { .. } | DocNodeType::ListItem => {
                format!("- {}", self.content)
            }
            DocNodeType::Blockquote => {
                format!("> {}", self.content)
            }
            DocNodeType::Table | DocNodeType::TableRow => self.content.clone(),
            DocNodeType::ThematicBreak => "---".to_string(),
            DocNodeType::Image { alt, url } => format!("![{}]({})", alt, url),
            DocNodeType::Paragraph | DocNodeType::Text => self.content.clone(),
        }
    }
}

impl DocumentNode for DocNode {
    fn span(&self) -> &Span {
        &self.span
    }

    fn depth(&self) -> usize {
        self.depth
    }
}

/// Document group type
///
/// Represents how document nodes are grouped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DocGroupType {
    /// Chapter (level 1 heading + content)
    Chapter,
    /// Section (level 2+ heading + content)
    Section,
    /// Standalone block (large code block, table, etc.)
    StandaloneBlock,
    /// Paragraph group (consecutive paragraphs)
    ParagraphGroup,
    /// List group (consecutive list items)
    ListGroup,
}

impl std::fmt::Display for DocGroupType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DocGroupType::Chapter => write!(f, "chapter"),
            DocGroupType::Section => write!(f, "section"),
            DocGroupType::StandaloneBlock => write!(f, "standalone"),
            DocGroupType::ParagraphGroup => write!(f, "paragraph_group"),
            DocGroupType::ListGroup => write!(f, "list_group"),
        }
    }
}

impl DocGroupType {
    /// Convert to source code GroupType for compatibility with ChunkedResult
    pub fn to_group_type(&self) -> cce_types::GroupType {
        match self {
            DocGroupType::Chapter => cce_types::GroupType::ModuleWithContents,
            DocGroupType::Section => cce_types::GroupType::ModuleWithContents,
            DocGroupType::StandaloneBlock => cce_types::GroupType::Standalone,
            DocGroupType::ParagraphGroup => cce_types::GroupType::RelatedFunctions,
            DocGroupType::ListGroup => cce_types::GroupType::RelatedFunctions,
        }
    }
}

/// Document group
///
/// A group of related document nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocGroup {
    /// Group ID
    pub group_id: String,
    /// Group type
    pub group_type: DocGroupType,
    /// Header node (for chapter/section groups)
    pub header: Option<DocNode>,
    /// Member nodes
    pub members: Vec<DocNode>,
    /// Combined text for embedding
    pub embedding_text: String,
    /// Combined text for BM25
    pub bm25_text: String,
    /// Estimated token count
    pub token_count: usize,
    /// Source span
    pub span: Span,
}

impl DocGroup {
    /// Create a new document group
    pub fn new(group_id: String, group_type: DocGroupType) -> Self {
        Self {
            group_id,
            group_type,
            header: None,
            members: Vec::new(),
            embedding_text: String::new(),
            bm25_text: String::new(),
            token_count: 0,
            span: Span::default(),
        }
    }
}

impl GenericGroup<DocNode> for DocGroup {
    fn group_id(&self) -> &str {
        &self.group_id
    }

    fn group_id_mut(&mut self) -> &mut String {
        &mut self.group_id
    }

    fn header(&self) -> Option<&DocNode> {
        self.header.as_ref()
    }

    fn header_mut(&mut self) -> &mut Option<DocNode> {
        &mut self.header
    }

    fn members(&self) -> &[DocNode] {
        &self.members
    }

    fn members_mut(&mut self) -> &mut Vec<DocNode> {
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

    fn node_to_embedding_text(node: &DocNode) -> String {
        node.to_embedding_text()
    }

    fn node_to_bm25_text(node: &DocNode) -> String {
        node.to_bm25_text()
    }

    fn node_id(node: &DocNode) -> &str {
        &node.id
    }

    fn node_span(node: &DocNode) -> &Span {
        &node.span
    }
}

/// Document summary
///
/// Structural metadata extracted from a document file for summary-level retrieval.
/// Only reliable structural information is populated — no semantic summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocSummary {
    /// File path
    pub file_path: String,
    /// Document type
    pub doc_type: DocType,
    /// Document title (from first level-1 heading)
    pub title: Option<String>,
    /// Natural language summary line derived from structural metadata.
    summary_text: Option<String>,
    /// Total heading count
    pub heading_count: usize,
    /// Total code block count
    pub code_block_count: usize,
    /// Main headings (top 10)
    pub main_headings: Vec<String>,
    /// Line count
    pub line_count: u32,
}

impl DocSummary {
    /// Create a new document summary
    pub fn new(file_path: String, doc_type: DocType) -> Self {
        Self {
            file_path,
            doc_type,
            title: None,
            summary_text: None,
            heading_count: 0,
            code_block_count: 0,
            main_headings: Vec::new(),
            line_count: 0,
        }
    }

    /// Borrow the derived summary text when available.
    pub fn summary_text(&self) -> Option<&str> {
        self.summary_text.as_deref()
    }

    /// Replace the derived summary text.
    pub fn set_summary_text(&mut self, summary_text: Option<String>) {
        self.summary_text = summary_text;
    }

    /// Generate embedding text for summary
    ///
    /// Only uses reliable structural metadata — no semantic summary text.
    pub fn to_embedding_text(&self) -> String {
        let mut parts = Vec::new();

        if let Some(ref title) = self.title {
            parts.push(format!("Document Title: {}", title));
        }

        parts.push(format!("Type: {}", self.doc_type));

        if self.heading_count > 0 || self.code_block_count > 0 {
            parts.push(format!(
                "Contains {} headers, {} code blocks",
                self.heading_count, self.code_block_count
            ));
        }

        if !self.main_headings.is_empty() {
            parts.push(format!("Main chapters: {}", self.main_headings.join("、")));
        }

        parts.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GenericGroup;
    use cce_types::Span;

    #[test]
    fn test_doc_node_heading() {
        let span = Span::from_lines(0, 1);
        let node = DocNode::new(
            "h1".to_string(),
            DocNodeType::Heading { level: 1 },
            "Introduction".to_string(),
            span,
        );

        assert_eq!(node.depth, 1);
        assert!(node.node_type.is_heading());
        assert_eq!(node.node_type.heading_level(), Some(1));
        assert!(node.to_embedding_text().contains("Chapter: Introduction"));
        assert!(node.to_bm25_text().starts_with("#"));
    }

    #[test]
    fn test_doc_node_code_block() {
        let span = Span::from_lines(0, 5);
        let node = DocNode::new(
            "code1".to_string(),
            DocNodeType::CodeBlock {
                language: Some("rust".to_string()),
            },
            "fn main() {}".to_string(),
            span,
        );

        assert!(node.node_type.is_block());
        assert!(node.to_embedding_text().contains("rust"));
        assert!(node.to_bm25_text().contains("```rust"));
    }

    #[test]
    fn test_doc_node_meta() {
        let mut meta = DocNodeMeta::new();
        meta.add_link("Rust".to_string(), "https://rust-lang.org".to_string());
        meta.add_inline_code("println!".to_string(), 0, 8);

        assert!(!meta.is_empty());
        assert_eq!(meta.links.len(), 1);
        assert_eq!(meta.inline_code.len(), 1);
        assert!(!meta.links[0].is_internal);
    }

    #[test]
    fn test_doc_group() {
        let mut group = DocGroup::new("group1".to_string(), DocGroupType::Chapter);

        let header = DocNode::new(
            "h1".to_string(),
            DocNodeType::Heading { level: 1 },
            "Chapter 1".to_string(),
            Span::from_lines(0, 1),
        );
        group.set_header(header);

        let member = DocNode::new(
            "p1".to_string(),
            DocNodeType::Paragraph,
            "Some content".to_string(),
            Span::from_lines(2, 3),
        );
        group.add_member(member);

        assert!(group.has_header());
        assert_eq!(group.members.len(), 1);
    }
}
