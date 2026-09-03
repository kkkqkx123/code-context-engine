//! Generic chunker trait for document processing
//!
//! This trait provides a common interface for chunking different group types
//! (JsonGroup, XmlGroup, DocGroup) to reduce code duplication.

use cce_config::modules::ChunkingConfig;
use cce_types::GroupType;
use cce_types::Span;
use cce_types::ast_to_nl::FileCategory;
use cce_types::ast_to_nl::options::OutputMode;
use cce_types::language::Language;
use cce_types::{
    ChunkContentType, ChunkMetadata, ChunkPath, ChunkedResult, CodeSpecificMetadata,
    DocumentSpecificMetadata,
};
use cce_utils::token_estimation::TokenEstimator;

use super::GenericGroup;

/// Macro to generate common chunker constructor and Default boilerplate.
///
/// # Usage
///
/// ```ignore
/// // Simple: new() + with_merging_config() + Default
/// chunker_boilerplate!(TomlChunker);
///
/// // With merging builder methods
/// chunker_boilerplate!(JsonChunker, with_merging_builders);
/// ```
#[macro_export]
macro_rules! chunker_boilerplate {
    ($chunker:ident) => {
        impl $chunker {
            pub fn new(config: ChunkingConfig) -> Self {
                Self {
                    config,
                    estimator: TokenEstimator::new(1.0, 0.25),
                    merging_config: MergingConfig::default(),
                }
            }

            pub fn with_merging_config(mut self, config: MergingConfig) -> Self {
                self.merging_config = config;
                self
            }
        }

        impl Default for $chunker {
            fn default() -> Self {
                Self::new(ChunkingConfig::default())
            }
        }
    };
    ($chunker:ident, with_merging_builders) => {
        impl $chunker {
            pub fn new(config: ChunkingConfig) -> Self {
                Self {
                    config,
                    estimator: TokenEstimator::default(),
                    merging_config: MergingConfig {
                        enable_smart_merging: true,
                        min_chunk_tokens: 20,
                        max_merge_expansion_factor: 1.5,
                        enable_key_based_association: false,
                    },
                }
            }

            pub fn with_merging_config(mut self, config: MergingConfig) -> Self {
                self.merging_config = config;
                self
            }

            pub fn with_smart_merging(mut self, enabled: bool) -> Self {
                self.merging_config.enable_smart_merging = enabled;
                self
            }

            pub fn with_min_chunk_tokens(mut self, min_tokens: usize) -> Self {
                self.merging_config.min_chunk_tokens = min_tokens;
                self
            }

            pub fn with_max_merge_expansion_factor(mut self, factor: f32) -> Self {
                self.merging_config.max_merge_expansion_factor = factor;
                self
            }
        }

        impl Default for $chunker {
            fn default() -> Self {
                Self::new(ChunkingConfig::default())
            }
        }
    };
}

