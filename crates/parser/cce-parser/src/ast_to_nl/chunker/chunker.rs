//! Group-level chunker
//!
//! Main chunking implementation for entity groups.
//!
//! # Input Contract
//!
//! This module expects `ConversionResult` objects from the converter with:
//! - Valid UTF-8 text in both `bm25_text` and `embedding_text` fields
//! - Aligned dual-path content (same semantic meaning, different representations)
//! - Accurate word counts when provided
//!
//! # Output Guarantees
//!
//! The chunker guarantees:
//! - All chunks respect configured token and word limits
//! - UTF-8 safety in all text operations
//! - Proper fragment metadata for multi-chunk entities
//! - Context preservation through overlaps and header repetition
//!
//! # Policy vs. Mechanism
//!
//! - **Policy** (when to split): Decided by `GroupChunker::chunk_group`
//! - **Mechanism** (how to split): Delegated to `TextSplitter`
//!
//! This separation allows flexible policy decisions while keeping splitting logic reusable.

use crate::grouper::EntityGroup;
use cce_plugin::CodePlugin;
use cce_types::ConversionResult;
use cce_types::Span;
use cce_types::entity::EntityId;
use cce_utils::token_estimation::TokenEstimator;
use std::sync::Arc;

/// (Before-builtin, below-builtin fallback) override-tier chunks.
type OverridePlugins = (Vec<Arc<dyn CodePlugin>>, Vec<Arc<dyn CodePlugin>>);

use super::boundary::{NlEntityBoundary, SplitReason};
use super::chunk_builder::{
    ChunkBuilder, SegmentHeaderContext, SingleChunkContext, UnsplitContext,
};
use super::config::ChunkingConfig;
use super::overlap::OverlapManager;
use super::result::{ChunkPath, ChunkedResult};
use super::splitter::TextSplitter;
use super::strategy::SplitStrategy;
use super::tracker::GroupTracker;

/// Group-level chunker
pub struct GroupChunker {
    config: ChunkingConfig,
    estimator: TokenEstimator,
    splitter: TextSplitter,
    overlap_manager: OverlapManager,
    tracker: GroupTracker,
    /// Plugin registry for the `Chunk` override capability.
    plugin_registry: Option<std::sync::Arc<crate::plugin::PluginRegistry>>,
}

impl GroupChunker {
    /// Create new GroupChunker
    pub fn new(config: ChunkingConfig) -> Self {
        Self {
            config: config.clone(),
            estimator: TokenEstimator::default(),
            splitter: TextSplitter::new(config.clone()),
            overlap_manager: OverlapManager::new(config.clone()),
            tracker: GroupTracker::new(),
            plugin_registry: None,
        }
    }

    /// Attach a plugin registry so `Chunk`-capability plugins can override
    /// the built-in chunking.
    pub fn with_plugin_registry(
        mut self,
        plugin_registry: std::sync::Arc<crate::plugin::PluginRegistry>,
    ) -> Self {
        self.plugin_registry = Some(plugin_registry);
        self
    }

    /// Process entity group with independent dual-path splitting
    ///
    /// Produces two independent sets of chunks:
    /// - **BM25 path**: splits `bm25_text` by `max_bm25_words` using entity-aware strategies
    /// - **Embedding path**: splits `embedding_text` by `max_tokens` using token-aware splitting
    ///
    /// Each path uses its own text and limits, producing separate chunks with
    /// path-specific `chunk_id` format: `{groupId}_{path}_{index}`.
    pub fn chunk_group(
        &mut self,
        group: &EntityGroup,
        conversion: &ConversionResult,
        file_path: &str,
    ) -> Vec<ChunkedResult> {
        self.tracker.record_group(group);

        let bm25_text = conversion.bm25_text.clone().unwrap_or_default();
        let embedding_text = conversion.embedding_text.clone().unwrap_or_default();

        let mut all_chunks = Vec::new();
        let strategy = SplitStrategy::for_group_type(group.group_type);

        let infra = ChunkInfrastructure {
            config: &self.config,
            estimator: &self.estimator,
            splitter: &self.splitter,
        };

        if !bm25_text.is_empty() {
            let input = ChunkInput {
                infra: &infra,
                group,
                text: &bm25_text,
                file_path,
                keywords: &conversion.keywords,
                path: ChunkPath::Bm25,
                strategy,
                nl_boundaries: None,
                header_mode: None,
            };
            let bm25_chunks = chunk_single_path(input);
            all_chunks.extend(bm25_chunks);
        }

        let bm25_count = all_chunks.len();

        let infra = ChunkInfrastructure {
            config: &self.config,
            estimator: &self.estimator,
            splitter: &self.splitter,
        };

        if !embedding_text.is_empty() {
            let input = ChunkInput {
                infra: &infra,
                group,
                text: &embedding_text,
                file_path,
                keywords: &conversion.keywords,
                path: ChunkPath::Embedding,
                strategy,
                nl_boundaries: None,
                header_mode: None,
            };
            let embedding_chunks = chunk_single_path(input);
            all_chunks.extend(embedding_chunks);
        }

        let total = all_chunks.len();
        if bm25_count > 0 {
            self.overlap_manager
                .apply_overlap(&mut all_chunks[..bm25_count], ChunkPath::Bm25);
        }
        if bm25_count < total {
            self.overlap_manager
                .apply_overlap(&mut all_chunks[bm25_count..], ChunkPath::Embedding);
        }

        add_relations(&self.tracker, &mut all_chunks);

        all_chunks
    }

