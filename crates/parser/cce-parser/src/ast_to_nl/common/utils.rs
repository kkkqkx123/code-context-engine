//! Common utility functions shared across AST-to-NL modules
//!
//! This module provides shared utility functions to reduce code duplication
//! between different components of the AST-to-NL pipeline:
//!
//! - **BM25 and Embedding Generators**: Shared entity group creation
//! - **Converter and Chunker**: UTF-8 boundary checking and text alignment
//! - **Text Processing**: Proportional position calculation for dual-path text
//!
//! # Design Principles
//!
//! Functions in this module should be:
//! 1. **Pure**: No side effects, deterministic output
//! 2. **Generic**: Applicable across multiple modules
//! 3. **Well-Tested**: Comprehensive test coverage for edge cases
//! 4. **Documented**: Clear examples and usage patterns

use crate::grouper::types::{EntityGroup, GroupType, PatternInfo};
use cce_types::entity::GroupedEntity;
use cce_utils::{normalize_whitespace, normalize_whitespace_preserving_newlines};
use compact_str::CompactString;
use once_cell::sync::Lazy;
use regex::Regex;

static RST_ROLE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r":\w+:`([^`]*)`").expect("valid RST role regex"));

/// Clean documentation content for export and indexing.
///
/// This helper removes fenced code blocks and code-like formatting noise while
/// keeping prose, headings, and list structure intact. It also intelligently
/// re-segments paragraphs by collapsing hard line breaks within paragraphs
/// into spaces, producing natural language text that is more suitable for
/// embedding and BM25 indexing.
pub fn clean_comment_content(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }

    let mut lines = Vec::new();
    let mut in_code_block = false;
    let mut last_was_blank = false;

    for raw_line in text.lines() {
        let trimmed = raw_line.trim();

        if is_code_fence_line(trimmed) {
            in_code_block = !in_code_block;
            last_was_blank = true;
            continue;
        }

        if in_code_block || is_rst_directive(trimmed) || is_link_definition(trimmed) {
            continue;
        }

        let cleaned = clean_comment_content_line(trimmed);
        if cleaned.is_empty() {
            if !last_was_blank && !lines.is_empty() {
                lines.push(String::new());
                last_was_blank = true;
            }
            continue;
        }

        lines.push(cleaned);
        last_was_blank = false;
    }

    while lines.first().is_some_and(|line| line.trim().is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }

    let normalized = normalize_whitespace_preserving_newlines(&lines.join("\n"));
    resegment_paragraphs(&normalized)
}

/// Re-segment paragraphs by collapsing hard line breaks within paragraphs.
///
/// This function processes cleaned comment text to produce more natural language-friendly
/// output by:
///
/// 1. **Preserving structure**: Headings, list items, and other structural elements
///    remain on separate lines
/// 2. **Collapsing paragraphs**: Consecutive non-empty lines that are part of the
///    same paragraph are joined with spaces
/// 3. **Maintaining separation**: Paragraphs are separated by exactly one blank line
///
/// The result is text that flows naturally without arbitrary hard line breaks,
/// making it more suitable for natural language processing tasks like embedding
/// generation and BM25 indexing.
///
/// # Examples
///
/// ```
/// use cce_parser::ast_to_nl::clean_comment_content;
///
/// let doc = "once_cell provides two new cell-like types, unsync OnceCell and\n\
///            sync OnceCell. A OnceCell might store arbitrary non-Copy types, can\n\
///            be assigned to at most once and provides direct access to the stored\n\
///            contents.";
///
/// let cleaned = clean_comment_content(doc);
/// assert!(!cleaned.contains('\n'));
/// ```
pub fn resegment_paragraphs(text: &str) -> String {
    let mut result = Vec::new();
    let mut current_paragraph = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            if !current_paragraph.is_empty() {
                result.push(current_paragraph.join(" "));
                current_paragraph.clear();
            }
            continue;
        }

        if is_structural_line(trimmed) {
            if !current_paragraph.is_empty() {
                result.push(current_paragraph.join(" "));
                current_paragraph.clear();
            }
            result.push(trimmed.to_string());
        } else {
            current_paragraph.push(trimmed.to_string());
        }
    }

    if !current_paragraph.is_empty() {
        result.push(current_paragraph.join(" "));
    }

    result.join("\n")
}

