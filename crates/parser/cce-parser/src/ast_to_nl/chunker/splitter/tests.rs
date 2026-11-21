use cce_config::modules::ChunkingConfig;
use cce_types::entity::EntityId;

use crate::grouper::types::EntityGroup;

use super::super::boundary::{ChunkBoundary, SplitReason};
use super::super::result::ChunkPath;
use super::super::strategy::SplitStrategy;
use super::*;

fn split_with_limits(text: &str, level: BoundaryLevel, max_tokens: usize) -> Vec<ChunkBoundary> {
    let config = ChunkingConfig {
        max_tokens,
        ..Default::default()
    };
    let splitter = TextSplitter::new(config);
    splitter
        .split_range_at(
            text,
            &EntityGroup::default(),
            ChunkPath::Embedding,
            None,
            0..text.len(),
            level,
            &[],
        )
        .expect("test splitter input must be valid")
}

fn split_at_level(
    text: &str,
    level: BoundaryLevel,
    path: ChunkPath,
    config: ChunkingConfig,
) -> Vec<ChunkSegment> {
    let splitter = TextSplitter::new(config);
    let boundaries = splitter
        .split_range_at(
            text,
            &EntityGroup::default(),
            path,
            None,
            0..text.len(),
            level,
            &[],
        )
        .expect("test splitter input must be valid");
    let segments = splitter
        .boundaries_to_segments(boundaries, text)
        .expect("test partition must be valid");
    splitter.maybe_merge_segments(&EntityGroup::default(), segments, path)
}

fn assert_partition(text: &str, boundaries: &[ChunkBoundary]) {
    assert!(!boundaries.is_empty(), "no boundaries produced");
    assert_eq!(boundaries.first().unwrap().start_byte, 0);
    assert_eq!(boundaries.last().unwrap().end_byte, text.len());
    let mut prev = 0;
    for b in boundaries {
        assert_eq!(b.start_byte, prev, "partition gap/overlap");
        assert!(b.start_byte < b.end_byte, "zero-length boundary");
        assert!(text.is_char_boundary(b.start_byte));
        assert!(text.is_char_boundary(b.end_byte));
        prev = b.end_byte;
    }
    assert_eq!(prev, text.len());
}

#[test]
fn test_boundaries_to_segments_utf8_mid_char() {
    let config = ChunkingConfig::default();
    let splitter = TextSplitter::new(config);
    let text = "Hello 世界 World";
    let boundaries =
        vec![ChunkBoundary::new(0, 7, SplitReason::SentenceBoundary).with_token_count(2)];
    // A non-char-aligned boundary violates the splitter contract: it is
    // reported as an error in every build profile, never silently repaired.
    assert!(splitter.boundaries_to_segments(boundaries, text).is_err());
}

#[test]
fn test_boundaries_to_segments_unicode_only() {
    let config = ChunkingConfig::default();
    let splitter = TextSplitter::new(config);
    let text = "世界你好";
    let boundaries =
        vec![ChunkBoundary::new(0, 12, SplitReason::SentenceBoundary).with_token_count(4)];
    let segments = splitter
        .boundaries_to_segments(boundaries, text)
        .expect("test partition must be valid");
    assert_eq!(segments.len(), 1);
    assert_eq!(segments[0].text, text);
}

#[test]
fn test_split_range_by_sentences_stops_at_tokens() {
    let text = "A B C D E F G H";
    let boundaries = split_with_limits(text, BoundaryLevel::Sentences, 2);
    assert!(boundaries.len() > 1);
    assert_partition(text, &boundaries);
}

#[test]
fn test_split_range_full_chain_from_nested_groups() {
    let text = "no nested groups no members no sentence ends no newlines no paragraphs";
    let boundaries = split_with_limits(text, BoundaryLevel::NestedGroups, 3);
    assert!(boundaries.len() > 1);
    assert_partition(text, &boundaries);
}

#[test]
fn test_split_range_full_chain_from_members() {
    let text = "no entity spans no sentences no lines no paras";
    let boundaries = split_with_limits(text, BoundaryLevel::Entities, 3);
    assert!(boundaries.len() > 1);
    assert_partition(text, &boundaries);
}

#[test]
fn test_split_range_from_sentences_to_tokens() {
    let text = "word word word word word word word";
    let boundaries = split_with_limits(text, BoundaryLevel::Sentences, 3);
    assert!(boundaries.len() > 1);
    assert_partition(text, &boundaries);
}

