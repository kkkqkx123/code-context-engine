//! Error node collector for tree-sitter AST
//!
//! This module provides functionality to collect ERROR nodes and missing tokens
//! from tree-sitter parse trees.

use serde::{Deserialize, Serialize};
use tree_sitter::{Node, Tree};

use cce_types::position::Position;

/// Error candidate from tree-sitter parsing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorCandidate {
    /// Candidate kind (ERROR node or missing token)
    pub kind: ErrorCandidateKind,

    /// Start position (line, column)
    pub start: Position,

    /// End position (line, column)
    pub end: Position,

    /// Start byte offset
    pub start_byte: usize,

    /// End byte offset
    pub end_byte: usize,

    /// Node text (optional, for debugging)
    pub text: Option<String>,

    /// Node kind from tree-sitter (e.g., "string_literal", "identifier")
    pub node_kind: Option<String>,
}

impl ErrorCandidate {
    /// Create from a tree-sitter node
    pub fn from_node(node: Node, source: &str) -> Self {
        let start_pos = node.start_position();
        let end_pos = node.end_position();

        Self {
            kind: ErrorCandidateKind::Error,
            start: Position::new(start_pos.row, start_pos.column),
            end: Position::new(end_pos.row, end_pos.column),
            start_byte: node.start_byte(),
            end_byte: node.end_byte(),
            text: node
                .utf8_text(source.as_bytes())
                .ok()
                .map(|s| s.to_string()),
            node_kind: Some(node.kind().to_string()),
        }
    }

    /// Create from a missing token
    pub fn from_missing(node: Node, source: &str) -> Self {
        let start_pos = node.start_position();
        let end_pos = node.end_position();

        Self {
            kind: ErrorCandidateKind::Missing,
            start: Position::new(start_pos.row, start_pos.column),
            end: Position::new(end_pos.row, end_pos.column),
            start_byte: node.start_byte(),
            end_byte: node.end_byte(),
            text: node
                .utf8_text(source.as_bytes())
                .ok()
                .map(|s| s.to_string()),
            node_kind: Some(node.kind().to_string()),
        }
    }

    /// Check if this candidate strictly contains another (not equal)
    pub fn strictly_contains(&self, other: &ErrorCandidate) -> bool {
        self.start_byte < other.start_byte && self.end_byte > other.end_byte
    }

    /// Check if this candidate overlaps with another
    pub fn overlaps(&self, other: &ErrorCandidate) -> bool {
        self.start_byte < other.end_byte && other.start_byte < self.end_byte
    }

    /// Get the length in bytes
    pub fn len(&self) -> usize {
        self.end_byte.saturating_sub(self.start_byte)
    }

    /// Check if the candidate is empty
    pub fn is_empty(&self) -> bool {
        self.start_byte == self.end_byte
    }
}

impl PartialEq for ErrorCandidate {
    fn eq(&self, other: &Self) -> bool {
        self.start_byte == other.start_byte && self.end_byte == other.end_byte
    }
}

impl Eq for ErrorCandidate {}

/// Error candidate kind
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCandidateKind {
    /// ERROR node from tree-sitter
    Error,

    /// Missing token
    Missing,
}

/// Error node collector
pub struct ErrorCollector;

impl ErrorCollector {
    /// Collect all ERROR nodes and missing tokens from a tree
    pub fn collect(tree: &Tree, source: &str) -> Vec<ErrorCandidate> {
        let mut candidates = Vec::new();
        Self::collect_recursive(tree.root_node(), source, &mut candidates);
        candidates
    }

