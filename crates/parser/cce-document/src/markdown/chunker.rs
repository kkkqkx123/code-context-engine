//! Markdown chunker
//!
//! Converts DocGroups to ChunkedResults for storage.
//! Uses a two-tier chunking scheme: embedding paths are guaranteed not to be too long (truncated), and BM25 is sub-chunked by word count.

use crate::GenericGroup;
use crate::common::{GenericChunker, MergingConfig, TwoTierParams, two_tier_chunking};
use crate::types::{DocGroup, DocGroupType, DocumentClassification};
use cce_config::modules::ChunkingConfig;
use cce_types::GroupType;
use cce_types::ast_to_nl::options::OutputMode;
use cce_types::language::Language;
use cce_types::{ChunkedResult, GroupRelation, GroupRelationType};
use cce_utils::token_estimation::TokenEstimator;

/// Document chunker with two-tier chunking + smart merging support.
pub struct DocChunker {
    config: ChunkingConfig,
    estimator: TokenEstimator,
    merging_config: MergingConfig,
}

crate::chunker_boilerplate!(DocChunker, with_merging_builders);

impl DocChunker {
    /// Extract a title from a DocGroup's header.
    fn extract_title(group: &DocGroup, file_path: &str) -> Option<String> {
        if let Some(ref header) = group.header {
            let content = header.content.trim();
            if !content.is_empty() {
                return Some(content.chars().take(80).collect());
            }
        }
        std::path::Path::new(file_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .map(String::from)
    }

    /// Chunk a single DocGroup using only its own header as context.
    pub fn chunk_group(
        &self,
        group: &DocGroup,
        file_path: &str,
        output_mode: OutputMode,
        classification: &DocumentClassification,
    ) -> Vec<ChunkedResult> {
        let context_path = group
            .header
            .as_ref()
            .map(|h| h.content.clone())
            .unwrap_or_default();
        self.chunk_group_with_context(group, &context_path, file_path, output_mode, classification)
    }

    /// Chunk a single DocGroup with a pre-computed context path.
    fn chunk_group_with_context(
        &self,
        group: &DocGroup,
        context_path: &str,
        file_path: &str,
        output_mode: OutputMode,
        classification: &DocumentClassification,
    ) -> Vec<ChunkedResult> {
        let gid = &group.group_id;
        let bm25_title = Self::extract_title(group, file_path);
        let embedding_text =
            self.to_embedding_text(&group.embedding_text, &group.group_type, context_path);

        two_tier_chunking(TwoTierParams {
            embedding_text: &embedding_text,
            bm25_text: &group.bm25_text,
            source_span: group.span,
            source_group_id: gid,
            file_path,
            config: &self.config,
            estimator: &self.estimator,
            group_type: group.group_type.to_group_type(),
            bm25_title,
            output_mode,
            content_type: classification.payload().clone(),
            file_category: classification.category(),
        })
    }

    /// Convert text to embedding-friendly format with hierarchical context
    fn to_embedding_text(
        &self,
        text: &str,
        group_type: &DocGroupType,
        context_path: &str,
    ) -> String {
        let prefix = match group_type {
            DocGroupType::Chapter => {
                if context_path.is_empty() {
                    "[Document Chapter]".to_string()
                } else {
                    format!("[Document Chapter | {}]", context_path)
                }
            }
            DocGroupType::Section => {
                if context_path.is_empty() {
                    "[Document Section]".to_string()
                } else {
                    format!("[Document Section | Under: {}]", context_path)
                }
            }
            DocGroupType::ParagraphGroup => "".to_string(),
            DocGroupType::ListGroup => "[List Items]".to_string(),
            DocGroupType::StandaloneBlock => {
                if context_path.is_empty() {
                    "[Standalone Code Block]".to_string()
                } else {
                    format!("[Standalone Code Block | Context: {}]", context_path)
                }
            }
        };

        if prefix.is_empty() {
            text.trim().to_string()
        } else {
            format!("{}\n{}", prefix, text.trim())
        }
    }

    /// Build a full hierarchical context path by walking backwards through groups.
    fn build_context_path(groups: &[DocGroup], index: usize) -> String {
        if index == 0 || groups.is_empty() {
            return String::new();
        }

        let current = &groups[index];
        let mut ancestors: Vec<String> = Vec::new();

        for i in (0..index).rev() {
            let g = &groups[i];
            match g.group_type {
                DocGroupType::Chapter | DocGroupType::Section => {
                    if let Some(ref header) = g.header {
                        let title = header.content.trim().to_string();
                        if !title.is_empty() {
                            ancestors.push(title);
                        }
                    }
                }
                _ => {}
            }

            // Stop at the nearest Chapter (highest ancestor)
            if g.group_type == DocGroupType::Chapter {
                break;
            }
        }

        ancestors.reverse();
        let mut path = ancestors.join(" > ");

        // Append the current group's header if it has one
        if let Some(ref header) = current.header {
            let title = header.content.trim();
            if !title.is_empty() {
                if !path.is_empty() {
                    path.push_str(" > ");
                }
                path.push_str(title);
            }
        }

        path
    }

    /// Chunk multiple groups with full hierarchical context.
    pub fn chunk_groups(
        &self,
        groups: &[DocGroup],
        file_path: &str,
        output_mode: OutputMode,
        classification: &DocumentClassification,
    ) -> Vec<ChunkedResult> {
        if groups.is_empty() {
            return Vec::new();
        }

        if !self.merging_config.enable_smart_merging {
            return groups
                .iter()
                .enumerate()
                .flat_map(|(i, g)| {
                    let ctx = Self::build_context_path(groups, i);
                    self.chunk_group_with_context(g, &ctx, file_path, output_mode, classification)
                })
                .collect();
        }

        self.chunk_groups_smart(groups, file_path, output_mode, classification)
    }

    /// Smart merging strategy: decide which groups should be merged into one chunk
    fn chunk_groups_smart(
        &self,
        groups: &[DocGroup],
        file_path: &str,
        output_mode: OutputMode,
        classification: &DocumentClassification,
    ) -> Vec<ChunkedResult> {
        let mut chunks = Vec::new();
        let mut pending_indices: Vec<usize> = Vec::new();

        for (i, group) in groups.iter().enumerate() {
            let group_tokens = self.estimator.estimate_text(&group.bm25_text);

            if group_tokens < self.merging_config.min_chunk_tokens {
                pending_indices.push(i);
                continue;
            }

            if !pending_indices.is_empty() {
                if let Some(merged) = self.try_merge_with_context(
                    &pending_indices,
                    i,
                    groups,
                    file_path,
                    output_mode,
                    classification,
                ) {
                    chunks.extend(merged);
                    pending_indices.clear();
                } else {
                    for &pi in &pending_indices {
                        let ctx = Self::build_context_path(groups, pi);
                        chunks.extend(self.chunk_group_with_context(
                            &groups[pi],
                            &ctx,
                            file_path,
                            output_mode,
                            classification,
                        ));
                    }
                    pending_indices.clear();
                }
            }

            let ctx = Self::build_context_path(groups, i);
            chunks.extend(self.chunk_group_with_context(
                group,
                &ctx,
                file_path,
                output_mode,
                classification,
            ));
        }

        for &pi in &pending_indices {
            let ctx = Self::build_context_path(groups, pi);
            chunks.extend(self.chunk_group_with_context(
                &groups[pi],
                &ctx,
                file_path,
                output_mode,
                classification,
            ));
        }

        chunks
    }

    /// Try to merge small groups with a context group
    fn try_merge_with_context(
        &self,
        small_indices: &[usize],
        context_index: usize,
        groups: &[DocGroup],
        file_path: &str,
        output_mode: OutputMode,
        classification: &DocumentClassification,
    ) -> Option<Vec<ChunkedResult>> {
        let small_groups: Vec<&DocGroup> = small_indices.iter().map(|&i| &groups[i]).collect();
        let context_group = &groups[context_index];

        let total_tokens: usize = small_groups
            .iter()
            .map(|g| self.estimator.estimate_text(&g.bm25_text))
            .sum::<usize>()
            + self.estimator.estimate_text(&context_group.bm25_text);

        let max_allowed = (self.config.max_tokens as f32
            * self.merging_config.max_merge_expansion_factor) as usize;

        if self.config.max_tokens > 0 && total_tokens > max_allowed {
            return None;
        }

        if !self.are_semantically_compatible(&small_groups, context_group) {
            return None;
        }

        Some(self.create_merged_chunk(
            small_indices,
            context_index,
            groups,
            file_path,
            output_mode,
            classification,
        ))
    }

    /// Check if groups are semantically compatible for merging
    fn are_semantically_compatible(
        &self,
        small_groups: &[&DocGroup],
        context_group: &DocGroup,
    ) -> bool {
        small_groups.iter().all(|g| {
            matches!(
                (g.group_type, context_group.group_type),
                (DocGroupType::ParagraphGroup, DocGroupType::Section)
                    | (DocGroupType::Section, DocGroupType::Chapter)
                    | (DocGroupType::ParagraphGroup, DocGroupType::ParagraphGroup)
                    | (DocGroupType::ListGroup, DocGroupType::Section)
            )
        })
    }

    /// Create a merged chunk from multiple groups (two-level chunking).
    fn create_merged_chunk(
        &self,
        small_indices: &[usize],
        context_index: usize,
        groups: &[DocGroup],
        file_path: &str,
        output_mode: OutputMode,
        classification: &DocumentClassification,
    ) -> Vec<ChunkedResult> {
        let small_groups: Vec<&DocGroup> = small_indices.iter().map(|&i| &groups[i]).collect();
        let context_group = &groups[context_index];

        let mut combined_bm25 = String::new();
        let mut combined_embedding = String::new();

        for g in &small_groups {
            combined_bm25.push_str(&g.bm25_text);
            combined_bm25.push_str("\n\n");
            combined_embedding.push_str(&g.embedding_text);
            combined_embedding.push_str("\n\n");
        }

        combined_bm25.push_str(&context_group.bm25_text);
        combined_embedding.push_str(&context_group.embedding_text);

        let related_groups: Vec<_> = small_groups
            .iter()
            .map(|g| GroupRelation {
                group_id: g.group_id.clone(),
                relation_type: GroupRelationType::SameHierarchy,
                strength: 1.0,
            })
            .collect();

        let gid = &context_group.group_id;
        let bm25_title = Self::extract_title(context_group, file_path);
        let context_path = Self::build_context_path(groups, context_index);
        let embedding_text = self.to_embedding_text(
            &combined_embedding,
            &context_group.group_type,
            &context_path,
        );

        let mut results = two_tier_chunking(TwoTierParams {
            embedding_text: &embedding_text,
            bm25_text: &combined_bm25,
            source_span: context_group.span,
            source_group_id: gid,
            file_path,
            config: &self.config,
            estimator: &self.estimator,
            group_type: context_group.group_type.to_group_type(),
            bm25_title,
            output_mode,
            content_type: classification.payload().clone(),
            file_category: classification.category(),
        });

        for chunk in &mut results {
            chunk.related_groups = related_groups.clone();
        }

        results
    }
}

impl GenericChunker<DocGroup, crate::types::DocNode> for DocChunker {
    fn config(&self) -> &ChunkingConfig {
        &self.config
    }