#[test]
fn test_split_range_with_text_within_other_limits() {
    let config = ChunkingConfig {
        max_tokens: 512,
        max_bm25_terms: 200,
        ..Default::default()
    };
    let splitter = TextSplitter::new(config);
    let text = "A B C D E F G H I J";
    let boundaries = splitter.split_range(
        text,
        &EntityGroup::default(),
        SplitStrategy::ByParagraphs,
        ChunkPath::Bm25,
        None,
        0..text.len(),
    );
    assert_eq!(boundaries.len(), 1);
    assert_eq!(boundaries[0].start_byte, 0);
    assert_eq!(boundaries[0].end_byte, text.len());
}

#[test]
fn test_split_range_no_boundaries_with_nl_entity_boundaries() {
    let config = ChunkingConfig {
        max_tokens: 3,
        ..Default::default()
    };
    let splitter = TextSplitter::new(config);
    let text = "aaa bbb ccc ddd eee fff";
    let boundaries = splitter.split_range(
        text,
        &EntityGroup::default(),
        SplitStrategy::ByNlEntityBoundaries,
        ChunkPath::Embedding,
        Some(&[]),
        0..text.len(),
    );
    assert!(boundaries.len() > 1);
    assert_partition(text, &boundaries);
}

#[test]
fn test_split_range_entity_boundaries_attributed_through_recursion() {
    let config = ChunkingConfig {
        max_bm25_terms: 2,
        ..Default::default()
    };
    let splitter = TextSplitter::new(config);
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
    let boundaries = splitter.split_range(
        text,
        &EntityGroup::default(),
        SplitStrategy::ByNlEntityBoundaries,
        ChunkPath::Bm25,
        Some(&nlb),
        0..text.len(),
    );
    assert!(boundaries.len() > 1);
    assert_partition(text, &boundaries);
    // Pieces overlapping an entity span inherit that entity's id through
    // the recursion (id 1 lives in the [0, 14) region).
    for b in boundaries.iter().filter(|b| b.end_byte <= 14) {
        assert!(
            b.entity_ids.contains(&EntityId(1)),
            "piece [{}, {}) lost inherited entity id",
            b.start_byte,
            b.end_byte
        );
    }
}

#[test]
fn test_split_range_single_paragraph_text_by_paragraphs() {
    let config = ChunkingConfig {
        max_tokens: 512,
        ..Default::default()
    };
    let splitter = TextSplitter::new(config);
    let text = "Short text.";
    let boundaries = splitter.split_range(
        text,
        &EntityGroup::default(),
        SplitStrategy::ByParagraphs,
        ChunkPath::Embedding,
        None,
        0..text.len(),
    );
    assert_eq!(boundaries.len(), 1);
    assert_eq!(boundaries[0].start_byte, 0);
    assert_eq!(boundaries[0].end_byte, text.len());
}

#[test]
fn test_split_range_empty_text() {
    let config = ChunkingConfig::default();
    let splitter = TextSplitter::new(config);
    let boundaries = splitter.split_range(
        "",
        &EntityGroup::default(),
        SplitStrategy::ByParagraphs,
        ChunkPath::Embedding,
        None,
        0..0,
    );
    assert!(boundaries.is_empty());
}

#[test]
fn test_split_range_oversized_pieces_all_within_limit() {
    let config = ChunkingConfig {
        max_bm25_terms: 4,
        ..Default::default()
    };
    let splitter = TextSplitter::new(config);
    let text = "one two three four five six seven eight nine ten";
    let boundaries = splitter
        .split_range_at(
            text,
            &EntityGroup::default(),
            ChunkPath::Bm25,
            None,
            0..text.len(),
            BoundaryLevel::Sentences,
            &[],
        )
        .expect("test splitter input must be valid");
    assert!(boundaries.len() >= 2);
    assert_partition(text, &boundaries);
    for b in &boundaries {
        assert!(
            text[b.start_byte..b.end_byte].split_whitespace().count() <= 4,
            "piece exceeds limit"
        );
    }
}

#[test]
fn test_split_range_oversized_paragraph_subdivides_by_hard_cut_on_bm25() {
    let config = ChunkingConfig {
        max_bm25_terms: 3,
        ..Default::default()
    };
    let splitter = TextSplitter::new(config);
    // The BM25 path subdivides oversized paragraphs at line boundaries
    // before the hard token cut so method signatures stay intact; only
    // line-oversize pieces use the hard cut.
    let text = "word1 word2 word3 word4 word5\nword6 word7\n\nword8 word9 word10 word11 word12\nword13 word14";
    let boundaries = splitter.split_range(
        text,
        &EntityGroup::default(),
        SplitStrategy::ByParagraphs,
        ChunkPath::Bm25,
        None,
        0..text.len(),
    );
    assert!(boundaries.len() >= 4);
    assert_partition(text, &boundaries);
    assert!(
        boundaries
            .iter()
            .all(|b| b.split_reason != SplitReason::SentenceBoundary),
        "BM25 must not subdivide into sentences"
    );
    for b in &boundaries {
        let words = text[b.start_byte..b.end_byte].split_whitespace().count();
        assert!(words <= 3, "piece of {} words exceeds limit", words);
    }
}

