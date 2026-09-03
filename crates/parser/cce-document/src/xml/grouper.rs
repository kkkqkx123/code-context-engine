//! XML grouper
//!
//! Groups XML nodes based on element boundaries.

use crate::xml::types::{XmlGroup, XmlGroupType, XmlNode, XmlNodeType};
use cce_text::MixedTokenizer;
use cce_types::ParseError;
use cce_utils::token_estimation::TokenEstimator;

/// XML grouper
#[derive(Clone)]
pub struct XmlGrouper {
    estimator: TokenEstimator,
    /// Minimum children to create a separate group
    min_children: usize,
    /// Maximum tokens per group for embedding path (0 = no limit)
    max_tokens: usize,
    /// Maximum words per group for BM25 path (0 = no limit)
    max_bm25_words: usize,
    /// Enable root element flattening for large root elements
    enable_root_flattening: bool,
    /// Maximum keys per flattened group (default: 5)
    flatten_max_keys: usize,
}

impl XmlGrouper {
    /// Create a new grouper with default settings
    pub fn new() -> Self {
        Self {
            estimator: TokenEstimator::default(),
            min_children: 2,
            max_tokens: 0,
            max_bm25_words: 200,
            enable_root_flattening: true,
            flatten_max_keys: 5,
        }
    }

    /// Set minimum children for separate grouping
    pub fn with_min_children(mut self, min: usize) -> Self {
        self.min_children = min;
        self
    }

    /// Set maximum tokens per group for embedding path
    pub fn with_max_tokens(mut self, max: usize) -> Self {
        self.max_tokens = max;
        self
    }

    /// Set maximum words per group for BM25 path
    pub fn with_max_bm25_words(mut self, max_words: usize) -> Self {
        self.max_bm25_words = max_words;
        self
    }

    /// Enable or disable root element flattening
    pub fn with_root_flattening(mut self, enabled: bool) -> Self {
        self.enable_root_flattening = enabled;
        self
    }

    /// Set maximum keys per flattened group
    pub fn with_flatten_max_keys(mut self, max_keys: usize) -> Self {
        self.flatten_max_keys = max_keys;
        self
    }

    /// Group parsed nodes by element boundaries
    pub fn group(&self, nodes: Vec<XmlNode>, file_path: &str) -> Result<Vec<XmlGroup>, ParseError> {
        let mut groups = Vec::new();
        let mut group_counter = 0;

        // Build a map for quick node lookup
        let node_map: std::collections::HashMap<&str, &XmlNode> =
            nodes.iter().map(|n| (n.id.as_str(), n)).collect();

        // Find all element nodes
        let elements: Vec<&XmlNode> = nodes
            .iter()
            .filter(|n| n.node_type.is_element() || matches!(n.node_type, XmlNodeType::Root))
            .collect();

        // Track which nodes have been grouped
        let mut grouped_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

        for element in elements {
            // Skip if already grouped
            if grouped_ids.contains(&element.id) {
                continue;
            }

            // Determine group type
            let group_type = match &element.node_type {
                XmlNodeType::Root => XmlGroupType::RootElement,
                XmlNodeType::Element { .. } => {
                    if element.depth == 0 {
                        XmlGroupType::RootElement
                    } else if element.has_children() {
                        XmlGroupType::ContainerElement
                    } else {
                        XmlGroupType::LeafElement
                    }
                }
                _ => continue,
            };

            // Get children of this element
            let children: Vec<&XmlNode> = element
                .children
                .iter()
                .filter_map(|id| node_map.get(id.as_str()))
                .copied()
                .collect();

            // Check if this is a root element that should be flattened
            if self.enable_root_flattening
                && matches!(element.node_type, XmlNodeType::Root)
                && children.len() > self.flatten_max_keys
            {
                // Flatten root element into multiple groups
                let flattened_groups = self.flatten_root_element(
                    element,
                    &children,
                    file_path,
                    &mut group_counter,
                    &mut grouped_ids,
                );
                groups.extend(flattened_groups);
                continue;
            }

            // Check if this element should be a separate group
            let should_be_separate = self.should_be_separate_group(element, &children);

            if should_be_separate {
                // Create a group for this element
                group_counter += 1;
                let mut group = XmlGroup::new(
                    format!("{}_{}_{}", file_path, group_type, group_counter),
                    group_type,
                    element.path.clone(),
                );

                // Add element as header
                group.set_header(element.clone());
                grouped_ids.insert(element.id.clone());

                // Add children as members
                for child in &children {
                    // Only add leaf nodes (text, comments, etc.)
                    if child.node_type.is_leaf() {
                        group.add_member((*child).clone());
                        grouped_ids.insert(child.id.clone());
                    }
                }

                group.finalize(&self.estimator);
                groups.push(group);
            } else {
                // Element is too small, add its children to parent group
                // This will be handled when processing the parent element
                for child in &children {
                    if child.node_type.is_leaf() {
                        grouped_ids.insert(child.id.clone());
                    }
                }
            }
        }

        // Handle remaining ungrouped nodes (text nodes at root level)
        let remaining: Vec<&XmlNode> = nodes
            .iter()
            .filter(|n| !grouped_ids.contains(&n.id) && n.node_type.is_leaf())
            .collect();

        if !remaining.is_empty() {
            group_counter += 1;
            let mut group = XmlGroup::new(
                format!("{}_text_{}", file_path, group_counter),
                XmlGroupType::TextGroup,
                String::new(),
            );

            for node in remaining {
                group.add_member(node.clone());
                grouped_ids.insert(node.id.clone());
            }

            group.finalize(&self.estimator);
            groups.push(group);
        }

        // If no groups were created, create a root group with all nodes
        if groups.is_empty() && !nodes.is_empty() {
            group_counter += 1;
            let mut group = XmlGroup::new(
                format!("{}_root_{}", file_path, group_counter),
                XmlGroupType::RootElement,
                String::new(),
            );

            for node in &nodes {
                if node.node_type.is_leaf() {
                    group.add_member(node.clone());
                }
            }

            group.finalize(&self.estimator);
            groups.push(group);
        }

        // Check for groups exceeding limits and split if necessary
        if self.max_tokens > 0 || self.max_bm25_words > 0 {
            groups = self.split_large_groups(groups, file_path);
        }

        Ok(groups)
    }

