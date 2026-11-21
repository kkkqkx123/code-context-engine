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
            ChunkBoundary::new(0, 200, SplitReason::TokenLimit).with_entity_ids(vec![EntityId(1)]),
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
fn test_continuation_identity_anchors_single_entity() {
    let mut group = method_group();
    group.group_type = GroupType::ClassWithMethods;
    group.name = CompactString::from("Engine");
    group.kind = EntityKind::Class;
    group.header = Some(GroupedEntity::new(
        EntityId(1),
        EntityKind::Method,
        "first".to_string(),
        "first()".to_string(),
    ));
    group.members = smallvec::smallvec![GroupedEntity::new(
        EntityId(2),
        EntityKind::Method,
        "second".to_string(),
        "second()".to_string(),
    )];
    let identity = ChunkBuilder::continuation_identity(
        &group,
        &[EntityId(1), EntityId(2)],
        "tail body without names",
        None,
        ChunkPath::Embedding,
    )
    .expect("continuation must anchor the covered entities");
    let trimmed = identity.trim_end().to_string();
    assert!(
        !trimmed.contains('\n'),
        "multi-entity prefix would exceed the single-line allowance, got: {identity}"
    );
    assert!(
        trimmed.contains("first") || trimmed.contains("second"),
        "anchor must name one covered entity, got: {identity}"
    );
    let allowance = ChunkBuilder::decoration_allowance(&group, ChunkPath::Embedding);
    assert!(
        crate::ast_to_nl::chunker::boundary::cost(&identity, ChunkPath::Embedding) <= allowance,
        "anchor must fit the reserved decoration room"
    );
}

