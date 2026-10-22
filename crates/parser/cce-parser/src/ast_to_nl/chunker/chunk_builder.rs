use std::path::Path;

use crate::grouper::types::GroupType;
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
pub struct SegmentHeaderContext<'a> {
    pub header_entity_id: Option<EntityId>,
    pub include_header_in_first_coverage: bool,
    /// Topic lines of the entities this batch covers, keyed by entity id.
    ///
    /// A continuation block repeats the topic of the entity it continues,
    /// so that a block holding the tail of an entity still states what the
    /// entity is about.
    pub topic_lines: &'a [(EntityId, Vec<String>)],
}

pub struct ChunkBuilder;

/// Identifier byte test shared by name-boundary matching in this module.
fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b >= 0x80
}

/// Prefixes of the text lines that state an entity's subject.
const TOPIC_LINE_PREFIXES: [&str; 3] = ["branches on: ", "control flow: ", "calls: "];

/// Maximum number of topic lines an entity contributes to a block head.
const MAX_TOPIC_LINES: usize = 3;

/// Whether a line states an entity's subject.
///
/// The three forms are the branch discriminants an entity dispatches on, the
/// shape of its control flow, and the functions it calls. All three name
/// what the entity is about; none of them describes the surrounding code.
fn is_topic_line(line: &str) -> bool {
    let line = line.trim_start();
    TOPIC_LINE_PREFIXES.iter().any(|prefix| {
        line.get(..prefix.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
    })
}

/// The topic lines of a converted entity.
///
/// A block that carries only the tail of an entity keeps none of them, so a
/// continuation block repeats them to stay retrievable on its own.
pub(crate) fn topic_lines(text: &str) -> Vec<&str> {
    text.lines()
        .filter(|line| is_topic_line(line))
        .take(MAX_TOPIC_LINES)
        .collect()
}

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
        topic_lines: &[(EntityId, Vec<String>)],
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
            let qualified: String = match owner {
                Some(prefix) if name != prefix && !name.starts_with(&format!("{prefix}.")) => {
                    format!("{prefix}.{name}")
                }
                _ => name.clone(),
            };
            if first_line.contains(&qualified) {
                continue;
            }
            parts.push(Self::signature_identity(group, id, &name, &qualified, kind));
            // The entity's own topic travels with the continuation: a block
            // that holds the tail of an entity says nothing about what the
            // entity is about without it.
            if let Some(lines) = topic_lines
                .iter()
                .find(|(topic_id, _)| *topic_id == id)
                .map(|(_, lines)| lines)
            {
                parts.push(lines.join("\n"));
            }
        }
        if parts.is_empty() {
            return None;
        }
        Some(parts.join("\n") + "\n\n")
    }

    /// Identity line for one continued entity.
    ///
    /// Continuation segments start mid-entity, so a bare kind-plus-name line
    /// is not enough to tell overloads apart or to match signature-level
    /// queries. When the entity carries a substantive signature (a parameter
    /// list mentioning its own name), the signature itself is repeated with
    /// the bare name owner-qualified; otherwise the previous kind-plus-name
    /// form is kept.
    fn signature_identity(
        group: &EntityGroup,
        id: EntityId,
        name: &str,
        qualified: &str,
        kind: EntityKind,
    ) -> String {
        if let Some(signature) = Self::entity_signature_by_id(group, id) {
            if signature.contains('(') && signature.contains(name) {
                return format!(
                    "{}.",
                    Self::qualify_name_in_text(&signature, name, qualified)
                );
            }
        }
        format!("{} {}.", kind.kind_label(), qualified)
    }

    /// First non-empty signature of an entity in the group, if any.
    fn entity_signature_by_id(group: &EntityGroup, id: EntityId) -> Option<String> {
        if let Some(header) = &group.header {
            if header.id == id && !header.signature.is_empty() {
                return Some(header.signature.clone());
            }
        }
        group
            .members
            .iter()
            .find(|m| m.id == id)
            .map(|m| m.signature.clone())
            .filter(|signature| !signature.is_empty())
    }

    /// Replace the bare entity name inside a signature with its qualified
    /// form, matching whole identifiers only.
    fn qualify_name_in_text(signature: &str, name: &str, qualified: &str) -> String {
        if name.is_empty() || name == qualified {
            return signature.to_string();
        }
        let mut out = String::with_capacity(signature.len() + qualified.len());
        let mut rest = signature;
        while let Some(pos) = rest.find(name) {
            let before = pos.checked_sub(1).map(|i| rest.as_bytes()[i]);
            let after = rest.as_bytes().get(pos + name.len()).copied();
            let boundary_before = before.is_none_or(|b| !is_ident_byte(b));
            let boundary_after = after.is_none_or(|b| !is_ident_byte(b));
            if boundary_before && boundary_after {
                out.push_str(&rest[..pos]);
                out.push_str(qualified);
                rest = &rest[pos + name.len()..];
            } else {
                out.push_str(&rest[..pos + 1]);
                rest = &rest[pos + 1..];
            }
        }
        out.push_str(rest);
        out
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
        if group.group_type == GroupType::MergedFragments && !holds_header {
            if let Some(id) = content_entity_ids.first()
                && let Some(name) = Self::entity_name_by_id(group, *id)
            {
                return name;
            }
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
        header_ctx: Option<SegmentHeaderContext<'_>>,
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
                // The source coverage names the entities the segment carries,
                // not the ones it is attributed to.
                let covered_content_ids: Vec<_> = if let Some(ctx) = &header_ctx {
                    covered_entity_ids
                        .iter()
                        .copied()
                        .filter(|id| Some(*id) != ctx.header_entity_id)
                        .collect()
                } else {
                    covered_entity_ids.clone()
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
                    if let Some(prefix) = Self::continuation_identity(
                        group,
                        ids,
                        &segment_text,
                        parent.as_deref(),
                        header_ctx
                            .as_ref()
                            .map(|ctx| ctx.topic_lines)
                            .unwrap_or_default(),
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
                            .chain(covered_content_ids.iter().copied())
                            .collect()
                    } else {
                        covered_content_ids.clone()
                    }
                } else {
                    covered_content_ids.clone()
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
                    // The embedding path carries no keyword index, but the
                    // title still names the chunk in filters, relation
                    // attribution and result presentation. Naming every
                    // block of a group after the group would give a merged
                    // group's blocks one shared identity, so the title is
                    // computed per block from the entities it actually
                    // holds.
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
                        keywords.to_vec(),
                    )
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
            // The embedding path indexes no keyword field, but the title
            // still names the chunk in filters, relation attribution and
            // result presentation. It is computed per chunk from the
            // entities the chunk holds, so the blocks of a merged group do
            // not share one identity.
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
                ctx.keywords,
            )
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
            &[],
        )
        .expect("covered method repeats its identity");
        assert!(
            identity.contains("Processor.findName(int hash)."),
            "continuation should repeat the signature, got: {identity}"
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
            &[],
        )
        .expect("identity without signature falls back to kind plus name");
        assert!(
            identity.contains("method Processor.plain."),
            "unexpected identity: {identity}"
        );
    }

    #[test]
    fn test_continuation_identity_qualifies_signature_name() {
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
            &[],
        )
        .expect("pointer-return function repeats its signature");
        assert!(
            identity.contains("mpeg-pat.pat_find (struct pat_t* pat, unsigned short pn)."),
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

    #[test]
    fn test_topic_lines_are_collected_for_continuation() {
        let text = "function handler.\nbranches on: rtmp->connect.\ncalls: assert.\nint depth = 0;";
        let lines = topic_lines(text);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "branches on: rtmp->connect.");
        assert_eq!(lines[1], "calls: assert.");
    }
}
