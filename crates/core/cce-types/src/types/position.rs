//! Position and span types for source code locations
//!
//! This module provides fundamental types for representing positions and spans
//! in source code. These types are used across the entire codebase and have
//! zero dependencies on other modules, making them safe to use anywhere.

use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize};
use serde::{Deserialize as SerdeDeserialize, Serialize as SerdeSerialize};

/// Position in source code (line, column)
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    SerdeSerialize,
    SerdeDeserialize,
    Default,
    Hash,
    Archive,
    RkyvDeserialize,
    Serialize,
)]
pub struct Position {
    /// Line number (0-indexed)
    pub row: usize,
    /// Column number (0-indexed, in bytes)
    pub column: usize,
}

impl Position {
    /// Create a new position
    pub fn new(row: usize, column: usize) -> Self {
        Self { row, column }
    }
}

/// Source span with both byte and line/column information
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    SerdeSerialize,
    SerdeDeserialize,
    Hash,
    Archive,
    RkyvDeserialize,
    Serialize,
)]
pub struct Span {
    /// Start byte offset
    pub start_byte: usize,
    /// End byte offset
    pub end_byte: usize,
    /// Start position (line, column)
    pub start_position: Position,
    /// End position (line, column)
    pub end_position: Position,
}

impl Default for Span {
    fn default() -> Self {
        Self::unavailable()
    }
}

impl Span {
    /// Create a new span from byte offsets and positions
    pub fn new(
        start_byte: usize,
        end_byte: usize,
        start_row: usize,
        start_column: usize,
        end_row: usize,
        end_column: usize,
    ) -> Self {
        Self {
            start_byte,
            end_byte,
            start_position: Position {
                row: start_row,
                column: start_column,
            },
            end_position: Position {
                row: end_row,
                column: end_column,
            },
        }
    }

    /// Create a span whose source location is unavailable.
    pub const fn unavailable() -> Self {
        Self {
            start_byte: 0,
            end_byte: 0,
            start_position: Position {
                row: usize::MAX,
                column: usize::MAX,
            },
            end_position: Position {
                row: usize::MAX,
                column: usize::MAX,
            },
        }
    }

    /// Whether this span carries a source location.
    pub const fn is_available(&self) -> bool {
        self.start_position.row != usize::MAX
    }

    /// Create a span from 0-indexed inclusive source lines.
    ///
    /// This is useful for parsers that only track line numbers.
    /// Byte offsets are set to 0 because they are not available. Internally,
    /// positions use a half-open range, so the end is the start of the line
    /// following `end_row`.
    pub fn from_lines(start_row: usize, end_row: usize) -> Self {
        debug_assert!(start_row <= end_row);
        Self::new(0, 0, start_row, 0, end_row.saturating_add(1), 0)
    }

    /// Create a span from a single line (start and end on same line)
    ///
    /// Uses inclusive end-position convention: the span occupies exactly one row.
    pub fn from_line(row: usize) -> Self {
        Self::from_lines(row, row)
    }

    /// Get the occupied line range as 1-indexed inclusive line numbers.
    ///
    /// Positions are 0-indexed and half-open, matching tree-sitter. If the end
    /// is at column zero, that row is not occupied by the span.
    pub fn line_range_opt(&self) -> Option<(usize, usize)> {
        if !self.is_available() {
            return None;
        }

        let start_line = self.start_position.row.saturating_add(1);
        let end_line =
            if self.end_position.column == 0 && self.end_position.row > self.start_position.row {
                self.end_position.row
            } else {
                self.end_position.row.saturating_add(1)
            };
        Some((start_line, end_line.max(start_line)))
    }