    /// Process entity group with multiple conversions
    ///
    /// This method handles the case where a group produces multiple conversion results
    /// (e.g., class description + method descriptions). It implements smart chunking:
    /// - Header (group overview) is included only in the first chunk for context
    /// - Members are grouped by token budget to avoid oversized chunks
    pub fn chunk_group_with_conversions(
        &mut self,
        group: &EntityGroup,
        group_conversions: &crate::ast_to_nl::converter::GroupConversions,
        file_path: &str,
    ) -> Vec<ChunkedResult> {
        let infra = ChunkInfrastructure {
            config: &self.config,
            estimator: &self.estimator,
            splitter: &self.splitter,
        };
        crate::ast_to_nl::chunker::header_chunk::chunk_group_with_conversions(
            &infra,
            &mut self.tracker,
            group,
            group_conversions,
            file_path,
        )
    }

    /// Reset chunker state (for new file)
    pub fn reset(&mut self) {
        self.tracker.reset();
    }

    /// Batch process groups
    ///
    /// Each group may have multiple conversion results (e.g., class + methods).
    /// This method properly handles the one-to-many relationship.
    ///
    /// After chunking all groups, performs a cross-group merge pass to combine
    /// small chunks with adjacent ones, producing more uniformly sized output.
    pub fn chunk_groups(
        &mut self,
        group_conversions: &[crate::ast_to_nl::converter::GroupConversions],
        file_path: &str,
    ) -> Vec<ChunkedResult> {
        self.tracker.reset();

        // Plugin `Chunk` override: the first matching plugin replaces the
        // built-in chunker entirely. The returned chunks must follow the
        // standard `ChunkedResult` shape.
        //
        // Three-tier order: override-tier plugins (priority >= 0) → built-in
        // chunker → below-builtin fallback plugins (negative priority, only
        // when the built-in produced no chunks).
        let language = group_conversions
            .first()
            .map(|gc| gc.group.language.to_string());
        let override_plugins: Option<OverridePlugins> =
            self.plugin_registry.as_ref().map(|registry| {
                let (above, below) = registry.get_override_plugins(
                    cce_plugin::PluginCapability::Chunk,
                    Some(file_path),
                    language.as_deref(),
                );
                (
                    above.into_iter().cloned().collect(),
                    below.into_iter().cloned().collect(),
                )
            });

        if let Some((above, below)) = override_plugins {
            if let Some(chunks) = self.try_plugin_chunk(group_conversions, file_path, &above) {
                return chunks;
            }

            let builtin = self.chunk_groups_builtin(group_conversions, file_path);

            if builtin.is_empty() {
                if let Some(chunks) = self.try_plugin_chunk(group_conversions, file_path, &below) {
                    return chunks;
                }
            }
            return builtin;
        }

        self.chunk_groups_builtin(group_conversions, file_path)
    }

