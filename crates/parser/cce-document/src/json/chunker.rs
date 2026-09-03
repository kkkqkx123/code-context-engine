//! JSON chunker
//!
//! Converts JsonGroups to ChunkedResults for storage.
//! Leverages GenericChunker trait with smart merging support.

use crate::common::{GenericChunker, MergingConfig};
use crate::json::types::{JsonGroup, JsonGroupType, JsonNode};
use crate::types::DocumentClassification;
use cce_config::modules::ChunkingConfig;
use cce_types::ChunkedResult;
use cce_types::GroupType;
use cce_types::ast_to_nl::options::OutputMode;
use cce_types::language::Language;
use cce_utils::token_estimation::TokenEstimator;

/// JSON chunker with smart merging support
pub struct JsonChunker {
    config: ChunkingConfig,
    estimator: TokenEstimator,
    merging_config: MergingConfig,
}

crate::chunker_boilerplate!(JsonChunker, with_merging_builders);

impl JsonChunker {
    /// Enable or disable key-based association merging
    pub fn with_key_based_association(mut self, enabled: bool) -> Self {
        self.merging_config.enable_key_based_association = enabled;
        self
    }
}

impl GenericChunker<JsonGroup, JsonNode> for JsonChunker {
    fn config(&self) -> &ChunkingConfig {
        &self.config
    }

    fn estimator(&self) -> &TokenEstimator {
        &self.estimator
    }

    fn language(&self) -> Language {
        Language::Json
    }

    fn to_group_type(group: &JsonGroup) -> GroupType {
        group.group_type.to_doc_group_type().to_group_type()
    }

    fn to_embedding_text(text: &str, group: &JsonGroup) -> String {
        // Add full path context for better RAG quality
        let context = if group.path_prefix.is_empty() {
            "[Root]".to_string()
        } else {
            // Convert dotted path to hierarchical structure
            let parts: Vec<&str> = group.path_prefix.split('.').collect();
            let path_chain = parts.join(" -> ");
            format!("[Context: {}]", path_chain)
        };

        // Add group type hint
        let type_hint = match group.group_type {
            JsonGroupType::RootObject => " (Root Config)",
            JsonGroupType::NestedObject => " (Object)",
            JsonGroupType::Array => " (Array)",
            JsonGroupType::ArrayElement => " (Array Element)",
            JsonGroupType::KeyValueGroup => "",
        };

        format!("{}{}\n{}", context, type_hint, text.trim())
    }

    /// Override chunk_groups to use smart merging strategy
    fn chunk_groups(
        &self,
        groups: &[JsonGroup],
        file_path: &str,
        output_mode: OutputMode,
        classification: &DocumentClassification,
    ) -> Vec<cce_types::ChunkedResult> {
        if !self.merging_config.enable_smart_merging || groups.is_empty() {
            // Fall back to default behavior: process each group independently
            return groups
                .iter()
                .flat_map(|g| self.chunk_group(g, file_path, output_mode, classification))
                .collect();
        }

        self.chunk_groups_smart(groups, file_path, output_mode, classification)
    }
}

impl JsonChunker {
    /// Smart merging strategy: decide which groups should be merged into one chunk
    fn chunk_groups_smart(
        &self,
        groups: &[JsonGroup],
        file_path: &str,
        output_mode: OutputMode,
        classification: &DocumentClassification,
    ) -> Vec<cce_types::ChunkedResult> {
        let mut chunks = Vec::new();
        let mut used_groups = std::collections::HashSet::new();

        for (i, group) in groups.iter().enumerate() {
            if used_groups.contains(&group.group_id) {
                continue; // Already merged into another chunk
            }

            // Collect groups to merge
            let mut merged_groups = vec![group];
            used_groups.insert(group.group_id.clone());

            // Check subsequent groups for potential merging
            for next_group in groups.iter().skip(i + 1) {
                if used_groups.contains(&next_group.group_id) {
                    continue;
                }

                // Rule A: Small groups merging
                if self.should_merge_small_groups(group, next_group) {
                    merged_groups.push(next_group);
                    used_groups.insert(next_group.group_id.clone());
                    continue;
                }

                // Rule B: Key-based association merging (if enabled)
                if self.merging_config.enable_key_based_association
                    && Self::detect_key_based_association(group, next_group).is_some()
                {
                    // Check if merged size is within limit
                    let combined_tokens =
                        merged_groups.iter().map(|g| g.token_count).sum::<usize>()
                            + next_group.token_count;

                    let max_allowed = (self.config.max_tokens as f32
                        * self.merging_config.max_merge_expansion_factor)
                        as usize;

                    if combined_tokens <= max_allowed {
                        merged_groups.push(next_group);
                        used_groups.insert(next_group.group_id.clone());
                    }
                }
            }

            // Create merged chunk
            let chunks_new =
                self.create_merged_chunk(&merged_groups, file_path, output_mode, classification);
            chunks.extend(chunks_new);
        }

        chunks
    }

