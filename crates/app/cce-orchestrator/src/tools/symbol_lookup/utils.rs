//! Utility functions for symbol lookup tools

use cce_types::{Entity, EntityId, Span};

/// Check if a position (line, column) is contained within a span
pub fn contains_position(span: &Span, line: usize, column: Option<usize>) -> bool {
    let line_matches = span.start_position.row < line && span.end_position.row + 1 >= line;

    if !line_matches {
        return false;
    }

    // If column is specified, check column range
    if let Some(col) = column {
        // For single-line spans, check column
        if span.start_position.row == span.end_position.row {
            return span.start_position.column < col && span.end_position.column + 1 >= col;
        }
        // For multi-line spans, if we're on the start line, check start column
        if span.start_position.row + 1 == line {
            return span.start_position.column < col;
        }
        // If we're on the end line, check end column
        if span.end_position.row + 1 == line {
            return span.end_position.column + 1 >= col;
        }
        // Otherwise, we're in the middle of a multi-line span
        return true;
    }

    true
}

/// Find the entity at a specific position in a list of entities
pub fn find_entity_at_position(
    entities: &[Entity],
    line: usize,
    column: Option<usize>,
) -> Option<EntityId> {
    entities
        .iter()
        .find(|e| contains_position(&e.span, line, column))
        .map(|e| e.id)
}

/// Extract context lines around a position from source code
pub fn extract_context_lines(
    source: &str,
    line: usize,
    context_lines: usize,
) -> Vec<(usize, String)> {
    let lines: Vec<&str> = source.lines().collect();

    if lines.is_empty() || line == 0 || line > lines.len() {
        return Vec::new();
    }

    let start_line = line.saturating_sub(context_lines + 1);
    let end_line = (line + context_lines).min(lines.len());

    (start_line..end_line)
        .map(|i| (i + 1, lines[i].to_string()))
        .collect()
}

/// Format a position as "line:column"
pub fn format_position(line: usize, column: usize) -> String {
    format!("{}:{}", line, column)
}

/// Format a span as "start_line:start_col-end_line:end_col"
pub fn format_span(span: &Span) -> String {
    format!(
        "{}:{}-{}:{}",
        span.start_position.row + 1,
        span.start_position.column + 1,
        span.end_position.row + 1,
        span.end_position.column + 1
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contains_position() {
        let span = Span::new(0, 10, 0, 0, 0, 10);

        // Line match
        assert!(contains_position(&span, 1, None));
        assert!(!contains_position(&span, 2, None));

        // Column match
        assert!(contains_position(&span, 1, Some(5)));
        assert!(!contains_position(&span, 1, Some(15)));
    }

    #[test]
    fn test_extract_context_lines() {
        let source = "line1\nline2\nline3\nline4\nline5";

        let context = extract_context_lines(source, 3, 1);
        assert_eq!(context.len(), 3);
        assert_eq!(context[0].0, 2);
        assert_eq!(context[1].0, 3);
        assert_eq!(context[2].0, 4);
    }

    #[test]
    fn test_format_position() {
        assert_eq!(format_position(10, 5), "10:5");
    }

    #[test]
    fn test_format_span() {
        let span = Span::new(0, 10, 0, 0, 0, 10);
        assert_eq!(format_span(&span), "1:1-1:11");
    }
}
