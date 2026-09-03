//! Markdown grouper
//!
//! Groups Markdown nodes based on heading hierarchy.

use crate::GenericGroup;
use crate::types::{DocGroup, DocGroupType, DocNode, DocNodeType};
use cce_text::MixedTokenizer;
use cce_types::ParseError;
use cce_utils::token_estimation::TokenEstimator;

/// Configuration for Markdown grouper
#[derive(Debug, Clone)]
pub struct MarkdownGrouperConfig {
    /// Maximum tokens per group for embedding path (0 = no limit)
    pub max_tokens: usize,
    /// Maximum words per group for BM25 path (0 = no limit)
    pub max_bm25_words: usize,
    /// Minimum tokens for standalone code block (embedding path)
    pub min_standalone_tokens: usize,
    /// Minimum BM25 words for standalone code block (BM25 path)
    pub min_standalone_bm25_words: usize,
    /// Merge adjacent paragraphs without headings
    pub merge_adjacent_paragraphs: bool,
    /// Keep code blocks with their preceding context
    pub preserve_code_context: bool,
}

impl Default for MarkdownGrouperConfig {
    fn default() -> Self {
        Self {
            max_tokens: 0,
            max_bm25_words: 200,
            min_standalone_tokens: 100,
            min_standalone_bm25_words: 80,
            merge_adjacent_paragraphs: true,
            preserve_code_context: true,
        }
    }
}

/// Markdown grouper
#[derive(Clone)]
pub struct DocGrouper {
    estimator: TokenEstimator,
    config: MarkdownGrouperConfig,
}

impl DocGrouper {
    /// Create a new grouper with default settings
    pub fn new() -> Self {
        Self {
            estimator: TokenEstimator::default(),
            config: MarkdownGrouperConfig::default(),
        }
    }

    /// Set configuration
    pub fn with_config(mut self, config: MarkdownGrouperConfig) -> Self {
        self.config = config;
        self
    }

    /// Set maximum tokens per group for embedding path
    pub fn with_max_tokens(mut self, max_tokens: usize) -> Self {
        self.config.max_tokens = max_tokens;
        self
    }

    /// Set maximum words per group for BM25 path
    pub fn with_max_bm25_words(mut self, max_words: usize) -> Self {
        self.config.max_bm25_words = max_words;
        self
    }

    /// Set minimum standalone tokens
    pub fn with_min_standalone_tokens(mut self, min_tokens: usize) -> Self {
        self.config.min_standalone_tokens = min_tokens;
        self
    }

    /// Set minimum BM25 words for standalone code blocks (BM25 path)
    pub fn with_min_standalone_bm25_words(mut self, min_words: usize) -> Self {
        self.config.min_standalone_bm25_words = min_words;
        self
    }

    /// Enable/disable merging adjacent paragraphs
    pub fn with_merge_adjacent_paragraphs(mut self, merge: bool) -> Self {
        self.config.merge_adjacent_paragraphs = merge;
        self
    }

