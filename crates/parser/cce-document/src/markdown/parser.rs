//! Enhanced Markdown parser
//!
//! This module provides parsing functionality for Markdown files using regex patterns.
//! It extracts structural elements like headings, code blocks, lists, etc.
//! Enhanced version preserves hierarchical structure with parent-child relationships.

use regex::Regex;
use std::sync::OnceLock;

use crate::types::{DocNode, DocNodeMeta, DocNodeType};
use cce_types::{ParseError, Span};

/// Helper macro to create a static OnceLock for regex patterns
macro_rules! regex_once_lock {
    ($name:ident, $pattern:expr, $error_msg:expr) => {
        static $name: OnceLock<Result<Regex, ParseError>> = OnceLock::new();
    };
}

// Static compiled regular expressions using OnceLock for performance
regex_once_lock!(HEADING_REGEX, r"^(#{1,6})\s+(.+)$", "Heading regex");
regex_once_lock!(
    CODE_BLOCK_START_REGEX,
    r"^```(\w*)$",
    "Code block start regex"
);
regex_once_lock!(LIST_REGEX, r"^[\s]*[-*+]\s+(.+)$", "List regex");
regex_once_lock!(
    ORDERED_LIST_REGEX,
    r"^[\s]*\d+\.\s+(.+)$",
    "Ordered list regex"
);
regex_once_lock!(BLOCKQUOTE_REGEX, r"^>\s*(.+)$", "Blockquote regex");
regex_once_lock!(THEMATIC_BREAK_REGEX, r"^[-*_]{3,}$", "Thematic break regex");
regex_once_lock!(TABLE_ROW_REGEX, r"^\|(.+)\|$", "Table row regex");
regex_once_lock!(_IMAGE_REGEX, r"!\[([^\]]*)\]\(([^)]+)\)", "Image regex");
regex_once_lock!(LINK_REGEX, r"\[([^\]]+)\]\(([^)]+)\)", "Link regex");
regex_once_lock!(INLINE_CODE_REGEX, r"`([^`]+)`", "Inline code regex");

/// Helper function to get or initialize a regex
fn get_regex(
    once_lock: &'static OnceLock<Result<Regex, ParseError>>,
    pattern: &str,
    error_msg: &str,
) -> Result<&'static Regex, ParseError> {
    once_lock
        .get_or_init(|| {
            Regex::new(pattern)
                .map_err(|e| ParseError::regex_compilation(format!("{}: {}", error_msg, e)))
        })
        .as_ref()
        .map_err(|e| (*e).clone())
}

/// Markdown parser
pub struct MarkdownParser {
    node_counter: usize,
}

impl MarkdownParser {
    /// Create a new Markdown parser
    pub fn new() -> Self {
        Self { node_counter: 0 }
    }

    /// Generate a unique node ID
    fn next_id(&mut self) -> String {
        self.node_counter += 1;
        format!("md_node_{}", self.node_counter)
    }