#[test]
fn test_bm25_single_entity_piece_stays_whole_even_beyond_tolerance() {
    // Single-entity BM25 stays whole even beyond tolerance: splitting it
    // could land a fragment on a blank-only line with no source coverage.
    // The hard limit truncates an over-bound whole to the extended bound.
    let config = ChunkingConfig {
        max_bm25_terms: 3,
        ..Default::default()
    };
    let splitter = TextSplitter::new(config);
    let text = "word1 word2 word3 word4 word5 word6 word7 word8 word9 word10 word11 word12";
    let nlb = vec![NlEntityBoundary {
        entity_id: EntityId(1),
        start_byte: 0,
        end_byte: text.len(),
    }];
    let boundaries = splitter.split_range(
        text,
        &EntityGroup::default(),
        SplitStrategy::ByNlEntityBoundaries,
        ChunkPath::Bm25,
        Some(&nlb),
        0..text.len(),
    );
    assert_eq!(
        boundaries.len(),
        1,
        "single-entity BM25 piece stays whole to avoid blank-only fragments: {boundaries:?}"
    );
    assert_partition(text, &boundaries);
}

#[test]
fn test_bm25_single_entity_piece_stays_whole_within_tolerance() {
    // Within three times the body budget the piece stays whole so one
    // function keeps one index entry and the reported span stays whole.
    let config = ChunkingConfig {
        max_bm25_terms: 3,
        ..Default::default()
    };
    let splitter = TextSplitter::new(config);
    let text = "word1 word2 word3 word4 word5 word6 word7 word8";
    let nlb = vec![NlEntityBoundary {
        entity_id: EntityId(1),
        start_byte: 0,
        end_byte: text.len(),
    }];
    let boundaries = splitter.split_range(
        text,
        &EntityGroup::default(),
        SplitStrategy::ByNlEntityBoundaries,
        ChunkPath::Bm25,
        Some(&nlb),
        0..text.len(),
    );
    assert_eq!(
        boundaries.len(),
        1,
        "single-entity BM25 piece stays whole within tolerance: {boundaries:?}"
    );
    assert_partition(text, &boundaries);
}

#[test]
fn test_embedding_single_entity_piece_still_subdivides() {
    // The embedding path keeps hard model limits: the same
    // over-budget single-entity piece subdivides there.
    let config = ChunkingConfig {
        max_tokens: 3,
        ..Default::default()
    };
    let splitter = TextSplitter::new(config);
    let text = "word1 word2 word3 word4 word5 word6 word7 word8";
    let nlb = vec![NlEntityBoundary {
        entity_id: EntityId(1),
        start_byte: 0,
        end_byte: text.len(),
    }];
    let boundaries = splitter.split_range(
        text,
        &EntityGroup::default(),
        SplitStrategy::ByNlEntityBoundaries,
        ChunkPath::Embedding,
        Some(&nlb),
        0..text.len(),
    );
    assert!(
        boundaries.len() > 1,
        "embedding path must still subdivide: {boundaries:?}"
    );
    assert_partition(text, &boundaries);
}

#[test]
fn test_bm25_module_entity_still_subdivides() {
    // Module entities keep their document-section splitting: their
    // sections are independent prose, not one callable whose parts
    // must stay together.
    use cce_types::entity::{EntityKind, GroupedEntity};

    let config = ChunkingConfig {
        max_bm25_terms: 3,
        ..Default::default()
    };
    let splitter = TextSplitter::new(config);
    let header = GroupedEntity::new(
        EntityId(1),
        EntityKind::Module,
        "m".to_string(),
        "m".to_string(),
    );
    let group = EntityGroup {
        header: Some(header),
        header_id: Some(EntityId(1)),
        ..Default::default()
    };
    let text = "word1 word2 word3 word4 word5 word6 word7 word8";
    let nlb = vec![NlEntityBoundary {
        entity_id: EntityId(1),
        start_byte: 0,
        end_byte: text.len(),
    }];
    let boundaries = splitter.split_range(
        text,
        &group,
        SplitStrategy::ByNlEntityBoundaries,
        ChunkPath::Bm25,
        Some(&nlb),
        0..text.len(),
    );
    assert!(
        boundaries.len() > 1,
        "module entities must still subdivide: {boundaries:?}"
    );
    assert_partition(text, &boundaries);
}

#[test]
fn test_split_range_unicode_text_no_zero_length() {
    let config = ChunkingConfig {
        max_tokens: 2,
        ..Default::default()
    };
    let splitter = TextSplitter::new(config);
    let text = "这是一段用于测试分块器不变量保证的中文文本，包含一些标点符号。以及换行\n测试。";
    let boundaries = splitter
        .split_range_at(
            text,
            &EntityGroup::default(),
            ChunkPath::Embedding,
            None,
            0..text.len(),
            BoundaryLevel::Sentences,
            &[],
        )
        .expect("test splitter input must be valid");
    assert!(boundaries.len() >= 2);
    assert_partition(text, &boundaries);
}