    /// Flatten a large root element into multiple manageable groups
    fn flatten_root_element(
        &self,
        root: &XmlNode,
        children: &[&XmlNode],
        file_path: &str,
        group_counter: &mut usize,
        grouped_ids: &mut std::collections::HashSet<String>,
    ) -> Vec<XmlGroup> {
        let mut groups = Vec::new();
        let mut current_batch = Vec::new();
        let mut batch_num = 0;

        for child in children {
            current_batch.push(child);

            // When we reach max_keys or it's the last child, create a group
            if current_batch.len() >= self.flatten_max_keys {
                batch_num += 1;
                *group_counter += 1;

                let mut group = XmlGroup::new(
                    format!("{}_root_batch_{}", file_path, batch_num),
                    XmlGroupType::RootElement,
                    root.path.clone(),
                );

                // Set root as header for first batch only
                if batch_num == 1 {
                    group.set_header(root.clone());
                    grouped_ids.insert(root.id.clone());
                }

                // Add batch children as members
                for &batch_child in &current_batch {
                    if batch_child.node_type.is_leaf() {
                        group.add_member((*batch_child).clone());
                        grouped_ids.insert(batch_child.id.clone());
                    }
                }

                group.finalize(&self.estimator);
                groups.push(group);
                current_batch.clear();
            }
        }

        // Handle remaining children
        if !current_batch.is_empty() {
            batch_num += 1;
            *group_counter += 1;

            let mut group = XmlGroup::new(
                format!("{}_root_batch_{}", file_path, batch_num),
                XmlGroupType::RootElement,
                root.path.clone(),
            );

            // Add remaining children
            for &batch_child in &current_batch {
                if batch_child.node_type.is_leaf() {
                    group.add_member((*batch_child).clone());
                    grouped_ids.insert(batch_child.id.clone());
                }
            }

            group.finalize(&self.estimator);
            groups.push(group);
        }

        groups
    }

    /// Check if an element should be a separate group
    fn should_be_separate_group(&self, element: &XmlNode, children: &[&XmlNode]) -> bool {
        match &element.node_type {
            XmlNodeType::Root => true,
            XmlNodeType::Element { .. } => {
                // Count leaf children
                let leaf_count = children.iter().filter(|c| c.node_type.is_leaf()).count();
                leaf_count >= self.min_children || element.has_attributes()
            }
            _ => false,
        }
    }

    /// Split groups that exceed limits
    fn split_large_groups(&self, groups: Vec<XmlGroup>, file_path: &str) -> Vec<XmlGroup> {
        let mut result = Vec::new();
        let mut group_counter = groups.len();

        for group in groups {
            let should_split = if self.max_bm25_words > 0 {
                let tokenizer = MixedTokenizer::new();
                let word_count = tokenizer.tokenize(&group.bm25_text).len();
                word_count > self.max_bm25_words
            } else if self.max_tokens > 0 {
                group.token_count > self.max_tokens
            } else {
                false
            };

            if !should_split {
                result.push(group);
            } else {
                // Split the group
                let split_groups = self.split_group(group, file_path, &mut group_counter);
                result.extend(split_groups);
            }
        }

        result
    }

