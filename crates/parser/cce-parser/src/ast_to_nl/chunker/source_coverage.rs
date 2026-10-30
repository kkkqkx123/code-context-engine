use crate::grouper::EntityGroup;
use cce_types::entity::EntityId;

use super::result::{ChunkMetadata, SourceSpanKind};

/// `doc_comment_start_line` recorded on one entity (0-indexed row), if any.
///
/// Doc comments are attached to entities by the comment processor. A leading
/// comment (`//`, `///`, `#`) sits above the entity span, while a docstring
/// sits inside it; only the leading comment is outside the coverage the
/// entity's own span already describes.
fn doc_comment_start_row(group: &EntityGroup, entity_id: EntityId) -> Option<usize> {
    group
        .header
        .iter()
        .chain(group.members.iter())
        .find(|entity| entity.id == entity_id)
        .and_then(|entity| entity.metadata.get("doc_comment_start_line"))
        .and_then(|line| line.parse::<usize>().ok())
        .map(|line| line.saturating_sub(1))
}

/// Expand `span` (and the first range of `ranges`, if any) so the source
/// coverage starts at the doc comment attached above the entity that opens it.
///
/// Only a region that starts at an entity's own first line carries that
/// entity's leading comment: a fragment starting mid-entity carries body code
/// and must not reach back over the lines before it, or every fragment would
/// claim the same lines again.
fn expand_to_doc_comment_start(
    group: &EntityGroup,
    entity_ids: &[EntityId],
    span: &mut cce_types::Span,
    ranges: &mut [cce_types::Span],
) {
    let start_row = span.start_position.row;
    let mut earliest: Option<usize> = None;
    for id in entity_ids {
        // An entity without a recorded span has no body of its own; the
        // group's first line is the line it opens.
        let entity_start_row = group
            .entity_spans
            .get(id)
            .map_or(group.span.start_position.row, |span| {
                span.start_position.row
            });
        if entity_start_row != start_row {
            continue;
        }
        let Some(doc_row) = doc_comment_start_row(group, *id) else {
            continue;
        };
        if doc_row < start_row {
            earliest = Some(earliest.map_or(doc_row, |row| row.min(doc_row)));
        }
    }
    let Some(doc_row) = earliest else {
        return;
    };
    span.start_position.row = doc_row;
    if let Some(first) = ranges.first_mut() {
        first.start_position.row = doc_row;
    }
}

/// Build source coverage from the entities a chunk carries.
pub fn source_coverage_for_entity_ids(
    group: &EntityGroup,
    entity_ids: &[EntityId],
    kind: SourceSpanKind,
    fallback_span: Option<cce_types::Span>,
) -> (cce_types::Span, Vec<cce_types::Span>, SourceSpanKind) {
    let spans = entity_ids
        .iter()
        .filter_map(|id| group.entity_spans.get(id).copied())
        .collect();
    source_coverage_from_spans(group, entity_ids, spans, kind, fallback_span)
}