    /// Chunk groups with the built-in chunker (merge + overlap passes).
    fn chunk_groups_builtin(
        &mut self,
        group_conversions: &[crate::ast_to_nl::converter::GroupConversions],
        file_path: &str,
    ) -> Vec<ChunkedResult> {
        let mut all_chunks = Vec::new();
        let groups: Vec<EntityGroup> = group_conversions
            .iter()
            .map(|gc| gc.group.clone())
            .collect();

        for gc in group_conversions {
            let chunks = self.chunk_group_with_conversions(&gc.group, gc, file_path);
            all_chunks.extend(chunks);
        }

        let group_spans: std::collections::HashMap<String, Span> = groups
            .iter()
            .map(|g| (g.group_id.to_string(), g.span))
            .collect();

        let mut emb_chunks: Vec<ChunkedResult> = all_chunks
            .iter()
            .filter(|c| c.path == ChunkPath::Embedding)
            .cloned()
            .collect();
        let mut bm25_chunks: Vec<ChunkedResult> = all_chunks
            .iter()
            .filter(|c| c.path == ChunkPath::Bm25)
            .cloned()
            .collect();

        emb_chunks =
            super::merge::merge_small_chunks_cross_group(emb_chunks, &group_spans, &self.config);
        bm25_chunks =
            super::merge::merge_small_chunks_cross_group(bm25_chunks, &group_spans, &self.config);

        self.overlap_manager
            .apply_overlap(&mut emb_chunks, ChunkPath::Embedding);
        self.overlap_manager
            .apply_overlap(&mut bm25_chunks, ChunkPath::Bm25);

        let total_chunks = emb_chunks.len() + bm25_chunks.len();

        let mut result = Vec::with_capacity(total_chunks);
        result.extend(emb_chunks);
        result.extend(bm25_chunks);
        result
    }

    /// Try to satisfy `chunk_groups` via a `Chunk`-capability plugin.
    ///
    /// Returns `Some` only when a plugin matched the file (by pattern) and
    /// returned a non-empty chunk list.
    fn try_plugin_chunk(
        &self,
        group_conversions: &[crate::ast_to_nl::converter::GroupConversions],
        file_path: &str,
        plugins: &[Arc<dyn CodePlugin>],
    ) -> Option<Vec<ChunkedResult>> {
        use tracing::warn;

        if plugins.is_empty() {
            return None;
        }

        let conversions: Vec<crate::ast_to_nl::converter::GroupConversions> =
            group_conversions.to_vec();
        for plugin in plugins {
            match plugin.chunk(conversions.clone(), file_path) {
                Ok(Some(chunks)) if !chunks.is_empty() => return Some(chunks),
                Ok(_) => {}
                Err(e) => {
                    warn!(
                        plugin = %plugin.metadata().id,
                        file_path = %file_path,
                        error = %e,
                        "Chunk plugin failed; falling back to built-in chunker"
                    );
                }
            }
        }
        None
    }

    /// Get tracker reference
    pub fn tracker(&self) -> &GroupTracker {
        &self.tracker
    }

    /// Get config
    pub fn config(&self) -> &ChunkingConfig {
        &self.config
    }
}

impl Default for GroupChunker {
    fn default() -> Self {
        Self::new(ChunkingConfig::default())
    }
}

/// Shared infrastructure for chunking operations.
///
/// Groups the three core services that are always passed together
/// through the chunking pipeline.
pub struct ChunkInfrastructure<'a> {
    pub config: &'a ChunkingConfig,
    pub estimator: &'a TokenEstimator,
    pub splitter: &'a TextSplitter,
}

/// Input parameters for chunking a single text path.
pub struct ChunkInput<'a> {
    pub infra: &'a ChunkInfrastructure<'a>,
    pub group: &'a EntityGroup,
    pub text: &'a str,
    pub file_path: &'a str,
    pub keywords: &'a [String],
    pub path: ChunkPath,
    pub strategy: SplitStrategy,
    pub nl_boundaries: Option<&'a [NlEntityBoundary]>,
    /// Header-specific parameters; `None` for the plain single-path flow.
    pub header_mode: Option<HeaderPathParams<'a>>,
}

/// Header-path parameters for `chunk_single_path`.
///
/// When present, `text` is a header + member combination and the chunk is
/// built with header semantics: the header entity is context (excluded from
/// content attribution), it joins the source coverage of the first chunk
/// only, and the unsplit fallback carries member-group metadata.
pub struct HeaderPathParams<'a> {
    pub tracker: &'a GroupTracker,
    pub header_entity_id: Option<EntityId>,
    /// Entity ids of the members that contributed text, in assembly order.
    pub member_entity_ids: &'a [EntityId],
    /// All entities whose content is folded into the header conversion text
    /// (e.g. every fragment of a merged group). These join the first chunk's
    /// content attribution and source coverage.
    pub header_source_entity_ids: &'a [EntityId],
    pub chunk_index: usize,
    pub total_chunks: usize,
    pub include_header_in_first_coverage: bool,
}