    /// Create a span for a valid UTF-8 byte range in `source`.
    pub fn from_byte_range(source: &str, start_byte: usize, end_byte: usize) -> Option<Self> {
        if start_byte > end_byte
            || end_byte > source.len()
            || !source.is_char_boundary(start_byte)
            || !source.is_char_boundary(end_byte)
        {
            return None;
        }

        let start_prefix = &source[..start_byte];
        let end_prefix = &source[..end_byte];
        let start_row = start_prefix.bytes().filter(|byte| *byte == b'\n').count();
        let end_row = end_prefix.bytes().filter(|byte| *byte == b'\n').count();
        let start_column = start_prefix
            .rfind('\n')
            .map_or(start_byte, |index| start_byte - index - 1);
        let end_column = end_prefix
            .rfind('\n')
            .map_or(end_byte, |index| end_byte - index - 1);

        Some(Self::new(
            start_byte,
            end_byte,
            start_row,
            start_column,
            end_row,
            end_column,
        ))
    }

    /// Create a full span with all position information
    ///
    /// This is the most complete form when you have byte offsets and precise positions.
    pub fn full(
        start_byte: usize,
        end_byte: usize,
        start_row: usize,
        start_column: usize,
        end_row: usize,
        end_column: usize,
    ) -> Self {
        Self::new(
            start_byte,
            end_byte,
            start_row,
            start_column,
            end_row,
            end_column,
        )
    }

    /// Get the length of the span in bytes
    pub fn len(&self) -> usize {
        self.end_byte.saturating_sub(self.start_byte)
    }

    /// Check if the span is empty
    pub fn is_empty(&self) -> bool {
        self.start_byte == self.end_byte
    }

    /// Check if this span contains another span
    pub fn contains(&self, other: &Span) -> bool {
        self.start_byte <= other.start_byte && self.end_byte >= other.end_byte
    }

    /// Check if this span overlaps with another span
    pub fn overlaps(&self, other: &Span) -> bool {
        self.start_byte < other.end_byte && other.start_byte < self.end_byte
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_position_creation() {
        let pos = Position::new(10, 5);
        assert_eq!(pos.row, 10);
        assert_eq!(pos.column, 5);
    }

    #[test]
    fn test_span_creation() {
        let span = Span::new(0, 10, 0, 0, 0, 10);
        assert_eq!(span.start_byte, 0);
        assert_eq!(span.end_byte, 10);
        assert_eq!(span.len(), 10);
    }

    #[test]
    fn test_line_range_single_line() {
        let span = Span::from_line(5);
        assert_eq!(span.line_range_opt(), Some((6, 6)));
    }

    #[test]
    fn test_line_range_multi_line() {
        let span = Span::from_lines(5, 7);
        assert_eq!(span.line_range_opt(), Some((6, 8)));
    }

    #[test]
    fn test_line_range_first_line() {
        let span = Span::from_lines(0, 0);
        assert_eq!(span.line_range_opt(), Some((1, 1)));
    }

    #[test]
    fn test_line_range_unavailable() {
        assert_eq!(Span::default().line_range_opt(), None);
    }

    #[test]
    fn test_tree_sitter_end_at_next_line_start_is_exclusive() {
        let span = Span::new(0, 4, 0, 0, 1, 0);
        assert_eq!(span.line_range_opt(), Some((1, 1)));
    }

    #[test]
    fn test_span_from_byte_range() {
        let source = "first\nsecond\nthird";
        let span = Span::from_byte_range(source, 6, 12).expect("range should be valid");
        assert_eq!(span.line_range_opt(), Some((2, 2)));
        assert_eq!(span.start_byte, 6);
        assert_eq!(span.end_byte, 12);
    }

    #[test]
    fn test_span_contains() {
        let outer = Span::new(0, 100, 0, 0, 5, 10);
        let inner = Span::new(10, 50, 1, 0, 2, 10);
        assert!(outer.contains(&inner));
        assert!(!inner.contains(&outer));
    }

    #[test]
    fn test_span_overlaps() {
        let span1 = Span::new(0, 10, 0, 0, 0, 10);
        let span2 = Span::new(5, 15, 0, 5, 0, 15);
        assert!(span1.overlaps(&span2));
        assert!(span2.overlaps(&span1));

        let span3 = Span::new(20, 30, 0, 20, 0, 30);
        assert!(!span1.overlaps(&span3));
    }
}
