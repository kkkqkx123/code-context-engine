//! YAML grouper
//!
//! This module provides grouping functionality for YAML nodes.
//! It groups nodes by mapping boundaries and semantic relationships.

use crate::GenericGroup;
use crate::yaml::types::{YamlGroup, YamlGroupType, YamlNode, YamlNodeType};
use cce_text::MixedTokenizer;
use cce_types::ParseError;
use cce_utils::token_estimation::TokenEstimator;

/// YAML grouper
pub struct YamlGrouper {
    estimator: TokenEstimator,
    group_counter: usize,
    /// Maximum members per root group (for flattening)
    max_root_members: usize,
    /// Maximum tokens per group for embedding path (0 = no limit)
    max_tokens: usize,
    /// Maximum words per group for BM25 path (0 = no limit)
    max_bm25_words: usize,
}

impl YamlGrouper {
    /// Create a new YAML grouper
    pub fn new() -> Self {
        Self {
            estimator: TokenEstimator::default(),
            group_counter: 0,
            max_root_members: 5, // Default: 5 keys per root group
            max_tokens: 0,
            max_bm25_words: 200,
        }
    }

    /// Set maximum members per root group
    pub fn with_max_root_members(mut self, max: usize) -> Self {
        self.max_root_members = max;
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

    /// Generate a unique group ID
    fn next_group_id(&mut self) -> String {
        self.group_counter += 1;
        format!("yaml_group_{}", self.group_counter)
    }

    /// Group YAML nodes into semantic groups
    pub fn group(
        &mut self,
        nodes: Vec<YamlNode>,
        _file_path: &str,
    ) -> Result<Vec<YamlGroup>, ParseError> {
        if nodes.is_empty() {
            return Ok(Vec::new());
        }

        let mut groups = Vec::new();

        // Find the root node
        let root = nodes
            .iter()
            .find(|n| matches!(n.node_type, YamlNodeType::Root))
            .expect("Root node should exist");

        // Build a map for quick node lookup
        let node_map: std::collections::HashMap<String, &YamlNode> =
            nodes.iter().map(|n| (n.id.clone(), n)).collect();

        // Track which nodes have been grouped
        let mut grouped_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

        // Handle root object specially - flatten it
        self.handle_root_mapping(root, &node_map, &mut groups, &mut grouped_ids);

        // Group nodes by their mapping context (excluding root)
        let mut mapping_groups: std::collections::HashMap<String, Vec<&YamlNode>> =
            std::collections::HashMap::new();

        for node in &nodes {
            // Skip root and container nodes
            if matches!(
                node.node_type,
                YamlNodeType::Root | YamlNodeType::Mapping { .. }
            ) {
                continue;
            }

            // Skip if already grouped
            if grouped_ids.contains(&node.id) {
                continue;
            }

            // Determine which mapping this node belongs to
            let mapping_key = Self::get_mapping_key(node, &node_map);

            // Skip if this belongs to root (already handled)
            if mapping_key.is_empty() {
                continue;
            }

            mapping_groups.entry(mapping_key).or_default().push(node);
        }

        // Create groups for each mapping
        for (mapping_key, members) in mapping_groups {
            let group_type = if mapping_key.contains('[') {
                // This shouldn't happen anymore as sequences are handled separately
                YamlGroupType::SequenceElement
            } else {
                YamlGroupType::NamedMapping
            };

            let mut group = YamlGroup::new(self.next_group_id(), group_type, mapping_key.clone());

            // Set header if we can find the mapping node
            if !mapping_key.is_empty() {
                // Try to find a node that represents this mapping
                let mapping_name = mapping_key.split('.').next_back().unwrap_or(&mapping_key);
                if let Some(mapping_node) = nodes.iter().find(|n| {
                    matches!(&n.node_type, YamlNodeType::Mapping { key: k } if k == mapping_name)
                }) {
                    group.set_header(mapping_node.clone());
                    grouped_ids.insert(mapping_node.id.clone());
                }
            }

            // Add members
            for member in members {
                if !grouped_ids.contains(&member.id) {
                    group.add_member(member.clone());
                    grouped_ids.insert(member.id.clone());
                }
            }

            group.finalize(&self.estimator);
            groups.push(group);
        }

        // If no groups were created (e.g., only root), create a root group
        if groups.is_empty() {
            let mut group = YamlGroup::new(
                self.next_group_id(),
                YamlGroupType::RootMapping,
                String::new(),
            );
            group.set_header(root.clone());
            group.finalize(&self.estimator);
            groups.push(group);
        }

        // Check for groups exceeding limits and split if necessary
        if self.max_tokens > 0 || self.max_bm25_words > 0 {
            groups = self.split_large_groups(groups);
        }

        Ok(groups)
    }

    /// Handle root mapping by flattening its children into groups
    fn handle_root_mapping(
        &mut self,
        root: &YamlNode,
        node_map: &std::collections::HashMap<String, &YamlNode>,
        groups: &mut Vec<YamlGroup>,
        grouped_ids: &mut std::collections::HashSet<String>,
    ) {
        // Get root's children
        let root_children: Vec<&YamlNode> = root
            .children
            .iter()
            .filter_map(|id| node_map.get(id))
            .copied()
            .collect();

        if root_children.is_empty() {
            return;
        }

        // Check if root has sequence children - handle them specially
        let sequences: Vec<&YamlNode> = root_children
            .iter()
            .filter(|n| matches!(n.node_type, YamlNodeType::Sequence { .. }))
            .copied()
            .collect();

        // Handle sequences - create one group per element
        for seq in sequences {
            self.handle_sequence_elements(seq, node_map, groups, grouped_ids);
        }

        // Get non-sequence children (mappings and key-values)
        let other_children: Vec<&YamlNode> = root_children
            .iter()
            .filter(|n| !matches!(n.node_type, YamlNodeType::Sequence { .. }))
            .copied()
            .collect();

        if other_children.is_empty() {
            return;
        }

        // Group the flat members into chunks
        let chunks = other_children.chunks(self.max_root_members);

        for chunk in chunks {
            let mut group = YamlGroup::new(
                self.next_group_id(),
                YamlGroupType::KeyValueGroup,
                String::new(),
            );

            for member in chunk {
                // Only add direct leaf children to root_flat groups
                // Mapping and container children are left for the mapping_groups phase
                if member.node_type.is_leaf() {
                    group.add_member((*member).clone());
                    grouped_ids.insert(member.id.clone());
                }
            }

            group.finalize(&self.estimator);
            groups.push(group);
        }

        // Mark root as grouped
        grouped_ids.insert(root.id.clone());
    }

    /// Handle sequence by creating one group per element
    fn handle_sequence_elements(
        &mut self,
        sequence: &YamlNode,
        node_map: &std::collections::HashMap<String, &YamlNode>,
        groups: &mut Vec<YamlGroup>,
        grouped_ids: &mut std::collections::HashSet<String>,
    ) {
        // Get sequence's children (SequenceElement nodes)
        let elements: Vec<&YamlNode> = sequence
            .children
            .iter()
            .filter_map(|id| node_map.get(id))
            .copied()
            .collect();

        // Create one group per sequence element
        for element in elements {
            if !element.node_type.is_leaf()
                && !matches!(element.node_type, YamlNodeType::Mapping { .. })
            {
                continue;
            }

            let mut group = YamlGroup::new(
                self.next_group_id(),
                YamlGroupType::SequenceElement,
                sequence.path.clone(),
            );

            // Add the element node as header
            group.set_header(element.clone());
            grouped_ids.insert(element.id.clone());

            // Add all descendants of this element (if it's a mapping)
            self.add_node_and_descendants(element, node_map, &mut group, grouped_ids);

            group.finalize(&self.estimator);
            groups.push(group);
        }

        // Mark sequence container as grouped
        grouped_ids.insert(sequence.id.clone());
    }

    /// Recursively add a node and all its descendants to a group
    fn add_node_and_descendants(
        &self,
        node: &YamlNode,
        node_map: &std::collections::HashMap<String, &YamlNode>,
        group: &mut YamlGroup,
        grouped_ids: &mut std::collections::HashSet<String>,
    ) {
        // Skip if already grouped
        if grouped_ids.contains(&node.id) {
            return;
        }

        // Only add leaf nodes as members (non-header nodes)
        if node.node_type.is_leaf() && group.header.is_some() {
            group.add_member(node.clone());
            grouped_ids.insert(node.id.clone());
        } else if !node.node_type.is_leaf() {
            // For container nodes, set path_prefix from the first container encountered
            if group.path_prefix.is_empty() {
                group.path_prefix = node.path.clone();
            }
        }

        // Recursively process children
        for child_id in &node.children {
            if let Some(child) = node_map.get(child_id) {
                self.add_node_and_descendants(child, node_map, group, grouped_ids);
            }
        }
    }

    /// Get the mapping key for a node
    fn get_mapping_key(
        node: &YamlNode,
        node_map: &std::collections::HashMap<String, &YamlNode>,
    ) -> String {
        // Traverse up to find the nearest mapping or root
        let mut current_id = node.parent_id.clone();

        while let Some(ref id) = current_id {
            if let Some(parent_node) = node_map.get(id) {
                match &parent_node.node_type {
                    YamlNodeType::Root => {
                        return String::new();
                    }
                    YamlNodeType::Mapping { key } => {
                        return format!("{}.{}", parent_node.path, key);
                    }
                    _ => {
                        current_id = parent_node.parent_id.clone();
                    }
                }
            } else {
                break;
            }
        }

        // If no mapping found, it belongs to root
        String::new()
    }

    /// Split groups that exceed limits
    fn split_large_groups(&self, groups: Vec<YamlGroup>) -> Vec<YamlGroup> {
        let mut result = Vec::new();
        let mut group_counter = self.group_counter;

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
                let split_groups = self.split_group(group, &mut group_counter);
                result.extend(split_groups);
            }
        }