#[test]
fn test_measured_allowance_below_universal_and_repair_covers_parent() {
    let group = method_group();
    let long_parent = "VeryLongParentModuleNameThatKeepsGrowingAndGrowing";
    let identity = ChunkBuilder::continuation_identity(
        &group,
        &[EntityId(2)],
        "tail body without names",
        Some(long_parent),
        ChunkPath::Embedding,
    )
    .expect("parent-qualified anchor must still be produced");
    let measured = ChunkBuilder::decoration_allowance(&group, ChunkPath::Embedding);
    let universal = ChunkBuilder::universal_decoration_upper_bound();
    assert!(
        measured < universal,
        "per-group measured room must stay below the universal bound, got measured={measured} universal={universal}"
    );
    assert!(
        crate::ast_to_nl::chunker::boundary::cost(&identity, ChunkPath::Embedding) > measured,
        "long parent anchor must exceed the measured room so repair is required, got: {identity}"
    );
    assert!(
        crate::ast_to_nl::chunker::boundary::cost(&identity, ChunkPath::Embedding) <= universal,
        "parent-qualified anchor must still fit the universal bound, got: {identity}"
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
fn trait_impl_keywords_carry_self_type_without_renaming_title() {
    let mut header = GroupedEntity::new(
        EntityId(1),
        EntityKind::TraitImpl,
        "Matcher".to_string(),
        "impl Matcher for RegexMatcher".to_string(),
    );
    header
        .metadata
        .insert("impl_for_type".to_string(), "RegexMatcher".to_string());
    let mut group = header_with_member_group();
    group.kind = EntityKind::TraitImpl;
    group.group_type = GroupType::ClassWithMethods;
    group.name = CompactString::from("Matcher");
    group.header = Some(header);
    group.header_id = Some(EntityId(1));
    let title = ChunkBuilder::bm25_title_for_ids(
        &group,
        &[EntityId(1)],
        None,
        "crates/regex/src/matcher.rs",
    );
    assert_eq!(title, "Matcher", "titles stay byte-identical to baseline");
    let member_title = ChunkBuilder::bm25_title_for_ids(
        &group,
        &[EntityId(2)],
        None,
        "crates/regex/src/matcher.rs",
    );
    assert_eq!(
        member_title, "Matcher.write_fn",
        "member titles keep the distinctive method name untouched"
    );
    let keywords = ChunkBuilder::bm25_keywords_for_ids(
        &group,
        &[EntityId(1), EntityId(2)],
        None,
        "crates/regex/src/matcher.rs",
        Some((0, 500)),
    );
    assert_eq!(
        keywords,
        vec!["matcher", "write_fn", "matcher.write_fn", "regexmatcher"],
        "keywords gain the self type and the qualified form additively, nothing renamed: {keywords:?}"
    );
}

#[test]
fn keywords_name_every_entity_the_chunk_span_overlaps() {
    // Header-only chunk whose span covers the member: the member name
    // must join the keywords so the answer vocabulary travels with
    // the covering chunk instead of living in a neighbour.
    let group = header_with_member_group();
    let keywords =
        ChunkBuilder::bm25_keywords_for_ids(&group, &[EntityId(1)], None, "x", Some((0, 500)));
    assert_eq!(
        keywords,
        vec!["read_fn", "write_fn"],
        "overlapped member joins the keywords: {keywords:?}"
    );
    // Same chunk, narrow span: no overlap, no addition.
    let keywords =
        ChunkBuilder::bm25_keywords_for_ids(&group, &[EntityId(1)], None, "x", Some((0, 399)));
    assert_eq!(
        keywords,
        vec!["read_fn"],
        "disjoint member stays out: {keywords:?}"
    );
    // No span information: legacy behavior preserved.
    let keywords = ChunkBuilder::bm25_keywords_for_ids(&group, &[EntityId(1)], None, "x", None);
    assert_eq!(
        keywords,
        vec!["read_fn"],
        "missing span adds nothing: {keywords:?}"
    );
}

#[test]
fn keywords_name_sibling_members_the_chunk_does_not_hold() {
    // A method chunk split away from its siblings loses the term
    // co-occurrence a query may need (`matched` + `context` of one
    // trait). Sibling names join the keywords in qualified form so
    // the owner stays attached, capped to bound dilution.
    let mut group = header_with_member_group();
    group.group_type = GroupType::ClassWithMethods;
    group.kind = EntityKind::Trait;
    group.name = CompactString::from("Sink");
    group.header = Some(GroupedEntity::new(
        EntityId(1),
        EntityKind::Trait,
        "Sink".to_string(),
        "pub trait Sink".to_string(),
    ));
    group.header_id = Some(EntityId(1));
    group.members = smallvec::smallvec![
        GroupedEntity::new(
            EntityId(2),
            EntityKind::Method,
            "matched".to_string(),
            "fn matched".to_string(),
        ),
        GroupedEntity::new(
            EntityId(3),
            EntityKind::Method,
            "context".to_string(),
            "fn context".to_string(),
        ),
        GroupedEntity::new(
            EntityId(4),
            EntityKind::Method,
            "finish".to_string(),
            "fn finish".to_string(),
        ),
    ];
    group.member_ids = smallvec::smallvec![EntityId(2), EntityId(3), EntityId(4)];
    let keywords = ChunkBuilder::bm25_keywords_for_ids(&group, &[EntityId(3)], None, "x", None);
    assert_eq!(
        keywords,
        vec![
            "context",
            "sink",
            "sink.context",
            "sink.matched",
            "sink.finish"
        ],
        "siblings join qualified, content member is not repeated: {keywords:?}"
    );
}

#[test]
fn keywords_skip_sibling_names_for_merged_fragments() {
    // Merged fragments are siblings, not owned members: qualifying
    // them with the group name would assert a false ownership.
    let group = header_with_member_group();
    assert_eq!(group.group_type, GroupType::MergedFragments);
    let keywords = ChunkBuilder::bm25_keywords_for_ids(&group, &[EntityId(1)], None, "x", None);
    assert_eq!(
        keywords,
        vec!["read_fn"],
        "merged groups gain no sibling names: {keywords:?}"
    );
}