/// Build source coverage from spans the caller resolved.
///
/// The spans may be narrower than the entities' own spans — a fragment owns a
/// line window of the entity it comes from — or endpoint-adjusted, as with a
/// group header that is reported as the region it declares rather than the
/// body it encloses. `entity_ids` names the entities the coverage belongs to
/// and drives doc-comment expansion.
pub fn source_coverage_from_spans(
    group: &EntityGroup,
    entity_ids: &[EntityId],
    spans: Vec<cce_types::Span>,
    kind: SourceSpanKind,
    fallback_span: Option<cce_types::Span>,
) -> (cce_types::Span, Vec<cce_types::Span>, SourceSpanKind) {
    let kind = if group.group_type == crate::grouper::GroupType::FileDocumentation {
        SourceSpanKind::DocumentRange
    } else {
        kind
    };
    let mut spans = spans;
    if spans.is_empty() {
        // No entity span states what the text covers. The group's own span
        // still encloses the whole group text, so an unsplit chunk (the only
        // caller that can get here) owns that span. Without a usable span the
        // coverage is unknown and must stay a fallback.
        let base_span = fallback_span.unwrap_or(group.span);
        let mut span = base_span;
        let mut ranges = vec![span];
        expand_to_doc_comment_start(group, entity_ids, &mut span, &mut ranges);
        let kind = if group.group_type == crate::grouper::GroupType::FileDocumentation {
            SourceSpanKind::DocumentRange
        } else if span.line_range_opt().is_some() {
            SourceSpanKind::EnclosingEntity
        } else {
            SourceSpanKind::GroupFallback
        };
        return (span, ranges, kind);
    }

    spans.sort_by_key(|span| (span.start_byte, span.end_byte));
    let mut ranges: Vec<cce_types::Span> = Vec::new();
    for span in spans {
        if let Some(last) = ranges.last_mut() {
            if span.start_byte <= last.end_byte.saturating_add(1) {
                if span.end_byte > last.end_byte {
                    last.end_byte = span.end_byte;
                    last.end_position = span.end_position;
                }
                continue;
            }
        }
        ranges.push(span);
    }

    for range in &ranges {
        assert!(
            range.start_position.row <= range.end_position.row,
            "invalid range produced by span merge: {:?}",
            range
        );
    }

    let first = ranges[0];
    let mut source_span = ranges
        .iter()
        .copied()
        .skip(1)
        .fold(first, |combined, current| {
            let start = if current.start_byte < combined.start_byte {
                current
            } else {
                combined
            };
            let end = if current.end_byte > combined.end_byte {
                current
            } else {
                combined
            };
            cce_types::Span::new(
                start.start_byte,
                end.end_byte,
                start.start_position.row,
                start.start_position.column,
                end.end_position.row,
                end.end_position.column,
            )
        });

    expand_to_doc_comment_start(group, entity_ids, &mut source_span, &mut ranges);

    (source_span, ranges, kind)
}

