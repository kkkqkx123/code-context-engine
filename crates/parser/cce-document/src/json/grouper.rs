//! JSON grouper
//!
//! Groups JSON nodes based on object/array boundaries.

use crate::GenericGroup;
use crate::json::types::{JsonGroup, JsonGroupType, JsonNode, JsonNodeType};
use cce_text::MixedTokenizer;
use cce_types::ParseError;
use cce_utils::token_estimation::TokenEstimator;

/// JSON grouper
#[derive(Clone)]
pub struct JsonGrouper {
    estimator: TokenEstimator,
    /// Minimum object members to create a separate group
    min_object_members: usize,
    /// Maximum tokens per group for embedding path (0 = no limit)
    max_tokens: usize,
    /// Maximum words per group for BM25 path (0 = no limit)
    max_bm25_words: usize,
}

impl JsonGrouper {
    /// Create a new grouper with default settings
    pub fn new() -> Self {
        Self {
            estimator: TokenEstimator::default(),
            min_object_members: 2,
            max_tokens: 0,
            max_bm25_words: 200,
        }
    }

    /// Set minimum object members for separate grouping
    pub fn with_min_object_members(mut self, min: usize) -> Self {
        self.min_object_members = min;
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

    /// Group parsed nodes by object/array boundaries
    pub fn group(
        &self,
        nodes: Vec<JsonNode>,
        file_path: &str,
    ) -> Result<Vec<JsonGroup>, ParseError> {
        let mut groups = Vec::new();
        let mut group_counter = 0;

        // Build a map for quick node lookup
        let node_map: std::collections::HashMap<&str, &JsonNode> =
            nodes.iter().map(|n| (n.id.as_str(), n)).collect();

        // Find all container nodes (objects and arrays)
        let containers: Vec<&JsonNode> = nodes
            .iter()
            .filter(|n| {
                matches!(
                    n.node_type,
                    JsonNodeType::Root | JsonNodeType::Object | JsonNodeType::Array
                )
            })
            .collect();

        // Track which nodes have been grouped
        let mut grouped_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

        for container in containers {
            // Skip if already grouped
            if grouped_ids.contains(&container.id) {
                continue;
            }

            // Handle root object specially - flatten it
            if matches!(container.node_type, JsonNodeType::Root) {
                self.handle_root_object(
                    container,
                    &node_map,
                    &mut groups,
                    &mut group_counter,
                    &mut grouped_ids,
                    file_path,
                );
                continue;
            }

            // Handle arrays specially - create one group per element
            if matches!(container.node_type, JsonNodeType::Array) {
                self.handle_array_elements(
                    container,
                    &node_map,
                    &mut groups,
                    &mut group_counter,
                    &mut grouped_ids,
                    file_path,
                );
                continue;
            }

            // Determine group type for nested objects
            let group_type = match &container.node_type {
                JsonNodeType::Object => JsonGroupType::NestedObject,
                _ => continue,
            };

            // Get children of this container
            let children: Vec<&JsonNode> = container
                .children
                .iter()
                .filter_map(|id| node_map.get(id.as_str()))
                .copied()
                .collect();

            // Check if this container should be a separate group
            let should_be_separate = self.should_be_separate_group(container, &children);

            if should_be_separate {
                // Create a group for this container
                group_counter += 1;
                let mut group = JsonGroup::new(
                    format!("{}_{}_{}", file_path, group_type, group_counter),
                    group_type,
                    container.path.clone(),
                );

                // Add container as header
                group.set_header(container.clone());
                grouped_ids.insert(container.id.clone());

                // Add children as members (don't recurse into sub-containers that qualify as separate groups)
                for child in &children {
                    self.add_node_and_descendants(
                        child,
                        &node_map,
                        &mut group,
                        &mut grouped_ids,
                        false,
                    );
                }

                group.finalize(&self.estimator);
                groups.push(group);
            } else {
                // Container is too small to be its own group.
                // Mark leaf children as grouped so they don't appear in "remaining".
                // Container children are left for the main loop to process,
                // allowing them to become their own groups if they qualify.
                for child in &children {
                    if child.node_type.is_leaf() {
                        grouped_ids.insert(child.id.clone());
                    }
                }
            }
        }

        // Handle remaining ungrouped nodes (key-value pairs at root level)
        let remaining: Vec<&JsonNode> = nodes
            .iter()
            .filter(|n| !grouped_ids.contains(&n.id) && n.node_type.is_leaf())
            .collect();

        if !remaining.is_empty() {
            group_counter += 1;
            let mut group = JsonGroup::new(
                format!("{}_keyvalue_{}", file_path, group_counter),
                JsonGroupType::KeyValueGroup,
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
            let mut group = JsonGroup::new(
                format!("{}_root_{}", file_path, group_counter),
                JsonGroupType::RootObject,
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

    /// Handle root object by flattening its children into groups
    fn handle_root_object(
        &self,
        root: &JsonNode,
        node_map: &std::collections::HashMap<&str, &JsonNode>,
        groups: &mut Vec<JsonGroup>,
        group_counter: &mut usize,
        grouped_ids: &mut std::collections::HashSet<String>,
        file_path: &str,
    ) {
        // Get root's children
        let root_children: Vec<&JsonNode> = root
            .children
            .iter()
            .filter_map(|id| node_map.get(id.as_str()))
            .copied()
            .collect();

        if root_children.is_empty() {
            return;
        }

        // Group the flat members into chunks
        let chunk_size = 5; // Default: 5 keys per group
        let chunks = root_children.chunks(chunk_size);

        for chunk in chunks {
            *group_counter += 1;
            let mut group = JsonGroup::new(
                format!("{}_root_flat_{}", file_path, group_counter),
                JsonGroupType::KeyValueGroup,
                String::new(),
            );

            for member in chunk {
                // Only add direct leaf children to root_flat groups
                // Container children are left for the main container loop
                // so they get their own structured groups
                if member.node_type.is_leaf() {
                    group.add_member((*member).clone());
                    grouped_ids.insert(member.id.clone());
                }
            }

            group.finalize(&self.estimator);
            if !group.members.is_empty() || group.header.is_some() {
                groups.push(group);
            }
        }

        // Mark root as grouped
        grouped_ids.insert(root.id.clone());
    }

    /// Handle array by creating one group per element
    fn handle_array_elements(
        &self,
        array: &JsonNode,
        node_map: &std::collections::HashMap<&str, &JsonNode>,
        groups: &mut Vec<JsonGroup>,
        group_counter: &mut usize,
        grouped_ids: &mut std::collections::HashSet<String>,
        file_path: &str,
    ) {
        // Get array's children (ArrayElement nodes)
        let elements: Vec<&JsonNode> = array
            .children
            .iter()
            .filter_map(|id| node_map.get(id.as_str()))
            .copied()
            .collect();

        // Create one group per array element
        for element in elements {
            if !matches!(element.node_type, JsonNodeType::Primitive(_))
                && !element.node_type.is_container()
            {
                continue;
            }

            *group_counter += 1;
            let mut group = JsonGroup::new(
                format!("{}_array_elem_{}", file_path, group_counter),
                JsonGroupType::ArrayElement,
                array.path.clone(),
            );

            // Add the element node as header
            group.set_header(element.clone());
            grouped_ids.insert(element.id.clone());

            // Add all descendants of this element (recurse into sub-containers)
            self.add_node_and_descendants(element, node_map, &mut group, grouped_ids, true);

            group.finalize(&self.estimator);
            groups.push(group);
        }

        // Mark array container as grouped
        grouped_ids.insert(array.id.clone());
    }

    /// Recursively add a node and its descendants to a group.
    /// `recurse_containers` controls whether sub-container children are recursed into.
    /// When false (used for Object container children), sub-containers that qualify as
    /// separate groups are skipped so they can form their own groups.
    fn add_node_and_descendants(
        &self,
        node: &JsonNode,
        node_map: &std::collections::HashMap<&str, &JsonNode>,
        group: &mut JsonGroup,
        grouped_ids: &mut std::collections::HashSet<String>,
        recurse_containers: bool,
    ) {
        // Skip if already grouped
        if grouped_ids.contains(&node.id) {
            return;
        }

        let is_leaf = node.node_type.is_leaf();

        if is_leaf && group.header.is_some() {
            group.add_member(node.clone());
            grouped_ids.insert(node.id.clone());
        } else if !is_leaf {
            // For container nodes, set path_prefix from the first container encountered
            if group.path_prefix.is_empty() {
                group.path_prefix = node.path.clone();
            }
            // If this sub-container qualifies as its own group, don't recurse into it
            if !recurse_containers {
                let child_refs: Vec<&JsonNode> = node
                    .children
                    .iter()
                    .filter_map(|id| node_map.get(id.as_str()))
                    .copied()
                    .collect();
                if self.should_be_separate_group(node, &child_refs) {
                    return;
                }
            }
        }

        // Recursively process children
        for child_id in &node.children {
            if let Some(child) = node_map.get(child_id.as_str()) {
                self.add_node_and_descendants(
                    child,
                    node_map,
                    group,
                    grouped_ids,
                    recurse_containers,
                );
            }
        }
    }

    /// Check if a container should be a separate group
    fn should_be_separate_group(&self, container: &JsonNode, children: &[&JsonNode]) -> bool {
        match &container.node_type {
            JsonNodeType::Root => false,
            JsonNodeType::Object => {
                // Count primitive children (direct values)
                let leaf_count = children.iter().filter(|c| c.node_type.is_leaf()).count();
                // Only nested objects (depth > 0) with enough members should be separate
                container.depth > 0 && leaf_count >= self.min_object_members
            }
            JsonNodeType::Array => false,
            JsonNodeType::Primitive(_) => false,
        }
    }

    /// Split groups that exceed limits
    fn split_large_groups(&self, groups: Vec<JsonGroup>, file_path: &str) -> Vec<JsonGroup> {
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
        group: JsonGroup,
        file_path: &str,
        group_counter: &mut usize,
    ) -> Vec<JsonGroup> {
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
                    .map(|m: &crate::json::types::JsonNode| {
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
                let mut new_group = JsonGroup::new(
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
            let mut new_group = JsonGroup::new(
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

impl Default for JsonGrouper {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::types::JsonValueType;
    use cce_types::Span;

    fn create_test_node(id: &str, node_type: JsonNodeType, path: &str) -> JsonNode {
        JsonNode::new(id.to_string(), node_type, path.to_string(), Span::default())
    }

    fn create_kv_node(id: &str, key: &str, path: &str, value: &str) -> JsonNode {
        JsonNode::new(
            id.to_string(),
            JsonNodeType::Primitive(JsonValueType::String),
            path.to_string(),
            Span::default(),
        )
        .with_key_name(key.to_string())
        .with_value(value.to_string())
    }

    #[test]
    fn test_group_simple_object() {
        let grouper = JsonGrouper::new();

        let mut nodes = vec![
            create_test_node("root", JsonNodeType::Root, ""),
            create_kv_node("kv1", "name", "name", "test"),
            create_kv_node("kv2", "value", "value", "123"),
        ];

        // Set up parent-child relationships
        nodes[0].add_child("kv1".to_string());
        nodes[0].add_child("kv2".to_string());
        nodes[1].parent_id = Some("root".to_string());
        nodes[2].parent_id = Some("root".to_string());

        let groups = grouper.group(nodes, "test.json").expect("should group");

        // Should have at least one group
        assert!(!groups.is_empty());
    }

    #[test]
    fn test_group_nested_objects() {
        let grouper = JsonGrouper::new();

        let mut nodes = vec![
            create_test_node("root", JsonNodeType::Root, ""),
            create_test_node("db", JsonNodeType::Object, "database"),
            create_kv_node("host", "host", "database.host", "localhost"),
            create_kv_node("port", "port", "database.port", "3306"),
        ];

        // Set up relationships
        nodes[0].add_child("db".to_string());
        nodes[1].parent_id = Some("root".to_string());
        nodes[1].add_child("host".to_string());
        nodes[1].add_child("port".to_string());
        nodes[2].parent_id = Some("db".to_string());
        nodes[3].parent_id = Some("db".to_string());

        let groups = grouper.group(nodes, "test.json").expect("should group");

        // Should have groups for root and nested object
        assert!(!groups.is_empty());
    }

    #[test]
    fn test_group_array() {
        let grouper = JsonGrouper::new();

        let mut nodes = vec![
            create_test_node("root", JsonNodeType::Root, ""),
            create_test_node("arr", JsonNodeType::Array, "items"),
            create_kv_node("elem0", "0", "items[0]", "a"),
            create_kv_node("elem1", "1", "items[1]", "b"),
            create_kv_node("elem2", "2", "items[2]", "c"),
            create_kv_node("elem3", "3", "items[3]", "d"),
        ];

        // Set up relationships
        nodes[0].add_child("arr".to_string());
        nodes[1].parent_id = Some("root".to_string());
        for i in 2..=5 {
            nodes[1].add_child(format!("elem{}", i - 2));
            nodes[i].parent_id = Some("arr".to_string());
        }

        let groups = grouper.group(nodes, "test.json").expect("should group");

        // Array elements should be separate groups
        assert!(
            groups
                .iter()
                .any(|g| g.group_type == JsonGroupType::ArrayElement)
        );
    }

    #[test]
    fn test_group_with_max_tokens() {
        let grouper = JsonGrouper::new().with_max_tokens(10);

        let mut nodes = vec![
            create_test_node("root", JsonNodeType::Root, ""),
            create_kv_node("kv1", "key1", "key1", "value1"),
            create_kv_node("kv2", "key2", "key2", "value2"),
            create_kv_node("kv3", "key3", "key3", "value3"),
        ];

        nodes[0].add_child("kv1".to_string());
        nodes[0].add_child("kv2".to_string());
        nodes[0].add_child("kv3".to_string());
        for node in &mut nodes[1..=3] {
            node.parent_id = Some("root".to_string());
        }

        let groups = grouper.group(nodes, "test.json").expect("should group");

        // May be split into multiple groups due to token limit
        assert!(!groups.is_empty());
    }
}
