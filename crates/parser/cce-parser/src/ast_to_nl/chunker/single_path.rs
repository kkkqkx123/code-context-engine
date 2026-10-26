//! Single-path chunking orchestration.
//!
//! This module owns the per-text-path flow: limit checks, splitting, segment
//! enforcement, and chunk assembly for one BM25 or Embedding path. It is the
//! mechanism behind `GroupChunker`; the policy of which groups to chunk and
//! how to merge them stays in the chunker.

use crate::grouper::EntityGroup;
use cce_types::ParseError;
use cce_types::entity::EntityId;
use cce_utils::token_estimation::TokenEstimator;

use super::boundary::{NlEntityBoundary, SplitReason};
use super::chunk_builder::{
    ChunkBuilder, SegmentHeaderContext, SingleChunkContext, UnsplitContext,
};
use super::chunker::ChunkOutput;
use super::config::ChunkingConfig;
use super::limits;
use super::result::{ChunkPath, ChunkedResult};
use super::segment_limit;
use super::splitter::TextSplitter;
use super::strategy::SplitStrategy;
use super::tracker::GroupTracker;

/// Shared infrastructure for chunking operations.
///
/// Groups the three core services that are always passed together
/// through the chunking pipeline.
pub struct ChunkInfrastructure<'a> {
    pub config: &'a ChunkingConfig,
    pub estimator: &'a TokenEstimator,
    pub splitter: &'a TextSplitter,
    /// The limit a chunk must satisfy before terminology injection.
    pub pre_injection_embedding: usize,
    /// The limit a BM25 chunk must satisfy before terminology injection.
    pub pre_injection_bm25: usize,
    /// Per-path budgets carrying hard limit, reserve and decoration.
    pub emb_budget: cce_config::modules::ChunkBudget,
    /// Per-path budgets carrying hard limit, reserve and decoration.
    pub bm25_budget: cce_config::modules::ChunkBudget,
    /// Whether the file carries a terminology table.
    pub has_vocabulary: bool,
}

impl ChunkInfrastructure<'_> {
    pub(crate) fn budget(&self, path: ChunkPath) -> cce_config::modules::ChunkBudget {
        match path {
            ChunkPath::Bm25 => self.bm25_budget,
            ChunkPath::Embedding => self.emb_budget,
        }
    }
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
    /// File-level group history for cross-group parent qualification.
    /// The plain single-path flow must use the caller-owned tracker, never
    /// a fresh empty one, or standalone child groups lose their parent.
    pub tracker: &'a GroupTracker,
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
    pub chunk_index: usize,
    pub total_chunks: usize,
    pub include_header_in_first_coverage: bool,
    /// Topic lines of the entities this batch covers, keyed by entity id.
    pub topic_lines: &'a [(EntityId, Vec<String>)],
}

/// Chunk a single text path (BM25 or Embedding) independently
///
/// Unified orchestration for both the plain single-path flow and the
/// header-path flow (header + members): check limits, split, enforce segment
/// limits, then build chunks.
pub fn chunk_single_path(input: ChunkInput) -> Result<ChunkOutput, ParseError> {
    let ChunkInput {
        infra,
        group,
        text,
        file_path,
        keywords,
        path,
        strategy,
        nl_boundaries,
        tracker,
        header_mode,
    } = input;
    // BM25 word count is only meaningful for the BM25 path; the embedding
    // path never carries a BM25 term count.
    let word_count = if path == ChunkPath::Bm25 {
        infra.config.cost(text, ChunkPath::Bm25)
    } else {
        0
    };

    let needs_split =
        infra.config.limit(path) > 0 && infra.config.cost(text, path) > infra.config.limit(path);

    if !needs_split {
        let plain_tracker: &GroupTracker = tracker;
        let active: &GroupTracker = header_mode.as_ref().map_or(plain_tracker, |h| h.tracker);
        let builder = ChunkBuilder::new();
        let chunks = if let Some(header) = &header_mode {
            vec![build_unsplit_header_chunk(
                &builder, active, header, group, file_path, path, text, word_count, keywords,
            )]
        } else {
            vec![builder.from_single_text(
                active,
                SingleChunkContext {
                    group,
                    file_path,
                    path,
                    text,
                    keywords,
                },
            )]
        };
        return Ok(ChunkOutput::with_chunks(
            limits::enforce_pre_injection_limit(
                chunks,
                infra.budget(path),
                file_path,
                group.group_id.as_str(),
            )?,
        ));
    }

    let segments = if let Some(nl) = nl_boundaries {
        infra
            .splitter
            .split_with_nl_boundaries(text, group, strategy, path, nl)?
    } else {
        infra.splitter.split(text, group, strategy, path)?
    };

    let segments = segment_limit::enforce_segment_max_limit(
        segments,
        path,
        infra,
        group,
        nl_boundaries.unwrap_or_default(),
    )?;

    if segments.is_empty() {
        return Err(ParseError::code_splitting(format!(
            "over-limit text produced no segments (path={path}, strategy={strategy:?}, group={})",
            group.group_id
        )));
    }

    let blank_count = segments.iter().filter(|s| s.text.trim().is_empty()).count();
    let segments = if blank_count > 0 {
        tracing::warn!(
            path = ?path,
            group = %group.group_id,
            blank = blank_count,
            "Dropping blank segments produced by splitting (splitter/converter bug)"
        );
        segments
            .into_iter()
            .filter(|s| !s.text.trim().is_empty())
            .collect::<Vec<_>>()
    } else {
        segments
    };
    if segments.is_empty() {
        return Ok(ChunkOutput {
            chunks: Vec::new(),
            dropped_blank_segments: blank_count,
            vocabulary: cce_plugin::vocabulary::VocabularyStats::default(),
        });
    }

    let active: &GroupTracker = header_mode.as_ref().map_or(tracker, |h| h.tracker);
    let builder = ChunkBuilder::new();

    let header_ctx = header_mode.as_ref().map(|h| SegmentHeaderContext {
        header_entity_id: h.header_entity_id,
        include_header_in_first_coverage: h.include_header_in_first_coverage,
        topic_lines: h.topic_lines,
    });

    let chunks = builder.from_segments(
        active,
        &segments,
        path,
        group,
        file_path,
        keywords,
        nl_boundaries.unwrap_or_default(),
        header_ctx,
    );
    Ok(ChunkOutput {
        chunks: limits::enforce_pre_injection_limit(
            chunks,
            infra.budget(path),
            file_path,
            group.group_id.as_str(),
        )?,
        dropped_blank_segments: blank_count,
        vocabulary: cce_plugin::vocabulary::VocabularyStats::default(),
    })
}

/// Build an unsplit chunk with header semantics (header + members).
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_unsplit_header_chunk(
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
            .header_entity_id
            .into_iter()
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
        parent_qualifier: ChunkBuilder::cross_group_parent(group, tracker),
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