/// Check if a line is a structural element that should remain on its own line.
///
/// Structural elements include:
/// - Markdown headings (`#`, `##`, etc.)
/// - List items (`-`, `*`, `+`, numbered lists)
/// - Table rows (starting with `|`)
pub fn is_structural_line(line: &str) -> bool {
    if line.starts_with('#') {
        return true;
    }

    if line.starts_with("- ") || line.starts_with("* ") || line.starts_with("+ ") {
        return true;
    }

    if line.starts_with('|') {
        return true;
    }

    if let Some(first_char) = line.chars().next() {
        if first_char.is_ascii_digit() {
            let after_digits: String = line.chars().take_while(|c| c.is_ascii_digit()).collect();
            let rest = &line[after_digits.len()..];
            if rest.starts_with(". ") {
                return true;
            }
        }
    }

    false
}

fn clean_comment_content_line(line: &str) -> String {
    if line.is_empty() {
        return String::new();
    }

    if is_table_row(line) {
        return line.to_string();
    }

    let (prefix, body) = split_markdown_prefix(line);

    let mut cleaned = body.to_string();
    cleaned = RST_ROLE_RE.replace_all(&cleaned, "$1").to_string();

    cleaned = normalize_whitespace(&cleaned);
    if cleaned.is_empty() {
        return String::new();
    }

    if prefix.is_empty() {
        cleaned
    } else {
        format!("{}{}", prefix, cleaned)
    }
}

fn is_table_row(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with('|') && trimmed.ends_with('|')
}

fn split_markdown_prefix(line: &str) -> (&str, &str) {
    if let Some((prefix_len, rest_index)) = heading_prefix(line) {
        return (&line[..prefix_len], &line[rest_index..]);
    }

    if let Some(rest) = line.strip_prefix("- ") {
        return ("- ", rest);
    }
    if let Some(rest) = line.strip_prefix("+ ") {
        return ("+ ", rest);
    }
    if let Some(rest) = line.strip_prefix("* ") {
        return ("* ", rest);
    }

    if let Some((prefix_len, rest_index)) = numbered_list_prefix(line) {
        return (&line[..prefix_len], &line[rest_index..]);
    }

    ("", line)
}

fn heading_prefix(line: &str) -> Option<(usize, usize)> {
    let bytes = line.as_bytes();
    let mut idx = 0;
    while idx < bytes.len() && bytes[idx] == b'#' {
        idx += 1;
    }

    if idx == 0 || idx >= bytes.len() || !bytes[idx].is_ascii_whitespace() {
        return None;
    }

    Some((idx + 1, idx + 1))
}

fn numbered_list_prefix(line: &str) -> Option<(usize, usize)> {
    let bytes = line.as_bytes();
    let mut idx = 0;
    while idx < bytes.len() && bytes[idx].is_ascii_digit() {
        idx += 1;
    }

    if idx == 0 || idx + 1 >= bytes.len() {
        return None;
    }

    if bytes[idx] != b'.' || !bytes[idx + 1].is_ascii_whitespace() {
        return None;
    }

    Some((idx + 2, idx + 2))
}

fn is_code_fence_line(line: &str) -> bool {
    line.starts_with("```") || line.starts_with("~~~")
}

fn is_rst_directive(line: &str) -> bool {
    line.starts_with(".. ")
}

fn is_link_definition(line: &str) -> bool {
    line.starts_with('[') && line.contains("]:")
}