    /// Rule A: Check if two small groups should be merged
    fn should_merge_small_groups(&self, group_a: &JsonGroup, group_b: &JsonGroup) -> bool {
        // Condition 1: Both groups are small
        let both_small = group_a.token_count < self.merging_config.min_chunk_tokens
            && group_b.token_count < self.merging_config.min_chunk_tokens;

        // Condition 2: Merged size doesn't exceed max token limit
        let fits_in_limit = (group_a.token_count + group_b.token_count) <= self.config.max_tokens;

        // Condition 3: Same parent or sibling relationship (maintain locality)
        let same_parent = Self::are_sibling_groups(group_a, group_b);

        both_small && fits_in_limit && same_parent
    }

    /// Check if two groups are siblings (same parent path)
    fn are_sibling_groups(group_a: &JsonGroup, group_b: &JsonGroup) -> bool {
        let parent_a = Self::get_parent_path(&group_a.path_prefix);
        let parent_b = Self::get_parent_path(&group_b.path_prefix);

        // Same non-empty parent → definitely siblings
        if !parent_a.is_empty() && parent_a == parent_b {
            return true;
        }
        // Both at root level → only siblings if same path_prefix
        if parent_a.is_empty() && parent_b.is_empty() {
            return group_a.path_prefix == group_b.path_prefix;
        }
        // One is sub-path of the other's parent (only when parent is non-empty)
        (!parent_b.is_empty() && group_a.path_prefix.starts_with(&parent_b))
            || (!parent_a.is_empty() && group_b.path_prefix.starts_with(&parent_a))
    }

    /// Get parent path from a full path
    fn get_parent_path(path: &str) -> String {
        if path.is_empty() {
            return String::new();
        }

        path.rsplit_once('.')
            .map(|(parent, _)| parent.to_string())
            .unwrap_or_default()
    }

    /// Rule B: Detect key-based association between two groups
    fn detect_key_based_association(
        group_a: &JsonGroup,
        group_b: &JsonGroup,
    ) -> Option<Vec<String>> {
        // Step 1: Extract key tokens from both groups
        let keys_a = Self::extract_key_tokens(group_a);
        let keys_b = Self::extract_key_tokens(group_b);

        // Step 2: Find shared tokens
        let shared: Vec<&String> = keys_a.iter().filter(|k| keys_b.contains(*k)).collect();

        if shared.is_empty() {
            return None;
        }

        // Filter 1: At least one meaningful shared keyword (exclude generic ones)
        let meaningful_shared: Vec<&&String> = shared
            .iter()
            .filter(|k| !Self::is_generic_keyword(k))
            .collect();

        if meaningful_shared.is_empty() {
            return None;
        }

        // Filter 2: Same depth level
        let depth_a = group_a
            .path_prefix
            .split('.')
            .filter(|s| !s.is_empty())
            .count();
        let depth_b = group_b
            .path_prefix
            .split('.')
            .filter(|s| !s.is_empty())
            .count();

        if depth_a != depth_b {
            return None;
        }

        // Filter 3: Consistent value types
        if !Self::has_consistent_value_types(group_a, group_b) {
            return None;
        }

        Some(meaningful_shared.iter().map(|s| s.to_string()).collect())
    }

