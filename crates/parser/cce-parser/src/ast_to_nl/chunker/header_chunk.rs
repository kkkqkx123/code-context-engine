use crate::grouper::EntityGroup;
use cce_types::ConversionResult;
use cce_types::entity::EntityId;

use super::chunk_builder::ChunkBuilder;
use super::chunker::ChunkInfrastructure;
use super::header::HeaderHelper;
use super::result::{ChunkPath, ChunkedResult};

use super::strategy::SplitStrategy;
use super::tracker::GroupTracker;

struct HeaderTexts {
    first: String,
    continuation: String,
}

/// Context for processing a group with header and members.
struct GroupChunkContext<'a> {
    infra: &'a ChunkInfrastructure<'a>,
    group: &'a EntityGroup,
    file_path: &'a str,
}

/// Path-specific input parameters for header chunk processing.
struct PathInput<'a> {
    header_conv: Option<&'a ConversionResult>,
    member_convs: &'a [ConversionResult],
    path: ChunkPath,
    texts: &'a HeaderTexts,
    helper: &'a HeaderHelper<'a>,
}

pub fn chunk_group_with_conversions(
    infra: &ChunkInfrastructure,
    tracker: &mut GroupTracker,
    group: &EntityGroup,
    group_conversions: &crate::ast_to_nl::converter::GroupConversions,
    file_path: &str,
) -> Vec<ChunkedResult> {
    let header_conv = &group_conversions.header_conversion;
    let member_convs = &group_conversions.member_conversions;

    if member_convs.is_empty() {
        return if let Some(header) = header_conv {
            tracker.record_group(group);

            let bm25_text = header.bm25_text.clone().unwrap_or_default();
            let embedding_text = header.embedding_text.clone().unwrap_or_default();

            let mut all_chunks = Vec::new();

            if !bm25_text.is_empty() {
                let nl_boundaries = super::boundary::locate_entities_in_nl_text(&bm25_text, group);
                let input = super::chunker::ChunkInput {
                    infra,
                    group,
                    text: &bm25_text,
                    file_path,
                    keywords: &header.keywords,
                    path: ChunkPath::Bm25,
                    strategy: if nl_boundaries.is_empty() {
                        SplitStrategy::for_group_type(group.group_type)
                    } else {
                        SplitStrategy::ByNlEntityBoundaries
                    },
                    nl_boundaries: (!nl_boundaries.is_empty()).then_some(nl_boundaries.as_slice()),
                    header_mode: None,
                };
                let bm25_chunks = super::chunker::chunk_single_path(input);
                all_chunks.extend(bm25_chunks);
            }

            if !embedding_text.is_empty() {
                let nl_boundaries =
                    super::boundary::locate_entities_in_nl_text(&embedding_text, group);
                let input = super::chunker::ChunkInput {
                    infra,
                    group,
                    text: &embedding_text,
                    file_path,
                    keywords: &header.keywords,
                    path: ChunkPath::Embedding,
                    strategy: if nl_boundaries.is_empty() {
                        SplitStrategy::for_group_type(group.group_type)
                    } else {
                        SplitStrategy::ByNlEntityBoundaries
                    },
                    nl_boundaries: (!nl_boundaries.is_empty()).then_some(nl_boundaries.as_slice()),
                    header_mode: None,
                };
                let embedding_chunks = super::chunker::chunk_single_path(input);
                all_chunks.extend(embedding_chunks);
            }

            super::chunker::add_relations(tracker, &mut all_chunks);

            all_chunks
        } else {
            vec![]
        };
    }

    smart_chunk_with_header(
        group,
        file_path,
        infra,
        tracker,
        header_conv.as_ref(),
        member_convs,
    )
}