/// Deterministic pseudo-random text generator over a mixed alphabet
/// (ASCII words, no-punctuation long lines, blank lines, CJK, pure
/// whitespace) used to exercise the split invariants.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }

    fn range(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn gen_text(&mut self, max_len: usize) -> String {
        let alphabets: &[&str] = &[
            "word1 word2 word3 word4 word5 word6 word7 word8 word9 word10 ",
            "a",
            "b ",
            "\n",
            "\n\n",
            "x.y z! q? r。 s。 ",
            "句段一。句段二。句段三。",
            "   ",
            "\t",
            "no_punctuation_long_line_without_any_spaces_abcdefghijklmnopqrstuvwxyz ",
            "λ=1, δ=2; Ω! ",
        ];
        let len = self.range(max_len) + 1;
        let mut s = String::new();
        while s.len() < len {
            s.push_str(alphabets[self.range(alphabets.len())]);
        }
        cce_utils::truncate_string_to_byte_len(&mut s, len);
        s
    }
}

#[test]
fn test_split_range_random_text_invariants() {
    let levels = [
        BoundaryLevel::NestedGroups,
        BoundaryLevel::Entities,
        BoundaryLevel::Sentences,
        BoundaryLevel::Paragraphs,
        BoundaryLevel::Tokens,
    ];
    let mut rng = Lcg(0x5eed_c0de_cafe_f00d);
    for round in 0..64 {
        let text = rng.gen_text(600);
        let path = if round % 2 == 0 {
            ChunkPath::Bm25
        } else {
            ChunkPath::Embedding
        };
        let limit = 3 + rng.range(10);
        let cfg = match path {
            ChunkPath::Bm25 => ChunkingConfig {
                max_bm25_terms: limit,
                ..Default::default()
            },
            ChunkPath::Embedding => ChunkingConfig {
                max_tokens: limit,
                ..Default::default()
            },
        };
        let level = levels[rng.range(levels.len())];
        let splitter = TextSplitter::new(cfg);
        let boundaries = splitter
            .split_range_at(
                &text,
                &EntityGroup::default(),
                path,
                None,
                0..text.len(),
                level,
                &[],
            )
            .expect("test splitter input must be valid");

        // Invariants: monotonic + contiguous, non-zero, full coverage,
        // char-aligned, and every piece within the path limit.
        assert!(!boundaries.is_empty(), "round {round}: empty partition");
        let mut prev = 0;
        for b in &boundaries {
            assert_eq!(
                b.start_byte, prev,
                "round {round}: level={level:?} path={path:?} partition gap/overlap"
            );
            assert!(b.start_byte < b.end_byte, "round {round}: zero-length");
            assert!(text.is_char_boundary(b.start_byte));
            assert!(text.is_char_boundary(b.end_byte));
            let piece = &text[b.start_byte..b.end_byte];
            if !text.trim().is_empty() && !piece.trim().is_empty() {
                let piece_cost = cost(piece, path);
                assert!(
                    piece_cost <= limit,
                    "round {round}: level={level:?} path={path:?} text={text:?} piece of {piece_cost} exceeds limit {limit}: {:?}",
                    piece
                );
            }
            prev = b.end_byte;
        }
        assert_eq!(prev, text.len(), "round {round}: coverage");
    }
}

#[test]
fn test_single_piece_between_body_and_pre_injection_subdivided() {
    let config = ChunkingConfig {
        max_bm25_terms: 150,
        ..Default::default()
    };
    // A real group with vocabulary and decoration shrinks the body limit
    // below the pre-injection limit; reproduce that shape directly.
    let mut bm25_budget = config.budget(ChunkPath::Bm25, false, 0);
    bm25_budget.body_limit = 50;
    let emb_budget = config.budget(ChunkPath::Embedding, false, 0);
    let splitter = TextSplitter::with_budgets(config, emb_budget, bm25_budget);
    let text = words(140);
    let boundaries = splitter
        .split_range_at(
            &text,
            &EntityGroup::default(),
            ChunkPath::Bm25,
            None,
            0..text.len(),
            BoundaryLevel::Paragraphs,
            &[],
        )
        .expect("test splitter input must be valid");
    // 50 < 140 <= 150: above the body limit, so the single piece is
    // subdivided even though it fits the pre-injection budget. Keeping it
    // whole would leave no room for the decoration appended later.
    assert!(
        boundaries.len() >= 2,
        "over-body piece must be subdivided, got {}",
        boundaries.len()
    );
    assert_partition(&text, &boundaries);
    for b in &boundaries {
        assert!(
            text[b.start_byte..b.end_byte].split_whitespace().count() <= 50,
            "piece exceeds the body limit"
        );
    }
}