/// Trait for generic chunker operations
///
/// This trait abstracts the common chunking operations needed for document groups,
/// allowing shared code to work with any group type.
pub trait GenericChunker<Group, Node>: Sized
where
    Group: GenericGroup<Node>,
{
    /// Get the chunking configuration
    fn config(&self) -> &ChunkingConfig;

    /// Get the token estimator for embedding path
    fn estimator(&self) -> &TokenEstimator;

    /// Get the language for this chunker
    fn language(&self) -> Language;

    /// Convert group type to GroupType for ChunkedResult
    fn to_group_type(group: &Group) -> GroupType;

    /// Convert text to embedding-friendly format
    ///
    /// Default implementation returns text as-is. Override to add context.
    fn to_embedding_text(text: &str, _group: &Group) -> String {
        text.to_string()
    }

    /// Populate document-specific metadata in ChunkMetadata
    ///
    /// Default implementation sets doc_type and doc_node_ids. Override for custom behavior.
    fn populate_doc_metadata(&self, metadata: &mut ChunkMetadata, group: &Group) {
        // Extract node IDs from group members if available
        let node_ids = Self::extract_node_ids(group);
        if let Some(doc_meta) = metadata.as_document_mut() {
            doc_meta.doc_node_ids = node_ids;
        }
    }

    /// Extract a human-readable BM25 title from a group for entity naming.
    /// Override for document chunkers that have structural information (path, header, etc.).
    fn bm25_title_for_group(_group: &Group, _file_path: &str) -> Option<String> {
        None
    }

    /// Extract node IDs from a group
    ///
    /// Default implementation returns empty vec. Override to extract actual node IDs.
    fn extract_node_ids(_group: &Group) -> Vec<String> {
        Vec::new()
    }

    // === Default implementations for common chunking operations ===

    /// Chunk a single group into ChunkedResults (two-level chunking scheme).
    ///
    /// **Embedding path**: Each group produces an embedding chunk.
    ///   If the text exceeds `max_tokens`, truncate to the token limit.
    ///
    /// **BM25 path**: BM25 text is divided into sub-blocks by `max_bm25_words`.
    ///   All sub-blocks share the same `source_group_id` for alignment.
    ///   This replaces the original token-driven paragraph/sentence segmentation - pure token hard segmentation
    ///   should not affect the alignment of the chunk.
    fn chunk_group(
        &self,
        group: &Group,
        file_path: &str,
        output_mode: OutputMode,
        classification: &crate::types::DocumentClassification,
    ) -> Vec<ChunkedResult> {
        let group_id = group.group_id().to_string();
        let bm25_title = Self::bm25_title_for_group(group, file_path);
        let embedding_text = Self::to_embedding_text(group.embedding_text(), group);

        let mut results = two_tier_chunking(TwoTierParams {
            embedding_text: &embedding_text,
            bm25_text: group.bm25_text(),
            source_span: *group.span(),
            source_group_id: &group_id,
            file_path,
            config: self.config(),
            estimator: self.estimator(),
            group_type: Self::to_group_type(group),
            bm25_title,
            output_mode,
            content_type: classification.payload().clone(),
            file_category: classification.category(),
        });

        // Allow subclasses to add format-specific metadata
        for chunk in &mut results {
            self.populate_doc_metadata(&mut chunk.metadata, group);
        }

        results
    }

    /// Chunk multiple groups (iterates calling chunk_group)
    fn chunk_groups(
        &self,
        groups: &[Group],
        file_path: &str,
        output_mode: OutputMode,
        classification: &crate::types::DocumentClassification,
    ) -> Vec<ChunkedResult> {
        groups
            .iter()
            .flat_map(|g| self.chunk_group(g, file_path, output_mode, classification))
            .collect()
    }
}

/// Parameters for two-tier chunking.
#[derive(Clone)]
pub struct TwoTierParams<'a> {
    pub embedding_text: &'a str,
    pub bm25_text: &'a str,
    pub source_span: Span,
    pub source_group_id: &'a str,
    pub file_path: &'a str,
    pub config: &'a ChunkingConfig,
    pub estimator: &'a TokenEstimator,
    pub group_type: GroupType,
    pub bm25_title: Option<String>,
    pub output_mode: OutputMode,
    /// Content type for chunk metadata. Documents use `ChunkContentType::Document`;
    /// config-format chunkers (JSON/XML) pass `ChunkContentType::Config`.
    pub content_type: ChunkContentType,
    /// Business-layer category stored next to `content_type`. Derived once at
    /// the pipeline entry; assigned together with `content_type` so the two
    /// labels can never disagree.
    pub file_category: FileCategory,
}