        result
    }

    /// Split a single group into multiple groups
    fn split_group(&self, group: YamlGroup, group_counter: &mut usize) -> Vec<YamlGroup> {
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
                    .map(|m: &crate::yaml::types::YamlNode| {
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
                let mut new_group = YamlGroup::new(
                    format!("yaml_group_{}", group_counter),
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
            let mut new_group = YamlGroup::new(
                format!("yaml_group_{}", group_counter),
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

impl Default for YamlGrouper {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::yaml::types::YamlValueType;
    use cce_types::Span;

    fn create_mock_node(id: &str, node_type: YamlNodeType, path: &str) -> YamlNode {
        YamlNode::new(id.to_string(), node_type, path.to_string(), Span::default())
    }

    #[test]
    fn test_group_simple_mapping() {
        let mut grouper = YamlGrouper::new();

        let mut root = create_mock_node("root", YamlNodeType::Root, "");
        let kv1 = create_mock_node(
            "kv1",
            YamlNodeType::KeyValue {
                key: "name".to_string(),
                value_type: YamlValueType::String,
            },
            "name",
        )
        .with_parent("root".to_string());

        let kv2 = create_mock_node(
            "kv2",
            YamlNodeType::KeyValue {
                key: "value".to_string(),
                value_type: YamlValueType::Integer,
            },
            "value",
        )
        .with_parent("root".to_string());

        // Set up parent-child relationships
        root.add_child("kv1".to_string());
        root.add_child("kv2".to_string());

        let nodes = vec![root, kv1, kv2];
        let groups = grouper.group(nodes, "test.yaml").expect("should group");

        // Root should be flattened into KeyValueGroup(s)
        assert!(!groups.is_empty());

        let kv_groups: Vec<&YamlGroup> = groups
            .iter()
            .filter(|g| g.group_type == YamlGroupType::KeyValueGroup)
            .collect();

        assert!(
            !kv_groups.is_empty(),
            "Should have at least one KeyValueGroup"
        );
    }

    #[test]
    fn test_group_with_nested_mappings() {
        let mut grouper = YamlGrouper::new();

        let mut root = create_mock_node("root", YamlNodeType::Root, "");
        let mut mapping1 = create_mock_node(
            "mapping1",
            YamlNodeType::Mapping {
                key: "database".to_string(),
            },
            "database",
        )
        .with_parent("root".to_string());

        let kv1 = create_mock_node(
            "kv1",
            YamlNodeType::KeyValue {
                key: "host".to_string(),
                value_type: YamlValueType::String,
            },
            "database.host",
        )
        .with_parent("mapping1".to_string());

        let kv2 = create_mock_node(
            "kv2",
            YamlNodeType::KeyValue {
                key: "name".to_string(),
                value_type: YamlValueType::String,
            },
            "name",
        )
        .with_parent("root".to_string());

        // Add another root-level key to trigger root flattening
        let kv3 = create_mock_node(
            "kv3",
            YamlNodeType::KeyValue {
                key: "version".to_string(),
                value_type: YamlValueType::String,
            },
            "version",
        )
        .with_parent("root".to_string());

        // Set up parent-child relationships
        root.add_child("mapping1".to_string());
        root.add_child("kv2".to_string());
        root.add_child("kv3".to_string());
        mapping1.add_child("kv1".to_string());

        let nodes = vec![root, mapping1, kv1, kv2, kv3];
        let groups = grouper.group(nodes, "test.yaml").expect("should group");

        // Should have at least 2 groups (database NamedMapping + root KeyValueGroup)
        assert!(groups.len() >= 2, "Should have at least 2 groups");

        // Find database group (NamedMapping)
        let db_group = groups.iter().find(|g| g.path_prefix.contains("database"));
        assert!(db_group.is_some(), "Should have database group");

        // Root members should be in a KeyValueGroup
        let _kv_groups: Vec<&YamlGroup> = groups
            .iter()
            .filter(|g| g.group_type == YamlGroupType::KeyValueGroup)
            .collect();

        assert!(
            !_kv_groups.is_empty(),
            "Should have at least one KeyValueGroup for root members"
        );
    }

    #[test]
    fn test_sequence_elements_as_separate_groups() {
        let mut grouper = YamlGrouper::new();

        let mut root = create_mock_node("root", YamlNodeType::Root, "");
        let mut sequence = create_mock_node(
            "seq",
            YamlNodeType::Sequence {
                key: Some("items".to_string()),
            },
            "items",
        )
        .with_parent("root".to_string());

        let mut elem1 = create_mock_node(
            "elem1",
            YamlNodeType::SequenceElement {
                index: 0,
                value_type: YamlValueType::Mapping,
            },
            "items[0]",
        )
        .with_parent("seq".to_string());

        let mut elem2 = create_mock_node(
            "elem2",
            YamlNodeType::SequenceElement {
                index: 1,
                value_type: YamlValueType::Mapping,
            },
            "items[1]",
        )
        .with_parent("seq".to_string());

        let kv1 = create_mock_node(
            "kv1",
            YamlNodeType::KeyValue {
                key: "name".to_string(),
                value_type: YamlValueType::String,
            },
            "items[0].name",
        )
        .with_parent("elem1".to_string());

        let kv2 = create_mock_node(
            "kv2",
            YamlNodeType::KeyValue {
                key: "name".to_string(),
                value_type: YamlValueType::String,
            },
            "items[1].name",
        )
        .with_parent("elem2".to_string());

        // Set up parent-child relationships
        root.add_child("seq".to_string());
        sequence.add_child("elem1".to_string());
        sequence.add_child("elem2".to_string());
        elem1.add_child("kv1".to_string());
        elem2.add_child("kv2".to_string());

        let nodes = vec![root, sequence, elem1, elem2, kv1, kv2];
        let groups = grouper.group(nodes, "test.yaml").expect("should group");

        // Should have separate SequenceElement groups
        let seq_elem_groups: Vec<&YamlGroup> = groups
            .iter()
            .filter(|g| g.group_type == YamlGroupType::SequenceElement)
            .collect();

        assert_eq!(
            seq_elem_groups.len(),
            2,
            "Should have 2 sequence element groups"
        );

        // Each sequence element group should have a header
        for group in &seq_elem_groups {
            assert!(
                group.has_header(),
                "Sequence element group should have header"
            );
        }
    }

    #[test]
    fn test_root_flattening() {
        let mut grouper = YamlGrouper::new().with_max_root_members(2);

        let mut root = create_mock_node("root", YamlNodeType::Root, "");

        let kv1 = create_mock_node(
            "kv1",
            YamlNodeType::KeyValue {
                key: "name".to_string(),
                value_type: YamlValueType::String,
            },
            "name",
        )
        .with_parent("root".to_string());

        let kv2 = create_mock_node(
            "kv2",
            YamlNodeType::KeyValue {
                key: "version".to_string(),
                value_type: YamlValueType::String,
            },
            "version",
        )
        .with_parent("root".to_string());

        let kv3 = create_mock_node(
            "kv3",
            YamlNodeType::KeyValue {
                key: "debug".to_string(),
                value_type: YamlValueType::Boolean,
            },
            "debug",
        )
        .with_parent("root".to_string());

        // Set up parent-child relationships
        root.add_child("kv1".to_string());
        root.add_child("kv2".to_string());
        root.add_child("kv3".to_string());

        let nodes = vec![root, kv1, kv2, kv3];
        let groups = grouper.group(nodes, "test.yaml").expect("should group");

        // With max_root_members=2 and 3 keys, should have at least 2 groups
        let kv_groups: Vec<&YamlGroup> = groups
            .iter()
            .filter(|g| g.group_type == YamlGroupType::KeyValueGroup)
            .collect();

        assert!(
            kv_groups.len() >= 2,
            "Should have at least 2 KeyValueGroups due to root flattening"
        );
    }
}