    /// Parse Markdown content into document nodes
    pub fn parse(&mut self, content: &str) -> Result<Vec<DocNode>, ParseError> {
        let mut nodes = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        let mut i = 0;

        // Get compiled regex patterns with error handling
        let heading_regex = get_regex(&HEADING_REGEX, r"^(#{1,6})\s+(.+)$", "Heading regex")?;
        let code_block_start_regex = get_regex(
            &CODE_BLOCK_START_REGEX,
            r"^```(\w*)$",
            "Code block start regex",
        )?;
        let list_regex = get_regex(&LIST_REGEX, r"^[\s]*[-*+]\s+(.+)$", "List regex")?;
        let ordered_list_regex = get_regex(
            &ORDERED_LIST_REGEX,
            r"^[\s]*\d+\.\s+(.+)$",
            "Ordered list regex",
        )?;
        let blockquote_regex = get_regex(&BLOCKQUOTE_REGEX, r"^>\s*(.+)$", "Blockquote regex")?;
        let thematic_break_regex = get_regex(
            &THEMATIC_BREAK_REGEX,
            r"^[-*_]{3,}$",
            "Thematic break regex",
        )?;
        let table_row_regex = get_regex(&TABLE_ROW_REGEX, r"^\|(.+)\|$", "Table row regex")?;

        while i < lines.len() {
            let line = lines[i];

            // Skip empty lines
            if line.trim().is_empty() {
                i += 1;
                continue;
            }

            // Heading (ATX style)
            if let Some(caps) = heading_regex.captures(line) {
                let level = caps.get(1).map(|m| m.as_str().len()).unwrap_or(1);
                let content = caps
                    .get(2)
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default();

                let mut metadata = DocNodeMeta::new();
                self.extract_inline_elements(&content, &mut metadata);

                let mut node = DocNode::new(
                    self.next_id(),
                    DocNodeType::Heading { level },
                    content,
                    Span::from_line(i),
                );
                node.depth = level;
                node.metadata = metadata;
                nodes.push(node);
                i += 1;
                continue;
            }

            // Code block
            if let Some(caps) = code_block_start_regex.captures(line) {
                let language = caps.get(1).map(|m| m.as_str().to_string());
                let start = i;
                i += 1;

                // Find code block end
                let mut code_content = String::new();
                while i < lines.len() && !lines[i].trim().starts_with("```") {
                    if !code_content.is_empty() {
                        code_content.push('\n');
                    }
                    code_content.push_str(lines[i]);
                    i += 1;
                }

                let mut node = DocNode::new(
                    self.next_id(),
                    DocNodeType::CodeBlock { language },
                    code_content,
                    Span::from_lines(start, i.min(lines.len().saturating_sub(1))),
                );
                node.depth = 0;
                nodes.push(node);
                i += 1; // Skip end marker
                continue;
            }

            // List item (unordered)
            if let Some(caps) = list_regex.captures(line) {
                let content = caps
                    .get(1)
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default();

                let mut metadata = DocNodeMeta::new();
                self.extract_inline_elements(&content, &mut metadata);

                let mut node = DocNode::new(
                    self.next_id(),
                    DocNodeType::List { ordered: false },
                    content,
                    Span::from_line(i),
                );
                node.metadata = metadata;
                nodes.push(node);
                i += 1;
                continue;
            }

            // List item (ordered)
            if let Some(caps) = ordered_list_regex.captures(line) {
                let content = caps
                    .get(1)
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default();

                let mut metadata = DocNodeMeta::new();
                self.extract_inline_elements(&content, &mut metadata);

                let mut node = DocNode::new(
                    self.next_id(),
                    DocNodeType::List { ordered: true },
                    content,
                    Span::from_line(i),
                );
                node.metadata = metadata;
                nodes.push(node);
                i += 1;
                continue;
            }

            // Blockquote
            if let Some(caps) = blockquote_regex.captures(line) {
                let content = caps
                    .get(1)
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default();

                let mut metadata = DocNodeMeta::new();
                self.extract_inline_elements(&content, &mut metadata);

                let mut node = DocNode::new(
                    self.next_id(),
                    DocNodeType::Blockquote,
                    content,
                    Span::from_line(i),
                );
                node.metadata = metadata;
                nodes.push(node);
                i += 1;
                continue;
            }

            // Thematic break
            if thematic_break_regex.is_match(line) {
                let node = DocNode::new(
                    self.next_id(),
                    DocNodeType::ThematicBreak,
                    String::new(),
                    Span::from_line(i),
                );
                nodes.push(node);
                i += 1;
                continue;
            }

            // Table row
            if let Some(caps) = table_row_regex.captures(line) {
                let content = caps
                    .get(1)
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default();

                let node = DocNode::new(
                    self.next_id(),
                    DocNodeType::TableRow,
                    content,
                    Span::from_line(i),
                );
                nodes.push(node);
                i += 1;
                continue;
            }

            // Default: treat as paragraph
            let start = i;
            let mut paragraph_content = String::new();

            // Collect consecutive non-empty lines as paragraph
            while i < lines.len() {
                let line = lines[i];
                if line.trim().is_empty() {
                    break;
                }
                // Check if it's the start of other special structures
                if heading_regex.is_match(line)
                    || code_block_start_regex.is_match(line)
                    || list_regex.is_match(line)
                    || ordered_list_regex.is_match(line)
                    || blockquote_regex.is_match(line)
                    || thematic_break_regex.is_match(line)
                    || table_row_regex.is_match(line)
                {
                    break;
                }
                if !paragraph_content.is_empty() {
                    paragraph_content.push(' ');
                }
                paragraph_content.push_str(line.trim());
                i += 1;
            }

            if !paragraph_content.is_empty() {
                let mut metadata = DocNodeMeta::new();
                self.extract_inline_elements(&paragraph_content, &mut metadata);

                let mut node = DocNode::new(
                    self.next_id(),
                    DocNodeType::Paragraph,
                    paragraph_content,
                    Span::from_lines(start, i.saturating_sub(1)),
                );
                node.metadata = metadata;
                nodes.push(node);
            }
        }

        // Build hierarchical relationships
        self.build_hierarchy(&mut nodes);

        Ok(nodes)
    }

