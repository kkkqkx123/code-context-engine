//! Utility functions for entity and relation extractors
//!
//! This module provides common utility functions shared between
//! EntityExtractor and RelationExtractor to avoid code duplication.

use crate::tree_sitter_query::executor::Capture;
use cce_types::Span;
use cce_utils::normalize_whitespace;
use tree_sitter::{Node, Tree};

/// Create a Span from a Capture
///
/// Converts tree-sitter capture position information into a Span structure.
/// This is a common operation in both entity and relation extraction.
///
/// # Arguments
///
/// * `capture` - The capture containing position information
///
/// # Returns
///
/// A Span with byte offsets and row/column positions
pub fn create_span_from_capture(capture: &Capture) -> Span {
    Span::new(
        capture.start_byte,
        capture.end_byte,
        capture.start_point.0,
        capture.start_point.1,
        capture.end_point.0,
        capture.end_point.1,
    )
}

/// Extract text from source code using byte offsets
///
/// Safely extracts a substring from source code given start and end byte positions.
/// Returns an empty string if the positions are invalid.
///
/// # Arguments
///
/// * `source` - The source code string
/// * `start` - Start byte offset (inclusive)
/// * `end` - End byte offset (exclusive)
///
/// # Returns
///
/// The extracted substring, or empty string if positions are invalid
pub fn extract_text_from_source(source: &str, start: usize, end: usize) -> String {
    if end <= source.len() && start < end {
        source[start..end].to_string()
    } else {
        String::new()
    }
}

/// Find the smallest span interval containing a position.
///
/// `spans` must be sorted by start byte. Returns the entity ID of the
/// smallest `(start, end)` interval with `start <= pos < end`; ties are
/// broken toward the latest start. Used to attribute a structural relation
/// to its owning component/element by span instead of a hardcoded caller.
pub fn find_smallest_containing(
    spans: &[(usize, usize, cce_types::EntityId)],
    pos: usize,
) -> Option<cce_types::EntityId> {
    if spans.is_empty() {
        return None;
    }
    let idx = match spans.binary_search_by_key(&pos, |&(start, _, _)| start) {
        Ok(i) => i,
        Err(i) => i.saturating_sub(1),
    };
    for i in (0..=idx).rev() {
        let (start, end, id) = spans[i];
        if start <= pos && pos < end {
            let mut best_id = id;
            let mut best_size = end - start;
            for j in (0..i).rev() {
                let (s, e, candidate_id) = spans[j];
                if s <= pos && pos < e {
                    let size = e - s;
                    if size < best_size {
                        best_size = size;
                        best_id = candidate_id;
                    }
                } else if e <= pos {
                    break;
                }
            }
            return Some(best_id);
        }
    }
    None
}

/// Sort entity spans by start byte (in place) for `find_smallest_containing`.
pub fn sort_spans_by_start(spans: &mut [(usize, usize, cce_types::EntityId)]) {
    spans.sort_by_key(|&(start, _, _)| start);
}

/// Extract text from source while removing comment nodes inside the span.
///
/// This is used by behavior/control-flow sidecars so the stored facts keep the
/// matched code structure without inheriting inline comments from the source.
pub fn extract_text_without_comments(
    tree: &Tree,
    source: &str,
    start: usize,
    end: usize,
) -> String {
    if end <= source.len() && start < end {
        let mut comment_ranges = Vec::new();
        collect_comment_ranges(tree.root_node(), start, end, &mut comment_ranges);

        if comment_ranges.is_empty() {
            return normalize_whitespace(&source[start..end]);
        }

        comment_ranges.sort_by_key(|(range_start, _)| *range_start);

        let mut cleaned = String::new();
        let mut cursor = start;

        for (comment_start, comment_end) in comment_ranges {
            let segment_end = comment_start.min(end);
            if cursor < segment_end {
                cleaned.push_str(&source[cursor..segment_end]);
            }
            cursor = cursor.max(comment_end.min(end));
        }

        if cursor < end {
            cleaned.push_str(&source[cursor..end]);
        }

        normalize_whitespace(&cleaned)
    } else {
        String::new()
    }
}