/// Find safe UTF-8 character boundary position
///
/// This function ensures that byte positions used for string slicing are at valid
/// UTF-8 character boundaries, preventing panics from invalid UTF-8 sequences.
///
/// # Arguments
///
/// * `text` - The text to check
/// * `pos` - The desired byte position
///
/// # Returns
///
/// A safe byte position that is a valid UTF-8 character boundary
///
/// # Examples
///
/// ```
/// use cce_parser::ast_to_nl::safe_utf8_boundary;
///
/// let text = "Hello é";
/// let safe_pos = safe_utf8_boundary(text, 7); // Returns 6 (before the second byte of 'é')
/// ```
pub fn safe_utf8_boundary(text: &str, pos: usize) -> usize {
    if pos >= text.len() {
        return text.len();
    }
    if text.is_char_boundary(pos) {
        return pos;
    }
    // Move backward to find safe boundary
    let mut p = pos;
    while p > 0 && !text.is_char_boundary(p) {
        p -= 1;
    }
    p
}

/// Create a standalone entity group from a single entity
///
/// This function is used by both EmbeddingGenerator and Bm25Generator
/// to convert a single entity into a group for unified processing.
///
/// # Arguments
///
/// * `entity` - The entity to convert into a standalone group
///
/// # Returns
///
/// An EntityGroup containing the entity as its header with no members
pub fn create_standalone_group(entity: &GroupedEntity) -> EntityGroup {
    EntityGroup {
        group_id: CompactString::from(format!("standalone_{}", entity.id.0)),
        group_type: GroupType::Standalone,
        header: Some(entity.clone()),
        header_id: Some(entity.id),
        members: smallvec::SmallVec::new(),
        member_ids: smallvec::SmallVec::new(),
        entity_spans: std::collections::HashMap::new(),
        combined_source: None,
        combined_source_lazy: std::sync::OnceLock::new(),
        span: cce_types::Span::default(),
        kind: entity.kind,
        name: CompactString::from(&entity.name),
        language: cce_types::language::Language::Unknown,
        pattern_info: PatternInfo::None,
        member_roles: smallvec::SmallVec::new(),
        nested_groups: Box::new([]),
        nesting_level: 0,
        parent_group_id: None,
        has_significant_nested: false,
        metadata: std::collections::HashMap::new(),
        test_info: cce_types::TestInfo::unknown(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use cce_types::entity::{EntityId, EntityKind};

    #[test]
    fn test_safe_utf8_boundary_ascii() {
        let text = "Hello World";
        assert_eq!(safe_utf8_boundary(text, 0), 0);
        assert_eq!(safe_utf8_boundary(text, 5), 5);
        assert_eq!(safe_utf8_boundary(text, 11), 11);
        assert_eq!(safe_utf8_boundary(text, 20), 11); // Beyond end
    }

    #[test]
    fn test_safe_utf8_boundary_unicode() {
        let text = "Hello 世界";
        // First CJK glyph starts at byte 6, second at byte 9
        assert_eq!(safe_utf8_boundary(text, 0), 0);
        assert_eq!(safe_utf8_boundary(text, 5), 5); // After 'o'
        assert_eq!(safe_utf8_boundary(text, 6), 6); // At first CJK glyph
        assert_eq!(safe_utf8_boundary(text, 7), 6); // Between the first glyph's bytes, moves back
        assert_eq!(safe_utf8_boundary(text, 8), 6); // Between the first glyph's bytes, moves back
        assert_eq!(safe_utf8_boundary(text, 9), 9); // At second CJK glyph
        assert_eq!(safe_utf8_boundary(text, 10), 9); // Between the second glyph's bytes, moves back
    }

    #[test]
    fn test_safe_utf8_boundary_emoji() {
        let text = "Hello 🌍";
        // Emoji is 4 bytes starting at position 6
        assert_eq!(safe_utf8_boundary(text, 6), 6);
        assert_eq!(safe_utf8_boundary(text, 7), 6); // Middle of emoji
        assert_eq!(safe_utf8_boundary(text, 8), 6); // Middle of emoji
        assert_eq!(safe_utf8_boundary(text, 9), 6); // Middle of emoji
        assert_eq!(safe_utf8_boundary(text, 10), 10); // After emoji
    }

    #[test]
    fn test_create_standalone_group() {
        let entity = GroupedEntity {
            id: EntityId(42),
            kind: EntityKind::Function,
            name: "test_function".to_string(),
            signature: "fn test_function() -> i32".to_string(),
            parameters: smallvec::smallvec![],
            return_type: Some("i32".to_string()),
            doc_comment: Some("/// Test function".to_string()),
            modifiers: Vec::new(),
            attributes: HashMap::new(),
            subtype: None,
            is_stdlib: false,
            stdlib_category: None,
            metadata: Default::default(),
        };

        let group = create_standalone_group(&entity);

        assert_eq!(group.group_id, "standalone_42");
        assert_eq!(group.group_type, GroupType::Standalone);
        assert!(group.header.is_some());
        assert_eq!(group.header.as_ref().unwrap().name, "test_function");
        assert!(group.members.is_empty());
        assert_eq!(group.kind, EntityKind::Function);
    }

    #[test]
    fn test_clean_comment_content_preserves_code_symbols() {
        let doc = r#"Use `*args` and `**kwargs` for variable arguments.

Access `arr[0]` and see `std::collections::HashMap`.
Use `Option<T>` for nullable values."#;
        let cleaned = clean_comment_content(doc);

        assert!(cleaned.contains("*args"));
        assert!(cleaned.contains("**kwargs"));
        assert!(cleaned.contains("arr[0]"));
        assert!(cleaned.contains("std::collections::HashMap"));
        assert!(cleaned.contains("Option<T>"));
    }

    #[test]
    fn test_clean_comment_content_preserves_inline_url() {
        let doc = r#"See [docs](https://docs.rs/once_cell) for more info."#;
        let cleaned = clean_comment_content(doc);

        assert!(cleaned.contains("https://docs.rs/once_cell"));
    }

    #[test]
    fn test_clean_comment_content_strips_rst_roles() {
        let doc = r#"Use :meth:`get_mut` to access the inner value.
See :class:`OnceCell` and :func:`new`."#;
        let cleaned = clean_comment_content(doc);

        assert!(cleaned.contains("get_mut"));
        assert!(cleaned.contains("OnceCell"));
        assert!(cleaned.contains("new"));
        assert!(!cleaned.contains(":meth:"));
        assert!(!cleaned.contains(":class:"));
        assert!(!cleaned.contains(":func:"));
    }

    #[test]
    fn test_clean_comment_content_removes_code_blocks_and_symbols() {
        let doc = r#"# Overview

`once_cell` provides [`unsync::OnceCell`] and [`sync::OnceCell`].

```rust
impl<T> OnceCell<T> {
    fn get(&self) -> Option<&T> { ... }
}
```

[`unsync::OnceCell`]: unsync/struct.OnceCell.html
"#;
        let cleaned = clean_comment_content(doc);

        assert!(cleaned.contains("# Overview"));
        assert!(cleaned.contains("once_cell"));
        assert!(cleaned.contains("unsync::OnceCell"));
        assert!(!cleaned.contains("```rust"));
        assert!(!cleaned.contains("impl<T>"));
        assert!(!cleaned.contains("[`unsync::OnceCell`]:"));
        assert!(!cleaned.contains("&T"));
    }

    #[test]
    fn test_clean_comment_content_resegments_paragraphs() {
        let doc = "once_cell provides two new cell-like types, unsync OnceCell and\n\
                   sync OnceCell. A OnceCell might store arbitrary non-Copy types, can\n\
                   be assigned to at most once and provides direct access to the stored\n\
                   contents. The core API looks roughly like this (and there's much more\n\
                   inside, read on!):";

        let cleaned = clean_comment_content(doc);

        assert!(!cleaned.contains('\n'));
        assert!(cleaned.contains(
            "once_cell provides two new cell-like types, unsync OnceCell and sync OnceCell."
        ));
        assert!(cleaned.contains("contents."));
    }

    #[test]
    fn test_clean_comment_content_preserves_headings() {
        let doc = "# Overview\n\nThis is the first paragraph that spans\nmultiple lines and should be joined.\n\n## Details\n\nAnother paragraph here.";

        let cleaned = clean_comment_content(doc);

        assert!(cleaned.contains("# Overview"));
        assert!(cleaned.contains("## Details"));
        assert!(
            cleaned.contains("first paragraph that spans multiple lines and should be joined.")
        );
        assert!(cleaned.contains("Another paragraph here."));
    }

    #[test]
    fn test_clean_comment_content_preserves_list_items() {
        let doc = "Some text here.\n\n- First item\n- Second item\n- Third item\n\nMore text.";

        let cleaned = clean_comment_content(doc);

        assert!(cleaned.contains("Some text here."));
        assert!(cleaned.contains("- First item"));
        assert!(cleaned.contains("- Second item"));
        assert!(cleaned.contains("- Third item"));
        assert!(cleaned.contains("More text."));
    }

    #[test]
    fn test_clean_comment_content_preserves_numbered_lists() {
        let doc = "Instructions:\n\n1. First step\n2. Second step\n3. Third step";

        let cleaned = clean_comment_content(doc);

        assert!(cleaned.contains("Instructions:"));
        assert!(cleaned.contains("1. First step"));
        assert!(cleaned.contains("2. Second step"));
        assert!(cleaned.contains("3. Third step"));
    }

    #[test]
    fn test_clean_comment_content_preserves_tables() {
        let doc = "Comparison:\n\n| Type | Access | Drawback |\n|------|--------|----------|\n| Cell | T | requires Copy |";

        let cleaned = clean_comment_content(doc);

        assert!(cleaned.contains("Comparison:"));
        assert!(cleaned.contains("| Type | Access | Drawback |"));
        assert!(cleaned.contains("| Cell | T | requires Copy |"));
        assert!(cleaned.contains("|------|--------|----------|"));
    }

    #[test]
    fn test_is_table_row() {
        assert!(is_table_row("| Column | Column |"));
        assert!(is_table_row("|------|--------|----------|"));
        assert!(!is_table_row("Not a table"));
        assert!(!is_table_row("|Incomplete"));
    }

    #[test]
    fn test_resegment_paragraphs_empty_input() {
        assert_eq!(resegment_paragraphs(""), "");
    }

    #[test]
    fn test_resegment_paragraphs_single_line() {
        assert_eq!(resegment_paragraphs("Single line"), "Single line");
    }

    #[test]
    fn test_is_structural_line_headings() {
        assert!(is_structural_line("# Heading"));
        assert!(is_structural_line("## Subheading"));
        assert!(is_structural_line("### Deep heading"));
        assert!(!is_structural_line("Not a heading"));
        assert!(is_structural_line("#NoSpace"));
    }

    #[test]
    fn test_is_structural_line_list_items() {
        assert!(is_structural_line("- Item"));
        assert!(is_structural_line("* Item"));
        assert!(is_structural_line("+ Item"));
        assert!(!is_structural_line("Not a list"));
        assert!(!is_structural_line("-NoSpace"));
    }

    #[test]
    fn test_is_structural_line_numbered_lists() {
        assert!(is_structural_line("1. First"));
        assert!(is_structural_line("10. Tenth"));
        assert!(!is_structural_line("1.NoSpace"));
        assert!(!is_structural_line("Just a number 123"));
    }

    #[test]
    fn test_is_structural_line_tables() {
        assert!(is_structural_line("| Column | Column |"));
        assert!(is_structural_line("|---|---|"));
        assert!(!is_structural_line("Not a table"));
    }
}
