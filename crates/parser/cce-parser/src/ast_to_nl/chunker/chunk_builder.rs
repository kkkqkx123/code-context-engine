use std::path::Path;

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
    /// Embedding-path keywords. The BM25 path recomputes keywords from
    /// content entity ids and ignores this field.
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
    /// Embedding-path keywords. The BM25 path recomputes keywords from
    /// content entity ids and ignores this field.
    pub keywords: Vec<String>,
    /// Cross-group parent name for BM25 title and keyword qualification.
    /// Resolved by the caller from the group tracker; `None` leaves
    /// parent-less groups and module-level items to the file-stem rule.
    pub parent_qualifier: Option<String>,
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

    fn entity_name_by_id(group: &EntityGroup, id: EntityId) -> Option<String> {
        if let Some(header) = &group.header {
            if header.id == id {
                return Some(header.name.clone());
            }
        }
        group
            .members
            .iter()
            .find(|m| m.id == id)
            .map(|m| m.name.clone())
    }

    fn sorted_content_ids(group: &EntityGroup, ids: &[EntityId]) -> Vec<EntityId> {
        let mut sorted = ids.to_vec();
        sorted.sort_by_key(|id| {
            group
                .entity_spans
                .get(id)
                .map(|s| s.start_byte)
                .unwrap_or(usize::MAX)
        });
        sorted
    }

    fn entity_name_and_kind(group: &EntityGroup, id: EntityId) -> Option<(String, EntityKind)> {
        if let Some(header) = &group.header {
            if header.id == id {
                return Some((header.name.clone(), header.kind));
            }
        }
        group
            .members
            .iter()
            .find(|m| m.id == id)
            .map(|m| (m.name.clone(), m.kind))
    }

    /// Identity prefix for a continuation segment.
    ///
    /// Continuation segments start mid-entity, so they repeat the identity
    /// of the entities they actually cover (kind plus owner-qualified name)
    /// instead of anonymous fragment counters. Fragment numbers and
    /// continuation markers describe split history, not content, and carry
    /// no retrieval signal, so they are not emitted. Parts whose qualified
    /// name already opens the segment are skipped: a split that falls
    /// exactly on an entity boundary needs no repeated identity.
    fn continuation_identity(
        group: &EntityGroup,
        entity_ids: &[EntityId],
        segment_text: &str,
        parent: Option<&str>,
    ) -> Option<String> {
        let first_line = segment_text.lines().next().unwrap_or_default();
        let owner = Self::resolve_owner(group, parent);
        let mut seen = std::collections::HashSet::new();
        let mut parts = Vec::new();
        for id in Self::sorted_content_ids(group, entity_ids) {
            if !seen.insert(id) {
                continue;
            }
            let (name, kind) = Self::entity_name_and_kind(group, id)?;
            if name.is_empty() {
                continue;
            }
            let qualified = match owner {
                Some(prefix) if name != prefix && !name.starts_with(&format!("{prefix}.")) => {
                    format!("{prefix}.{name}")
                }
                _ => name,
            };
            if first_line.contains(&qualified) {
                continue;
            }
            parts.push(format!("{} {}.", kind.kind_label(), qualified));
        }
        if parts.is_empty() {
            return None;
        }
        Some(parts.join("\n") + "\n\n")
    }

    /// Cross-group parent name for a standalone child group.
    ///
    /// Resolves `parent_group_id` against previously recorded groups. Module-like
    /// parents are skipped: a file module is not an ownership qualifier.
    pub(crate) fn cross_group_parent(
        group: &EntityGroup,
        tracker: &GroupTracker,
    ) -> Option<String> {
        let parent_id = group.parent_group_id.as_ref()?;
        let (parent_name, parent_kind) = tracker.lookup_identity(parent_id.as_str())?;
        if parent_kind.is_module_like() || parent_name.is_empty() || parent_name == group.name {
            return None;
        }
        Some(parent_name)
    }

    /// Ownership qualifier shared by BM25 titles, keywords, and identities.
    ///
    /// A recorded cross-group parent wins over the current group name: for
    /// standalone child groups (and merged groups named after one member)
    /// the parent is the real owner. Container groups keep their own name,
    /// which already qualifies their members. Without a parent the group
    /// name stays the qualifier, preserving the previous behavior.
    fn resolve_owner<'a>(group: &'a EntityGroup, parent: Option<&'a str>) -> Option<&'a str> {
        if group.group_type.is_container() && !group.name.is_empty() {
            return Some(group.name.as_str());
        }
        if let Some(name) = parent.filter(|p| !p.is_empty()) {
            return Some(name);
        }
        if group.name.is_empty() {
            return None;
        }
        Some(group.name.as_str())
    }

    /// File module stem used to qualify module-level free functions.
    fn module_stem(file_path: &str) -> Option<String> {
        let stem = Path::new(file_path).file_stem()?.to_string_lossy();
        if stem.is_empty() {
            return None;
        }
        Some(stem.to_string())
    }

    /// Qualify a bare BM25 title that carries no ownership information.
    ///
    /// Container members are already qualified via the group name. Remaining
    /// bare titles are cross-group children (qualified by the recorded parent
    /// name) or module-level free functions (qualified by the file stem).
    /// Titles that already carry a qualifier are returned unchanged, as are
    /// non-function groups without a recorded parent.
    fn qualify_bm25_title(
        title: String,
        group: &EntityGroup,
        parent: Option<&str>,
        file_path: &str,
    ) -> String {
        if title.contains('.') {
            return title;
        }
        if let Some(parent_name) = parent.filter(|p| !p.is_empty()) {
            return format!("{}.{}", parent_name, title);
        }
        if group.kind == EntityKind::Function
            && let Some(stem) = Self::module_stem(file_path)
        {
            return format!("{}.{}", stem, title);
        }
        title
    }

    fn push_keyword_part(
        result: &mut Vec<String>,
        seen: &mut std::collections::HashSet<String>,
        part: &str,
    ) {
        let lower = part.to_lowercase();
        if lower.len() >= 2
            && !lower.chars().all(|c| c.is_ascii_digit())
            && seen.insert(lower.clone())
        {
            result.push(lower);
        }
    }

    fn bm25_title_for_ids(
        group: &EntityGroup,
        content_ids: &[EntityId],
        parent: Option<&str>,
        file_path: &str,
    ) -> String {
        let sorted = Self::sorted_content_ids(group, content_ids);
        if let Some(first) = sorted.first() {
            if let Some(name) = Self::entity_name_by_id(group, *first) {
                if Some(*first) != group.header_id
                    && !group.name.is_empty()
                    && name != group.name.as_str()
                {
                    // A recorded parent is the real owner for standalone and
                    // merged groups; only container members use the group name.
                    if let Some(owner) = Self::resolve_owner(group, parent) {
                        return format!("{owner}.{name}");
                    }
                    return format!("{}.{}", group.name, name);
                }
                return Self::qualify_bm25_title(name, group, parent, file_path);
            }
        }
        Self::qualify_bm25_title(group.name.to_string(), group, parent, file_path)
    }

    fn bm25_keywords_for_ids(
        group: &EntityGroup,
        content_ids: &[EntityId],
        parent: Option<&str>,
        file_path: &str,
    ) -> Vec<String> {
        let sorted = Self::sorted_content_ids(group, content_ids);
        let mut seen = std::collections::HashSet::new();
        let mut result = Vec::new();
        for id in &sorted {
            if let Some(name) = Self::entity_name_by_id(group, *id) {
                Self::push_keyword_part(&mut result, &mut seen, &name);
            }
        }
        let has_member = sorted.iter().any(|id| Some(*id) != group.header_id);
        if has_member && group.group_type.is_container() && !group.name.is_empty() {
            Self::push_keyword_part(&mut result, &mut seen, &group.name);
        }
        if let Some(parent_name) = parent.filter(|p| !p.is_empty()) {
            Self::push_keyword_part(&mut result, &mut seen, parent_name);
        } else if group.kind == EntityKind::Function
            && let Some(stem) = Self::module_stem(file_path)
        {
            Self::push_keyword_part(&mut result, &mut seen, &stem);
        }
        result
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

        let (title, keywords) = if ctx.path == ChunkPath::Bm25 {
            let parent = Self::cross_group_parent(ctx.group, tracker);
            (
                Some(Self::bm25_title_for_ids(
                    ctx.group,
                    &content_entity_ids,
                    parent.as_deref(),
                    ctx.file_path,
                )),
                Self::bm25_keywords_for_ids(
                    ctx.group,
                    &content_entity_ids,
                    parent.as_deref(),
                    ctx.file_path,
                ),
            )
        } else {
            (Some(ctx.group.name.to_string()), ctx.keywords.to_vec())
        };
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
            truncated: false,
            metadata: {
                let mut meta = ChunkMetadata::for_code(
                    ctx.file_path.to_string(),
                    source_span,
                    ctx.group.language,
                    CodeSpecificMetadata {
                        content_entity_names: ctx.group.entity_display_names(&content_entity_ids),
                        content_entity_kinds: ctx.group.entity_display_kinds(&content_entity_ids),
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
        let related_groups = tracker.get_related_groups(&group.group_id);

        segments
            .iter()
            .enumerate()
            .map(|(index, segment)| {
                let chunk_id = format!("{}_{}_{}", group.group_id, path, index);
                let mut segment_text = segment.text.clone();
                let is_continuation = index > 0 && total > 1;
                // Resolved once per chunk: titles, keywords, and continuation
                // identities share the same owner.
                let parent = if path == ChunkPath::Bm25 {
                    Self::cross_group_parent(group, tracker)
                } else {
                    None
                };
                if path == ChunkPath::Embedding && !is_continuation {
                    Self::prepend_identity_if_needed(
                        &mut segment_text,
                        group.name.as_str(),
                        group.kind,
                    );
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
                if is_continuation {
                    // Continuation segments repeat the identity of the
                    // entities they cover (see `continuation_identity`) on
                    // both paths: a BM25 fragment starting mid-method would
                    // otherwise lose the `Class.method` co-occurrence that
                    // qualified queries depend on. Segments with no
                    // attributable entity fall back to the group identity line.
                    let fallback_ids;
                    let ids = if content_entity_ids.is_empty() {
                        fallback_ids = group.all_entity_ids();
                        &fallback_ids
                    } else {
                        &content_entity_ids
                    };
                    if let Some(prefix) =
                        Self::continuation_identity(group, ids, &segment_text, parent.as_deref())
                    {
                        segment_text = prefix + &segment_text;
                    } else {
                        Self::prepend_identity_if_needed(
                            &mut segment_text,
                            group.name.as_str(),
                            group.kind,
                        );
                    }
                }
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
                        // The group header is context, not content: a chunk
                        // holding only the header (or the header plus bare
                        // fields) carries no independent topic even when the
                        // header itself is documented. Fall back to the
                        // group's own header id when no header context was
                        // supplied for this segment batch.
                        header_ctx
                            .as_ref()
                            .and_then(|c| c.header_entity_id)
                            .or(group.header_id),
                    )
                    .is_some_and(|id| entity_has_own_descriptor(group, id));

                let (bm25_title, bm25_keywords) = if path == ChunkPath::Bm25 {
                    let effective_ids = if content_entity_ids.is_empty() {
                        group.all_entity_ids()
                    } else {
                        content_entity_ids.clone()
                    };
                    (
                        Some(Self::bm25_title_for_ids(
                            group,
                            &effective_ids,
                            parent.as_deref(),
                            file_path,
                        )),
                        Self::bm25_keywords_for_ids(
                            group,
                            &effective_ids,
                            parent.as_deref(),
                            file_path,
                        ),
                    )
                } else {
                    (Some(group.name.to_string()), keywords.to_vec())
                };

                ChunkedResult {
                    chunk_id,
                    source_group_id: group.group_id.to_string(),
                    path,
                    group_type: group.group_type,
                    chunk_index: index,
                    total_chunks: total,
                    text: segment_text,
                    bm25_title,
                    bm25_keywords,
                    token_count,
                    start_byte: segment.boundary.start_byte,
                    end_byte: segment.boundary.end_byte,
                    prev_overlap: None,
                    next_overlap: None,
                    related_groups: related_groups.clone(),
                    self_contained,
                    truncated: false,
                    metadata: {
                        let mut meta = ChunkMetadata::for_code(
                            file_path.to_string(),
                            source_span,
                            group.language,
                            CodeSpecificMetadata {
                                content_entity_names: group
                                    .entity_display_names(&content_entity_ids),
                                content_entity_kinds: group
                                    .entity_display_kinds(&content_entity_ids),
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

        let (bm25_title, bm25_keywords) = if ctx.path == ChunkPath::Bm25 {
            let effective_ids = if content_entity_ids.is_empty() {
                ctx.group.all_entity_ids()
            } else {
                content_entity_ids.clone()
            };
            (
                Some(Self::bm25_title_for_ids(
                    ctx.group,
                    &effective_ids,
                    ctx.parent_qualifier.as_deref(),
                    ctx.file_path,
                )),
                Self::bm25_keywords_for_ids(
                    ctx.group,
                    &effective_ids,
                    ctx.parent_qualifier.as_deref(),
                    ctx.file_path,
                ),
            )
        } else {
            (Some(ctx.group.name.to_string()), ctx.keywords)
        };

        ChunkedResult {
            chunk_id: ctx.chunk_id,
            source_group_id: ctx.group.group_id.to_string(),
            path: ctx.path,
            group_type: ctx.group.group_type,
            chunk_index: ctx.chunk_index,
            total_chunks: ctx.total_chunks,
            text,
            bm25_title,
            bm25_keywords,
            token_count,
            start_byte: 0,
            end_byte: ctx.end_byte,
            prev_overlap: None,
            next_overlap: None,
            related_groups: ctx.related_groups,
            self_contained,
            truncated: false,
            metadata: {
                let mut meta = ChunkMetadata::for_code(
                    ctx.file_path.to_string(),
                    source_span,
                    ctx.group.language,
                    CodeSpecificMetadata {
                        content_entity_names: ctx.group.entity_display_names(&content_entity_ids),
                        content_entity_kinds: ctx.group.entity_display_kinds(&content_entity_ids),
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

    /// Aggregate member keywords for the embedding path.
    ///
    /// The BM25 path recomputes titles and keywords per chunk from content
    /// entity ids and never uses this helper.
    pub fn aggregate_embedding_keywords(members: &[ConversionResult]) -> Vec<String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grouper::types::GroupType;
    use cce_types::entity::GroupedEntity;
    use cce_types::language::Language;
    use compact_str::CompactString;
    use smallvec::SmallVec;
    use std::collections::HashMap;

    fn method_group() -> EntityGroup {
        EntityGroup {
            group_id: CompactString::from("group_2"),
            group_type: GroupType::Standalone,
            header: Some(GroupedEntity::new(
                EntityId(2),
                EntityKind::Method,
                "findName".to_string(),
                "findName(int hash)".to_string(),
            )),
            header_id: Some(EntityId(2)),
            members: SmallVec::new(),
            member_ids: SmallVec::new(),
            entity_spans: HashMap::new(),
            combined_source: None,
            combined_source_lazy: std::sync::OnceLock::new(),
            span: cce_types::Span::default(),
            kind: EntityKind::Method,
            name: CompactString::from("findName"),
            language: Language::Java,
            pattern_info: crate::grouper::types::PatternInfo::None,
            member_roles: SmallVec::new(),
            nested_groups: Box::new([]),
            nesting_level: 0,
            parent_group_id: Some(CompactString::from("group_1")),
            has_significant_nested: false,
            metadata: Default::default(),
            test_info: cce_types::TestInfo::unknown(),
        }
    }

    fn tracker_with_class_parent() -> GroupTracker {
        let mut tracker = GroupTracker::new();
        let parent = EntityGroup {
            group_id: CompactString::from("group_1"),
            name: CompactString::from("Processor"),
            kind: EntityKind::Class,
            ..method_group()
        };
        tracker.register_identity(&parent);
        tracker
    }

    #[test]
    fn test_standalone_method_title_uses_recorded_parent() {
        let group = method_group();
        let tracker = tracker_with_class_parent();
        let builder = ChunkBuilder::new();
        let text = "method Processor.findName.\nfindName(int hash)";
        let chunk = builder.from_single_text(
            &tracker,
            SingleChunkContext {
                group: &group,
                file_path: "Processor.java",
                path: ChunkPath::Bm25,
                text,
                keywords: &[],
            },
        );
        assert_eq!(chunk.bm25_title.as_deref(), Some("Processor.findName"));
        assert!(chunk.bm25_keywords.contains(&"findname".to_string()));
        assert!(chunk.bm25_keywords.contains(&"processor".to_string()));
    }

    #[test]
    fn test_standalone_method_title_falls_back_without_parent() {
        let mut group = method_group();
        group.parent_group_id = None;
        let tracker = GroupTracker::new();
        let builder = ChunkBuilder::new();
        let chunk = builder.from_single_text(
            &tracker,
            SingleChunkContext {
                group: &group,
                file_path: "Processor.java",
                path: ChunkPath::Bm25,
                text: "method findName.\nfindName(int hash)",
                keywords: &[],
            },
        );
        assert_eq!(chunk.bm25_title.as_deref(), Some("findName"));
    }

    #[test]
    fn test_module_parent_is_not_an_owner() {
        let group = method_group();
        let mut tracker = GroupTracker::new();
        let parent = EntityGroup {
            group_id: CompactString::from("group_1"),
            name: CompactString::from("tools.jackson.core.sym"),
            kind: EntityKind::Package,
            ..method_group()
        };
        tracker.register_identity(&parent);
        assert_eq!(ChunkBuilder::cross_group_parent(&group, &tracker), None);
    }

    #[test]
    fn test_continuation_identity_uses_parent_qualifier() {
        let group = method_group();
        let identity = ChunkBuilder::continuation_identity(
            &group,
            &[EntityId(2)],
            "body fragment without the signature line",
            Some("Processor"),
        )
        .expect("covered method repeats its identity");
        assert!(
            identity.contains("method Processor.findName."),
            "unexpected identity: {identity}"
        );
    }

    #[test]
    fn test_merged_group_prefers_parent_over_group_name() {
        let member = GroupedEntity::new(
            EntityId(3),
            EntityKind::Method,
            "helper".to_string(),
            "helper()".to_string(),
        );
        let group = EntityGroup {
            group_id: CompactString::from("group_9"),
            group_type: GroupType::MergedFragments,
            header: None,
            header_id: None,
            members: smallvec::smallvec![member],
            member_ids: smallvec::smallvec![EntityId(3)],
            entity_spans: HashMap::new(),
            combined_source: None,
            combined_source_lazy: std::sync::OnceLock::new(),
            span: cce_types::Span::default(),
            kind: EntityKind::Method,
            name: CompactString::from("Utils"),
            language: Language::Java,
            pattern_info: crate::grouper::types::PatternInfo::None,
            member_roles: SmallVec::new(),
            nested_groups: Box::new([]),
            nesting_level: 0,
            parent_group_id: Some(CompactString::from("group_1")),
            has_significant_nested: false,
            metadata: Default::default(),
            test_info: cce_types::TestInfo::unknown(),
        };
        let tracker = tracker_with_class_parent();
        let title = ChunkBuilder::bm25_title_for_ids(
            &group,
            &[EntityId(3)],
            ChunkBuilder::cross_group_parent(&group, &tracker).as_deref(),
            "U.java",
        );
        assert_eq!(title, "Processor.helper");
    }
}