    /// Recursive collection helper
    fn collect_recursive(node: Node, source: &str, candidates: &mut Vec<ErrorCandidate>) {
        // Check if this is an ERROR node
        if node.kind() == "ERROR" {
            candidates.push(ErrorCandidate::from_node(node, source));
        }

        // Check for missing tokens
        if node.is_missing() {
            candidates.push(ErrorCandidate::from_missing(node, source));
        }

        // Recursively process children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            Self::collect_recursive(child, source, candidates);
        }
    }

    /// Filter to keep only innermost ERROR nodes
    ///
    /// When ERROR nodes are nested, we keep only the innermost (smallest) ones
    /// to provide more precise error locations.
    pub fn filter_innermost(candidates: Vec<ErrorCandidate>) -> Vec<ErrorCandidate> {
        if candidates.len() <= 1 {
            return candidates;
        }

        // For each candidate, check if it contains any other candidate
        // If it does, it's an outer candidate and should be removed
        let mut result = Vec::new();
        for (i, candidate) in candidates.iter().enumerate() {
            let contains_other = candidates
                .iter()
                .enumerate()
                .any(|(j, other)| i != j && candidate.strictly_contains(other));
            if !contains_other {
                result.push(candidate.clone());
            }
        }
        result
    }

    /// Sort candidates by position (line, then column)
    pub fn sort_by_position(mut candidates: Vec<ErrorCandidate>) -> Vec<ErrorCandidate> {
        candidates.sort_by(|a, b| match a.start.row.cmp(&b.start.row) {
            std::cmp::Ordering::Equal => a.start.column.cmp(&b.start.column),
            other => other,
        });
        candidates
    }

    /// Remove duplicate candidates (same position)
    pub fn deduplicate(mut candidates: Vec<ErrorCandidate>) -> Vec<ErrorCandidate> {
        candidates.sort_by(|a, b| match a.start.row.cmp(&b.start.row) {
            std::cmp::Ordering::Equal => a.start.column.cmp(&b.start.column),
            other => other,
        });
        candidates.dedup_by(|a, b| a.start_byte == b.start_byte && a.end_byte == b.end_byte);
        candidates
    }

    /// Full processing pipeline: collect, filter, deduplicate, sort
    pub fn collect_and_process(tree: &Tree, source: &str) -> Vec<ErrorCandidate> {
        let candidates = Self::collect(tree, source);
        let filtered = Self::filter_innermost(candidates);
        let deduped = Self::deduplicate(filtered);
        Self::sort_by_position(deduped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_candidate_from_node() {
        // This test would require a real tree-sitter tree
        // For now, we test the struct creation
        let candidate = ErrorCandidate {
            kind: ErrorCandidateKind::Error,
            start: Position::new(0, 5),
            end: Position::new(0, 10),
            start_byte: 5,
            end_byte: 10,
            text: Some("test".to_string()),
            node_kind: Some("identifier".to_string()),
        };

        assert_eq!(candidate.len(), 5);
        assert!(!candidate.is_empty());
    }

    #[test]
    fn test_strictly_contains() {
        let outer = ErrorCandidate {
            kind: ErrorCandidateKind::Error,
            start: Position::new(0, 0),
            end: Position::new(0, 20),
            start_byte: 0,
            end_byte: 20,
            text: None,
            node_kind: None,
        };

        let inner = ErrorCandidate {
            kind: ErrorCandidateKind::Error,
            start: Position::new(0, 5),
            end: Position::new(0, 15),
            start_byte: 5,
            end_byte: 15,
            text: None,
            node_kind: None,
        };

        assert!(outer.strictly_contains(&inner));
        assert!(!inner.strictly_contains(&outer));
        assert!(!outer.strictly_contains(&outer));
    }

    #[test]
    fn test_overlaps() {
        let first = ErrorCandidate {
            kind: ErrorCandidateKind::Error,
            start: Position::new(0, 0),
            end: Position::new(0, 10),
            start_byte: 0,
            end_byte: 10,
            text: None,
            node_kind: None,
        };

        let second = ErrorCandidate {
            kind: ErrorCandidateKind::Error,
            start: Position::new(0, 5),
            end: Position::new(0, 15),
            start_byte: 5,
            end_byte: 15,
            text: None,
            node_kind: None,
        };

        let third = ErrorCandidate {
            kind: ErrorCandidateKind::Error,
            start: Position::new(0, 20),
            end: Position::new(0, 30),
            start_byte: 20,
            end_byte: 30,
            text: None,
            node_kind: None,
        };

        assert!(first.overlaps(&second));
        assert!(second.overlaps(&first));
        assert!(!first.overlaps(&third));
    }

    #[test]
    fn test_filter_innermost_single() {
        let candidate = ErrorCandidate {
            kind: ErrorCandidateKind::Error,
            start: Position::new(0, 0),
            end: Position::new(0, 10),
            start_byte: 0,
            end_byte: 10,
            text: None,
            node_kind: None,
        };

        let candidates = vec![candidate.clone()];
        let filtered = ErrorCollector::filter_innermost(candidates);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0], candidate);
    }

    #[test]
    fn test_filter_innermost_nested() {
        let outer = ErrorCandidate {
            kind: ErrorCandidateKind::Error,
            start: Position::new(0, 0),
            end: Position::new(0, 20),
            start_byte: 0,
            end_byte: 20,
            text: None,
            node_kind: None,
        };

        let inner = ErrorCandidate {
            kind: ErrorCandidateKind::Error,
            start: Position::new(0, 5),
            end: Position::new(0, 15),
            start_byte: 5,
            end_byte: 15,
            text: None,
            node_kind: None,
        };

        // Put inner first to test the filter correctly
        let candidates = vec![inner, outer];
        let filtered = ErrorCollector::filter_innermost(candidates);

        // Should keep only the inner one
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].start_byte, 5);
        assert_eq!(filtered[0].end_byte, 15);
    }

    #[test]
    fn test_deduplicate() {
        let candidate1 = ErrorCandidate {
            kind: ErrorCandidateKind::Error,
            start: Position::new(0, 0),
            end: Position::new(0, 10),
            start_byte: 0,
            end_byte: 10,
            text: None,
            node_kind: None,
        };

        let candidate2 = ErrorCandidate {
            kind: ErrorCandidateKind::Error,
            start: Position::new(0, 0),
            end: Position::new(0, 10),
            start_byte: 0,
            end_byte: 10,
            text: None,
            node_kind: None,
        };

        let candidate3 = ErrorCandidate {
            kind: ErrorCandidateKind::Error,
            start: Position::new(0, 20),
            end: Position::new(0, 30),
            start_byte: 20,
            end_byte: 30,
            text: None,
            node_kind: None,
        };

        let candidates = vec![candidate1, candidate2, candidate3];
        let deduped = ErrorCollector::deduplicate(candidates);

        assert_eq!(deduped.len(), 2);
    }

    #[test]
    fn test_sort_by_position() {
        let first = ErrorCandidate {
            kind: ErrorCandidateKind::Error,
            start: Position::new(2, 0),
            end: Position::new(2, 10),
            start_byte: 20,
            end_byte: 30,
            text: None,
            node_kind: None,
        };

        let second = ErrorCandidate {
            kind: ErrorCandidateKind::Error,
            start: Position::new(0, 5),
            end: Position::new(0, 15),
            start_byte: 5,
            end_byte: 15,
            text: None,
            node_kind: None,
        };

        let third = ErrorCandidate {
            kind: ErrorCandidateKind::Error,
            start: Position::new(0, 0),
            end: Position::new(0, 10),
            start_byte: 0,
            end_byte: 10,
            text: None,
            node_kind: None,
        };

        let candidates = vec![first, second, third];
        let sorted = ErrorCollector::sort_by_position(candidates);

        assert_eq!(sorted[0].start.row, 0);
        assert_eq!(sorted[0].start.column, 0);
        assert_eq!(sorted[1].start.row, 0);
        assert_eq!(sorted[1].start.column, 5);
        assert_eq!(sorted[2].start.row, 2);
        assert_eq!(sorted[2].start.column, 0);
    }
}