#[test]
fn test_single_piece_over_pre_injection_subdivided() {
    let config = ChunkingConfig {
        max_bm25_terms: 150,
        ..Default::default()
    };
    let mut bm25_budget = config.budget(ChunkPath::Bm25, false, 0);
    bm25_budget.body_limit = 50;
    let emb_budget = config.budget(ChunkPath::Embedding, false, 0);
    let splitter = TextSplitter::with_budgets(config, emb_budget, bm25_budget);
    let text = words(200);
    let boundaries = splitter
        .split_range_at(
            &text,
            &EntityGroup::default(),
            ChunkPath::Bm25,
            None,
            0..text.len(),
            BoundaryLevel::Paragraphs,
            &[],
        )
        .expect("test splitter input must be valid");
    // 200 > 150: beyond the pre-injection limit, subdivision still runs
    // and every piece is cut to the body limit.
    assert!(boundaries.len() >= 2, "over-pre piece must be subdivided");
    assert_partition(&text, &boundaries);
    for b in &boundaries {
        assert!(
            text[b.start_byte..b.end_byte].split_whitespace().count() <= 50,
            "piece exceeds the body limit"
        );
    }
}

#[test]
fn test_entity_subdivision_routes_bm25_to_entities_embedding_to_sentences() {
    // Two entities: BM25 stops at the entity level before descending
    // to paragraphs, while over-body single-entity pieces subdivide
    // further. Paragraph-level BM25 splitting without entity data is covered by the oversized-paragraph test.
    // Each half stays over the embedding budget so the embedding path
    // still descends to sentences.
    let text = "alpha beta gamma. delta epsilon zeta.\n\neta theta iota kappa. ".repeat(3);
    let text = text.as_str();
    let mid = text.len() / 2;
    let nlb = vec![
        NlEntityBoundary {
            entity_id: EntityId(1),
            start_byte: 0,
            end_byte: mid,
        },
        NlEntityBoundary {
            entity_id: EntityId(2),
            start_byte: mid,
            end_byte: text.len(),
        },
    ];

    let bm25_config = ChunkingConfig {
        max_bm25_terms: 5,
        ..Default::default()
    };
    let bm25_splitter = TextSplitter::new(bm25_config);
    let boundaries = bm25_splitter
        .split_range_at(
            text,
            &EntityGroup::default(),
            ChunkPath::Bm25,
            Some(&nlb),
            0..text.len(),
            BoundaryLevel::Entities,
            &[],
        )
        .expect("test splitter input must be valid");
    assert!(boundaries.len() >= 2);
    assert_partition(text, &boundaries);
    for b in &boundaries {
        assert_ne!(
            b.split_reason,
            SplitReason::SentenceBoundary,
            "BM25 subdivision must skip the sentence level"
        );
    }

    let emb_config = ChunkingConfig {
        max_tokens: 10,
        ..Default::default()
    };
    let emb_splitter = TextSplitter::new(emb_config);
    let boundaries = emb_splitter
        .split_range_at(
            text,
            &EntityGroup::default(),
            ChunkPath::Embedding,
            Some(&nlb),
            0..text.len(),
            BoundaryLevel::Entities,
            &[],
        )
        .expect("test splitter input must be valid");
    assert!(boundaries.len() >= 2);
    assert_partition(text, &boundaries);
    assert!(
        boundaries
            .iter()
            .any(|b| b.split_reason == SplitReason::SentenceBoundary),
        "embedding subdivision must still use sentence boundaries"
    );
}

fn words(n: usize) -> String {
    let mut s = (0..n)
        .map(|i| format!("w{i}"))
        .collect::<Vec<_>>()
        .join(" ");
    s.push(' ');
    s
}

fn segment(text: &str, start: usize, end: usize) -> ChunkSegment {
    ChunkSegment::new(
        ChunkBoundary::new(start, end, SplitReason::MemberBoundary),
        text.to_string(),
    )
}

#[test]
fn test_merge_small_chunks_both_below_threshold() {
    let config = ChunkingConfig {
        max_bm25_terms: 512,
        min_chunk_bm25_terms: 60,
        ..Default::default()
    };
    let splitter = TextSplitter::new(config);

    let segments = vec![segment(&words(30), 0, 60), segment(&words(30), 60, 120)];

    let merged = splitter.merge_small_segments(&EntityGroup::default(), segments, ChunkPath::Bm25);

    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].boundary.token_count, 60);
}