/// Chunk a single text path (BM25 or Embedding) independently
///
/// Unified orchestration for both the plain single-path flow and the
/// header-path flow (header + members): check limits, split, enforce segment
/// limits, then build chunks.
pub fn chunk_single_path(input: ChunkInput) -> Vec<ChunkedResult> {
    let ChunkInput {
        infra,
        group,
        text,
        file_path,
        keywords,
        path,
        strategy,
        nl_boundaries,
        header_mode,
    } = input;
    let word_count = text.split_whitespace().filter(|w| !w.is_empty()).count();

    let needs_split = infra.config.exceeds_limit(text, path);

    if !needs_split {
        let fresh_tracker = GroupTracker::new();
        let tracker: &GroupTracker = header_mode.as_ref().map_or(&fresh_tracker, |h| h.tracker);
        let builder = ChunkBuilder::new();
        return if let Some(header) = &header_mode {
            vec![build_unsplit_header_chunk(
                &builder, tracker, header, group, file_path, path, text, word_count, keywords,
            )]
        } else {
            vec![builder.from_single_text(
                tracker,
                SingleChunkContext {
                    group,
                    file_path,
                    path,
                    text,
                    keywords,
                },
            )]
        };
    }

    let segments = if let Some(nl) = nl_boundaries {
        infra
            .splitter
            .split_with_nl_boundaries(text, group, strategy, path, nl)
    } else {
        infra.splitter.split(text, group, strategy, path)
    };

    let segments = super::segment_limit::enforce_segment_max_limit(
        segments,
        path,
        infra,
        group,
        nl_boundaries.unwrap_or_default(),
    );

    // Over-limit text must always produce at least one segment; empty output
    // means content was silently lost, which the splitter invariants forbid.
    assert!(
        !segments.is_empty(),
        "over-limit text produced no segments (path={path}, strategy={strategy:?})"
    );

    let fresh_tracker = GroupTracker::new();
    let tracker: &GroupTracker = header_mode.as_ref().map_or(&fresh_tracker, |h| h.tracker);
    let builder = ChunkBuilder::new();

    let header_ctx = header_mode.as_ref().map(|h| SegmentHeaderContext {
        header_entity_id: h.header_entity_id,
        include_header_in_first_coverage: h.include_header_in_first_coverage,
    });

    builder.from_segments(
        tracker,
        &segments,
        path,
        group,
        file_path,
        keywords,
        nl_boundaries.unwrap_or_default(),
        header_ctx,
    )
}

/// Build an unsplit chunk with header semantics (header + members).
#[allow(clippy::too_many_arguments)]
fn build_unsplit_header_chunk(
    builder: &ChunkBuilder,
    tracker: &GroupTracker,
    header: &HeaderPathParams,
    group: &EntityGroup,
    file_path: &str,
    path: ChunkPath,
    text: &str,
    word_count: usize,
    keywords: &[String],
) -> ChunkedResult {
    let content_entity_ids = if header.chunk_index == 0 {
        let mut ids: Vec<EntityId> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for id in header
            .header_source_entity_ids
            .iter()
            .copied()
            .chain(header.header_entity_id)
            .chain(header.member_entity_ids.iter().copied())
        {
            if seen.insert(id) {
                ids.push(id);
            }
        }
        ids
    } else {
        header.member_entity_ids.to_vec()
    };
    let context_entity_ids = header.header_entity_id.into_iter().collect();
    let chunk_id = format!("{}_{}_{}", group.group_id, path, header.chunk_index);
    let split_reason = if header.total_chunks > 1 {
        SplitReason::MemberBoundary
    } else {
        SplitReason::NotSplit
    };
    builder.from_unsplit(UnsplitContext {
        group,
        file_path,
        path,
        chunk_id,
        chunk_index: header.chunk_index,
        total_chunks: header.total_chunks,
        text: text.to_string(),
        word_count,
        end_byte: text.len(),
        content_entity_ids,
        context_entity_ids,
        keywords: keywords.to_vec(),
        split_reason,
        related_groups: tracker.get_related_groups(&group.group_id),
    })
}

/// Add relations to chunks
pub fn add_relations(tracker: &GroupTracker, chunks: &mut [ChunkedResult]) {
    if let Some(first) = chunks.first() {
        let group_id = &first.source_group_id;
        let relations = tracker.get_related_groups(group_id);

        for chunk in chunks {
            chunk.related_groups = relations.clone();
        }
    }
}

/// Compute NL entity boundaries for a header + member group.
///
/// The combined text layout is: `header + "\n\n" + member1 + "\n\n" + member2 + ...`
/// This function returns per-entity byte ranges within that combined text,
/// which the splitter can use to split at entity boundaries.
pub fn compute_nl_boundaries_for_group(
    header_text: &str,
    header_entity_id: Option<cce_types::entity::EntityId>,
    members: &[ConversionResult],
    path: ChunkPath,
) -> Vec<NlEntityBoundary> {
    super::boundary::compute_nl_entity_boundaries(header_text, header_entity_id, members, path)
}