    /// Group parsed nodes by heading hierarchy
    pub fn group(&self, nodes: Vec<DocNode>, file_path: &str) -> Result<Vec<DocGroup>, ParseError> {
        let mut groups = Vec::new();
        let mut group_counter = 0;

        // Strategy: Group by heading hierarchy
        // - Level 1 heading starts a chapter group
        // - Level 2+ heading starts a section group
        // - Non-heading content belongs to the nearest heading group
        // - Large code blocks can be standalone groups

        let mut current_group: Option<DocGroup> = None;
        let mut current_heading_level = 0;

        for node in nodes {
            let heading_level = node.node_type.heading_level();

            match heading_level {
                Some(1) => {
                    // Level 1 heading: save current group and start new chapter
                    if let Some(group) = current_group.take() {
                        groups.push(group);
                    }
                    current_heading_level = 1;
                    group_counter += 1;
                    current_group = Some(DocGroup::new(
                        format!("{}_chapter_{}", file_path, group_counter),
                        DocGroupType::Chapter,
                    ));
                    current_group.as_mut().unwrap().set_header(node);
                }
                Some(level) if level > current_heading_level || current_group.is_none() => {
                    // Deeper heading or no current group: start new section
                    if let Some(group) = current_group.take() {
                        groups.push(group);
                    }
                    current_heading_level = level;
                    group_counter += 1;
                    current_group = Some(DocGroup::new(
                        format!("{}_section_{}", file_path, group_counter),
                        DocGroupType::Section,
                    ));
                    current_group.as_mut().unwrap().set_header(node);
                }
                Some(level) if level <= current_heading_level => {
                    // Same or higher level heading: save current and start new
                    if let Some(group) = current_group.take() {
                        groups.push(group);
                    }
                    current_heading_level = level;
                    group_counter += 1;
                    let group_type = if level == 1 {
                        DocGroupType::Chapter
                    } else {
                        DocGroupType::Section
                    };
                    current_group = Some(DocGroup::new(
                        format!("{}_{}_{}", file_path, group_type, group_counter),
                        group_type,
                    ));
                    current_group.as_mut().unwrap().set_header(node);
                }
                _ => {
                    // Non-heading node
                    // Check if it should be a standalone group (large code block)
                    if self.should_be_standalone(&node) {
                        // Save current group first
                        if let Some(group) = current_group.take() {
                            groups.push(group);
                        }
                        // Create standalone group
                        group_counter += 1;
                        let mut standalone = DocGroup::new(
                            format!("{}_standalone_{}", file_path, group_counter),
                            DocGroupType::StandaloneBlock,
                        );
                        standalone.add_member(node);
                        standalone.finalize(&self.estimator);
                        groups.push(standalone);
                    } else if let Some(ref mut group) = current_group {
                        // Add to current group
                        group.add_member(node);

                        // Check if group exceeds limits (both BM25 words and embedding tokens)
                        let should_split = if self.config.max_bm25_words > 0 {
                            let tokenizer = MixedTokenizer::new();
                            let word_count = tokenizer.tokenize(&group.bm25_text).len();
                            word_count > self.config.max_bm25_words && !group.members.is_empty()
                        } else if self.config.max_tokens > 0 {
                            let tokens = self.estimator.estimate_text(&group.bm25_text);
                            tokens > self.config.max_tokens && !group.members.is_empty()
                        } else {
                            false
                        };

                        if should_split {
                            // Remove last member and finalize current group
                            let last = group.members.pop().unwrap();
                            group.finalize(&self.estimator);
                            let completed_group = current_group.take().unwrap();
                            groups.push(completed_group);

                            // Start new group with the removed member
                            group_counter += 1;
                            current_group = Some(DocGroup::new(
                                format!("{}_section_{}", file_path, group_counter),
                                DocGroupType::Section,
                            ));
                            current_group.as_mut().unwrap().add_member(last);
                        }
                    } else {
                        // No current group (content before first heading)
                        group_counter += 1;
                        current_group = Some(DocGroup::new(
                            format!("{}_paragraph_{}", file_path, group_counter),
                            DocGroupType::ParagraphGroup,
                        ));
                        current_group.as_mut().unwrap().add_member(node);
                    }
                }
            }
        }

        // Finalize last group
        if let Some(mut group) = current_group {
            group.finalize(&self.estimator);
            groups.push(group);
        }

        // Finalize all groups
        for group in &mut groups {
            group.finalize(&self.estimator);
        }

        Ok(groups)
    }

    /// Check if a node should be a standalone group
    fn should_be_standalone(&self, node: &DocNode) -> bool {
        match &node.node_type {
            DocNodeType::CodeBlock { .. } => {
                // Check both token count and BM25 word count
                let token_count = self.estimator.estimate_text(&node.content);
                let tokenizer = MixedTokenizer::new();
                let bm25_word_count = tokenizer.tokenize(&node.content).len();

                // Standalone if either threshold is met
                token_count >= self.config.min_standalone_tokens
                    || bm25_word_count >= self.config.min_standalone_bm25_words
            }
            DocNodeType::Table | DocNodeType::TableRow => {
                // Tables with multiple rows could be standalone
                // For now, keep tables with their parent heading
                false
            }
            _ => false,
        }
    }
}

impl Default for DocGrouper {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::Span;

    fn create_heading_node(level: usize, content: &str) -> DocNode {
        DocNode::new(
            format!("h{}", level),
            DocNodeType::Heading { level },
            content.to_string(),
            Span::default(),
        )
    }

    fn create_paragraph_node(content: &str) -> DocNode {
        DocNode::new(
            "p".to_string(),
            DocNodeType::Paragraph,
            content.to_string(),
            Span::default(),
        )
    }

    #[test]
    fn test_group_chapters() {
        let grouper = DocGrouper::new();
        let nodes = vec![
            create_heading_node(1, "Chapter 1"),
            create_paragraph_node("Content 1"),
            create_heading_node(1, "Chapter 2"),
            create_paragraph_node("Content 2"),
        ];

        let groups = grouper.group(nodes, "test.md").expect("should group");

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].group_type, DocGroupType::Chapter);
        assert_eq!(groups[1].group_type, DocGroupType::Chapter);
        assert!(groups[0].has_header());
        assert!(groups[1].has_header());
    }

    #[test]
    fn test_group_sections() {
        let grouper = DocGrouper::new();
        let nodes = vec![
            create_heading_node(1, "Chapter"),
            create_heading_node(2, "Section 1"),
            create_paragraph_node("Content 1"),
            create_heading_node(2, "Section 2"),
            create_paragraph_node("Content 2"),
        ];

        let groups = grouper.group(nodes, "test.md").expect("should group");

        // Chapter + Section 1 + Section 2
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].group_type, DocGroupType::Chapter);
        assert_eq!(groups[1].group_type, DocGroupType::Section);
        assert_eq!(groups[2].group_type, DocGroupType::Section);
    }

    #[test]
    fn test_group_no_heading() {
        let grouper = DocGrouper::new();
        let nodes = vec![
            create_paragraph_node("Paragraph 1"),
            create_paragraph_node("Paragraph 2"),
        ];

        let groups = grouper.group(nodes, "test.md").expect("should group");

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].group_type, DocGroupType::ParagraphGroup);
        assert!(!groups[0].has_header());
        assert_eq!(groups[0].members.len(), 2);
    }
}
