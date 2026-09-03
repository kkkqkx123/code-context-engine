use crate::grouper::EntityGroup;
use cce_types::ConversionResult;
use cce_types::entity::{EntityId, EntityKind};

use super::boundary::{ChunkSegment, NlEntityBoundary, SplitReason, cost};
use super::result::{
    ChunkMetadata, ChunkPath, ChunkedResult, CodeSpecificMetadata, SourceSpanKind,
};
use super::source_coverage::{self, set_source_coverage};
use super::tracker::GroupTracker;

pub struct SingleChunkContext<'a> {
    pub group: &'a EntityGroup,
    pub file_path: &'a str,
    pub path: ChunkPath,
    pub text: &'a str,
    pub keywords: &'a [String],
}

pub struct UnsplitContext<'a> {
    pub group: &'a EntityGroup,
    pub file_path: &'a str,
    pub path: ChunkPath,
    pub chunk_id: String,
    pub chunk_index: usize,
    pub total_chunks: usize,
    pub text: String,
    pub word_count: usize,
    pub end_byte: usize,
    pub content_entity_ids: Vec<cce_types::entity::EntityId>,
    pub context_entity_ids: Vec<cce_types::entity::EntityId>,
    pub keywords: Vec<String>,
    pub split_reason: SplitReason,
    pub related_groups: Vec<super::result::GroupRelation>,
}

/// Header-specific parameters for `from_segments`.
///
/// The header path (header + member groups) treats the group header entity
/// differently from member entities: it is excluded from content attribution
/// (it is context), and it joins the source coverage of the first segment of
/// the first member group.
pub struct SegmentHeaderContext {
    pub header_entity_id: Option<EntityId>,
    pub include_header_in_first_coverage: bool,
}

pub struct ChunkBuilder;

/// Whether an entity carries its own docstring, making it semantically
/// self-contained. Members with an independent description must stay in
/// their own chunk (Embedding path) so their topic is not diluted by
/// adjacent members without docstrings.
pub(crate) fn entity_has_own_descriptor(group: &EntityGroup, entity_id: EntityId) -> bool {
    group
        .members
        .iter()
        .chain(group.header.iter())
        .find(|m| m.id == entity_id)
        .and_then(|m| m.doc_comment.as_deref())
        .is_some_and(|doc| !doc.trim().is_empty())
}

/// First contributing entity id of a chunk, excluding the group header
/// (which is context on the header path, not content).
fn first_content_entity_id<'a>(
    mut content_entity_ids: impl Iterator<Item = &'a EntityId>,
    header_entity_id: Option<EntityId>,
) -> Option<EntityId> {
    content_entity_ids
        .find(|id| Some(**id) != header_entity_id)
        .copied()
}

impl Default for ChunkBuilder {
    fn default() -> Self {
        Self
    }
}

impl ChunkBuilder {
    pub fn new() -> Self {
        Self
    }

    fn prepend_identity_if_needed(text: &mut String, name: &str, kind: EntityKind) {
        // Import-like groups produce a bulky structured identity line
        // (e.g. `std::{...} (import).`) that adds no retrieval value.
        if matches!(
            kind,
            EntityKind::Import | EntityKind::Require | EntityKind::Include | EntityKind::Export
        ) {
            return;
        }
        let identity_line = format!("{} ({}).\n", name, kind.kind_label());
        if text.starts_with(&identity_line) {
            return;
        }
        let first_line_has_name = text
            .lines()
            .next()
            .is_some_and(|first| first.contains(name));
        if !first_line_has_name {
            *text = identity_line + &*text;
        }
    }