/// Create a two-tier chunking result from pre-processed text.
///
/// **Embedding path**: 1 chunk when within `config.max_tokens`, otherwise N
/// sub-chunks split at token-estimated boundaries (shared `source_group_id`).
/// **BM25 path**: N sub-chunks, split by `config.max_bm25_words` with inline
/// word overlap.
///
/// The caller is responsible for any pre-processing (context path prefixes, etc.)
/// applied to `embedding_text` and `bm25_text` before calling this function.
pub fn two_tier_chunking(p: TwoTierParams<'_>) -> Vec<ChunkedResult> {
    let TwoTierParams {
        embedding_text,
        bm25_text,
        source_span,
        source_group_id,
        file_path,
        config,
        estimator,
        group_type,
        bm25_title,
        output_mode,
        content_type,
        file_category,
    } = p;
    let mut results = Vec::new();

    // === Embedding path: single chunk, or split into sub-chunks when oversized ===
    if output_mode.produces_embedding() {
        let segments = if embedding_text.is_empty() {
            vec![String::new()]
        } else {
            crate::common::token_split::split_by_tokens(
                embedding_text,
                ChunkPath::Embedding,
                config,
                estimator,
            )
            .into_iter()
            .map(|b| embedding_text[b.start_byte..b.end_byte].to_string())
            .collect()
        };
        let total = segments.len();

        for (i, text) in segments.iter().enumerate() {
            let token_count = estimator.estimate_text(text);
            let mut meta = metadata_for(&content_type, file_category, file_path, source_span);
            meta.bm25_word_count = None;
            meta.segment_id = source_group_id.to_string();

            results.push(ChunkedResult {
                chunk_id: format!("{}_emb_{}", source_group_id, i),
                source_group_id: source_group_id.to_string(),
                path: ChunkPath::Embedding,
                group_type,
                chunk_index: i,
                total_chunks: total,
                text: text.clone(),
                token_count,
                start_byte: source_span.start_byte,
                end_byte: source_span.end_byte,
                prev_overlap: None,
                next_overlap: None,
                related_groups: Vec::new(),
                self_contained: false,
                bm25_title: bm25_title.clone(),
                bm25_keywords: Vec::new(),
                metadata: meta,
            });
        }
    }

    // === BM25 path: possibly multiple sub-chunks (all share group_id) ===
    if output_mode.produces_bm25() {
        let max_words = config.max_bm25_words;
        let overlap = config.overlap_bm25_words;

        let sub_texts = split_bm25_by_words(bm25_text, max_words, overlap);
        let total = sub_texts.len();

        for (i, sub_text) in sub_texts.iter().enumerate() {
            let word_count = sub_text
                .split_whitespace()
                .filter(|w| !w.is_empty())
                .count();

            let mut meta = metadata_for(&content_type, file_category, file_path, source_span);
            meta.bm25_word_count = Some(word_count);
            meta.segment_id = source_group_id.to_string();

            results.push(ChunkedResult {
                chunk_id: format!("{}_bm25_{}", source_group_id, i),
                source_group_id: source_group_id.to_string(),
                path: ChunkPath::Bm25,
                group_type,
                chunk_index: i,
                total_chunks: total,
                text: sub_text.clone(),
                token_count: word_count,
                start_byte: source_span.start_byte,
                end_byte: source_span.end_byte,
                prev_overlap: None,
                next_overlap: None,
                related_groups: Vec::new(),
                self_contained: false,
                bm25_title: bm25_title.clone(),
                bm25_keywords: Vec::new(),
                metadata: meta,
            });
        }
    }

    results
}

/// Build base metadata from the explicitly paired payload type + category
/// (both derived once at the pipeline entry; see
/// [`ChunkMetadata::with_classification`]).
fn metadata_for(
    content_type: &ChunkContentType,
    file_category: FileCategory,
    file_path: &str,
    source_span: Span,
) -> ChunkMetadata {
    match content_type {
        ChunkContentType::Code { language } => ChunkMetadata::for_code(
            file_path.to_string(),
            source_span,
            *language,
            CodeSpecificMetadata::default(),
        ),
        other => ChunkMetadata::with_classification(
            other.clone(),
            file_category,
            file_path.to_string(),
            source_span,
            Some(DocumentSpecificMetadata::default()),
        ),
    }
}

/// Compute the enclosing span covering all given spans.
///
/// Used by merged-group chunkers (JSON/XML) so a combined chunk spans every
/// group it represents instead of only the first group's range.
pub(crate) fn merged_span(spans: &[Span]) -> Span {
    let mut min_start_row = usize::MAX;
    let mut max_end_row = 0;
    for span in spans {
        if span.start_position.row < min_start_row {
            min_start_row = span.start_position.row;
        }
        if span.end_position.row > max_end_row {
            max_end_row = span.end_position.row;
        }
    }
    if min_start_row == usize::MAX {
        return Span::default();
    }
    Span::from_lines(min_start_row, max_end_row)
}