    fn estimator(&self) -> &TokenEstimator {
        &self.estimator
    }

    fn language(&self) -> Language {
        Language::Unknown
    }

    fn to_group_type(_group: &DocGroup) -> GroupType {
        // DocGroup has its own to_group_type method
        GroupType::Standalone
    }

    fn to_embedding_text(text: &str, _group: &DocGroup) -> String {
        // Markdown uses custom context generation in to_embedding_text method
        text.to_string()
    }

    fn extract_node_ids(group: &DocGroup) -> Vec<String> {
        group.all_node_ids()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::ChunkPath;
    use cce_types::Span;

    fn create_test_group(text: &str, token_count: usize) -> DocGroup {
        let mut group = DocGroup::new("test_group".to_string(), DocGroupType::Section);
        group.bm25_text = text.to_string();
        group.embedding_text = text.to_string();
        group.token_count = token_count;
        group.span = Span::from_lines(0, 10);
        group
    }

    #[test]
    fn test_chunk_group_no_split() {
        let config = ChunkingConfig::default();
        let chunker = DocChunker::new(config);

        let group = create_test_group("Short text content.", 10);
        let chunks = chunker.chunk_group(
            &group,
            "test.md",
            OutputMode::default(),
            &DocumentClassification::detect("test.md"),
        );

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].total_chunks, 1);
        assert!(chunks[0].is_first());
        assert!(chunks[0].is_last());
        // Document chunks don't have code_metadata, so is_fragment() returns false
        assert!(!chunks[0].metadata.is_fragment());
    }