#[test]
fn test_merge_small_chunks_one_above_threshold() {
    let config = ChunkingConfig {
        max_bm25_terms: 512,
        min_chunk_bm25_terms: 60,
        ..Default::default()
    };
    let splitter = TextSplitter::new(config);

    // Segment with 50 words (below 60 threshold) should be merged
    // into the adjacent 150-word chunk.
    let segments = vec![segment(&words(50), 0, 100), segment(&words(150), 100, 300)];

    let merged = splitter.merge_small_segments(&EntityGroup::default(), segments, ChunkPath::Bm25);

    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].boundary.token_count, 200);
}

#[test]
fn test_merge_small_chunks_assimilates_small_fragments() {
    let config = ChunkingConfig {
        max_bm25_terms: 512,
        min_chunk_bm25_terms: 60,
        ..Default::default()
    };
    let splitter = TextSplitter::new(config);

    // [40w, 200w, 30w]: the leading 40w chunk is below min and absorbs the
    // 200w neighbor; the trailing 30w fragment is below min so it is
    // absorbed by its large left neighbor (bidirectional merge).
    let segments = vec![
        segment(&words(40), 0, 50),
        segment(&words(200), 50, 250),
        segment(&words(30), 250, 300),
    ];

    let merged = splitter.merge_small_segments(&EntityGroup::default(), segments, ChunkPath::Bm25);

    assert_eq!(merged.len(), 1);
    assert_eq!(cost(&merged[0].text, ChunkPath::Bm25), 270);
}

#[test]
fn test_merge_small_chunks_cascading() {
    let config = ChunkingConfig {
        max_bm25_terms: 512,
        min_chunk_bm25_terms: 60,
        ..Default::default()
    };
    let splitter = TextSplitter::new(config);

    // Bidirectional merge absorbs trailing small fragments as well,
    // so all three undersized pieces collapse into one chunk.
    let segments = vec![
        segment(&words(30), 0, 50),
        segment(&words(30), 50, 100),
        segment(&words(30), 100, 150),
    ];

    let merged = splitter.merge_small_segments(&EntityGroup::default(), segments, ChunkPath::Bm25);

    assert_eq!(merged.len(), 1);
    assert_eq!(cost(&merged[0].text, ChunkPath::Bm25), 90);
}

#[test]
fn test_merge_small_chunks_embedding_below_threshold() {
    let config = ChunkingConfig {
        max_tokens: 512,
        min_chunk_tokens: 10,
        ..Default::default()
    };
    let splitter = TextSplitter::new(config);
    let segments = vec![segment(&words(5), 0, 10), segment(&words(5), 10, 20)];
    let merged =
        splitter.merge_small_segments(&EntityGroup::default(), segments, ChunkPath::Embedding);
    assert_eq!(merged.len(), 1);
}

#[test]
fn test_merge_small_chunks_embedding_one_above() {
    let config = ChunkingConfig {
        max_tokens: 512,
        min_chunk_tokens: 50,
        ..Default::default()
    };
    let splitter = TextSplitter::new(config);
    let segments = vec![segment(&words(20), 0, 30), segment(&words(100), 30, 200)];
    let merged =
        splitter.merge_small_segments(&EntityGroup::default(), segments, ChunkPath::Embedding);
    assert_eq!(merged.len(), 1);
}

#[test]
fn test_same_unit_fragments_pack_to_the_ceiling() {
    let config = ChunkingConfig {
        max_tokens: 300,
        min_chunk_tokens: 100,
        glossary_reserve_ratio: 0.0,
        ..Default::default()
    };
    let splitter = TextSplitter::new(config);
    // Six fragments of one entity, each far above the min threshold:
    // they still pack together because they are parts of one unit.
    let segments: Vec<ChunkSegment> = (0..6)
        .map(|i| {
            ChunkSegment::new(
                ChunkBoundary::new(i * 60, (i + 1) * 60, SplitReason::MemberBoundary)
                    .with_entity_ids(vec![EntityId(1)]),
                words(60),
            )
        })
        .collect();

    let merged =
        splitter.merge_small_segments(&EntityGroup::default(), segments, ChunkPath::Embedding);

    let piece = cost(&words(60), ChunkPath::Embedding);
    assert_eq!(merged.len(), 2);
    assert!(
        cost(&merged[0].text, ChunkPath::Embedding) > piece * 3,
        "one entity's fragments must pack together, got {} pieces",
        merged.len()
    );
    for segment in &merged {
        assert!(
            cost(&segment.text, ChunkPath::Embedding) <= 300,
            "a packed fragment must stay within the ceiling"
        );
    }
}

