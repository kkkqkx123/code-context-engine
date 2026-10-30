use std::path::Path;

use crate::grouper::EntityGroup;
use crate::grouper::types::GroupType;
use cce_text::truncate_to_cost;
use cce_types::ConversionResult;
use cce_types::entity::{EntityId, EntityKind};

use super::boundary::{ChunkSegment, NlEntityBoundary, SplitReason, cost, unit};
use super::result::{
    ChunkMetadata, ChunkPath, ChunkedResult, CodeSpecificMetadata, SourceSpanKind,
};
use super::source_coverage::{self, set_source_coverage};
use super::tracker::GroupTracker;

pub use super::chunk_contexts::{SegmentHeaderContext, SingleChunkContext, UnsplitContext};

pub struct ChunkBuilder;

// Per-fragment test attribution from covered entities. Each grouped entity
// already merges the file path rule with its own AST signal, so merging the
// covered set is sufficient. The header counts only when it is content,
// never when carried as context. Empty coverage falls back to the group.

/// Prefixes of the text lines that state an entity's subject.
const TOPIC_LINE_PREFIXES: [&str; 3] = ["branches on: ", "control flow: ", "calls: "];

/// Whether a line states an entity's subject.
///
/// The three forms are the branch discriminants an entity dispatches on, the
/// shape of its control flow, and the functions it calls. All three name what
/// the entity is about; none of them describes the surrounding code.
fn is_topic_line(line: &str) -> bool {
    let line = line.trim_start();
    TOPIC_LINE_PREFIXES.iter().any(|prefix| {
        line.get(..prefix.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
    })
}

/// Maximum cost of one identity line.
const IDENTITY_COST_CAP: usize = 80;

/// Maximum cost of a synthetic block head.
pub(crate) const SYNTHETIC_HEAD_COST_CAP: usize = 32;

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

/// Overlap between a segment's NL-text range and an entity boundary.
fn nl_overlap(segment: &ChunkSegment, boundary: &NlEntityBoundary) -> usize {
    segment
        .boundary
        .end_byte
        .min(boundary.end_byte)
        .saturating_sub(segment.boundary.start_byte.max(boundary.start_byte))
}

/// Slice `base` at line offsets `[start_offset, end_offset)` of its line range.
///
/// The byte offsets are interpolated over the same proportions so the slice
/// stays inside `base` and adjacent slices abut; only the line range is exact.
fn slice_span_lines(
    base: &cce_types::Span,
    start_offset: usize,
    end_offset: usize,
) -> cce_types::Span {
    let Some((first_line, last_line)) = base.line_range_opt() else {
        return *base;
    };
    let line_count = last_line - first_line + 1;
    let start_offset = start_offset.min(line_count - 1);
    let end_offset = end_offset.clamp(start_offset + 1, line_count);
    let byte_len = base.end_byte.saturating_sub(base.start_byte);
    cce_types::Span::new(
        base.start_byte + byte_len * start_offset / line_count,
        base.start_byte + byte_len * end_offset / line_count,
        first_line - 1 + start_offset,
        0,
        first_line - 1 + end_offset,
        0,
    )
}

/// Line slice of `span` covered by the byte interval `[start, end)` of a
/// `total`-byte text that the span is unrelated to.
///
/// Used when a fragment states no entity at all: the fragment still occupies
/// a defined fraction of the text, so it owns the same fraction of the
/// group's lines and never the whole group.
fn text_slice_of_span(
    span: &cce_types::Span,
    start: usize,
    end: usize,
    total: usize,
) -> cce_types::Span {
    let Some((first_line, last_line)) = span.line_range_opt() else {
        return *span;
    };
    if total == 0 {
        return *span;
    }
    let line_count = last_line - first_line + 1;
    let start_offset = start.min(total) * line_count / total;
    let end_offset = (end.min(total) * line_count / total).max(start_offset + 1);
    slice_span_lines(span, start_offset, end_offset)
}

/// Source lines the group header owns when it is carried as context.
///
/// A class header encloses every member, so its span covers the whole body.
/// A chunk that carries only the header's text therefore covers the header's
/// own declaration lines: the region from the group start up to the first
/// member. `None` when the header's span is already its whole extent — no
/// enclosed member, or a function whose members are internal parts of the
/// body its own text describes.
fn header_region_span(group: &EntityGroup, header_id: EntityId) -> Option<cce_types::Span> {
    if group.group_type == GroupType::FunctionWithMembers {
        return None;
    }
    let header = group.entity_spans.get(&header_id).copied()?;
    let first_member = group
        .members
        .iter()
        .filter_map(|member| group.entity_spans.get(&member.id).copied())
        .filter(|span| span.start_byte > header.start_byte && span.start_byte < header.end_byte)
        .min_by_key(|span| span.start_byte)?;
    Some(cce_types::Span::new(
        header.start_byte,
        first_member.start_byte,
        header.start_position.row,
        header.start_position.column,
        first_member.start_position.row,
        0,
    ))
}

/// Split the source lines an entity owns across the fragments carrying it.
///
/// Fragment boundaries only carry NL-text offsets, so the source lines behind
/// a fragment cannot be read off directly. A fragment's share of the entity's
/// NL text is mapped onto a contiguous slice of the entity's line range: the
/// slices tile the whole range, so together the fragments cover the entity
/// once and no two fragments claim the same lines. A header-path chunk
/// partitions the header region instead of the body the header encloses.
fn fragment_source_windows(
    group: &EntityGroup,
    segments: &[ChunkSegment],
    nl_boundaries: &[NlEntityBoundary],
    header_region: Option<(EntityId, cce_types::Span)>,
) -> std::collections::HashMap<(usize, EntityId), cce_types::Span> {
    let mut pieces: std::collections::HashMap<EntityId, Vec<(usize, usize)>> =
        std::collections::HashMap::new();
    for (index, segment) in segments.iter().enumerate() {
        let length = segment
            .boundary
            .end_byte
            .saturating_sub(segment.boundary.start_byte)
            .max(1);
        let mut claimed: Vec<EntityId> = Vec::new();
        for id in &segment.boundary.entity_ids {
            if claimed.contains(id) {
                continue;
            }
            claimed.push(*id);
            let overlap = nl_boundaries
                .iter()
                .find(|boundary| boundary.entity_id == *id)
                .map(|boundary| nl_overlap(segment, boundary))
                .filter(|overlap| *overlap > 0)
                .unwrap_or(length);
            pieces.entry(*id).or_default().push((index, overlap));
        }
        for boundary in nl_boundaries {
            if claimed.contains(&boundary.entity_id) {
                continue;
            }
            let overlap = nl_overlap(segment, boundary);
            if overlap > 0 {
                pieces
                    .entry(boundary.entity_id)
                    .or_default()
                    .push((index, overlap));
            }
        }
    }

    let mut windows = std::collections::HashMap::new();
    for (entity_id, entity_pieces) in pieces {
        let base = match header_region {
            Some((header_id, region)) if header_id == entity_id => region,
            _ => match group.entity_spans.get(&entity_id) {
                Some(span) => *span,
                None => continue,
            },
        };
        if let [(index, _)] = entity_pieces[..] {
            windows.insert((index, entity_id), base);
            continue;
        }
        let Some((first_line, last_line)) = base.line_range_opt() else {
            continue;
        };
        let line_count = last_line - first_line + 1;
        let total_weight: usize = entity_pieces.iter().map(|(_, weight)| *weight).sum();
        let mut consumed = 0usize;
        for (index, weight) in entity_pieces {
            let start_offset = consumed * line_count / total_weight;
            consumed += weight;
            let end_offset = (consumed * line_count / total_weight).max(start_offset + 1);
            windows.insert(
                (index, entity_id),
                slice_span_lines(&base, start_offset, end_offset),
            );
        }
    }
    windows
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

    pub(crate) fn fragment_test_info(
        group: &EntityGroup,
        content_entity_ids: &[EntityId],
    ) -> cce_types::TestInfo {
        if content_entity_ids.is_empty() {
            return group.test_info;
        }
        let mut info = cce_types::TestInfo::unknown();
        let mut matched = false;
        for id in content_entity_ids {
            let marker = if Some(*id) == group.header_id {
                group.header.as_ref().map(|header| header.test_info)
            } else {
                group
                    .members
                    .iter()
                    .find(|member| member.id == *id)
                    .map(|member| member.test_info)
            };
            if let Some(marker) = marker {
                matched = true;
                info = info.merge(&marker);
            }
        }
        if matched { info } else { group.test_info }
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
        path: ChunkPath,
    ) -> Option<String> {
        let first_line = segment_text.lines().next().unwrap_or_default();
        let owner = Self::resolve_owner(group, parent);
        let unit = unit(path);
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
            let qualified: String = match owner {
                Some(prefix) if name != prefix && !name.starts_with(&format!("{prefix}.")) => {
                    format!("{prefix}.{name}")
                }
                _ => name.clone(),
            };
            if first_line.contains(&qualified) {
                continue;
            }
            let identity = Self::identity_line(kind, &qualified);
            let identity = truncate_to_cost(&identity, unit, IDENTITY_COST_CAP);
            if !identity.trim().is_empty() {
                parts.push(identity.to_string());
            }
        }
        if parts.is_empty() {
            return None;
        }
        Some(parts.join("\n") + "\n\n")
    }

    /// Upper bound of the text appended after splitting for one group.
    ///
    /// The splitter works on a body limit; this allowance protects the lines
    /// assembly adds back: the identity line of the first chunk, the identity
    /// anchor of every continuation chunk, and the synthetic head that names
    /// the members of a later block. It counts only decoration that can
    /// actually appear for this group, because every token reserved here is a
    /// token the body cannot use.
    pub(crate) fn decoration_allowance(group: &EntityGroup, path: ChunkPath) -> usize {
        let unit = unit(path);
        // A synthetic head exists only where member blocks are built, which is
        // the header path of a container group. A standalone group is split
        // into fragments of its own body and never gets a block head.
        let head_cap = if group.group_type.is_container() {
            SYNTHETIC_HEAD_COST_CAP
        } else {
            0
        };
        let first_line = format!("{} ({}).\n", group.name, group.kind.kind_label());
        let first = if path == ChunkPath::Embedding {
            cost(&first_line, path)
        } else {
            0
        };
        let mut continuation = 0usize;
        for id in group.all_entity_ids() {
            let Some((name, kind)) = Self::entity_name_and_kind(group, id) else {
                continue;
            };
            if name.is_empty() {
                continue;
            }
            let qualified = if group.name.is_empty() || name == group.name {
                name.clone()
            } else {
                format!("{}.{}", group.name, name)
            };
            let identity = Self::identity_line(kind, &qualified);
            let identity = truncate_to_cost(&identity, unit, IDENTITY_COST_CAP);
            continuation = continuation.max(cost(identity, path).saturating_add(2));
        }
        continuation = continuation.max(cost(&first_line, path).saturating_add(2));
        first.max(continuation).saturating_add(head_cap)
    }

    /// Identity line for one continued entity.
    ///
    /// The anchor is one kind-plus-qualified-name line. Repeating the entity's
    /// signature here would copy a parameter list that can be longer than the
    /// fragment's own body into every fragment of the group, so each fragment
    /// would spend most of its budget on text identical to its siblings and
    /// cover only a handful of source lines. Overloads are still told apart by
    /// the body each fragment carries; the body, not the repeated signature, is
    /// what retrieval matches.
    fn identity_line(kind: EntityKind, qualified: &str) -> String {
        format!("{} {}.", kind.kind_label(), qualified)
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
    /// standalone child groups the parent is the real owner. Container groups
    /// keep their own name, which already qualifies their members. A merged
    /// group has no owner at all: it is named after its first fragment,
    /// which is a sibling rather than the owner of the rest, so qualifying
    /// with it would assert a false relation. Such groups fall back to the
    /// file scope. Without a parent the group name stays the qualifier,
    /// preserving the previous behavior.
    fn resolve_owner<'a>(group: &'a EntityGroup, parent: Option<&'a str>) -> Option<&'a str> {
        if group.group_type.is_container() && !group.name.is_empty() {
            return Some(group.name.as_str());
        }
        if let Some(name) = parent.filter(|p| !p.is_empty()) {
            return Some(name);
        }
        if group.group_type == GroupType::MergedFragments {
            return None;
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
                    // A recorded parent is the real owner for standalone
                    // groups; only container members use the group name.
                    if let Some(owner) = Self::resolve_owner(group, parent) {
                        return format!("{owner}.{name}");
                    }
                    // A merged group is named after one of its own
                    // fragments, so it never owns the others. Such a title
                    // falls back to the file scope instead of claiming a
                    // sibling as a member.
                    if group.group_type != GroupType::MergedFragments {
                        return format!("{}.{}", group.name, name);
                    }
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

    /// Whether a chunk states a topic of its own.
    ///
    /// A chunk states a topic when it names what it calls or branches on.
    /// Such a chunk is retrievable on its own. A chunk that states neither
    /// says nothing about its subject, and how long it is does not change
    /// that: a dense body of code is not a topic.
    pub(crate) fn states_own_topic(text: &str) -> bool {
        text.lines().any(is_topic_line)
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

    /// Name that identifies the content a chunk actually holds.
    ///
    /// A merged group is named after its first fragment, which is a sibling
    /// of the other fragments rather than their owner. A block that does
    /// not hold that fragment must not carry its name: the name would
    /// claim a subject the block does not contain and would make every
    /// block of the group compete for the same queries. Such a block names
    /// the first entity it holds instead.
    fn chunk_identity_name(group: &EntityGroup, content_entity_ids: &[EntityId]) -> String {
        let holds_header = group
            .header_id
            .is_some_and(|header_id| content_entity_ids.contains(&header_id));
        if group.group_type == GroupType::MergedFragments
            && !holds_header
            && let Some(id) = content_entity_ids.first()
            && let Some(name) = Self::entity_name_by_id(group, *id)
        {
            return name;
        }
        group.name.to_string()
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
                None,
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
        let word_count = cost(&text, ChunkPath::Bm25);
        let token_count = cost(&text, ctx.path);
        let fragment_info = Self::fragment_test_info(ctx.group, &content_entity_ids);

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
                meta.bm25_word_count = if ctx.path == ChunkPath::Bm25 {
                    Some(word_count)
                } else {
                    None
                };
                meta.segment_id = ctx.group.group_id.to_string();
                meta.test_info = fragment_info;
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
        // Header-path chunks carry the header as context: its conversion
        // states what the group is about, not the body the header encloses,
        // so those chunks claim the header region instead of the whole body.
        let header_region = header_ctx
            .as_ref()
            .and_then(|ctx| ctx.header_entity_id)
            .and_then(|id| header_region_span(group, id).map(|region| (id, region)));
        let fragment_windows =
            fragment_source_windows(group, segments, nl_boundaries, header_region);
        let text_len = segments
            .last()
            .map(|segment| segment.boundary.end_byte)
            .unwrap_or(0);

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
                    let identity = Self::chunk_identity_name(group, &segment.boundary.entity_ids);
                    Self::prepend_identity_if_needed(&mut segment_text, &identity, group.kind);
                }
                // Strategies without entity boundaries (paragraphs, tokens,
                // lines) produce segments with no entity ids. The entities
                // whose NL ranges intersect this segment's byte range are
                // the ones the segment actually carries.
                let covered_entity_ids: Vec<_> = if segment.boundary.entity_ids.is_empty() {
                    super::boundary::intersect_entities_in_range(
                        nl_boundaries,
                        segment.boundary.start_byte,
                        segment.boundary.end_byte,
                    )
                } else {
                    segment.boundary.entity_ids.clone()
                };
                // Attribution falls back to the whole group when the segment
                // carries no entity at all (extreme single-entity or
                // boundary-less cases): the chunk still needs a subject. The
                // fallback is an attribution, never a source range: a chunk
                // that carries a fraction of the group must not claim the
                // group's source ranges.
                let raw_entity_ids: Vec<_> = if covered_entity_ids.is_empty() {
                    group.all_entity_ids()
                } else {
                    covered_entity_ids.clone()
                };
                // The header entity is context, not content: exclude it from
                // content attribution on the header path. The first chunk that
                // carries the header's own text keeps the header as content so
                // it does not end up with an empty entity list; continuation
                // chunks keep the header as context only.
                let content_entity_ids: Vec<_> = if let Some(ctx) = &header_ctx {
                    let filtered: Vec<_> = raw_entity_ids
                        .iter()
                        .copied()
                        .filter(|id| Some(*id) != ctx.header_entity_id)
                        .collect();
                    if filtered.is_empty() && index == 0 && ctx.include_header_in_first_coverage {
                        raw_entity_ids.clone()
                    } else {
                        filtered
                    }
                } else {
                    raw_entity_ids
                };
                // The source coverage names the entities the segment carries,
                // not the ones it is attributed to. The group header is
                // context on the attribution side, but a segment that
                // carries the header's own text covers the header's source
                // lines: dropping the header here leaves such a segment
                // with no source coverage at all.
                if is_continuation {
                    // Continuation segments repeat the identity of the
                    // entities they cover (see `continuation_identity`) on
                    // both paths: a BM25 fragment starting mid-method would
                    // otherwise lose the `Class.method` co-occurrence that
                    // qualified queries depend on.
                    if let Some(prefix) = Self::continuation_identity(
                        group,
                        &content_entity_ids,
                        &segment_text,
                        parent.as_deref(),
                        path,
                    ) {
                        segment_text = prefix + &segment_text;
                    } else {
                        Self::prepend_identity_if_needed(
                            &mut segment_text,
                            group.name.as_str(),
                            group.kind,
                        );
                    }
                }
                let mut coverage_entity_ids: Vec<_> = if let Some(ctx) = &header_ctx {
                    if index == 0 && ctx.include_header_in_first_coverage {
                        ctx.header_entity_id
                            .into_iter()
                            .chain(covered_entity_ids.iter().copied())
                            .collect()
                    } else {
                        covered_entity_ids.clone()
                    }
                } else {
                    covered_entity_ids.clone()
                };
                let mut seen = std::collections::HashSet::new();
                coverage_entity_ids.retain(|id| seen.insert(*id));
                // A fragment owns the line window of the entity it comes from
                // (see `fragment_source_windows`); an entity that appears
                // whole in this segment keeps its own span.
                let mut coverage_spans: Vec<cce_types::Span> = coverage_entity_ids
                    .iter()
                    .filter_map(|id| {
                        fragment_windows
                            .get(&(index, *id))
                            .copied()
                            .or_else(|| group.entity_spans.get(id).copied())
                    })
                    .collect();
                let windowed = coverage_entity_ids
                    .iter()
                    .any(|id| fragment_windows.contains_key(&(index, *id)));
                let hard_split_single_entity = matches!(
                    segment.boundary.split_reason,
                    SplitReason::TokenLimit | SplitReason::HardLimit
                ) && content_entity_ids.len() == 1;
                let mut source_kind = if windowed || hard_split_single_entity {
                    SourceSpanKind::EnclosingEntity
                } else {
                    SourceSpanKind::ExactEntities
                };
                if coverage_spans.is_empty() {
                    // Nothing states which entity the segment carries. Take
                    // the entity boundaries its NL text overlaps, or the
                    // nearest one, so the fragment still owns a source range.
                    let mut ids = super::boundary::intersect_entities_in_range(
                        nl_boundaries,
                        segment.boundary.start_byte,
                        segment.boundary.end_byte,
                    );
                    if ids.is_empty() {
                        if let Some(boundary) = nl_boundaries.iter().min_by_key(|b| {
                            (b.start_byte as isize - segment.boundary.start_byte as isize).abs()
                        }) {
                            ids.push(boundary.entity_id);
                        }
                    }
                    let mut seen = std::collections::HashSet::new();
                    ids.retain(|id| seen.insert(*id));
                    coverage_spans = ids
                        .iter()
                        .filter_map(|id| {
                            fragment_windows
                                .get(&(index, *id))
                                .copied()
                                .or_else(|| group.entity_spans.get(id).copied())
                        })
                        .collect();
                    if coverage_spans.is_empty() {
                        // A fragment with no entity at all claims the slice
                        // of the group its text occupies, never the group.
                        coverage_spans.push(text_slice_of_span(
                            &group.span,
                            segment.boundary.start_byte,
                            segment.boundary.end_byte,
                            text_len,
                        ));
                        source_kind = SourceSpanKind::EnclosingEntity;
                    }
                    coverage_entity_ids = ids;
                }
                let (source_span, source_ranges, source_span_kind) =
                    source_coverage::source_coverage_from_spans(
                        group,
                        &coverage_entity_ids,
                        coverage_spans,
                        source_kind,
                        None,
                    );

                let word_count = cost(&segment_text, ChunkPath::Bm25);
                let token_count = cost(&segment_text, path);

                let self_contained = path == ChunkPath::Embedding
                    && (first_content_entity_id(
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
                    .is_some_and(|id| entity_has_own_descriptor(group, id))
                        // A chunk that names what it calls or branches on
                        // states its own subject even when its entity has no
                        // documentation.
                        || Self::states_own_topic(&segment_text));

                let (bm25_title, bm25_keywords) = if path == ChunkPath::Bm25 {
                    let effective_ids = content_entity_ids.clone();
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
                    // The embedding path carries no keyword index, but the
                    // title still names the chunk in filters, relation
                    // attribution and result presentation. Naming every
                    // block of a group after the group would give a merged
                    // group's blocks one shared identity, so the title is
                    // computed per block from the entities it actually
                    // holds.
                    let effective_ids = content_entity_ids.clone();
                    (
                        Some(Self::bm25_title_for_ids(
                            group,
                            &effective_ids,
                            parent.as_deref(),
                            file_path,
                        )),
                        keywords.to_vec(),
                    )
                };
                let fragment_info = Self::fragment_test_info(group, &content_entity_ids);

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
                        meta.bm25_word_count = if path == ChunkPath::Bm25 {
                            Some(word_count)
                        } else {
                            None
                        };
                        meta.segment_id = group.group_id.to_string();
                        meta.test_info = fragment_info;
                        meta
                    },
                }
            })
            .collect()
    }

    pub fn from_unsplit(&self, mut ctx: UnsplitContext) -> ChunkedResult {
        let content_entity_ids = ctx.content_entity_ids;
        if ctx.path == ChunkPath::Embedding {
            let identity = Self::chunk_identity_name(ctx.group, &content_entity_ids);
            Self::prepend_identity_if_needed(&mut ctx.text, &identity, ctx.group.kind);
        }
        let (source_span, source_ranges, source_span_kind) =
            source_coverage::source_coverage_for_entity_ids(
                ctx.group,
                &content_entity_ids,
                SourceSpanKind::ExactEntities,
                None,
            );

        let token_count = cost(&ctx.text, ctx.path);
        let states_topic = Self::states_own_topic(&ctx.text);
        let text = ctx.text;

        let self_contained = ctx.path == ChunkPath::Embedding
            && (first_content_entity_id(content_entity_ids.iter(), ctx.group.header_id)
                .is_some_and(|id| entity_has_own_descriptor(ctx.group, id))
                // A chunk that names what it calls or branches on states its
                // own subject even when its entity has no documentation.
                || states_topic);

        let (bm25_title, bm25_keywords) = if ctx.path == ChunkPath::Bm25 {
            let effective_ids = content_entity_ids.clone();
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
            // The embedding path indexes no keyword field, but the title
            // still names the chunk in filters, relation attribution and
            // result presentation. It is computed per chunk from the
            // entities the chunk holds, so the blocks of a merged group do
            // not share one identity.
            let effective_ids = content_entity_ids.clone();
            (
                Some(Self::bm25_title_for_ids(
                    ctx.group,
                    &effective_ids,
                    ctx.parent_qualifier.as_deref(),
                    ctx.file_path,
                )),
                ctx.keywords,
            )
        };
        let fragment_info = Self::fragment_test_info(ctx.group, &content_entity_ids);

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
                meta.bm25_word_count = if ctx.path == ChunkPath::Bm25 {
                    Some(ctx.word_count)
                } else {
                    None
                };
                meta.segment_id = ctx.group.group_id.to_string();
                meta.test_info = fragment_info;
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

    use super::super::boundary::ChunkBoundary;

    /// Group whose header is one fragment and whose member is another.
    ///
    /// The header owns source lines 11-40, the member owns lines 43-51.
    fn header_with_member_group() -> EntityGroup {
        let mut group = EntityGroup {
            group_id: CompactString::from("merged_group_1"),
            group_type: GroupType::MergedFragments,
            header: Some(GroupedEntity::new(
                EntityId(1),
                EntityKind::Function,
                "read_fn".to_string(),
                "read_fn".to_string(),
            )),
            header_id: Some(EntityId(1)),
            members: smallvec::smallvec![GroupedEntity::new(
                EntityId(2),
                EntityKind::Function,
                "write_fn".to_string(),
                "write_fn".to_string(),
            )],
            member_ids: smallvec::smallvec![EntityId(2)],
            entity_spans: HashMap::new(),
            combined_source: None,
            combined_source_lazy: std::sync::OnceLock::new(),
            span: cce_types::Span::default(),
            kind: EntityKind::Function,
            name: CompactString::from("read_fn"),
            language: Language::C,
            pattern_info: crate::grouper::types::PatternInfo::None,
            member_roles: SmallVec::new(),
            nested_groups: Box::new([]),
            nesting_level: 0,
            parent_group_id: None,
            has_significant_nested: false,
            metadata: Default::default(),
            test_info: cce_types::TestInfo::unknown(),
        };
        group
            .entity_spans
            .insert(EntityId(1), cce_types::Span::new(0, 400, 10, 0, 40, 0));
        group
            .entity_spans
            .insert(EntityId(2), cce_types::Span::new(400, 500, 42, 0, 50, 0));
        group
    }

    #[test]
    fn continuation_of_the_group_header_keeps_source_coverage() {
        // A group header whose own text is long enough to be split leaves a
        // continuation block that carries the header's body. The header is
        // context on the attribution side, but the block still covers the
        // header's source lines: without the header in the coverage set the
        // block would own no source range at all. The blocks partition the
        // header region, so no two of them claim the same lines.
        let group = header_with_member_group();
        let tracker = GroupTracker::new();
        let segments = vec![
            ChunkSegment::new(
                ChunkBoundary::new(0, 200, SplitReason::TokenLimit)
                    .with_entity_ids(vec![EntityId(1)]),
                "function read_fn.\nfirst half of the header body".to_string(),
            ),
            ChunkSegment::new(
                ChunkBoundary::new(200, 400, SplitReason::TokenLimit)
                    .with_entity_ids(vec![EntityId(1)]),
                "second half of the header body".to_string(),
            ),
        ];
        let chunks = ChunkBuilder::new().from_segments(
            &tracker,
            &segments,
            ChunkPath::Embedding,
            &group,
            "lib.c",
            &[],
            &[],
            Some(SegmentHeaderContext {
                header_entity_id: group.header_id,
                include_header_in_first_coverage: true,
            }),
        );

        assert_eq!(chunks.len(), 2);
        for chunk in &chunks {
            assert!(
                !chunk.metadata.source_ranges().is_empty(),
                "block {} must own a source range",
                chunk.chunk_id
            );
        }
        let first = chunks[0].metadata.source_ranges();
        let second = chunks[1].metadata.source_ranges();
        assert_eq!(first.len(), 1);
        assert_eq!(second.len(), 1);
        let first = first[0].line_range_opt().expect("first block line range");
        let second = second[0].line_range_opt().expect("continuation line range");
        assert_eq!(first.0, 11, "the blocks start at the header declaration");
        assert!(
            first.1 < second.0,
            "the blocks must not claim the same source lines: {first:?} vs {second:?}"
        );
        assert!(
            second.1 <= 42,
            "the blocks stay inside the header region, got {second:?}"
        );
        assert!(
            chunks[1].metadata.content_entity_ids().is_empty(),
            "the header stays context for attribution"
        );
    }

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
            ChunkPath::Embedding,
        )
        .expect("covered method repeats its identity");
        assert!(
            identity.contains("Processor.findName"),
            "continuation should qualify the name, got: {identity}"
        );
        assert!(
            !identity.contains("int hash"),
            "continuation must not repeat the signature, got: {identity}"
        );
    }

    #[test]
    fn test_continuation_identity_falls_back_without_signature() {
        let mut group = method_group();
        group.header = Some(GroupedEntity::new(
            EntityId(2),
            EntityKind::Method,
            "plain".to_string(),
            String::new(),
        ));
        let identity = ChunkBuilder::continuation_identity(
            &group,
            &[EntityId(2)],
            "body fragment",
            Some("Processor"),
            ChunkPath::Embedding,
        )
        .expect("identity without signature falls back to kind plus name");
        assert!(
            identity.contains("method Processor.plain."),
            "unexpected identity: {identity}"
        );
    }

    #[test]
    fn test_continuation_identity_never_repeats_the_signature() {
        let mut group = method_group();
        group.header = Some(GroupedEntity::new(
            EntityId(2),
            EntityKind::Function,
            "pat_find".to_string(),
            "struct pmt_t* pat_find (struct pat_t* pat, unsigned short pn)".to_string(),
        ));
        let identity = ChunkBuilder::continuation_identity(
            &group,
            &[EntityId(2)],
            "loop body fragment",
            Some("mpeg-pat"),
            ChunkPath::Embedding,
        )
        .expect("a pointer-return function repeats its name");
        // A parameter list can be longer than the fragment body, so repeating
        // it would spend the fragment's budget on text shared with every
        // sibling fragment.
        assert!(
            identity.contains("function mpeg-pat.pat_find."),
            "unexpected identity: {identity}"
        );
        assert!(
            !identity.contains("unsigned short"),
            "the signature must not be repeated: {identity}"
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

    #[test]
    fn test_topic_line_recognizes_every_shape() {
        // The three shapes of a topic statement are equally a statement of
        // what the entity is about.
        assert!(ChunkBuilder::states_own_topic(
            "function handler.\nbranches on: rtmp->connect, rtmp->play."
        ));
        assert!(ChunkBuilder::states_own_topic(
            "function handler.\ncontrol flow: 2 branches."
        ));
        assert!(ChunkBuilder::states_own_topic(
            "function handler.\ncalls: assert, memset."
        ));
        assert!(ChunkBuilder::states_own_topic(
            "function handler.\nCalls: assert, memset."
        ));
        assert!(!ChunkBuilder::states_own_topic(
            "function handler.\nint depth = 0;\nreturn depth;"
        ));
    }
}

#[cfg(test)]
mod fragment_test_info_tests {
    use super::*;
    use crate::grouper::types::GroupType;
    use cce_types::entity::GroupedEntity;
    use cce_types::language::Language;
    use compact_str::CompactString;
    use smallvec::SmallVec;
    use std::collections::HashMap;

    fn mixed_group() -> EntityGroup {
        let mut header = GroupedEntity::new(
            EntityId(10),
            EntityKind::Function,
            "impl".to_string(),
            "impl".to_string(),
        );
        header.test_info = cce_types::TestInfo::unknown();
        let mut prod = GroupedEntity::new(
            EntityId(11),
            EntityKind::Function,
            "new".to_string(),
            "new".to_string(),
        );
        prod.test_info = cce_types::TestInfo::unknown();
        let mut helper = GroupedEntity::new(
            EntityId(12),
            EntityKind::Function,
            "add_str".to_string(),
            "add_str".to_string(),
        );
        helper.test_info = cce_types::TestInfo::test_ast();
        EntityGroup {
            group_id: CompactString::from("group_10"),
            group_type: GroupType::ClassWithMethods,
            header: Some(header),
            header_id: Some(EntityId(10)),
            members: smallvec::smallvec![prod, helper],
            member_ids: smallvec::smallvec![EntityId(11), EntityId(12)],
            entity_spans: HashMap::new(),
            combined_source: None,
            combined_source_lazy: std::sync::OnceLock::new(),
            span: cce_types::Span::default(),
            kind: EntityKind::Function,
            name: CompactString::from("Builder"),
            language: Language::Rust,
            pattern_info: crate::grouper::types::PatternInfo::None,
            member_roles: SmallVec::new(),
            nested_groups: Box::new([]),
            nesting_level: 0,
            parent_group_id: None,
            has_significant_nested: false,
            metadata: Default::default(),
            test_info: cce_types::TestInfo::unknown(),
        }
    }

    #[test]
    fn test_fragment_info_separates_prod_and_test() {
        let group = mixed_group();
        let prod = ChunkBuilder::fragment_test_info(&group, &[EntityId(11)]);
        assert!(!prod.is_test());
        let helper = ChunkBuilder::fragment_test_info(&group, &[EntityId(12)]);
        assert!(helper.is_test());
        let mixed = ChunkBuilder::fragment_test_info(&group, &[EntityId(11), EntityId(12)]);
        assert!(mixed.is_test());
        let empty = ChunkBuilder::fragment_test_info(&group, &[]);
        assert_eq!(empty, group.test_info);
    }
}