    #[test]
    fn test_chunk_group_bm25_sub_split() {
        // Use Both mode to produce BM25 + Embedding chunks
        // Test that BM25 is split by word count when max_bm25_words is low
        let config = ChunkingConfig {
            max_bm25_words: 5,
            max_tokens: 500,
            ..Default::default()
        };
        let chunker = DocChunker::new(config);

        // ~12 words, should split into ~3 BM25 sub-chunks
        let long_text = "one two three four five six seven eight nine ten eleven twelve";
        let group = create_test_group(long_text, 10);
        let chunks = chunker.chunk_group(
            &group,
            "test.md",
            OutputMode::Both,
            &DocumentClassification::detect("test.md"),
        );

        // Should have: 1 embedding + multiple BM25 sub-chunks
        let emb_count = chunks
            .iter()
            .filter(|c| c.path == ChunkPath::Embedding)
            .count();
        let bm25_count = chunks.iter().filter(|c| c.path == ChunkPath::Bm25).count();

        assert_eq!(emb_count, 1, "Should have exactly one embedding chunk");
        assert!(
            bm25_count >= 2,
            "Expected >=2 BM25 chunks, got {}",
            bm25_count
        );

        // All BM25 chunks share the same source_group_id
        let gid = &chunks[0].source_group_id;
        for c in &chunks {
            assert_eq!(&c.source_group_id, gid);
        }
    }
}