#[test]
fn test_same_unit_fragments_never_exceed_the_ceiling() {
    let config = ChunkingConfig {
        max_tokens: 300,
        min_chunk_tokens: 100,
        glossary_reserve_ratio: 0.0,
        ..Default::default()
    };
    let splitter = TextSplitter::new(config);
    // Fragments of one entity still stop at the ceiling: merging them
    // would produce a chunk the path's limit rejects.
    let segments = vec![
        ChunkSegment::new(
            ChunkBoundary::new(0, 170, SplitReason::MemberBoundary)
                .with_entity_ids(vec![EntityId(1)]),
            words(170),
        ),
        ChunkSegment::new(
            ChunkBoundary::new(170, 340, SplitReason::MemberBoundary)
                .with_entity_ids(vec![EntityId(1)]),
            words(170),
        ),
    ];

    let merged =
        splitter.merge_small_segments(&EntityGroup::default(), segments, ChunkPath::Embedding);

    assert_eq!(merged.len(), 2);
}

#[test]
fn test_merge_small_chunks_embedding_trailing() {
    let config = ChunkingConfig {
        max_tokens: 512,
        min_chunk_tokens: 50,
        ..Default::default()
    };
    let splitter = TextSplitter::new(config);
    // Bidirectional merge: the trailing small fragment is absorbed by
    // its large left neighbor.
    let segments = vec![segment(&words(100), 0, 150), segment(&words(20), 150, 180)];
    let merged =
        splitter.merge_small_segments(&EntityGroup::default(), segments, ChunkPath::Embedding);
    assert_eq!(merged.len(), 1);
}

#[test]
fn test_entities_call_related_detects_edge() {
    use cce_types::entity::{EntityKind, GroupedEntity};

    let mut caller = GroupedEntity::new(
        EntityId(1),
        EntityKind::Method,
        "ShouldBindWith".to_string(),
        "ShouldBindWith".to_string(),
    );
    caller.metadata.insert(
        cce_types::entity::meta_keys::CALL_PATHS.to_string(),
        "BodyWith".to_string(),
    );
    let callee = GroupedEntity::new(
        EntityId(2),
        EntityKind::Method,
        "BodyWith".to_string(),
        "BodyWith".to_string(),
    );
    let other = GroupedEntity::new(
        EntityId(3),
        EntityKind::Method,
        "Unrelated".to_string(),
        "Unrelated".to_string(),
    );
    let group = EntityGroup {
        members: [caller, callee, other].into_iter().collect(),
        ..EntityGroup::default()
    };
    assert!(entities_call_related(
        &group,
        &[EntityId(1)],
        &[EntityId(2)]
    ));
    assert!(entities_call_related(
        &group,
        &[EntityId(2)],
        &[EntityId(1)]
    ));
    assert!(!entities_call_related(
        &group,
        &[EntityId(1)],
        &[EntityId(3)]
    ));
    assert!(!entities_call_related(
        &group,
        &[EntityId(2)],
        &[EntityId(3)]
    ));
}

#[test]
fn test_call_related_neighbours_merge_up_to_pre_injection() {
    use cce_config::modules::ChunkBudget;
    use cce_types::entity::{EntityKind, GroupedEntity};

    // Merge ceiling sits below the pre-injection ceiling: an undersized
    // fragment plus its call neighbour combine past the merge ceiling
    // but inside the pre-injection room.
    let config = ChunkingConfig {
        max_bm25_terms: 1000,
        min_chunk_bm25_terms: 60,
        ..Default::default()
    };
    let bm25_budget = ChunkBudget {
        unit: cce_text::ChunkUnit::Terms,
        hard_limit: 1000,
        reserve: 0,
        decoration_cost: 0,
        pre_injection_limit: 300,
        body_limit: 1000,
        min_limit: 60,
        merge_limit: 100,
    };
    let emb_budget = config.budget(cce_types::ChunkPath::Embedding, false, 0);
    let splitter = TextSplitter::with_budgets(config, emb_budget, bm25_budget);
    let mut caller = GroupedEntity::new(
        EntityId(1),
        EntityKind::Method,
        "ShouldBindWith".to_string(),
        "ShouldBindWith".to_string(),
    );
    caller.metadata.insert(
        cce_types::entity::meta_keys::CALL_PATHS.to_string(),
        "BodyWith".to_string(),
    );
    let callee = GroupedEntity::new(
        EntityId(2),
        EntityKind::Method,
        "BodyWith".to_string(),
        "BodyWith".to_string(),
    );
    let group = EntityGroup {
        members: [caller, callee].into_iter().collect(),
        ..EntityGroup::default()
    };
    let pair = || {
        vec![
            ChunkSegment::new(
                ChunkBoundary::new(0, 50, SplitReason::MemberBoundary)
                    .with_entity_ids(vec![EntityId(1)]),
                words(50),
            ),
            ChunkSegment::new(
                ChunkBoundary::new(50, 170, SplitReason::MemberBoundary)
                    .with_entity_ids(vec![EntityId(2)]),
                words(120),
            ),
        ]
    };
    let merged = splitter.merge_small_segments(&group, pair(), ChunkPath::Bm25);
    assert_eq!(
        merged.len(),
        1,
        "call-related neighbours stay in one BM25 chunk"
    );
    let plain = EntityGroup::default();
    let merged_plain = splitter.merge_small_segments(&plain, pair(), ChunkPath::Bm25);
    assert_eq!(
        merged_plain.len(),
        2,
        "unrelated neighbours respect the merge ceiling"
    );
}