/// Find a capture by name pattern
///
/// Searches through captures to find one whose name matches the given predicate.
/// This is useful for finding specific types of captures (e.g., name captures, type captures).
///
/// # Arguments
///
/// * `captures` - Slice of captures to search
/// * `predicate` - Function that returns true for matching capture names
///
/// # Returns
///
/// The first matching capture, or None if no match is found
pub fn find_capture_by_name<F>(captures: &[Capture], predicate: F) -> Option<&Capture>
where
    F: Fn(&str) -> bool,
{
    captures.iter().find(|c| predicate(&c.name))
}

/// Check if a capture name ends with a specific suffix
///
/// Helper function to check capture name patterns like ".name", ".type", etc.
///
/// # Arguments
///
/// * `capture_name` - The capture name to check
/// * `suffix` - The suffix to look for (e.g., ".name")
///
/// # Returns
///
/// True if the capture name ends with the given suffix
pub fn capture_name_ends_with(capture_name: &str, suffix: &str) -> bool {
    capture_name.ends_with(suffix)
}

/// Check if a capture name contains a specific substring
///
/// Case-insensitive check for substrings in capture names.
/// Useful for finding parameter-related captures, return type captures, etc.
///
/// # Arguments
///
/// * `capture_name` - The capture name to check
/// * `substring` - The substring to search for
///
/// # Returns
///
/// True if the capture name contains the substring (case-insensitive)
pub fn capture_name_contains(capture_name: &str, substring: &str) -> bool {
    capture_name
        .to_lowercase()
        .contains(&substring.to_lowercase())
}

fn collect_comment_ranges(
    node: Node<'_>,
    start: usize,
    end: usize,
    ranges: &mut Vec<(usize, usize)>,
) {
    if node.end_byte() <= start || node.start_byte() >= end {
        return;
    }

    if node.kind().contains("comment") {
        ranges.push((node.start_byte().max(start), node.end_byte().min(end)));
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_comment_ranges(child, start, end, ranges);
    }
}