    /// Extract normalized key tokens from a group
    fn extract_key_tokens(group: &JsonGroup) -> Vec<String> {
        let mut tokens = Vec::new();

        // Add header key if exists
        if let Some(ref header) = group.header {
            if let Some(ref key) = header.key_name {
                tokens.extend(Self::normalize_and_split_key(key));
            }
        }

        // Add member keys
        for member in &group.members {
            if let Some(ref key) = member.key_name {
                tokens.extend(Self::normalize_and_split_key(key));
            }
        }

        tokens
    }

    /// Normalize and split a key into tokens
    fn normalize_and_split_key(key: &str) -> Vec<String> {
        // Replace delimiters with spaces, convert to lowercase
        let normalized = key.replace(['_', '.', '-'], " ").to_lowercase();

        normalized
            .split_whitespace()
            .map(|s| s.to_string())
            .collect()
    }

    /// Check if a keyword is too generic
    fn is_generic_keyword(keyword: &str) -> bool {
        matches!(keyword, "id" | "name" | "type" | "value" | "data" | "info")
    }

    /// Check if two groups have consistent value types
    fn has_consistent_value_types(group_a: &JsonGroup, group_b: &JsonGroup) -> bool {
        use crate::json::types::JsonValueType;

        // Get value types from members
        let types_a: Vec<Option<JsonValueType>> = group_a
            .members
            .iter()
            .map(|m| m.node_type.value_type())
            .collect();

        let types_b: Vec<Option<JsonValueType>> = group_b
            .members
            .iter()
            .map(|m| m.node_type.value_type())
            .collect();

        // Simple check: if both have members, at least some types should match
        if types_a.is_empty() || types_b.is_empty() {
            return true; // Can't determine, assume compatible
        }

        // Check if there's at least one common value type
        let unique_types_a: std::collections::HashSet<_> = types_a.iter().flatten().collect();
        let unique_types_b: std::collections::HashSet<_> = types_b.iter().flatten().collect();

        !unique_types_a.is_disjoint(&unique_types_b)
    }