#[test]
fn test_merge_small_segments_respects_test_boundary() {
    use cce_types::entity::{EntityKind, GroupedEntity};
    use cce_types::language::Language;
    use smallvec::smallvec;

    use crate::grouper::types::GroupType;

    let config = ChunkingConfig {
        max_tokens: 512,
        min_chunk_tokens: 50,
        ..Default::default()
    };
    let splitter = TextSplitter::new(config);
    // Mixed group: a production method next to a test helper. Both
    // fragments sit far below the min threshold, so the cost rule
    // alone would glue them; the test boundary must keep them apart
    // or the merged chunk would hide production content from
    // test-filtered retrieval.
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
        "check".to_string(),
        "check".to_string(),
    );
    helper.test_info = cce_types::TestInfo::test_ast();
    let mut group = EntityGroup {
        group_type: GroupType::ClassWithMethods,
        language: Language::Rust,
        ..EntityGroup::default()
    };
    group.members = smallvec![prod, helper];
    group.member_ids = smallvec![EntityId(11), EntityId(12)];
    let mixed = |ids: Vec<EntityId>| {
        ChunkSegment::new(
            ChunkBoundary::new(0, 30, SplitReason::MemberBoundary).with_entity_ids(ids),
            words(20),
        )
    };
    let segments = vec![mixed(vec![EntityId(11)]), mixed(vec![EntityId(12)])];
    let merged = splitter.merge_small_segments(&group, segments, ChunkPath::Embedding);
    assert_eq!(
        merged.len(),
        2,
        "production and test fragments must never merge"
    );
    // Same-side fragments still merge: two production pieces collapse.
    let prod_only = vec![mixed(vec![EntityId(11)]), mixed(vec![EntityId(11)])];
    let merged = splitter.merge_small_segments(&group, prod_only, ChunkPath::Embedding);
    assert_eq!(merged.len(), 1, "same-side fragments must still merge");
}

#[test]
fn test_merge_small_chunks_respect_boundaries_disabled() {
    let config = ChunkingConfig {
        max_bm25_terms: 2,
        min_chunk_bm25_terms: 3,
        respect_boundaries: false,
        ..Default::default()
    };
    let text = "one two. three four. five six. seven eight.";
    let segments = split_at_level(text, BoundaryLevel::Sentences, ChunkPath::Bm25, config);
    assert!(segments.len() >= 3);
}

#[test]
fn test_merge_small_chunks_applied_in_split() {
    let config = ChunkingConfig {
        max_bm25_terms: 512,
        ..Default::default()
    };
    let text = "one two. three four. five six. seven eight.";
    let segments = split_at_level(text, BoundaryLevel::Sentences, ChunkPath::Bm25, config);
    assert_eq!(segments.len(), 1);
    assert_eq!(segments[0].text, text);
    assert_eq!(segments[0].boundary.token_count, 8);
}

#[test]
fn test_merge_small_chunks_trailing() {
    let config = ChunkingConfig {
        max_bm25_terms: 512,
        min_chunk_bm25_terms: 60,
        ..Default::default()
    };
    let splitter = TextSplitter::new(config);

    // Bidirectional merge: a large leading chunk absorbs the trailing
    // small fragment when the combined cost fits the ceiling.
    let segments = vec![segment(&words(150), 0, 200), segment(&words(40), 200, 250)];

    let merged = splitter.merge_small_segments(&EntityGroup::default(), segments, ChunkPath::Bm25);

    assert_eq!(merged.len(), 1);
    assert_eq!(cost(&merged[0].text, ChunkPath::Bm25), 190);
}

#[test]
fn test_merge_small_chunks_rescues_undersized_leading_chunk() {
    let config = ChunkingConfig {
        max_bm25_terms: 512,
        min_chunk_bm25_terms: 60,
        ..Default::default()
    };
    let splitter = TextSplitter::new(config);

    // The leading chunk is below min, so it absorbs the (larger) neighbor:
    // a merge rescues the undersized chunk rather than being driven by
    // the neighbor's size.
    let segments = vec![segment(&words(40), 0, 50), segment(&words(200), 50, 250)];

    let merged = splitter.merge_small_segments(&EntityGroup::default(), segments, ChunkPath::Bm25);

    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].boundary.token_count, 240);
}
