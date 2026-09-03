//! Generic group trait for document processing
//!
//! This trait provides a common interface for different group types
//! (JsonGroup, XmlGroup, DocGroup) to reduce code duplication.

use cce_types::Span;
use cce_utils::token_estimation::TokenEstimator;

/// Trait for generic group operations
///
/// This trait abstracts the common operations needed for document groups,
/// allowing shared code to work with any group type.
pub trait GenericGroup<Node> {
    /// Get the group ID
    fn group_id(&self) -> &str;

    /// Get mutable group ID
    fn group_id_mut(&mut self) -> &mut String;

    /// Get the header node (if any)
    fn header(&self) -> Option<&Node>;

    /// Get mutable header node
    fn header_mut(&mut self) -> &mut Option<Node>;

    /// Get member nodes
    fn members(&self) -> &[Node];

    /// Get mutable member nodes
    fn members_mut(&mut self) -> &mut Vec<Node>;

    /// Get embedding text
    fn embedding_text(&self) -> &str;

    /// Get mutable embedding text
    fn embedding_text_mut(&mut self) -> &mut String;

    /// Get BM25 text
    fn bm25_text(&self) -> &str;

    /// Get mutable BM25 text
    fn bm25_text_mut(&mut self) -> &mut String;

    /// Get token count
    fn token_count(&self) -> usize;

    /// Get mutable token count
    fn token_count_mut(&mut self) -> &mut usize;

    /// Get source span
    fn span(&self) -> &Span;

    /// Get mutable source span
    fn span_mut(&mut self) -> &mut Span;

    /// Generate embedding text for a node
    fn node_to_embedding_text(node: &Node) -> String;

    /// Generate BM25 text for a node
    fn node_to_bm25_text(node: &Node) -> String;

    /// Get node ID
    fn node_id(node: &Node) -> &str;

    /// Get node span
    fn node_span(node: &Node) -> &Span;

    // === Default implementations for common operations ===

    /// Set the header node
    fn set_header(&mut self, node: Node) {
        *self.embedding_text_mut() = Self::node_to_embedding_text(&node);
        *self.bm25_text_mut() = Self::node_to_bm25_text(&node);
        *self.span_mut() = *Self::node_span(&node);
        *self.header_mut() = Some(node);
    }

    /// Add a member node
    fn add_member(&mut self, node: Node) {
        self.members_mut().push(node);
    }

    /// Finalize group (compute combined text and token count)
    fn finalize(&mut self, estimator: &TokenEstimator) {
        let mut embedding_parts = Vec::new();
        let mut bm25_parts = Vec::new();

        if let Some(header) = self.header() {
            let text = Self::node_to_embedding_text(header);
            if !text.is_empty() {
                embedding_parts.push(text);
            }
        }

        // Track only known spans. Zero is a valid file position.
        let mut combined_span = self.span().is_available().then_some(*self.span());

        for member in self.members() {
            let emb_text = Self::node_to_embedding_text(member);
            let bm_text = Self::node_to_bm25_text(member);

            if !emb_text.is_empty() {
                embedding_parts.push(emb_text);
            }
            if !bm_text.is_empty() {
                bm25_parts.push(bm_text);
            }

            // Update span
            let member_span = Self::node_span(member);
            if member_span.is_available() {
                combined_span = Some(match combined_span {
                    Some(current) => Span::new(
                        current.start_byte.min(member_span.start_byte),
                        current.end_byte.max(member_span.end_byte),
                        current
                            .start_position
                            .row
                            .min(member_span.start_position.row),
                        0,
                        current.end_position.row.max(member_span.end_position.row),
                        0,
                    ),
                    None => *member_span,
                });
            }
        }

        *self.embedding_text_mut() = embedding_parts.join("\n");
        *self.bm25_text_mut() = bm25_parts.join("\n");
        *self.token_count_mut() = estimator.estimate_text(self.bm25_text());

        if let Some(span) = combined_span {
            *self.span_mut() = span;
        }
    }

    /// Check if group has header
    fn has_header(&self) -> bool {
        self.header().is_some()
    }

    /// Get all node IDs in this group
    fn all_node_ids(&self) -> Vec<String> {
        let mut ids = Vec::new();
        if let Some(header) = self.header() {
            ids.push(Self::node_id(header).to_string());
        }
        for member in self.members() {
            ids.push(Self::node_id(member).to_string());
        }
        ids
    }
}

#[cfg(test)]
mod tests {
    // Tests will be added when implementing the trait for concrete types
}