fn smart_chunk_with_header(
    group: &EntityGroup,
    file_path: &str,
    infra: &ChunkInfrastructure,
    tracker: &mut GroupTracker,
    header_conv: Option<&ConversionResult>,
    member_convs: &[ConversionResult],
) -> Vec<ChunkedResult> {
    tracker.record_group(group);

    let helper = HeaderHelper::new(infra.config);

    let ctx = GroupChunkContext {
        infra,
        group,
        file_path,
    };

    let header_bm25 = header_conv
        .and_then(|c| c.bm25_text.as_ref())
        .cloned()
        .unwrap_or_default();
    let header_embedding = header_conv
        .and_then(|c| c.embedding_text.as_ref())
        .cloned()
        .unwrap_or_default();

    let brief_bm25 = header_conv
        .and_then(|c| c.bm25_brief_header.as_ref())
        .cloned()
        .unwrap_or_default();
    let brief_embedding = header_conv
        .and_then(|c| c.embedding_brief_header.as_ref())
        .cloned()
        .unwrap_or_default();

    let mut all_chunks = Vec::new();

    if !header_bm25.is_empty() || member_convs.iter().any(|c| c.bm25_text.is_some()) {
        let texts = HeaderTexts {
            first: helper.compact_header(&header_bm25, &brief_bm25, ChunkPath::Bm25),
            continuation: helper.compact_header(&brief_bm25, &brief_bm25, ChunkPath::Bm25),
        };
        let path_input = PathInput {
            header_conv,
            member_convs,
            path: ChunkPath::Bm25,
            texts: &texts,
            helper: &helper,
        };
        let groups = process_path(&ctx, tracker, path_input);
        all_chunks.extend(groups);
    }

    if !header_embedding.is_empty() || member_convs.iter().any(|c| c.embedding_text.is_some()) {
        let texts = HeaderTexts {
            first: helper.compact_header(&header_embedding, &brief_embedding, ChunkPath::Embedding),
            // Continuation chunks repeat only a minimal identity title: the
            // full class docstring belongs to the first chunk only. Reusing
            // the brief header here would inject the same boilerplate prefix
            // into every continuation chunk, dragging their embeddings
            // toward a shared common-component direction.
            continuation: format!("{} {}.", group.kind.kind_label(), group.name),
        };
        let path_input = PathInput {
            header_conv,
            member_convs,
            path: ChunkPath::Embedding,
            texts: &texts,
            helper: &helper,
        };
        let groups = process_path(&ctx, tracker, path_input);
        all_chunks.extend(groups);
    }

    super::chunker::add_relations(tracker, &mut all_chunks);

    all_chunks
}

fn process_path(
    ctx: &GroupChunkContext,
    tracker: &mut GroupTracker,
    input: PathInput,
) -> Vec<ChunkedResult> {
    let first_budget = input.helper.header_budget(&input.texts.first, input.path);
    let continuation_budget = input
        .helper
        .header_budget(&input.texts.continuation, input.path);
    let member_self_contained: Vec<bool> = input
        .member_convs
        .iter()
        .map(|m| super::chunk_builder::entity_has_own_descriptor(ctx.group, m.entity_id))
        .collect();
    let member_groups = input.helper.group_members_by_header_budget(
        input.member_convs,
        first_budget,
        continuation_budget,
        input.path,
        &member_self_contained,
    );
    let total = member_groups.len();

    // Entities folded into the header conversion text (e.g. every fragment of
    // a merged group). The first chunk's content attribution and source
    // coverage must include them so the chunk's line range reflects the text
    // it actually carries.
    let header_source_entity_ids: Vec<EntityId> = input
        .header_conv
        .as_ref()
        .map(|c| c.source_entity_ids.clone())
        .unwrap_or_default();

    let mut chunks: Vec<ChunkedResult> = member_groups
        .into_iter()
        .enumerate()
        .flat_map(|(idx, members)| {
            let header_text = if idx == 0 {
                &input.texts.first
            } else {
                &input.texts.continuation
            };
            let nl_boundaries = super::chunker::compute_nl_boundaries_for_group(
                header_text,
                input.header_conv.as_ref().map(|c| c.entity_id),
                &members,
                input.path,
            );
            let (combined_text, member_entity_ids) =
                assemble_combined_text(header_text, &members, input.path);
            let input = super::chunker::ChunkInput {
                infra: ctx.infra,
                group: ctx.group,
                text: &combined_text,
                file_path: ctx.file_path,
                keywords: &ChunkBuilder::aggregate_keywords(&members),
                path: input.path,
                strategy: if nl_boundaries.is_empty() {
                    SplitStrategy::for_group_type(ctx.group.group_type)
                } else {
                    SplitStrategy::ByNlEntityBoundaries
                },
                nl_boundaries: (!nl_boundaries.is_empty()).then_some(nl_boundaries.as_slice()),
                header_mode: Some(super::chunker::HeaderPathParams {
                    tracker,
                    header_entity_id: ctx.group.header_id,
                    member_entity_ids: &member_entity_ids,
                    header_source_entity_ids: &header_source_entity_ids,
                    chunk_index: idx,
                    total_chunks: total,
                    include_header_in_first_coverage: idx == 0,
                }),
            };
            super::chunker::chunk_single_path(input)
        })
        .collect();

    finalize_group_path_chunks(&mut chunks);
    chunks
}