    /// Split a single group into multiple groups
    fn split_group(
        &self,
        group: XmlGroup,
        file_path: &str,
        group_counter: &mut usize,
    ) -> Vec<XmlGroup> {
        let mut result = Vec::new();
        let mut current_members = Vec::new();
        let mut current_word_count = 0;
        let tokenizer = MixedTokenizer::new();

        for member in group.members {
            let member_bm25 = member.to_bm25_text();
            let member_words = tokenizer.tokenize(&member_bm25).len();

            let should_split = if self.max_bm25_words > 0 {
                current_word_count + member_words > self.max_bm25_words
                    && !current_members.is_empty()
            } else if self.max_tokens > 0 {
                let member_tokens = self.estimator.estimate_text(&member_bm25);
                let current_tokens: usize = current_members
                    .iter()
                    .map(|m: &crate::xml::types::XmlNode| {
                        self.estimator.estimate_text(&m.to_bm25_text())
                    })
                    .sum();
                current_tokens + member_tokens > self.max_tokens && !current_members.is_empty()
            } else {
                false
            };

            if should_split {
                // Create a new group with current members
                *group_counter += 1;
                let mut new_group = XmlGroup::new(
                    format!("{}_split_{}", file_path, group_counter),
                    group.group_type,
                    group.path_prefix.clone(),
                );

                for m in current_members.drain(..) {
                    new_group.add_member(m);
                }

                new_group.finalize(&self.estimator);
                result.push(new_group);
                current_word_count = 0;
            }

            current_members.push(member);
            current_word_count += member_words;
        }

        // Add remaining members
        if !current_members.is_empty() {
            *group_counter += 1;
            let mut new_group = XmlGroup::new(
                format!("{}_split_{}", file_path, group_counter),
                group.group_type,
                group.path_prefix.clone(),
            );

            for m in current_members {
                new_group.add_member(m);
            }

            new_group.finalize(&self.estimator);
            result.push(new_group);
        }

        result
    }
}

impl Default for XmlGrouper {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::Span;

    fn create_element_node(id: &str, tag: &str, path: &str) -> XmlNode {
        XmlNode::new(
            id.to_string(),
            XmlNodeType::Element {
                tag: tag.to_string(),
            },
            path.to_string(),
            Span::default(),
        )
    }

    fn create_text_node(id: &str, text: &str, path: &str) -> XmlNode {
        XmlNode::new(
            id.to_string(),
            XmlNodeType::Text,
            path.to_string(),
            Span::default(),
        )
        .with_text(text.to_string())
    }

    #[test]
    fn test_group_simple_element() {
        let grouper = XmlGrouper::new();

        let mut nodes = vec![
            XmlNode::new(
                "root".to_string(),
                XmlNodeType::Root,
                String::new(),
                Span::default(),
            ),
            create_element_node("elem", "config", "config"),
            create_text_node("text", "value", "config.text"),
        ];

        // Set up parent-child relationships
        nodes[0].add_child("elem".to_string());
        nodes[1].parent_id = Some("root".to_string());
        nodes[1].add_child("text".to_string());
        nodes[2].parent_id = Some("elem".to_string());

        let groups = grouper.group(nodes, "test.xml").expect("should group");

        // Should have at least one group
        assert!(!groups.is_empty());
    }

    #[test]
    fn test_group_nested_elements() {
        let grouper = XmlGrouper::new();

        let mut nodes = vec![
            XmlNode::new(
                "root".to_string(),
                XmlNodeType::Root,
                String::new(),
                Span::default(),
            ),
            create_element_node("parent", "parent", "parent"),
            create_element_node("child", "child", "parent.child"),
            create_text_node("text", "value", "parent.child.text"),
        ];

        // Set up relationships
        nodes[0].add_child("parent".to_string());
        nodes[1].parent_id = Some("root".to_string());
        nodes[1].add_child("child".to_string());
        nodes[2].parent_id = Some("parent".to_string());
        nodes[2].add_child("text".to_string());
        nodes[3].parent_id = Some("child".to_string());

        let groups = grouper.group(nodes, "test.xml").expect("should group");

        // Should have groups for root and nested elements
        assert!(!groups.is_empty());
    }

    #[test]
    fn test_group_with_max_tokens() {
        let grouper = XmlGrouper::new().with_max_tokens(10);

        let mut nodes = vec![
            XmlNode::new(
                "root".to_string(),
                XmlNodeType::Root,
                String::new(),
                Span::default(),
            ),
            create_text_node("text1", "value1", "text1"),
            create_text_node("text2", "value2", "text2"),
            create_text_node("text3", "value3", "text3"),
        ];

        nodes[0].add_child("text1".to_string());
        nodes[0].add_child("text2".to_string());
        nodes[0].add_child("text3".to_string());
        for node in &mut nodes[1..=3] {
            node.parent_id = Some("root".to_string());
        }

        let groups = grouper.group(nodes, "test.xml").expect("should group");

        // May be split into multiple groups due to token limit
        assert!(!groups.is_empty());
    }
}
