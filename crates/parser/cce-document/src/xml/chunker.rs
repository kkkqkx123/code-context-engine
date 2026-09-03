//! XML chunker
//!
//! Converts XmlGroups to ChunkedResults for storage.
//! Leverages GenericChunker trait with smart merging support.

use crate::common::{GenericChunker, MergingConfig};
use crate::types::DocumentClassification;
use crate::xml::types::{XmlGroup, XmlGroupType, XmlNode};
use cce_config::modules::ChunkingConfig;
use cce_types::GroupType;
use cce_types::ast_to_nl::options::OutputMode;
use cce_types::language::Language;
use cce_utils::token_estimation::TokenEstimator;

/// XML chunker with smart merging support
pub struct XmlChunker {
    config: ChunkingConfig,
    estimator: TokenEstimator,
    merging_config: MergingConfig,
}

crate::chunker_boilerplate!(XmlChunker, with_merging_builders);

impl GenericChunker<XmlGroup, XmlNode> for XmlChunker {
    fn config(&self) -> &ChunkingConfig {
        &self.config
    }

    fn estimator(&self) -> &TokenEstimator {
        &self.estimator
    }

    fn language(&self) -> Language {
        Language::Xml
    }

    fn to_group_type(group: &XmlGroup) -> GroupType {
        group.group_type.to_doc_group_type().to_group_type()
    }

    fn bm25_title_for_group(group: &XmlGroup, file_path: &str) -> Option<String> {
        if !group.path_prefix.is_empty() {
            return Some(group.path_prefix.clone());
        }
        if let Some(ref header) = group.header {
            if let Some(ref tag) = header.tag {
                return Some(tag.clone());
            }
        }
        std::path::Path::new(file_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .map(String::from)
    }

    fn to_embedding_text(text: &str, group: &XmlGroup) -> String {
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
            XmlGroupType::RootElement => " (Root Config)",
            XmlGroupType::NestedElement => "",
            XmlGroupType::ContainerElement => " (Container)",
            XmlGroupType::LeafElement => " (Element)",
            XmlGroupType::TextGroup => " (Text)",
        };

        format!("{}{}\n{}", context, type_hint, text.trim())
    }

    /// Override chunk_groups to use smart merging strategy
    fn chunk_groups(
        &self,
        groups: &[XmlGroup],
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

impl XmlChunker {
    /// Smart merging strategy: decide which groups should be merged into one chunk
    fn chunk_groups_smart(
        &self,
        groups: &[XmlGroup],
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
                // Rule B: Same parent element merging
                if self.should_merge_small_groups(group, next_group)
                    || self.should_merge_same_parent(group, next_group)
                {
                    merged_groups.push(next_group);
                    used_groups.insert(next_group.group_id.clone());
                }
            }

            // Create chunk from merged groups
            if merged_groups.len() == 1 {
                // Single group, use standard chunking
                chunks.extend(self.chunk_group(group, file_path, output_mode, classification));
            } else {
                // Multiple groups merged, create combined chunk
                let chunks_new = self.create_merged_chunk(
                    &merged_groups,
                    file_path,
                    output_mode,
                    classification,
                );
                chunks.extend(chunks_new);
            }
        }

        chunks
    }

    /// Determine if two groups should be merged based on size
    fn should_merge_small_groups(&self, current: &XmlGroup, next: &XmlGroup) -> bool {
        let combined_tokens = current.token_count + next.token_count;
        let max_allowed = (self.config.max_tokens as f32
            * self.merging_config.max_merge_expansion_factor) as usize;

        // Both groups are small and combined size is within limits
        current.token_count < self.merging_config.min_chunk_tokens
            && next.token_count < self.merging_config.min_chunk_tokens
            && combined_tokens <= max_allowed
    }

    /// Determine if two groups share the same parent element
    fn should_merge_same_parent(&self, current: &XmlGroup, next: &XmlGroup) -> bool {
        if current.path_prefix.is_empty() || next.path_prefix.is_empty() {
            return false;
        }

        // Extract parent path (everything before the last dot)
        let get_parent_path = |path: &str| -> String {
            if let Some(pos) = path.rfind('.') {
                path[..pos].to_string()
            } else {
                String::new()
            }
        };

        let current_parent = get_parent_path(&current.path_prefix);
        let next_parent = get_parent_path(&next.path_prefix);

        !current_parent.is_empty() && current_parent == next_parent
    }

    /// Create chunks from multiple merged groups using two-tier scheme.
    fn create_merged_chunk(
        &self,
        groups: &[&XmlGroup],
        file_path: &str,
        output_mode: OutputMode,
        classification: &DocumentClassification,
    ) -> Vec<cce_types::ChunkedResult> {
        use crate::common::chunker::{TwoTierParams, merged_span, two_tier_chunking};

        let mut combined_embedding = String::new();
        let mut combined_bm25 = String::new();

        for (idx, group) in groups.iter().enumerate() {
            if idx > 0 {
                combined_embedding.push_str("\n\n");
                combined_bm25.push_str("\n\n");
            }

            combined_embedding.push_str(&group.embedding_text);
            combined_bm25.push_str(&group.bm25_text);
        }

        let span = merged_span(&groups.iter().map(|g| g.span).collect::<Vec<_>>());
        let embedding_text = Self::to_embedding_text(&combined_embedding, groups[0]);

        two_tier_chunking(TwoTierParams {
            embedding_text: &embedding_text,
            bm25_text: &combined_bm25,
            source_span: span,
            source_group_id: &groups[0].group_id,
            file_path,
            config: &self.config,
            estimator: &self.estimator,
            group_type: Self::to_group_type(groups[0]),
            bm25_title: Self::bm25_title_for_group(groups[0], file_path),
            output_mode,
            content_type: classification.payload().clone(),
            file_category: classification.category(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::Span;

    fn create_test_group(text: &str, token_count: usize) -> XmlGroup {
        let mut group = XmlGroup::new(
            "test_group".to_string(),
            XmlGroupType::NestedElement,
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
        let chunker = XmlChunker::new(config);

        let group = create_test_group("<element>value</element>", 10);
        let chunks = chunker.chunk_group(
            &group,
            "test.xml",
            OutputMode::default(),
            &DocumentClassification::detect("test.xml"),
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
        let chunker = XmlChunker::new(config);

        // Create a long text with multiple lines
        let long_text = "<e1>v1</e1>\n<e2>v2</e2>\n<e3>v3</e3>\n<e4>v4</e4>\n<e5>v5</e5>";
        let group = create_test_group(long_text, 100);
        let chunks = chunker.chunk_group(
            &group,
            "test.xml",
            OutputMode::default(),
            &DocumentClassification::detect("test.xml"),
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
        let chunker = XmlChunker::new(config).with_smart_merging(false);

        let groups = vec![
            create_test_group("<e1>v1</e1>", 10),
            create_test_group("<e2>v2</e2>", 10),
        ];

        let chunks = chunker.chunk_groups(
            &groups,
            "test.xml",
            OutputMode::default(),
            &DocumentClassification::detect("test.xml"),
        );
        assert_eq!(chunks.len(), 2);
    }
}
