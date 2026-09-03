//! Common utility functions for language extractors

use cce_types::position::Span;
use tree_sitter::Node;

/// Get node text safely
pub fn node_text(node: &Node, source: &str) -> String {
    let start = node.start_byte();
    let end = node.end_byte();
    if start < end && end <= source.len() {
        source[start..end].to_string()
    } else {
        String::new()
    }
}

/// Create a Span from a tree-sitter node
pub fn node_to_span(node: &Node) -> Span {
    let start_pos = node.start_position();
    let end_pos = node.end_position();
    Span::new(
        node.start_byte(),
        node.end_byte(),
        start_pos.row,
        start_pos.column,
        end_pos.row,
        end_pos.column,
    )
}

/// Find a child node by kind
pub fn find_child_by_kind<'a>(node: &'a Node, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .find(|child| child.kind() == kind)
}

/// Find all children by kind
pub fn find_children_by_kind<'a>(node: &'a Node, kind: &str) -> Vec<Node<'a>> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .filter(|child| child.kind() == kind)
        .collect()
}

/// Find all descendants by kind (recursive)
pub fn find_descendants_by_kind<'a>(node: &'a Node, kind: &str) -> Vec<Node<'a>> {
    let mut result = Vec::new();
    let mut cursor = node.walk();
    let mut stack = vec![*node];

    while let Some(current) = stack.pop() {
        for child in current.children(&mut cursor) {
            if child.kind() == kind {
                result.push(child);
            }
            stack.push(child);
        }
    }

    result
}