    pub fn from_single_text(
        &self,
        tracker: &GroupTracker,
        ctx: SingleChunkContext,
    ) -> ChunkedResult {
        let chunk_id = format!("{}_{}_0", ctx.group.group_id, ctx.path);
        let mut text = ctx.text.to_string();
        if ctx.path == ChunkPath::Embedding {
            Self::prepend_identity_if_needed(&mut text, ctx.group.name.as_str(), ctx.group.kind);
        }
        let content_entity_ids = ctx.group.all_entity_ids();
        let (source_span, source_ranges, source_span_kind) =
            source_coverage::source_coverage_for_entity_ids(
                ctx.group,
                &content_entity_ids,
                SourceSpanKind::ExactEntities,
            );

        let title = Some(ctx.group.name.to_string());
        let keywords = ctx.keywords.to_vec();
        let text_len = text.len();
        let word_count = text.split_whitespace().filter(|w| !w.is_empty()).count();
        let token_count = cost(&text, ctx.path);

        ChunkedResult {
            chunk_id,
            source_group_id: ctx.group.group_id.to_string(),
            path: ctx.path,
            group_type: ctx.group.group_type,
            chunk_index: 0,
            total_chunks: 1,
            text,
            bm25_title: title,
            bm25_keywords: keywords,
            token_count,
            start_byte: 0,
            end_byte: text_len,
            prev_overlap: None,
            next_overlap: None,
            related_groups: tracker.get_related_groups(&ctx.group.group_id),
            self_contained: false,
            metadata: {
                let mut meta = ChunkMetadata::for_code(
                    ctx.file_path.to_string(),
                    source_span,
                    ctx.group.language,
                    CodeSpecificMetadata {
                        content_entity_names: ctx.group.entity_display_names(&content_entity_ids),
                        content_entity_ids,
                        entity_kind: ctx.group.kind,
                        modifiers: ctx
                            .group
                            .header
                            .as_ref()
                            .map(|h| h.modifiers.clone())
                            .unwrap_or_default(),
                        split_reason: SplitReason::NotSplit,
                        pattern_info: serde_json::to_string(&ctx.group.pattern_info).ok(),
                        ..Default::default()
                    },
                );
                set_source_coverage(&mut meta, source_ranges, source_span_kind);
                meta.bm25_word_count = Some(word_count);
                meta.segment_id = ctx.group.group_id.to_string();
                meta.test_info = ctx.group.test_info;
                meta
            },
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn from_segments(
        &self,
        tracker: &GroupTracker,
        segments: &[ChunkSegment],
        path: ChunkPath,
        group: &EntityGroup,
        file_path: &str,
        keywords: &[String],
        nl_boundaries: &[NlEntityBoundary],
        header_ctx: Option<SegmentHeaderContext>,
    ) -> Vec<ChunkedResult> {
        let total = segments.len();
        let is_fragment = total > 1;
        let original_entity_id = group.header_id;
        let bm25_title = Some(group.name.to_string());
        let bm25_keywords = keywords.to_vec();
        let related_groups = tracker.get_related_groups(&group.group_id);

        segments
            .iter()
            .enumerate()
            .map(|(index, segment)| {
                let chunk_id = format!("{}_{}_{}", group.group_id, path, index);
                let mut segment_text = segment.text.clone();
                if path == ChunkPath::Embedding {
                    if index > 0 && total > 1 {
                        // Fragment continuation: inject a structured identity
                        // header so the fragment is self-describing even when
                        // its first byte falls mid-text.
                        let prefix = format!(
                            "{} ({} continuation, fragment {}/{})\n\n",
                            group.name,
                            group.kind.kind_label(),
                            index + 1,
                            total
                        );
                        segment_text = prefix + &segment_text;
                    } else {
                        Self::prepend_identity_if_needed(
                            &mut segment_text,
                            group.name.as_str(),
                            group.kind,
                        );
                    }
                }
                // Strategies without entity boundaries (paragraphs, tokens,
                // lines) produce segments with no entity ids. Attribute the
                // entities whose NL ranges intersect this segment's byte
                // range; only fall back to the whole group when no entity
                // matches (extreme single-entity or boundary-less cases).
                let raw_entity_ids: Vec<_> = if segment.boundary.entity_ids.is_empty() {
                    let intersected = super::boundary::intersect_entities_in_range(
                        nl_boundaries,
                        segment.boundary.start_byte,
                        segment.boundary.end_byte,
                    );
                    if intersected.is_empty() {
                        group.all_entity_ids()
                    } else {
                        intersected
                    }
                } else {
                    segment.boundary.entity_ids.clone()
                };
                // The header entity is context, not content: exclude it from
                // content attribution on the header path.
                let content_entity_ids: Vec<_> = if let Some(ctx) = &header_ctx {
                    raw_entity_ids
                        .iter()
                        .copied()
                        .filter(|id| Some(*id) != ctx.header_entity_id)
                        .collect()
                } else {
                    raw_entity_ids
                };
                let source_kind = if matches!(
                    segment.boundary.split_reason,
                    SplitReason::TokenLimit | SplitReason::HardLimit
                ) && content_entity_ids.len() == 1
                {
                    SourceSpanKind::EnclosingEntity
                } else {
                    SourceSpanKind::ExactEntities
                };
                let coverage_entity_ids: Vec<_> = if let Some(ctx) = &header_ctx {
                    if index == 0 && ctx.include_header_in_first_coverage {
                        ctx.header_entity_id
                            .into_iter()
                            .chain(content_entity_ids.iter().copied())
                            .collect()
                    } else if content_entity_ids.is_empty() {
                        group.all_entity_ids()
                    } else {
                        content_entity_ids.clone()
                    }
                } else {
                    content_entity_ids.clone()
                };
                let (source_span, source_ranges, source_span_kind) =
                    source_coverage::source_coverage_for_entity_ids(
                        group,
                        &coverage_entity_ids,
                        source_kind,
                    );

                let word_count = segment_text
                    .split_whitespace()
                    .filter(|w| !w.is_empty())
                    .count();
                let token_count = cost(&segment_text, path);

                let self_contained = path == ChunkPath::Embedding
                    && first_content_entity_id(
                        content_entity_ids.iter(),
                        header_ctx.as_ref().and_then(|c| c.header_entity_id),
                    )
                    .is_some_and(|id| entity_has_own_descriptor(group, id));

                ChunkedResult {
                    chunk_id,
                    source_group_id: group.group_id.to_string(),
                    path,
                    group_type: group.group_type,
                    chunk_index: index,
                    total_chunks: total,
                    text: segment_text,
                    bm25_title: bm25_title.clone(),
                    bm25_keywords: bm25_keywords.clone(),
                    token_count,
                    start_byte: segment.boundary.start_byte,
                    end_byte: segment.boundary.end_byte,
                    prev_overlap: None,
                    next_overlap: None,
                    related_groups: related_groups.clone(),
                    self_contained,
                    metadata: {
                        let mut meta = ChunkMetadata::for_code(
                            file_path.to_string(),
                            source_span,
                            group.language,
                            CodeSpecificMetadata {
                                content_entity_names: group
                                    .entity_display_names(&content_entity_ids),
                                content_entity_ids,
                                context_entity_ids: header_ctx
                                    .as_ref()
                                    .and_then(|ctx| ctx.header_entity_id)
                                    .into_iter()
                                    .collect(),
                                entity_kind: group.kind,
                                modifiers: group
                                    .header
                                    .as_ref()
                                    .map(|h| h.modifiers.clone())
                                    .unwrap_or_default(),
                                split_reason: segment.boundary.split_reason,
                                is_fragment,
                                fragment_index: if is_fragment { Some(index) } else { None },
                                total_fragments: if is_fragment { Some(total) } else { None },
                                original_entity_id: if is_fragment {
                                    original_entity_id
                                } else {
                                    None
                                },
                                pattern_info: serde_json::to_string(&group.pattern_info).ok(),
                                ..Default::default()
                            },
                        );
                        set_source_coverage(&mut meta, source_ranges, source_span_kind);
                        meta.bm25_word_count = Some(word_count);
                        meta.segment_id = group.group_id.to_string();
                        meta.test_info = group.test_info;
                        meta
                    },
                }
            })
            .collect()
    }

    pub fn from_unsplit(&self, mut ctx: UnsplitContext) -> ChunkedResult {
        if ctx.path == ChunkPath::Embedding {
            Self::prepend_identity_if_needed(
                &mut ctx.text,
                ctx.group.name.as_str(),
                ctx.group.kind,
            );
        }
        let content_entity_ids = ctx.content_entity_ids;
        let (source_span, source_ranges, source_span_kind) =
            source_coverage::source_coverage_for_entity_ids(
                ctx.group,
                &content_entity_ids,
                SourceSpanKind::ExactEntities,
            );

        let token_count = cost(&ctx.text, ctx.path);
        let text = ctx.text;

        let self_contained = ctx.path == ChunkPath::Embedding
            && first_content_entity_id(content_entity_ids.iter(), ctx.group.header_id)
                .is_some_and(|id| entity_has_own_descriptor(ctx.group, id));

        ChunkedResult {
            chunk_id: ctx.chunk_id,
            source_group_id: ctx.group.group_id.to_string(),
            path: ctx.path,
            group_type: ctx.group.group_type,
            chunk_index: ctx.chunk_index,
            total_chunks: ctx.total_chunks,
            text,
            bm25_title: Some(ctx.group.name.to_string()),
            bm25_keywords: ctx.keywords,
            token_count,
            start_byte: 0,
            end_byte: ctx.end_byte,
            prev_overlap: None,
            next_overlap: None,
            related_groups: ctx.related_groups,
            self_contained,
            metadata: {
                let mut meta = ChunkMetadata::for_code(
                    ctx.file_path.to_string(),
                    source_span,
                    ctx.group.language,
                    CodeSpecificMetadata {
                        content_entity_names: ctx.group.entity_display_names(&content_entity_ids),
                        content_entity_ids,
                        context_entity_ids: ctx.context_entity_ids,
                        entity_kind: ctx.group.kind,
                        modifiers: ctx
                            .group
                            .header
                            .as_ref()
                            .map(|h| h.modifiers.clone())
                            .unwrap_or_default(),
                        split_reason: ctx.split_reason,
                        fragment_index: if ctx.total_chunks > 1 {
                            Some(ctx.chunk_index)
                        } else {
                            None
                        },
                        total_fragments: if ctx.total_chunks > 1 {
                            Some(ctx.total_chunks)
                        } else {
                            None
                        },
                        original_entity_id: ctx.group.header_id,
                        pattern_info: serde_json::to_string(&ctx.group.pattern_info).ok(),
                        ..Default::default()
                    },
                );
                set_source_coverage(&mut meta, source_ranges, source_span_kind);
                meta.bm25_word_count = Some(ctx.word_count);
                meta.segment_id = ctx.group.group_id.to_string();
                meta.test_info = ctx.group.test_info;
                meta
            },
        }
    }

    pub fn aggregate_keywords(members: &[ConversionResult]) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut result = Vec::new();
        for member in members {
            for kw in &member.keywords {
                if seen.insert(kw.clone()) {
                    result.push(kw.clone());
                }
            }
        }
        result
    }
}