/// Expand a byte span to the nearest complete statement boundary.
///
/// Walks up the tree-sitter node tree to find the smallest statement-level
/// node that fully contains the given span. This ensures behavior facts
/// capture complete expressions/statements rather than partial fragments.
///
/// # Arguments
///
/// * `tree` - The parsed tree-sitter tree
/// * `start` - Start byte offset of the original span
/// * `end` - End byte offset of the original span
///
/// # Returns
///
/// A tuple of (expanded_start, expanded_end) byte offsets
pub fn expand_to_statement_boundary(tree: &Tree, start: usize, end: usize) -> (usize, usize) {
    let root = tree.root_node();
    let Some(mut node) = root.descendant_for_byte_range(start, end) else {
        return (start, end);
    };

    const STATEMENT_KINDS: &[&str] = &[
        "expression_statement",
        "let_declaration",
        "return_statement",
        "if_statement",
        "match_statement",
        "for_statement",
        "while_statement",
        "loop_statement",
        "assignment_expression",
        "call_expression",
        "binary_expression",
        "method_invocation",
        "field_expression",
        "index_expression",
        "await_expression",
        "yield_expression",
        "throw_statement",
        "try_statement",
        "break_statement",
        "continue_statement",
        "use_statement",
        "import_statement",
        "export_statement",
    ];

    // Structural wrappers that are part of a larger expression: walking
    // through them lets the expansion reach the enclosing statement
    // (e.g. `&[pattern]` -> `arguments` -> `call_expression`).
    const EXPRESSION_WRAPPER_KINDS: &[&str] = &[
        "arguments",
        "member_expression",
        "subscript",
        "subscript_expression",
        "parenthesized_expression",
        "grouped_expression",
        "array_expression",
        "tuple_expression",
        "unary_expression",
        "reference_expression",
    ];

    while let Some(parent) = node.parent() {
        let parent_kind = parent.kind();
        if parent_kind == "source_file"
            || parent_kind == "block"
            || parent_kind == "statement_block"
        {
            break;
        }

        if STATEMENT_KINDS.contains(&parent_kind) || EXPRESSION_WRAPPER_KINDS.contains(&parent_kind)
        {
            node = parent;
        } else {
            break;
        }
    }

    (node.start_byte(), node.end_byte())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast_parser::AstParser;
    use crate::tree_sitter_query::executor::Capture;
    use cce_types::language::Language;

    fn create_test_capture(name: &str, start_byte: usize, end_byte: usize) -> Capture {
        Capture {
            name: name.to_string(),
            text: "test".to_string(),
            start_byte,
            end_byte,
            start_point: (0, 0),
            end_point: (0, 4),
        }
    }

    #[test]
    fn test_create_span_from_capture() {
        let capture = create_test_capture("test.capture", 10, 20);
        let span = create_span_from_capture(&capture);

        assert_eq!(span.start_byte, 10);
        assert_eq!(span.end_byte, 20);
        assert_eq!(span.start_position.row, 0);
        assert_eq!(span.start_position.column, 0);
        assert_eq!(span.end_position.row, 0);
        assert_eq!(span.end_position.column, 4);
    }

    #[test]
    fn test_extract_text_from_source_valid() {
        let source = "fn hello() {}";
        let text = extract_text_from_source(source, 3, 8);
        assert_eq!(text, "hello");
    }

    #[test]
    fn test_extract_text_from_source_invalid_end() {
        let source = "fn hello() {}";
        let text = extract_text_from_source(source, 0, 100);
        assert_eq!(text, "");
    }

    #[test]
    fn test_extract_text_from_source_invalid_range() {
        let source = "fn hello() {}";
        let text = extract_text_from_source(source, 10, 5);
        assert_eq!(text, "");
    }

    #[test]
    fn test_extract_text_without_comments_removes_comment_nodes() {
        let mut parser = AstParser::new();
        let code = r#"
fn demo() {
    let value = foo(); // trailing comment
    let url = "https://example.com";
    let other = 1 << 2; /* block comment */
}
"#;
        let tree = parser
            .parse_with_tree(code, &Language::Rust)
            .expect("Failed to parse Rust code")
            .0;

        let start = code.find("let value").expect("expected value statement");
        let end = code.rfind('}').expect("expected closing brace") + 1;
        let cleaned = extract_text_without_comments(&tree, code, start, end);

        assert!(!cleaned.contains("trailing comment"));
        assert!(!cleaned.contains("block comment"));
        assert!(cleaned.contains("let value = foo();"));
        assert!(cleaned.contains("https://example.com"));
        assert!(cleaned.contains("1 << 2"));
    }

    #[test]
    fn test_find_capture_by_name() {
        let captures = vec![
            create_test_capture("entity.function.name", 0, 5),
            create_test_capture("entity.function.type", 6, 10),
            create_test_capture("entity.class.name", 11, 16),
        ];

        // Find by exact suffix
        let result = find_capture_by_name(&captures, |name| name.ends_with(".name"));
        assert!(result.is_some());
        assert_eq!(result.unwrap().name, "entity.function.name");

        // Find by contains
        let result = find_capture_by_name(&captures, |name| name.contains("class"));
        assert!(result.is_some());
        assert_eq!(result.unwrap().name, "entity.class.name");

        // No match
        let result = find_capture_by_name(&captures, |name| name.contains("method"));
        assert!(result.is_none());
    }

    #[test]
    fn test_capture_name_ends_with() {
        assert!(capture_name_ends_with("entity.function.name", ".name"));
        assert!(capture_name_ends_with("entity.class.name", ".name"));
        assert!(!capture_name_ends_with("entity.function.type", ".name"));
        assert!(!capture_name_ends_with("entity.function", ".name"));
    }

    #[test]
    fn test_capture_name_contains_case_insensitive() {
        assert!(capture_name_contains("entity.PARAMETER.name", "parameter"));
        assert!(capture_name_contains("entity.parameter.name", "PARAMETER"));
        assert!(capture_name_contains("entity.Parameter.name", "parameter"));
        assert!(!capture_name_contains("entity.function.name", "parameter"));
    }

    #[test]
    fn test_expand_to_statement_boundary() {
        let mut parser = AstParser::new();
        let code = r#"
fn demo() {
    let result = self.build_many(&[pattern]);
    result
}
"#;
        let tree = parser
            .parse_with_tree(code, &Language::Rust)
            .expect("Failed to parse")
            .0;

        let pattern_start = code.find("&[pattern]").expect("expected &[pattern]");
        let pattern_end = pattern_start + "&[pattern]".len();

        let (expanded_start, expanded_end) =
            expand_to_statement_boundary(&tree, pattern_start, pattern_end);

        let expanded_text = &code[expanded_start..expanded_end];
        assert!(
            expanded_text.contains("self.build_many"),
            "should expand to full call expression, got: {}",
            expanded_text
        );
    }
}