    /// Extract inline elements (links, inline code, etc.) from content
    fn extract_inline_elements(&self, content: &str, metadata: &mut DocNodeMeta) {
        // Extract links (but not images)
        if let Ok(link_regex) = get_regex(&LINK_REGEX, r"\[([^\]]+)\]\(([^)]+)\)", "Link regex") {
            for caps in link_regex.captures_iter(content) {
                if let (Some(text_match), Some(url_match)) = (caps.get(1), caps.get(2)) {
                    // Skip if this is actually an image (preceded by !)
                    let url = url_match.as_str();
                    let text = text_match.as_str();
                    metadata.add_link(text.to_string(), url.to_string());
                }
            }
        }

        // Extract inline code
        if let Ok(inline_code_regex) =
            get_regex(&INLINE_CODE_REGEX, r"`([^`]+)`", "Inline code regex")
        {
            for caps in inline_code_regex.captures_iter(content) {
                if let Some(code_match) = caps.get(1) {
                    let code = code_match.as_str();
                    let start = code_match.start();
                    let end = code_match.end();
                    metadata.add_inline_code(code.to_string(), start, end);
                }
            }
        }
    }

    /// Build parent-child relationships based on heading hierarchy
    fn build_hierarchy(&self, nodes: &mut [DocNode]) {
        // Stack of heading node indices, keyed by their level
        let mut heading_stack: Vec<(usize, usize)> = Vec::new(); // (index, level)

        for i in 0..nodes.len() {
            let current_level = nodes[i].node_type.heading_level();

            if let Some(level) = current_level {
                // Pop headings with same or higher level
                while let Some((_, last_level)) = heading_stack.last() {
                    if *last_level >= level {
                        heading_stack.pop();
                    } else {
                        break;
                    }
                }

                // Set parent
                if let Some(&(parent_idx, _)) = heading_stack.last() {
                    nodes[i].parent_id = Some(nodes[parent_idx].id.clone());
                    nodes[i].depth = level;
                    nodes[parent_idx].add_child(nodes[i].id.clone());
                }

                // Push current heading
                heading_stack.push((i, level));
            } else {
                // Non-heading: find nearest parent heading
                if let Some(&(parent_idx, parent_level)) = heading_stack.last() {
                    nodes[i].parent_id = Some(nodes[parent_idx].id.clone());
                    nodes[i].depth = parent_level;
                    nodes[parent_idx].add_child(nodes[i].id.clone());
                }
            }
        }
    }
}

impl Default for MarkdownParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_heading() {
        let mut parser = MarkdownParser::new();
        let content = "# Heading 1\n## Heading 2\n### Heading 3";
        let nodes = parser.parse(content).expect("should parse headings");

        assert_eq!(nodes.len(), 3);
        assert!(matches!(
            nodes[0].node_type,
            DocNodeType::Heading { level: 1 }
        ));
        assert!(matches!(
            nodes[1].node_type,
            DocNodeType::Heading { level: 2 }
        ));
        assert!(matches!(
            nodes[2].node_type,
            DocNodeType::Heading { level: 3 }
        ));
    }

    #[test]
    fn test_parse_code_block() {
        let mut parser = MarkdownParser::new();
        let content = "```rust\nfn main() {\n    println!(\"Hello\");\n}\n```";
        let nodes = parser.parse(content).expect("should parse code block");

        assert_eq!(nodes.len(), 1);
        assert!(matches!(
            nodes[0].node_type,
            DocNodeType::CodeBlock { language: Some(_) }
        ));
        if let DocNodeType::CodeBlock { language } = &nodes[0].node_type {
            assert_eq!(language.as_deref(), Some("rust"));
        }
        assert!(nodes[0].content.contains("println"));
    }

    #[test]
    fn test_hierarchy() {
        let mut parser = MarkdownParser::new();
        let content = "# Chapter\nSome text\n## Section\nMore text";
        let nodes = parser.parse(content).expect("should parse with hierarchy");

        assert_eq!(nodes.len(), 4);

        // Chapter heading
        assert!(nodes[0].parent_id.is_none());
        assert!(nodes[0].has_children());

        // First paragraph under Chapter
        assert_eq!(nodes[1].parent_id, Some(nodes[0].id.clone()));
        assert_eq!(nodes[1].depth, 1);

        // Section heading
        assert_eq!(nodes[2].parent_id, Some(nodes[0].id.clone()));
        assert!(nodes[2].has_children());

        // Second paragraph under Section
        assert_eq!(nodes[3].parent_id, Some(nodes[2].id.clone()));
        assert_eq!(nodes[3].depth, 2);
    }

    #[test]
    fn test_inline_elements() {
        let mut parser = MarkdownParser::new();
        let content = "Check out [Rust](https://rust-lang.org) and use `println!` macro.";
        let nodes = parser
            .parse(content)
            .expect("should parse with inline elements");

        assert_eq!(nodes.len(), 1);
        assert!(!nodes[0].metadata.links.is_empty());
        assert!(!nodes[0].metadata.inline_code.is_empty());
        assert_eq!(nodes[0].metadata.links[0].text, "Rust");
        assert_eq!(nodes[0].metadata.inline_code[0].content, "println!");
    }
}