pub fn set_source_coverage(
    meta: &mut ChunkMetadata,
    source_ranges: Vec<cce_types::Span>,
    source_span_kind: SourceSpanKind,
) {
    meta.source_ranges = source_ranges;
    meta.source_span_kind = source_span_kind;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grouper::GroupType;
    use cce_types::Span;
    use cce_types::entity::{EntityId, EntityKind, GroupedEntity};
    use cce_types::language::Language;
    use compact_str::CompactString;
    use std::collections::HashMap;

    fn make_header(id: u64, doc_start_line: Option<&str>) -> GroupedEntity {
        let mut metadata = HashMap::new();
        if let Some(line) = doc_start_line {
            metadata.insert("doc_comment_start_line".to_string(), line.to_string());
        }
        GroupedEntity {
            id: EntityId(id),
            name: format!("entity_{id}"),
            kind: EntityKind::Function,
            signature: String::new(),
            parameters: Default::default(),
            return_type: None,
            doc_comment: None,
            modifiers: Vec::new(),
            attributes: HashMap::new(),
            subtype: None,
            is_stdlib: false,
            stdlib_category: None,
            metadata,
            test_info: cce_types::TestInfo::unknown(),
        }
    }

    fn make_group(group_type: GroupType, header: Option<GroupedEntity>, span: Span) -> EntityGroup {
        EntityGroup {
            group_id: CompactString::from("test_group"),
            group_type,
            header,
            header_id: None,
            members: Default::default(),
            member_ids: Default::default(),
            entity_spans: HashMap::new(),
            combined_source: None,
            combined_source_lazy: Default::default(),
            span,
            kind: EntityKind::Function,
            name: CompactString::from("test"),
            language: Language::Python,
            pattern_info: Default::default(),
            member_roles: Default::default(),
            nested_groups: Box::new([]),
            nesting_level: 0,
            parent_group_id: None,
            has_significant_nested: false,
            metadata: Default::default(),
            test_info: cce_types::TestInfo::unknown(),
        }
    }

    fn make_span(start_byte: usize, end_byte: usize, start_row: usize, end_row: usize) -> Span {
        Span::new(start_byte, end_byte, start_row, 0, end_row, 0)
    }

    #[test]
    fn merged_fragments_should_expand_to_doc_comment() {
        let header = make_header(1, Some("1"));
        let group_span = make_span(100, 500, 41, 90);
        let group = make_group(GroupType::MergedFragments, Some(header), group_span);

        let entity_ids = vec![EntityId(1)];
        let (source_span, _, _) = source_coverage_for_entity_ids(
            &group,
            &entity_ids,
            SourceSpanKind::GroupFallback,
            None,
        );

        assert_eq!(
            source_span.start_position.row, 0,
            "MergedFragments should expand to doc comment start line"
        );
    }

    #[test]
    fn non_merged_group_should_expand_to_doc_comment() {
        let header = make_header(1, Some("1"));
        let group_span = make_span(100, 500, 41, 90);
        let group = make_group(GroupType::Standalone, Some(header), group_span);

        let entity_ids = vec![EntityId(1)];
        let (source_span, _, _) = source_coverage_for_entity_ids(
            &group,
            &entity_ids,
            SourceSpanKind::GroupFallback,
            None,
        );

        assert_eq!(
            source_span.start_position.row, 0,
            "Non-merged groups should expand to doc comment start line"
        );
    }

    #[test]
    fn merged_fragments_with_spans_should_expand_to_doc_comment() {
        let header = make_header(1, Some("1"));
        let mut group = make_group(
            GroupType::MergedFragments,
            Some(header),
            make_span(100, 500, 41, 90),
        );
        group
            .entity_spans
            .insert(EntityId(1), make_span(100, 200, 45, 60));

        let entity_ids = vec![EntityId(1)];
        let (source_span, ranges, _) = source_coverage_for_entity_ids(
            &group,
            &entity_ids,
            SourceSpanKind::GroupFallback,
            None,
        );

        assert_eq!(
            source_span.start_position.row, 0,
            "MergedFragments with entity spans should expand to doc comment start"
        );
        assert_eq!(
            ranges[0].start_position.row, 0,
            "Expanded doc comment start must be written back into source ranges"
        );
    }

    #[test]
    fn unsplit_without_entity_owns_the_group_range() {
        let header = make_header(1, None);
        let group_span = make_span(100, 500, 41, 90);
        let group = make_group(GroupType::MergedFragments, Some(header), group_span);

        // A chunk that covers the whole group text and names no entity owns
        // the group's own span: the text is the group, so the span encloses
        // exactly what the chunk carries. It is enclosing, not exact.
        let (source_span, ranges, kind) =
            source_coverage_for_entity_ids(&group, &[], SourceSpanKind::ExactEntities, None);

        assert_eq!(source_span.start_byte, group_span.start_byte);
        assert!(!ranges.is_empty());
        assert_eq!(kind, SourceSpanKind::EnclosingEntity);
    }

    #[test]
    fn member_doc_comment_start_expands_coverage() {
        let header = make_header(1, None);
        let mut group = make_group(
            GroupType::MergedFragments,
            Some(header),
            make_span(100, 500, 41, 90),
        );
        let mut member = GroupedEntity::new(
            EntityId(2),
            EntityKind::Field,
            "glob".to_string(),
            "glob: Option<String>".to_string(),
        );
        member
            .metadata
            .insert("doc_comment_start_line".to_string(), "10".to_string());
        group.members.push(member);
        group
            .entity_spans
            .insert(EntityId(2), make_span(200, 300, 20, 30));

        let entity_ids = vec![EntityId(2)];
        let (source_span, ranges, _) = source_coverage_for_entity_ids(
            &group,
            &entity_ids,
            SourceSpanKind::GroupFallback,
            None,
        );

        assert_eq!(
            source_span.start_position.row, 9,
            "Member doc comment start must expand the coverage start"
        );
        assert_eq!(ranges[0].start_position.row, 9);
    }

    #[test]
    fn no_doc_comment_keeps_entity_start() {
        let header = make_header(1, None);
        let mut group = make_group(
            GroupType::MergedFragments,
            Some(header),
            make_span(100, 500, 41, 90),
        );
        group
            .entity_spans
            .insert(EntityId(1), make_span(100, 200, 45, 60));

        let entity_ids = vec![EntityId(1)];
        let (source_span, _, _) = source_coverage_for_entity_ids(
            &group,
            &entity_ids,
            SourceSpanKind::GroupFallback,
            None,
        );

        assert_eq!(
            source_span.start_position.row, 45,
            "No doc comment metadata means no expansion"
        );
    }
}