/// Split text by word count into sub-chunks (BM25 sub-chunking).
///
/// If `max_words` is 0 or text fits within the limit, returns the full text as one segment.
/// Otherwise splits at word boundaries with optional overlap.
pub fn split_bm25_by_words(text: &str, max_words: usize, overlap_words: usize) -> Vec<String> {
    if max_words == 0 {
        return vec![text.to_string()];
    }

    let words: Vec<&str> = text.split_whitespace().collect();
    let total = words.len();
    if total <= max_words {
        return vec![text.to_string()];
    }

    let mut segments = Vec::new();
    let mut start = 0usize;

    while start < total {
        let end = (start + max_words).min(total);
        segments.push(words[start..end].join(" "));

        if end >= total {
            break;
        }
        let advance = max_words.saturating_sub(overlap_words).max(1);
        start += advance;
    }

    segments
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_bm25_by_words_no_split() {
        let text = "short text";
        let result = split_bm25_by_words(text, 10, 2);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "short text");
    }

    #[test]
    fn test_split_bm25_by_words_with_split() {
        let text = "word word word word word word word word word word word word";
        let result = split_bm25_by_words(text, 5, 1);
        assert!(result.len() >= 2);
        // First segment: 5 words
        assert_eq!(result[0].split_whitespace().count(), 5);
        // All segments share same word count
        for seg in &result {
            assert!(seg.split_whitespace().count() <= 5);
        }
    }

    #[test]
    fn test_two_tier_chunking_segment_id_shared_across_paths() {
        let config = ChunkingConfig::default();
        let estimator = TokenEstimator::default();
        let span = Span::default();
        let source_group_id = "doc_group_1";

        let results = two_tier_chunking(TwoTierParams {
            embedding_text: "embedding text content",
            bm25_text: "bm25 text content for search",
            source_span: span,
            source_group_id,
            file_path: "docs/test.md",
            config: &config,
            estimator: &estimator,
            group_type: GroupType::Standalone,
            bm25_title: Some("Test Section".to_string()),
            output_mode: OutputMode::Both,
            content_type: ChunkContentType::Document,
            file_category: FileCategory::Documentation,
        });

        assert!(!results.is_empty());

        for chunk in &results {
            assert_eq!(
                chunk.metadata.segment_id, source_group_id,
                "segment_id should match source_group_id for chunk {:?}",
                chunk.path
            );
        }

        let emb_chunks: Vec<_> = results
            .iter()
            .filter(|c| c.path == ChunkPath::Embedding)
            .collect();
        let bm25_chunks: Vec<_> = results
            .iter()
            .filter(|c| c.path == ChunkPath::Bm25)
            .collect();

        assert!(!emb_chunks.is_empty(), "should have embedding chunks");
        assert!(!bm25_chunks.is_empty(), "should have BM25 chunks");

        for chunk in &emb_chunks {
            assert_eq!(chunk.metadata.segment_id, source_group_id);
        }
        for chunk in &bm25_chunks {
            assert_eq!(chunk.metadata.segment_id, source_group_id);
        }
    }

    #[test]
    fn test_two_tier_chunking_bm25_sub_chunks_share_segment_id() {
        let config = ChunkingConfig {
            max_bm25_words: 5,
            overlap_bm25_words: 1,
            ..Default::default()
        };
        let estimator = TokenEstimator::default();
        let span = Span::default();
        let source_group_id = "doc_group_2";

        let bm25_text = "one two three four five six seven eight nine ten";
        let results = two_tier_chunking(TwoTierParams {
            embedding_text: "embedding text",
            bm25_text,
            source_span: span,
            source_group_id,
            file_path: "docs/test.md",
            config: &config,
            estimator: &estimator,
            group_type: GroupType::Standalone,
            bm25_title: None,
            output_mode: OutputMode::Both,
            content_type: ChunkContentType::Document,
            file_category: FileCategory::Documentation,
        });

        let bm25_chunks: Vec<_> = results
            .iter()
            .filter(|c| c.path == ChunkPath::Bm25)
            .collect();

        assert!(
            bm25_chunks.len() > 1,
            "BM25 text should be split into multiple sub-chunks"
        );

        for chunk in &bm25_chunks {
            assert_eq!(
                chunk.metadata.segment_id, source_group_id,
                "all BM25 sub-chunks should share the same segment_id"
            );
        }
    }

    #[test]
    fn test_two_tier_chunking_embedding_only_mode() {
        let config = ChunkingConfig::default();
        let estimator = TokenEstimator::default();
        let span = Span::default();
        let source_group_id = "doc_group_3";

        let results = two_tier_chunking(TwoTierParams {
            embedding_text: "embedding only text",
            bm25_text: "bm25 text",
            source_span: span,
            source_group_id,
            file_path: "docs/test.md",
            config: &config,
            estimator: &estimator,
            group_type: GroupType::Standalone,
            bm25_title: None,
            output_mode: OutputMode::Embedding,
            content_type: ChunkContentType::Document,
            file_category: FileCategory::Documentation,
        });

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].path, ChunkPath::Embedding);
        assert_eq!(results[0].metadata.segment_id, source_group_id);
    }

    #[test]
    fn test_two_tier_chunking_embedding_split_when_oversized() {
        let config = ChunkingConfig {
            max_tokens: 20,
            ..Default::default()
        };
        let estimator = TokenEstimator::default();
        let span = Span::default();
        let source_group_id = "doc_group_4";

        let long_text = (0..40)
            .map(|i| format!("sentence number {} with some words here.", i))
            .collect::<Vec<_>>()
            .join(" ");
        let results = two_tier_chunking(TwoTierParams {
            embedding_text: &long_text,
            bm25_text: "bm25 text",
            source_span: span,
            source_group_id,
            file_path: "docs/test.md",
            config: &config,
            estimator: &estimator,
            group_type: GroupType::Standalone,
            bm25_title: None,
            output_mode: OutputMode::Embedding,
            content_type: ChunkContentType::Document,
            file_category: FileCategory::Documentation,
        });

        assert!(
            results.len() > 1,
            "oversized embedding text should split into multiple chunks"
        );
        let full: String = results
            .iter()
            .map(|c| c.text.as_str())
            .collect::<Vec<_>>()
            .join("");
        assert_eq!(
            full, long_text,
            "split sub-chunks must preserve the full text (no truncation)"
        );
        for chunk in &results {
            assert_eq!(chunk.path, ChunkPath::Embedding);
            assert_eq!(chunk.metadata.segment_id, source_group_id);
        }
        for (i, chunk) in results.iter().enumerate() {
            assert_eq!(chunk.chunk_index, i);
            assert_eq!(chunk.total_chunks, results.len());
        }
    }

    #[test]
    fn test_two_tier_chunking_bm25_metrics_use_word_counts() {
        let config = ChunkingConfig {
            max_bm25_words: 5,
            ..Default::default()
        };
        let estimator = TokenEstimator::default();
        let span = Span::default();
        let source_group_id = "doc_group_5";

        let bm25_text = "one two three four five six seven eight nine ten";
        let results = two_tier_chunking(TwoTierParams {
            embedding_text: "embedding text",
            bm25_text,
            source_span: span,
            source_group_id,
            file_path: "docs/test.md",
            config: &config,
            estimator: &estimator,
            group_type: GroupType::Standalone,
            bm25_title: None,
            output_mode: OutputMode::Bm25,
            content_type: ChunkContentType::Document,
            file_category: FileCategory::Documentation,
        });

        assert!(results.len() > 1, "BM25 text should split into sub-chunks");
        for chunk in &results {
            assert_eq!(chunk.path, ChunkPath::Bm25);
            let words = chunk
                .text
                .split_whitespace()
                .filter(|w| !w.is_empty())
                .count();
            assert_eq!(
                chunk.metadata.bm25_word_count,
                Some(words),
                "bm25_word_count must equal the actual word count"
            );
            assert_eq!(
                chunk.token_count, words,
                "token_count must equal the actual word count for BM25 chunks"
            );
        }
    }

    #[test]
    fn test_two_tier_chunking_content_type_config() {
        let config = ChunkingConfig::default();
        let estimator = TokenEstimator::default();
        let span = Span::default();
        let source_group_id = "doc_group_6";

        let results = two_tier_chunking(TwoTierParams {
            embedding_text: "embedding text",
            bm25_text: "bm25 text",
            source_span: span,
            source_group_id,
            file_path: "test.json",
            config: &config,
            estimator: &estimator,
            group_type: GroupType::Standalone,
            bm25_title: None,
            output_mode: OutputMode::Both,
            content_type: ChunkContentType::Config {
                format: "json".to_string(),
            },
            file_category: FileCategory::Config,
        });

        assert!(!results.is_empty());
        for chunk in &results {
            assert!(
                chunk.metadata.content_type.is_config(),
                "config chunkers must produce Config content type"
            );
        }
    }
}
