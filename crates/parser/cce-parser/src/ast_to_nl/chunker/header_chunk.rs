use crate::grouper::EntityGroup;
use crate::grouper::types::GroupType;
use cce_text::truncate_to_cost;
use cce_types::ConversionResult;
use cce_types::ParseError;

use super::boundary::unit;
use super::chunk_builder::ChunkBuilder;
use super::chunker::{ChunkInfrastructure, ChunkOutput};
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
) -> Result<ChunkOutput, ParseError> {
    let header_conv = &group_conversions.header_conversion;
    let member_convs = &group_conversions.member_conversions;

    if member_convs.is_empty() {
        return if let Some(header) = header_conv {
            tracker.record_group(group);

            let bm25_text = header.bm25_text.clone().unwrap_or_default();
            let embedding_text = header.embedding_text.clone().unwrap_or_default();

            let mut all_chunks = ChunkOutput::default();

            // The group's text is the header entity's own text: no member
            // contributes a text region of its own here. Locating member
            // names inside that text would cut the header's own body at an
            // incidental mention (a local variable in a C function body, a
            // name in the member table) and hand the remaining body to that
            // member, so the fragment would claim a range narrower than the
            // code it carries. The header's own region is the only interval
            // the conversion layer can state here.
            let header_entity_id = header.entity_id;

            if !bm25_text.is_empty() {
                let nl_boundaries = super::boundary::compute_nl_entity_boundaries(
                    &bm25_text,
                    Some(header_entity_id),
                    &[],
                    ChunkPath::Bm25,
                );
                let input = super::chunker::ChunkInput {
                    infra,
                    group,
                    text: &bm25_text,
                    file_path,
                    keywords: &header.keywords,
                    path: ChunkPath::Bm25,
                    strategy: SplitStrategy::ByNlEntityBoundaries,
                    nl_boundaries: Some(nl_boundaries.as_slice()),
                    tracker: &*tracker,
                    header_mode: None,
                };
                all_chunks.absorb(super::chunker::chunk_single_path(input)?);
            }

            if !embedding_text.is_empty() {
                let nl_boundaries = super::boundary::compute_nl_entity_boundaries(
                    &embedding_text,
                    Some(header_entity_id),
                    &[],
                    ChunkPath::Embedding,
                );
                let input = super::chunker::ChunkInput {
                    infra,
                    group,
                    text: &embedding_text,
                    file_path,
                    keywords: &header.keywords,
                    path: ChunkPath::Embedding,
                    strategy: SplitStrategy::ByNlEntityBoundaries,
                    nl_boundaries: Some(nl_boundaries.as_slice()),
                    tracker: &*tracker,
                    header_mode: None,
                };
                all_chunks.absorb(super::chunker::chunk_single_path(input)?);
            }

            super::chunker::add_relations(tracker, &mut all_chunks.chunks);

            Ok(all_chunks)
        } else {
            Ok(ChunkOutput::default())
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
) -> Result<ChunkOutput, ParseError> {
    tracker.record_group(group);

    let mut body_config = infra.config.clone();
    if infra.emb_budget.hard_limit > 0 {
        body_config.max_tokens = infra.emb_budget.body_limit;
    }
    if infra.bm25_budget.hard_limit > 0 {
        body_config.max_bm25_terms = infra.bm25_budget.body_limit;
    }
    let helper = HeaderHelper::new(&body_config);

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

    let brief_embedding = header_conv
        .and_then(|c| c.embedding_brief_header.as_ref())
        .cloned()
        .unwrap_or_default();

    let mut all_chunks = ChunkOutput::default();

    if !header_bm25.is_empty() || member_convs.iter().any(|c| c.bm25_text.is_some()) {
        // Continuation chunks carry member text only. Repeating the class
        // header in every BM25 continuation dilutes each chunk's topic and
        // violates the one-entity-one-block shape; titles and keywords
        // already identify the owner per chunk.
        let texts = HeaderTexts {
            first: helper.compact_header(&header_bm25, "", ChunkPath::Bm25),
            continuation: String::new(),
        };
        let path_input = PathInput {
            header_conv,
            member_convs,
            path: ChunkPath::Bm25,
            texts: &texts,
            helper: &helper,
        };
        all_chunks.absorb(process_path(&ctx, tracker, path_input)?);
    }

    if !header_embedding.is_empty() || member_convs.iter().any(|c| c.embedding_text.is_some()) {
        // Merged-fragment groups have no real owner: the header is only the
        // first sibling fragment, so compacting it to a brief would silently
        // drop that fragment's body while members keep theirs in full (e.g. a
        // read/write function pair keeping only the write body). Keep the
        // full header text here; budget packing below still bounds the chunk.
        let first_text = if group.group_type == crate::grouper::types::GroupType::MergedFragments {
            header_embedding.clone()
        } else {
            helper.compact_header(&header_embedding, &brief_embedding, ChunkPath::Embedding)
        };
        let texts = HeaderTexts {
            first: first_text,
            // Continuation chunks repeat only a minimal identity title: the
            // full class docstring belongs to the first chunk only. Reusing
            // the brief header here would inject the same boilerplate prefix
            // into every continuation chunk, dragging their embeddings
            // toward a shared common-component direction.
            //
            // A merged group has no own topic to repeat: its name is the
            // first fragment's name. Such a block falls back to the file
            // scope, which states where the block comes from without
            // claiming a sibling as its subject.
            continuation: if group.group_type == GroupType::MergedFragments {
                match crate::ast_to_nl::converter::entity_converter::module_stem(file_path) {
                    Some(stem) => format!("{} in {}.", group.kind.kind_label(), stem),
                    None => format!("{} {}.", group.kind.kind_label(), group.name),
                }
            } else {
                format!("{} {}.", group.kind.kind_label(), group.name)
            },
        };
        let path_input = PathInput {
            header_conv,
            member_convs,
            path: ChunkPath::Embedding,
            texts: &texts,
            helper: &helper,
        };
        all_chunks.absorb(process_path(&ctx, tracker, path_input)?);
    }

    super::chunker::add_relations(tracker, &mut all_chunks.chunks);

    Ok(all_chunks)
}

fn process_path(
    ctx: &GroupChunkContext,
    tracker: &mut GroupTracker,
    input: PathInput,
) -> Result<ChunkOutput, ParseError> {
    let mut ordered_members: Vec<ConversionResult> = input.member_convs.to_vec();
    ordered_members.sort_by_key(|m| {
        ctx.group
            .entity_spans
            .get(&m.entity_id)
            .map(|s| (s.start_byte, s.end_byte))
            .unwrap_or((usize::MAX, usize::MAX))
    });
    let member_convs_sorted = ordered_members;
    let member_convs: &[ConversionResult] = &member_convs_sorted;
    let first_budget = input.helper.header_budget(&input.texts.first, input.path);
    let continuation_budget = input
        .helper
        .header_budget(&input.texts.continuation, input.path);
    let member_self_contained: Vec<bool> = member_convs
        .iter()
        .map(|m| super::chunk_builder::entity_has_own_descriptor(ctx.group, m.entity_id))
        .collect();
    // A member whose own source spans more than one line carries an
    // implementation and can stand alone as a chunk; a single declaration
    // line cannot.
    let member_multiline: Vec<bool> = member_convs
        .iter()
        .map(|m| {
            ctx.group
                .entity_spans
                .get(&m.entity_id)
                .is_some_and(|span| span.end_position.row > span.start_position.row)
        })
        .collect();
    let member_groups = input.helper.group_members_by_header_budget(
        member_convs,
        first_budget,
        continuation_budget,
        input.path,
        &member_self_contained,
        &member_multiline,
    );
    let total = member_groups.len();

    let mut output = ChunkOutput::default();
    for (idx, members) in member_groups.into_iter().enumerate() {
        let continuation_for_block: String;
        let header_text = if idx == 0 {
            &input.texts.first
        } else if input.path == ChunkPath::Embedding {
            if let Some(first) = members.first() {
                if ctx.group.group_type == GroupType::MergedFragments {
                    // A merged group is named after its first fragment, which
                    // is a sibling and not the owner of the rest. Its block
                    // head must state what the block actually holds:
                    // qualifying the members with the group name asserts an
                    // ownership that does not exist and pulls every block of
                    // the group toward the first fragment.
                    let names: Vec<&str> = members.iter().map(|m| m.name.as_str()).collect();
                    continuation_for_block =
                        format!("{} {}.", first.kind.kind_label(), names.join(", "));
                } else {
                    let owner =
                        if ctx.group.name.is_empty() || ctx.group.name == first.name.as_str() {
                            String::new()
                        } else {
                            format!("{}.{}", ctx.group.name, first.name)
                        };
                    let display = if owner.is_empty() {
                        first.name.clone()
                    } else {
                        owner
                    };
                    // Continuation blocks name the members they hold. When
                    // the member carries its own descriptor (docstring), it
                    // is retrievable on its own: repeating the group topic
                    // ahead of it only adds a shared common component that
                    // pulls every block of the group toward the same
                    // direction and dilutes the member's own signal.
                    // Tiny members without a descriptor keep the group
                    // topic so they still carry file-level intent.
                    if super::chunk_builder::entity_has_own_descriptor(ctx.group, first.entity_id) {
                        continuation_for_block =
                            format!("{} {}.", first.kind.kind_label(), display);
                    } else {
                        // Continuation blocks keep the group topic sentence ahead of
                        // the member identity so split halves and tiny merged members
                        // still carry file-level intent in vector space.
                        continuation_for_block = format!(
                            "{} {}. {} {}.",
                            ctx.group.kind.kind_label(),
                            ctx.group.name,
                            first.kind.kind_label(),
                            display
                        );
                    }
                }
                &continuation_for_block
            } else {
                &input.texts.continuation
            }
        } else {
            &input.texts.continuation
        };
        // A synthetic head names the members of a continuation block. It is
        // decoration, not body: cap it in the path's unit so a group with
        // many members cannot push the block over the budget.
        let capped_head;
        let header_text = if idx == 0 {
            header_text
        } else {
            capped_head = truncate_to_cost(
                header_text,
                unit(input.path),
                super::chunk_builder::SYNTHETIC_HEAD_COST_CAP,
            )
            .to_string();
            capped_head.as_str()
        };
        // The first member group carries the group's own header text; every
        // later group starts with a synthetic head that names the members
        // this block holds. That head belongs to no group header, so the
        // header entity is left out and the head is attributed to the
        // block's first member instead.
        let header_entity_id = if idx == 0 {
            input.header_conv.as_ref().map(|c| c.entity_id)
        } else {
            None
        };
        let (combined_text, nl_boundaries, member_entity_ids) =
            super::boundary::assemble_with_boundaries(
                header_text,
                header_entity_id,
                &members,
                input.path,
            );
        // The BM25 path recomputes keywords per chunk from content entity
        // ids; the aggregate is only consumed by embedding chunks.
        let member_keywords = if input.path == ChunkPath::Embedding {
            ChunkBuilder::aggregate_embedding_keywords(&members)
        } else {
            Vec::new()
        };
        let chunk_input = super::chunker::ChunkInput {
            infra: ctx.infra,
            group: ctx.group,
            text: &combined_text,
            file_path: ctx.file_path,
            keywords: &member_keywords,
            path: input.path,
            strategy: if nl_boundaries.is_empty() {
                if input.path == ChunkPath::Bm25 {
                    SplitStrategy::for_bm25_group_type(ctx.group.group_type)
                } else {
                    SplitStrategy::for_group_type(ctx.group.group_type)
                }
            } else {
                SplitStrategy::ByNlEntityBoundaries
            },
            nl_boundaries: (!nl_boundaries.is_empty()).then_some(nl_boundaries.as_slice()),
            tracker: &*tracker,
            header_mode: Some(super::chunker::HeaderPathParams {
                tracker,
                header_entity_id: ctx.group.header_id,
                member_entity_ids: &member_entity_ids,
                chunk_index: idx,
                total_chunks: total,
                include_header_in_first_coverage: idx == 0,
            }),
        };
        let path_out = super::chunker::chunk_single_path(chunk_input)?;
        output.absorb(path_out);
    }

    finalize_group_path_chunks(&mut output.chunks);
    Ok(output)
}

#[cfg(test)]
fn assemble_combined_text(
    header_text: &str,
    members: &[ConversionResult],
    path: ChunkPath,
) -> (String, Vec<cce_types::entity::EntityId>) {
    let (text, _boundaries, ids) =
        super::boundary::assemble_with_boundaries(header_text, None, members, path);
    (text, ids)
}

fn finalize_group_path_chunks(chunks: &mut [ChunkedResult]) {
    let total = chunks.len();
    for (index, chunk) in chunks.iter_mut().enumerate() {
        chunk.chunk_id = format!("{}_{}_{}", chunk.source_group_id, chunk.path, index);
        chunk.chunk_index = index;
        chunk.total_chunks = total;
        if let Some(code) = chunk.metadata.code_metadata.as_mut() {
            code.is_fragment = total > 1;
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
            pre_injection_embedding: cfg.max_tokens,
            pre_injection_bm25: cfg.max_bm25_terms,
            emb_budget: cfg.budget(super::super::result::ChunkPath::Embedding, false, 0),
            bm25_budget: cfg.budget(super::super::result::ChunkPath::Bm25, false, 0),
            has_vocabulary: false,
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
            tracker,
            header_mode: Some(super::super::chunker::HeaderPathParams {
                tracker,
                header_entity_id: group.header_id,
                member_entity_ids,
                chunk_index,
                total_chunks: 1,
                include_header_in_first_coverage: chunk_index == 0,
            }),
        }
    }

    #[test]
    fn test_enforce_segment_retained_between_body_and_pre_injection() {
        let cfg = ChunkingConfig {
            max_bm25_terms: 150,
            ..Default::default()
        };
        let est = TokenEstimator::default();
        let mut bm25_budget = cfg.budget(ChunkPath::Bm25, false, 0);
        bm25_budget.body_limit = 50;
        let emb_budget = cfg.budget(ChunkPath::Embedding, false, 0);
        let splitter = TextSplitter::with_budgets(cfg.clone(), emb_budget, bm25_budget);
        let infra = super::super::chunker::ChunkInfrastructure {
            config: &cfg,
            estimator: &est,
            splitter: &splitter,
            pre_injection_embedding: emb_budget.pre_injection_limit,
            pre_injection_bm25: bm25_budget.pre_injection_limit,
            emb_budget,
            bm25_budget,
            has_vocabulary: false,
        };
        let group = test_group();
        let nlb: [NlEntityBoundary; 0] = [];
        // 50 < 140 <= 150: above the body limit but within the pre-injection
        // limit, matching the splitter's small over-limit retention.
        let text = n_words(140);
        let seg = ChunkSegment::new(
            ChunkBoundary::new(0, text.len(), SplitReason::MemberBoundary),
            text.clone(),
        );
        let result = super::super::segment_limit::enforce_segment_max_limit(
            vec![seg],
            ChunkPath::Bm25,
            &infra,
            &group,
            &nlb,
        )
        .expect("segment split must succeed");
        assert_eq!(result.len(), 1, "over-body segment must be kept");
        assert_eq!(result[0].text, text);
    }

    #[test]
    fn test_enforce_segment_over_pre_injection_resplit() {
        let cfg = ChunkingConfig {
            max_bm25_terms: 150,
            ..Default::default()
        };
        let est = TokenEstimator::default();
        let mut bm25_budget = cfg.budget(ChunkPath::Bm25, false, 0);
        bm25_budget.body_limit = 50;
        let emb_budget = cfg.budget(ChunkPath::Embedding, false, 0);
        let splitter = TextSplitter::with_budgets(cfg.clone(), emb_budget, bm25_budget);
        let infra = super::super::chunker::ChunkInfrastructure {
            config: &cfg,
            estimator: &est,
            splitter: &splitter,
            pre_injection_embedding: emb_budget.pre_injection_limit,
            pre_injection_bm25: bm25_budget.pre_injection_limit,
            emb_budget,
            bm25_budget,
            has_vocabulary: false,
        };
        let group = test_group();
        let nlb: [NlEntityBoundary; 0] = [];
        // 200 > 150: beyond the pre-injection limit, the segment is still
        // re-split and every piece comes back within the body limit.
        let text = n_words(200);
        let seg = ChunkSegment::new(
            ChunkBoundary::new(0, text.len(), SplitReason::MemberBoundary),
            text,
        );
        let result = super::super::segment_limit::enforce_segment_max_limit(
            vec![seg],
            ChunkPath::Bm25,
            &infra,
            &group,
            &nlb,
        )
        .expect("segment split must succeed");
        assert!(result.len() >= 2, "over-pre segment must be re-split");
        for s in &result {
            assert!(
                s.text.split_whitespace().count() <= 50,
                "piece exceeds the body limit"
            );
        }
    }

    #[test]
    fn test_enforce_segment_limit_passthrough() {
        let cfg = ChunkingConfig {
            max_bm25_terms: 100,
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
        )
        .expect("segment split must succeed");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].text, "small segment");
        assert_eq!(result[1].text, "another seg");
    }

    #[test]
    fn test_enforce_segment_limit_descends_entity_boundaries() {
        let cfg = ChunkingConfig {
            max_bm25_terms: 2,
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
        )
        .expect("segment split must succeed");
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
            max_bm25_terms: 4,
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
        )
        .expect("segment split must succeed");
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
            max_bm25_terms: 3,
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
        )
        .expect("segment split must succeed");
        assert!(result.len() >= 2);
        for s in &result {
            assert!(matches!(
                s.boundary.split_reason,
                SplitReason::HardLimit | SplitReason::TokenLimit
            ));
            assert!(s.boundary.entity_ids.contains(&EntityId(42)));
        }
        assert_eq!(result.first().unwrap().boundary.start_byte, 0);
    }

    #[test]
    fn test_enforce_segment_limit_rebases_offsets() {
        let cfg = ChunkingConfig {
            max_bm25_terms: 3,
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
        )
        .expect("segment split must succeed");
        assert_eq!(result.first().unwrap().boundary.start_byte, base_offset);
        assert_eq!(
            result.last().unwrap().boundary.end_byte,
            base_offset + text_len
        );
    }

    #[test]
    fn test_enforce_segment_limit_intersects_entity_ids() {
        let cfg = ChunkingConfig {
            max_bm25_terms: 4,
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
        )
        .expect("segment split must succeed");
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
            max_bm25_terms: 100,
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

        let results = super::super::chunker::chunk_single_path(input)
            .expect("chunk")
            .chunks;
        assert_eq!(results.len(), 1);
        assert!(!results[0].text.is_empty());
        assert_eq!(results[0].chunk_index, 0);
    }

    #[test]
    fn test_create_chunk_with_header_exceeds_limit_entity_bounds() {
        let base = ChunkingConfig {
            max_bm25_terms: 150,
            ..Default::default()
        };
        let est = TokenEstimator::default();
        let tracker = GroupTracker::new();
        let group = test_group();

        let header_text = "entity one";
        let long_member = "entity two ".to_string() + &n_words(200);
        let member_len = long_member.len();
        let member = ConversionResult {
            entity_id: EntityId(2),
            bm25_text: Some(long_member),
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
                end_byte: header_text.len() + 2 + member_len,
            },
        ];
        let (combined, member_ids) = assemble_combined_text(header_text, &members, ChunkPath::Bm25);
        let emb_dec = ChunkBuilder::decoration_allowance(&group, ChunkPath::Embedding);
        let bm25_dec = ChunkBuilder::decoration_allowance(&group, ChunkPath::Bm25);
        let emb_budget = base.budget(ChunkPath::Embedding, false, emb_dec);
        let bm25_budget = base.budget(ChunkPath::Bm25, false, bm25_dec);
        let splitter = TextSplitter::with_budgets(base.clone(), emb_budget, bm25_budget);
        let infra = super::super::chunker::ChunkInfrastructure {
            config: &base,
            estimator: &est,
            splitter: &splitter,
            pre_injection_embedding: emb_budget.pre_injection_limit,
            pre_injection_bm25: bm25_budget.pre_injection_limit,
            emb_budget,
            bm25_budget,
            has_vocabulary: false,
        };
        let input = header_input(&infra, &group, &combined, &nlb, &tracker, &member_ids, 0);

        let results = super::super::chunker::chunk_single_path(input)
            .expect("chunk")
            .chunks;
        assert!(results.len() > 1);
        assert!(results.iter().all(|r| !r.text.is_empty()));
    }

    #[test]
    fn test_create_chunk_with_header_exceeds_limit_no_boundaries() {
        let base = ChunkingConfig {
            max_bm25_terms: 150,
            ..Default::default()
        };
        let est = TokenEstimator::default();
        let tracker = GroupTracker::new();
        let group = test_group();
        let nlb: [NlEntityBoundary; 0] = [];
        let empty: [ConversionResult; 0] = [];
        let text = n_words(300);
        let (combined, member_ids) = assemble_combined_text(&text, &empty, ChunkPath::Bm25);
        let emb_dec = ChunkBuilder::decoration_allowance(&group, ChunkPath::Embedding);
        let bm25_dec = ChunkBuilder::decoration_allowance(&group, ChunkPath::Bm25);
        let emb_budget = base.budget(ChunkPath::Embedding, false, emb_dec);
        let bm25_budget = base.budget(ChunkPath::Bm25, false, bm25_dec);
        let splitter = TextSplitter::with_budgets(base.clone(), emb_budget, bm25_budget);
        let infra = super::super::chunker::ChunkInfrastructure {
            config: &base,
            estimator: &est,
            splitter: &splitter,
            pre_injection_embedding: emb_budget.pre_injection_limit,
            pre_injection_bm25: bm25_budget.pre_injection_limit,
            emb_budget,
            bm25_budget,
            has_vocabulary: false,
        };
        let input = header_input(&infra, &group, &combined, &nlb, &tracker, &member_ids, 0);

        let results = super::super::chunker::chunk_single_path(input)
            .expect("chunk")
            .chunks;
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

    fn method_group() -> EntityGroup {
        let mut group = test_group();
        group.name = CompactString::from("Engine");
        group.kind = EntityKind::Struct;
        group.header = Some(GroupedEntity::new(
            EntityId(0),
            EntityKind::Struct,
            "Engine".to_string(),
            "Engine".to_string(),
        ));
        let member = GroupedEntity::new(
            EntityId(1),
            EntityKind::Method,
            "handleHTTPRequest".to_string(),
            "handleHTTPRequest".to_string(),
        );
        group.members = [member].into_iter().collect();
        group.member_ids = [EntityId(1)].into_iter().collect();
        group
    }

    #[test]
    fn test_continuation_segment_repeats_entity_identity() {
        let group = method_group();
        let tracker = GroupTracker::new();
        let nlb: [NlEntityBoundary; 0] = [];
        let segs = vec![
            ChunkSegment::new(
                ChunkBoundary::new(0, 50, SplitReason::MemberBoundary)
                    .with_entity_ids(vec![EntityId(1)]),
                "method Engine.handleHTTPRequest.\n\nfirst half of the handler body".to_string(),
            ),
            ChunkSegment::new(
                ChunkBoundary::new(50, 100, SplitReason::SentenceBoundary)
                    .with_entity_ids(vec![EntityId(1)]),
                "second half of the handler body".to_string(),
            ),
        ];
        let results = from_segments_for_test(&tracker, segs, &group, ChunkPath::Embedding, &nlb);
        assert_eq!(results.len(), 2);
        assert!(
            results[1]
                .text
                .starts_with("method Engine.handleHTTPRequest.\n\n"),
            "continuation must repeat the covered entity identity, got {:?}",
            results[1].text
        );
        assert!(
            !results[1].text.contains("fragment"),
            "fragment counters carry no retrieval signal, got {:?}",
            results[1].text
        );
        assert!(
            !results[1].text.contains("continuation"),
            "continuation markers carry no retrieval signal, got {:?}",
            results[1].text
        );
    }

    #[test]
    fn test_continuation_segment_skips_identity_on_entity_boundary() {
        let group = method_group();
        let tracker = GroupTracker::new();
        let nlb: [NlEntityBoundary; 0] = [];
        let aligned = "Engine.handleHTTPRequest(). method Engine.handleHTTPRequest (c: *Context).\n\ntail body";
        let segs = vec![
            ChunkSegment::new(
                ChunkBoundary::new(0, 50, SplitReason::MemberBoundary)
                    .with_entity_ids(vec![EntityId(1)]),
                "method Engine.handleHTTPRequest.\n\nfirst half of the handler body".to_string(),
            ),
            ChunkSegment::new(
                ChunkBoundary::new(50, 100, SplitReason::MemberBoundary)
                    .with_entity_ids(vec![EntityId(1)]),
                aligned.to_string(),
            ),
        ];
        let results = from_segments_for_test(&tracker, segs, &group, ChunkPath::Embedding, &nlb);
        assert_eq!(results.len(), 2);
        assert_eq!(
            results[1].text, aligned,
            "boundary-aligned segments need no repeated identity"
        );
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

    fn merged_pair_group() -> EntityGroup {
        let mut group = test_group();
        group.group_id = CompactString::from("merged_pair");
        group.group_type = GroupType::MergedFragments;
        group.name = CompactString::from("read_fn");
        group.header = Some(GroupedEntity::new(
            EntityId(10),
            EntityKind::Function,
            "read_fn".to_string(),
            "read_fn".to_string(),
        ));
        group.header_id = Some(EntityId(10));
        let member = GroupedEntity::new(
            EntityId(11),
            EntityKind::Function,
            "write_fn".to_string(),
            "write_fn".to_string(),
        );
        group.members = [member].into_iter().collect();
        group.member_ids = [EntityId(11)].into_iter().collect();
        group.entity_spans = [
            (
                EntityId(10),
                Span {
                    start_byte: 0,
                    end_byte: 100,
                    ..Default::default()
                },
            ),
            (
                EntityId(11),
                Span {
                    start_byte: 100,
                    end_byte: 200,
                    ..Default::default()
                },
            ),
        ]
        .into_iter()
        .collect();
        group
    }

    fn long_body(marker: &str, words: usize) -> String {
        let body: Vec<String> = (0..words).map(|i| format!("{marker}{i}")).collect();
        format!("function mod.{marker} (x: int) -> int.\n{}", body.join(" "))
    }

    #[test]
    fn test_merged_fragments_keep_full_header_body_on_embedding_path() {
        let cfg = ChunkingConfig {
            max_tokens: 2048,
            ..Default::default()
        };
        let est = TokenEstimator::default();
        let splitter = TextSplitter::new(cfg.clone());
        let infra = infra(&cfg, &est, &splitter);
        let mut tracker = GroupTracker::new();
        let group = merged_pair_group();
        let header_conv = ConversionResult::embedding_only(
            EntityId(10),
            EntityKind::Function,
            "read_fn".to_string(),
            "test.c".to_string(),
            long_body("readbody", 200),
        );
        let member_conv = ConversionResult::embedding_only(
            EntityId(11),
            EntityKind::Function,
            "write_fn".to_string(),
            "test.c".to_string(),
            long_body("writebody", 100),
        );
        let conversions = crate::ast_to_nl::converter::GroupConversions {
            group: group.clone(),
            header_conversion: Some(header_conv),
            member_conversions: vec![member_conv],
        };
        let out = super::chunk_group_with_conversions(
            &infra,
            &mut tracker,
            &group,
            &conversions,
            "test.c",
        )
        .expect("chunk");
        assert!(!out.chunks.is_empty());
        let joined: String = out.chunks.iter().map(|c| c.text.as_str()).collect();
        assert!(
            joined.contains("readbody199"),
            "merged header body must survive compacting, got {:?}",
            joined
        );
        assert!(
            joined.contains("writebody99"),
            "merged member body must survive, got {:?}",
            joined
        );
    }
}