/// Assemble the combined header + member text for one member group, along
/// with the entity ids of the members that contributed text (in order).
fn assemble_combined_text(
    header_text: &str,
    members: &[ConversionResult],
    path: ChunkPath,
) -> (String, Vec<EntityId>) {
    let mut combined_text = String::new();
    if !header_text.is_empty() {
        combined_text.push_str(header_text);
    }

    let mut member_entity_ids = Vec::new();
    for member in members {
        let member_text = match path {
            ChunkPath::Bm25 => &member.bm25_text,
            ChunkPath::Embedding => &member.embedding_text,
        };
        if let Some(text) = member_text {
            if !combined_text.is_empty() {
                combined_text.push_str("\n\n");
            }
            combined_text.push_str(text);
            member_entity_ids.push(member.entity_id);
        }
    }
    (combined_text, member_entity_ids)
}

fn finalize_group_path_chunks(chunks: &mut [ChunkedResult]) {
    let total = chunks.len();
    for (index, chunk) in chunks.iter_mut().enumerate() {
        chunk.chunk_id = format!("{}_{}_{}", chunk.source_group_id, chunk.path, index);
        chunk.chunk_index = index;
        chunk.total_chunks = total;
        if let Some(code) = chunk.metadata.code_metadata.as_mut() {
            code.fragment_index = (total > 1).then_some(index);
            code.total_fragments = (total > 1).then_some(total);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::boundary::{ChunkBoundary, ChunkSegment, NlEntityBoundary, SplitReason};
    use super::super::config::ChunkingConfig;
    use super::super::splitter::TextSplitter;
    use super::super::tracker::GroupTracker;
    use super::*;
    use crate::grouper::types::{EntityGroup, GroupType};
    use cce_types::Span;
    use cce_types::entity::{EntityId, EntityKind, GroupedEntity};
    use cce_types::language::Language;
    use cce_utils::token_estimation::TokenEstimator;
    use compact_str::CompactString;
    use smallvec::SmallVec;
    use std::collections::HashMap;
    use std::sync::Arc;

    fn n_words(n: usize) -> String {
        let words: Vec<String> = (0..n).map(|i| format!("w{}", i)).collect();
        words.join(" ")
    }

    fn test_group() -> EntityGroup {
        EntityGroup {
            group_id: CompactString::from("test"),
            group_type: GroupType::Standalone,
            header: Some(GroupedEntity::new(
                EntityId(0),
                EntityKind::Function,
                "test".to_string(),
                "test".to_string(),
            )),
            header_id: Some(EntityId(0)),
            members: SmallVec::new(),
            member_ids: SmallVec::new(),
            entity_spans: HashMap::new(),
            combined_source: Some(Arc::from("")),
            combined_source_lazy: std::sync::OnceLock::new(),
            span: Span::default(),
            kind: EntityKind::Function,
            name: CompactString::from("test"),
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

    fn infra<'a>(
        cfg: &'a ChunkingConfig,
        est: &'a TokenEstimator,
        splitter: &'a TextSplitter,
    ) -> super::super::chunker::ChunkInfrastructure<'a> {
        super::super::chunker::ChunkInfrastructure {
            config: cfg,
            estimator: est,
            splitter,
        }
    }

    fn header_input<'a>(
        infra: &'a ChunkInfrastructure<'a>,
        group: &'a EntityGroup,
        text: &'a str,
        nlb: &'a [NlEntityBoundary],
        tracker: &'a GroupTracker,
        member_entity_ids: &'a [EntityId],
        chunk_index: usize,
    ) -> super::super::chunker::ChunkInput<'a> {
        super::super::chunker::ChunkInput {
            infra,
            group,
            text,
            file_path: "test.rs",
            keywords: &[],
            path: ChunkPath::Bm25,
            strategy: if nlb.is_empty() {
                SplitStrategy::for_group_type(group.group_type)
            } else {
                SplitStrategy::ByNlEntityBoundaries
            },
            nl_boundaries: (!nlb.is_empty()).then_some(nlb),
            header_mode: Some(super::super::chunker::HeaderPathParams {
                tracker,
                header_entity_id: group.header_id,
                member_entity_ids,
                header_source_entity_ids: &[],
                chunk_index,
                total_chunks: 1,
                include_header_in_first_coverage: chunk_index == 0,
            }),
        }
    }

    #[test]
    fn test_enforce_segment_limit_passthrough() {
        let cfg = ChunkingConfig {
            max_bm25_words: 100,
            ..Default::default()
        };
        let est = TokenEstimator::default();
        let splitter = TextSplitter::new(cfg.clone());
        let group = test_group();
        let nlb: [NlEntityBoundary; 0] = [];
        let infra = infra(&cfg, &est, &splitter);
        let segs = vec![
            ChunkSegment::new(
                ChunkBoundary::new(0, 10, SplitReason::MemberBoundary),
                "small segment".to_string(),
            ),
            ChunkSegment::new(
                ChunkBoundary::new(10, 20, SplitReason::MemberBoundary),
                "another seg".to_string(),
            ),
        ];
        let result = super::super::segment_limit::enforce_segment_max_limit(
            segs,
            ChunkPath::Bm25,
            &infra,
            &group,
            &nlb,
        );
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].text, "small segment");
        assert_eq!(result[1].text, "another seg");
    }

    #[test]
    fn test_enforce_segment_limit_descends_entity_boundaries() {
        let cfg = ChunkingConfig {
            max_bm25_words: 2,
            ..Default::default()
        };
        let est = TokenEstimator::default();
        let splitter = TextSplitter::new(cfg.clone());
        let group = test_group();
        let text = "alpha one two beta three four gamma five";
        let nlb = vec![
            NlEntityBoundary {
                entity_id: EntityId(1),
                start_byte: 0,
                end_byte: 5,
            },
            NlEntityBoundary {
                entity_id: EntityId(2),
                start_byte: 14,
                end_byte: 18,
            },
            NlEntityBoundary {
                entity_id: EntityId(3),
                start_byte: 30,
                end_byte: 35,
            },
        ];
        let infra = infra(&cfg, &est, &splitter);
        let seg = ChunkSegment::new(
            ChunkBoundary::new(0, text.len(), SplitReason::MemberBoundary),
            text.to_string(),
        );
        let result = super::super::segment_limit::enforce_segment_max_limit(
            vec![seg],
            ChunkPath::Bm25,
            &infra,
            &group,
            &nlb,
        );
        assert!(result.len() > 1);
        assert!(
            result
                .iter()
                .all(|s| s.text.split_whitespace().count() <= 2)
        );
    }

    #[test]
    fn test_enforce_segment_limit_descends_sentences() {
        let cfg = ChunkingConfig {
            max_bm25_words: 4,
            ..Default::default()
        };
        let est = TokenEstimator::default();
        let splitter = TextSplitter::new(cfg.clone());
        let group = test_group();
        let nlb: [NlEntityBoundary; 0] = [];
        let infra = infra(&cfg, &est, &splitter);
        let text = "one two three four. five six seven eight.";
        let seg = ChunkSegment::new(
            ChunkBoundary::new(0, text.len(), SplitReason::MemberBoundary),
            text.to_string(),
        );
        let result = super::super::segment_limit::enforce_segment_max_limit(
            vec![seg],
            ChunkPath::Bm25,
            &infra,
            &group,
            &nlb,
        );
        assert!(result.len() >= 2);
        assert!(
            result
                .iter()
                .all(|s| s.text.split_whitespace().count() <= 4)
        );
        assert_eq!(result.last().unwrap().boundary.end_byte, text.len());
    }

    #[test]
    fn test_enforce_segment_limit_hard_splits_at_chain_end() {
        let cfg = ChunkingConfig {
            max_bm25_words: 3,
            ..Default::default()
        };
        let est = TokenEstimator::default();
        let splitter = TextSplitter::new(cfg.clone());
        let group = test_group();
        let nlb: [NlEntityBoundary; 0] = [];
        let infra = infra(&cfg, &est, &splitter);
        let text = n_words(25);
        let seg = ChunkSegment::new(
            ChunkBoundary::new(0, text.len(), SplitReason::MemberBoundary)
                .with_entity_ids(vec![EntityId(42)]),
            text,
        );
        let result = super::super::segment_limit::enforce_segment_max_limit(
            vec![seg],
            ChunkPath::Bm25,
            &infra,
            &group,
            &nlb,
        );
        assert!(result.len() >= 2);
        for s in &result {
            assert_eq!(s.boundary.split_reason, SplitReason::HardLimit);
            assert!(s.boundary.entity_ids.contains(&EntityId(42)));
        }
        assert_eq!(result.first().unwrap().boundary.start_byte, 0);
    }

    #[test]
    fn test_enforce_segment_limit_rebases_offsets() {
        let cfg = ChunkingConfig {
            max_bm25_words: 3,
            ..Default::default()
        };
        let est = TokenEstimator::default();
        let splitter = TextSplitter::new(cfg.clone());
        let group = test_group();
        let nlb: [NlEntityBoundary; 0] = [];
        let infra = infra(&cfg, &est, &splitter);
        let base_offset = 100;
        let text = n_words(25);
        let text_len = text.len();
        let seg = ChunkSegment::new(
            ChunkBoundary::new(
                base_offset,
                base_offset + text_len,
                SplitReason::MemberBoundary,
            ),
            text,
        );
        let result = super::super::segment_limit::enforce_segment_max_limit(
            vec![seg],
            ChunkPath::Bm25,
            &infra,
            &group,
            &nlb,
        );
        assert_eq!(result.first().unwrap().boundary.start_byte, base_offset);
        assert_eq!(
            result.last().unwrap().boundary.end_byte,
            base_offset + text_len
        );
    }

    #[test]
    fn test_enforce_segment_limit_intersects_entity_ids() {
        let cfg = ChunkingConfig {
            max_bm25_words: 4,
            ..Default::default()
        };
        let est = TokenEstimator::default();
        let splitter = TextSplitter::new(cfg.clone());
        let group = test_group();
        let nlb = vec![
            NlEntityBoundary {
                entity_id: EntityId(1),
                start_byte: 0,
                end_byte: 10,
            },
            NlEntityBoundary {
                entity_id: EntityId(2),
                start_byte: 20,
                end_byte: 30,
            },
        ];
        let infra = infra(&cfg, &est, &splitter);
        let text = "one two three four. five six seven eight.";
        let seg = ChunkSegment::new(
            ChunkBoundary::new(0, text.len(), SplitReason::MemberBoundary),
            text.to_string(),
        );
        let result = super::super::segment_limit::enforce_segment_max_limit(
            vec![seg],
            ChunkPath::Bm25,
            &infra,
            &group,
            &nlb,
        );
        assert!(result.len() >= 2);
        assert!(
            result
                .iter()
                .any(|s| s.boundary.entity_ids.contains(&EntityId(1)))
        );
    }

    #[test]
    fn test_create_chunk_with_header_within_limit() {
        let cfg = ChunkingConfig {
            max_bm25_words: 100,
            ..Default::default()
        };
        let est = TokenEstimator::default();
        let splitter = TextSplitter::new(cfg.clone());
        let tracker = GroupTracker::new();
        let group = test_group();
        let nlb: [NlEntityBoundary; 0] = [];
        let empty: [ConversionResult; 0] = [];
        let (combined, member_ids) =
            assemble_combined_text("header within limit", &empty, ChunkPath::Bm25);
        let infra = infra(&cfg, &est, &splitter);
        let input = header_input(&infra, &group, &combined, &nlb, &tracker, &member_ids, 0);

        let results = super::super::chunker::chunk_single_path(input);
        assert_eq!(results.len(), 1);
        assert!(!results[0].text.is_empty());
        assert_eq!(results[0].chunk_index, 0);
    }

    #[test]
    fn test_create_chunk_with_header_exceeds_limit_entity_bounds() {
        let cfg = ChunkingConfig {
            max_bm25_words: 3,
            ..Default::default()
        };
        let est = TokenEstimator::default();
        let splitter = TextSplitter::new(cfg.clone());
        let tracker = GroupTracker::new();
        let group = test_group();

        let header_text = "entity one";
        let member = ConversionResult {
            entity_id: EntityId(2),
            bm25_text: Some("entity two very long exceeds limit three".to_string()),
            ..Default::default()
        };
        let members = vec![member];
        let nlb = vec![
            NlEntityBoundary {
                entity_id: EntityId(1),
                start_byte: 0,
                end_byte: header_text.len(),
            },
            NlEntityBoundary {
                entity_id: EntityId(2),
                start_byte: header_text.len() + 2,
                end_byte: header_text.len() + 2 + 42,
            },
        ];
        let (combined, member_ids) = assemble_combined_text(header_text, &members, ChunkPath::Bm25);
        let infra = infra(&cfg, &est, &splitter);
        let input = header_input(&infra, &group, &combined, &nlb, &tracker, &member_ids, 0);

        let results = super::super::chunker::chunk_single_path(input);
        assert!(results.len() > 1);
        assert!(results.iter().all(|r| !r.text.is_empty()));
    }

    #[test]
    fn test_create_chunk_with_header_exceeds_limit_no_boundaries() {
        let cfg = ChunkingConfig {
            max_bm25_words: 3,
            ..Default::default()
        };
        let est = TokenEstimator::default();
        let splitter = TextSplitter::new(cfg.clone());
        let tracker = GroupTracker::new();
        let group = test_group();
        let nlb: [NlEntityBoundary; 0] = [];
        let empty: [ConversionResult; 0] = [];
        let text = n_words(25);
        let (combined, member_ids) = assemble_combined_text(&text, &empty, ChunkPath::Bm25);
        let infra = infra(&cfg, &est, &splitter);
        let input = header_input(&infra, &group, &combined, &nlb, &tracker, &member_ids, 0);

        let results = super::super::chunker::chunk_single_path(input);
        assert!(results.len() > 1);
        assert!(results.iter().all(|r| !r.text.is_empty()));
    }

    fn from_segments_for_test(
        tracker: &GroupTracker,
        segs: Vec<ChunkSegment>,
        group: &EntityGroup,
        path: ChunkPath,
        nlb: &[NlEntityBoundary],
    ) -> Vec<ChunkedResult> {
        let builder = ChunkBuilder::new();
        builder.from_segments(
            tracker,
            &segs,
            path,
            group,
            "test.rs",
            &[],
            nlb,
            Some(super::super::chunk_builder::SegmentHeaderContext {
                header_entity_id: group.header_id,
                include_header_in_first_coverage: true,
            }),
        )
    }

    #[test]
    fn test_segments_to_results_single() {
        let group = test_group();
        let tracker = GroupTracker::new();
        let nlb: [NlEntityBoundary; 0] = [];
        let segs = vec![ChunkSegment::new(
            ChunkBoundary::new(0, 10, SplitReason::MemberBoundary),
            "some text".to_string(),
        )];
        let results = from_segments_for_test(&tracker, segs, &group, ChunkPath::Bm25, &nlb);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].text, "some text");
        assert_eq!(results[0].chunk_index, 0);
        assert_eq!(results[0].total_chunks, 1);
    }

    #[test]
    fn test_segments_to_results_with_entity_ids() {
        let group = test_group();
        let tracker = GroupTracker::new();
        let nlb: [NlEntityBoundary; 0] = [];
        let segs = vec![ChunkSegment::new(
            ChunkBoundary::new(0, 10, SplitReason::MemberBoundary)
                .with_entity_ids(vec![EntityId(0), EntityId(1)]),
            "first".to_string(),
        )];
        let results = from_segments_for_test(&tracker, segs, &group, ChunkPath::Bm25, &nlb);
        assert_eq!(results.len(), 1);
        let code = results[0].metadata.code_metadata.as_ref().unwrap();
        assert!(code.content_entity_ids.contains(&EntityId(1)));
        assert!(!code.content_entity_ids.contains(&EntityId(0)));
    }

    #[test]
    fn test_segments_to_results_source_kind() {
        let group = test_group();
        let tracker = GroupTracker::new();
        let nlb: [NlEntityBoundary; 0] = [];
        let segs = vec![ChunkSegment::new(
            ChunkBoundary::new(0, 10, SplitReason::HardLimit).with_entity_ids(vec![EntityId(1)]),
            "text".to_string(),
        )];
        let results = from_segments_for_test(&tracker, segs, &group, ChunkPath::Bm25, &nlb);
        let code = results[0].metadata.code_metadata.as_ref().unwrap();
        assert_eq!(code.split_reason, SplitReason::HardLimit);
    }

    #[test]
    fn test_segments_to_results_bm25_word_count() {
        let group = test_group();
        let tracker = GroupTracker::new();
        let nlb: [NlEntityBoundary; 0] = [];
        let segs = vec![ChunkSegment::new(
            ChunkBoundary::new(0, 20, SplitReason::MemberBoundary),
            "four words here now".to_string(),
        )];
        let results = from_segments_for_test(&tracker, segs, &group, ChunkPath::Bm25, &nlb);
        assert_eq!(results[0].metadata.bm25_word_count, Some(4));
    }
}