    /// Extract a human-readable title from JSON groups.
    fn extract_title(groups: &[&JsonGroup], file_path: &str) -> Option<String> {
        let first = groups.first()?;
        if !first.path_prefix.is_empty() {
            return Some(first.path_prefix.clone());
        }
        if let Some(ref header) = first.header {
            if let Some(ref key) = header.key_name {
                return Some(key.clone());
            }
        }
        std::path::Path::new(file_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .map(String::from)
    }

    /// Create a merged chunk from multiple groups using two-tier scheme.
    fn create_merged_chunk(
        &self,
        groups: &[&JsonGroup],
        file_path: &str,
        output_mode: OutputMode,
        classification: &DocumentClassification,
    ) -> Vec<ChunkedResult> {
        use crate::common::chunker::{TwoTierParams, merged_span, two_tier_chunking};

        let mut all_bm25_parts = Vec::new();
        let mut all_embedding_parts = Vec::new();
        // Collect all nodes from merged groups
        for group in groups {
            if let Some(ref header) = group.header {
                let bm25 = header.to_bm25_text();
                if !bm25.is_empty() {
                    all_bm25_parts.push(bm25);
                }
                let emb = header.to_embedding_text();
                if !emb.is_empty() {
                    all_embedding_parts.push(emb);
                }
            }

            for member in &group.members {
                let bm25 = member.to_bm25_text();
                if !bm25.is_empty() {
                    all_bm25_parts.push(bm25);
                }
                let emb = member.to_embedding_text();
                if !emb.is_empty() {
                    all_embedding_parts.push(emb);
                }
            }
        }

        let combined_bm25 = all_bm25_parts.join("\n");
        let embedding_text = Self::to_embedding_text(&all_embedding_parts.join("\n"), groups[0]);
        let span = merged_span(&groups.iter().map(|g| g.span).collect::<Vec<_>>());

        two_tier_chunking(TwoTierParams {
            embedding_text: &embedding_text,
            bm25_text: &combined_bm25,
            source_span: span,
            source_group_id: &groups[0].group_id,
            file_path,
            config: &self.config,
            estimator: &self.estimator,
            group_type: Self::to_group_type(groups[0]),
            bm25_title: Self::extract_title(groups, file_path),
            output_mode,
            content_type: classification.payload().clone(),
            file_category: classification.category(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GenericGroup;
    use cce_types::Span;

    fn create_test_group(text: &str, token_count: usize) -> JsonGroup {
        let mut group = JsonGroup::new(
            "test_group".to_string(),
            JsonGroupType::NestedObject,
            "test".to_string(),
        );
        group.bm25_text = text.to_string();
        group.embedding_text = text.to_string();
        group.token_count = token_count;
        group.span = Span::from_lines(0, 10);
        group
    }

    #[test]
    fn test_chunk_group_no_split() {
        let config = ChunkingConfig::default();
        let chunker = JsonChunker::new(config);

        let group = create_test_group("key = value", 10);
        let chunks = chunker.chunk_group(
            &group,
            "test.json",
            OutputMode::default(),
            &DocumentClassification::detect("test.json"),
        );

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].total_chunks, 1);
        assert!(chunks[0].is_first());
        assert!(chunks[0].is_last());
        // Document chunks don't have code_metadata, so is_fragment() returns false
        assert!(!chunks[0].metadata.is_fragment());
    }

    #[test]
    fn test_chunk_group_with_split() {
        let config = ChunkingConfig {
            max_tokens: 5,     // Very small to force splitting
            max_bm25_words: 2, // Also force BM25 path splitting (each line ~3 words)
            ..Default::default()
        };
        let chunker = JsonChunker::new(config);

        // Create a long text with multiple lines
        let long_text = "key1 = value1\nkey2 = value2\nkey3 = value3\nkey4 = value4\nkey5 = value5";
        let group = create_test_group(long_text, 100);
        let chunks = chunker.chunk_group(
            &group,
            "test.json",
            OutputMode::default(),
            &DocumentClassification::detect("test.json"),
        );

        // With max_tokens=5 and a long text, we should get multiple chunks
        assert!(
            chunks.len() > 1,
            "Expected multiple chunks but got {}",
            chunks.len()
        );
        for chunk in &chunks {
            // Document chunks don't have code_metadata, so is_fragment() returns false
            assert!(!chunk.metadata.is_fragment());
        }
    }

    #[test]
    fn test_chunk_groups_multiple() {
        let config = ChunkingConfig::default();
        let chunker = JsonChunker::new(config).with_smart_merging(false); // Disable smart merging for this test

        let groups = vec![
            create_test_group("key1 = value1", 10),
            create_test_group("key2 = value2", 10),
        ];

        let chunks = chunker.chunk_groups(
            &groups,
            "test.json",
            OutputMode::default(),
            &DocumentClassification::detect("test.json"),
        );
        assert_eq!(chunks.len(), 2);
    }

    #[test]
    fn test_smart_merging_small_groups() {
        use cce_types::Span;

        let config = ChunkingConfig::default();
        let chunker = JsonChunker::new(config)
            .with_smart_merging(true)
            .with_min_chunk_tokens(20);

        // Create two small groups that should be merged
        let mut group1 = JsonGroup::new(
            "test_array_elem_1".to_string(),
            JsonGroupType::ArrayElement,
            "items".to_string(),
        );
        group1.token_count = 10;
        group1.bm25_text = "items[0] = value1".to_string();
        group1.embedding_text = "items[0]: value1".to_string();
        group1.span = Span::from_line(1);

        let mut group2 = JsonGroup::new(
            "test_array_elem_2".to_string(),
            JsonGroupType::ArrayElement,
            "items".to_string(),
        );
        group2.token_count = 10;
        group2.bm25_text = "items[1] = value2".to_string();
        group2.embedding_text = "items[1]: value2".to_string();
        group2.span = Span::from_line(2);

        let groups = vec![group1, group2];
        let chunks = chunker.chunk_groups(
            &groups,
            "test.json",
            OutputMode::default(),
            &DocumentClassification::detect("test.json"),
        );

        // Should merge into one chunk (both are small and siblings)
        assert_eq!(
            chunks.len(),
            1,
            "Expected merged chunk but got {}",
            chunks.len()
        );
    }

    #[test]
    fn test_key_based_association() {
        use crate::json::types::{JsonNode, JsonNodeType, JsonValueType};
        use cce_types::Span;

        let config = ChunkingConfig::default();
        let chunker = JsonChunker::new(config)
            .with_smart_merging(true)
            .with_key_based_association(true);

        // Create database.timeout group
        let mut group_db = JsonGroup::new(
            "test_database".to_string(),
            JsonGroupType::NestedObject,
            "database".to_string(),
        );

        let timeout_node = JsonNode::new(
            "db_timeout".to_string(),
            JsonNodeType::Primitive(JsonValueType::Number),
            "database.timeout".to_string(),
            Span::default(),
        )
        .with_key_name("timeout".to_string())
        .with_value("30".to_string());

        group_db.add_member(timeout_node);
        group_db.token_count = 15;
        group_db.bm25_text = "database.timeout = 30".to_string();
        group_db.embedding_text = "database.timeout: 30".to_string();

        // Create cache.timeout group
        let mut group_cache = JsonGroup::new(
            "test_cache".to_string(),
            JsonGroupType::NestedObject,
            "cache".to_string(),
        );

        let cache_timeout_node = JsonNode::new(
            "cache_timeout".to_string(),
            JsonNodeType::Primitive(JsonValueType::Number),
            "cache.timeout".to_string(),
            Span::default(),
        )
        .with_key_name("timeout".to_string())
        .with_value("10".to_string());

        group_cache.add_member(cache_timeout_node);
        group_cache.token_count = 15;
        group_cache.bm25_text = "cache.timeout = 10".to_string();
        group_cache.embedding_text = "cache.timeout: 10".to_string();

        let groups = vec![group_db, group_cache];
        let chunks = chunker.chunk_groups(
            &groups,
            "test.json",
            OutputMode::default(),
            &DocumentClassification::detect("test.json"),
        );

        // Should merge due to shared "timeout" key
        assert_eq!(
            chunks.len(),
            1,
            "Expected merged chunk for related configs but got {}",
            chunks.len()
        );
    }

    #[test]
    fn test_no_merge_different_depths() {
        use crate::json::types::{JsonNode, JsonNodeType, JsonValueType};
        use cce_types::Span;

        let config = ChunkingConfig::default();
        let chunker = JsonChunker::new(config)
            .with_smart_merging(true)
            .with_key_based_association(true);

        // Create database.timeout group (depth 1)
        let mut group_db = JsonGroup::new(
            "test_database".to_string(),
            JsonGroupType::NestedObject,
            "database".to_string(),
        );

        let timeout_node = JsonNode::new(
            "db_timeout".to_string(),
            JsonNodeType::Primitive(JsonValueType::Number),
            "database.timeout".to_string(),
            Span::default(),
        )
        .with_key_name("timeout".to_string())
        .with_value("30".to_string());

        group_db.add_member(timeout_node);
        group_db.token_count = 30; // Above min_chunk_tokens threshold
        group_db.bm25_text = "database.timeout = 30".to_string();
        group_db.embedding_text = "database.timeout: 30".to_string();

        // Create api.settings.timeout group (depth 2)
        let mut group_api = JsonGroup::new(
            "test_api".to_string(),
            JsonGroupType::NestedObject,
            "api.settings".to_string(),
        );

        let api_timeout_node = JsonNode::new(
            "api_timeout".to_string(),
            JsonNodeType::Primitive(JsonValueType::Number),
            "api.settings.timeout".to_string(),
            Span::default(),
        )
        .with_key_name("timeout".to_string())
        .with_value("5".to_string());

        group_api.add_member(api_timeout_node);
        group_api.token_count = 30; // Above min_chunk_tokens threshold
        group_api.bm25_text = "api.settings.timeout = 5".to_string();
        group_api.embedding_text = "api.settings.timeout: 5".to_string();

        let groups = vec![group_db, group_api];
        let chunks = chunker.chunk_groups(
            &groups,
            "test.json",
            OutputMode::default(),
            &DocumentClassification::detect("test.json"),
        );

        // Should NOT merge due to different depths
        assert_eq!(
            chunks.len(),
            2,
            "Should not merge groups at different depths"
        );
    }
}
