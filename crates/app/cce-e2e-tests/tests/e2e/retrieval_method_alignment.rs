//! Retrieval-method benchmark alignment integration tests.
//!
//! Verifies the alignment contract end to end (see
//! `docs/issue/retrieval-benchmark-alignment.md`):
//!
//! - **Data generation**: `full_pipeline` / `full_pipeline_raw_source` must
//!   parse each file once (`OutputMode::Both`) so both retrieval paths share
//!   the same entity ids / segment ids — regression for the previously
//!   reported 0% alignment coverage caused by double parsing.
//! - **Measurement granularity**: single-path rows count each chunk once
//!   (raw chunk ranking, identical to the no-aggregation baseline benchmark);
//!   fused rows expand multi-entity chunks per entity key and evaluate the
//!   union of both paths' best chunks, so hits are independent of which
//!   path's chunk became the representative output chunk.
//!
//! All tests are offline (fixture parsing / in-memory data, no embedding API,
//! no Qdrant, no Tantivy).

use std::collections::HashSet;

use cce_e2e_tests::FixtureSpec;
use cce_e2e_tests::baselines::full_pipeline::gen_full_pipeline_chunks;
use cce_e2e_tests::baselines::full_pipeline_raw::gen_full_pipeline_raw_source_chunks;
use cce_e2e_tests::bench_data::{
    BenchmarkData, Bm25DocRecord, ChunkData, ChunkSourceRange, QueryData, QueryType,
    RelevanceJudgment, RelevanceLevel, RetrieverDataset,
};
use cce_e2e_tests::init_minimal_logging;
use cce_e2e_tests::retrieval_method::benchmark::{collect_alignment_stat, evaluate_baseline};

/// A chunk with explicit entity ids, source ranges and navigation span.
fn chunk(id: &str, entity_ids: Vec<i64>, ranges: Vec<(usize, usize)>) -> ChunkData {
    let (start, end) = ranges
        .iter()
        .fold((usize::MAX, 0usize), |(lo, hi), (s, e)| {
            (lo.min(*s), hi.max(*e))
        });
    ChunkData {
        chunk_id: id.to_string(),
        entity_name: String::new(),
        file_path: "src/lib.rs".to_string(),
        start_line: start,
        end_line: end,
        source_ranges: ranges
            .into_iter()
            .map(|(s, e)| ChunkSourceRange {
                start_line: s,
                end_line: e,
            })
            .collect(),
        source_span_kind: String::new(),
        test_info: cce_types::TestInfo::unknown(),
        language: None,
        entity_ids,
        segment_id: String::new(),
    }
}

/// In-memory benchmark dataset with deliberately separable scores.
///
/// Paths: 5 entities with one-hot 4-d vectors on the embedding side and
/// keyword texts on the BM25 side. Chunk `e12` contains TWO entities (3, 4);
/// its embedding chunk covers lines 21-24 while its BM25 chunk covers lines
/// 25-28 — disjoint ranges for the same entity key.
fn synthetic_benchmark() -> BenchmarkData {
    let chunk =
        |id: &str, entity_ids: Vec<i64>, ranges: Vec<(usize, usize)>| chunk(id, entity_ids, ranges);
    BenchmarkData {
        queries: vec![
            QueryData {
                id: "Q1".to_string(),
                text: "alpha".to_string(),
                query_type: QueryType::Qualified,
                relevant_names: vec![],
                irrelevant_names: vec![],
            },
            QueryData {
                id: "Q2".to_string(),
                text: "gamma".to_string(),
                query_type: QueryType::Qualified,
                relevant_names: vec![],
                irrelevant_names: vec![],
            },
            QueryData {
                id: "Q3".to_string(),
                text: "prime".to_string(),
                query_type: QueryType::Semantic,
                relevant_names: vec![],
                irrelevant_names: vec![],
            },
            QueryData {
                id: "Q4".to_string(),
                text: "config".to_string(),
                query_type: QueryType::Semantic,
                relevant_names: vec![],
                irrelevant_names: vec![],
            },
        ],
        query_texts: vec![
            "alpha".to_string(),
            "gamma".to_string(),
            "prime".to_string(),
            "config".to_string(),
        ],
        embedding: RetrieverDataset {
            chunks: vec![
                chunk("e1_emb_0", vec![1], vec![(1, 10)]),
                chunk("e2_emb_0", vec![2], vec![(11, 20)]),
                chunk("e12_emb_0", vec![3, 4], vec![(21, 24)]),
                chunk("e6_emb_0", vec![6], vec![(50, 55)]),
                chunk("e7_emb_0", vec![7], vec![(60, 65)]),
            ],
            texts: vec![
                "alpha implementation".to_string(),
                "beta implementation".to_string(),
                "gamma implementation".to_string(),
                "unrelated note about storage".to_string(),
                "prime target function".to_string(),
            ],
            // One-hot vectors: v1 = e_x, v7 = [1,0,0,0]; queries: q1 = [1,0,0,0],
            // q2 = [0,0,1,0], q3 = [0,0,0,1], q4 = [0,0,1,0].
            vectors: vec![
                1.0, 0.0, 0.0, 0.0, // e1
                0.0, 1.0, 0.0, 0.0, // e2
                0.0, 0.0, 1.0, 0.0, // e12 (entities 3,4)
                0.0, 0.0, 0.0, 1.0, // e6
                1.0, 0.0, 0.0, 0.0, // e7
            ],
            query_vectors: vec![
                1.0, 0.0, 0.0, 0.0, // Q1
                0.0, 0.0, 1.0, 0.0, // Q2
                0.0, 0.0, 0.0, 1.0, // Q3
                0.0, 0.0, 1.0, 0.0, // Q4
            ],
            dimension: 4,
        },
        bm25: RetrieverDataset {
            chunks: vec![
                chunk("e1_bm25_0", vec![1], vec![(5, 8)]),
                chunk("e2_bm25_0", vec![2], vec![(13, 19)]),
                chunk("e12_bm25_0", vec![3, 4], vec![(25, 28)]),
                chunk("e6_bm25_0", vec![6], vec![(50, 55)]),
                chunk("e7_bm25_0", vec![7], vec![(60, 65)]),
            ],
            texts: vec![
                "alpha implementation".to_string(),
                "beta implementation".to_string(),
                "gamma implementation".to_string(),
                "unrelated note about storage".to_string(),
                "prime target function".to_string(),
            ],
            vectors: Vec::new(),
            query_vectors: Vec::new(),
            dimension: 0,
        },
        bm25_documents: vec![
            Bm25DocRecord {
                title: String::new(),
                keywords: String::new(),
                content: "alpha implementation".to_string(),
            },
            Bm25DocRecord {
                title: String::new(),
                keywords: String::new(),
                content: "beta implementation".to_string(),
            },
            Bm25DocRecord {
                title: String::new(),
                keywords: String::new(),
                content: "gamma implementation".to_string(),
            },
            Bm25DocRecord {
                title: String::new(),
                keywords: String::new(),
                content: "unrelated note about storage".to_string(),
            },
            Bm25DocRecord {
                title: String::new(),
                keywords: String::new(),
                content: "prime target function".to_string(),
            },
        ],
        call_edges: Vec::new(),
    }
}

fn judgment(id: &str, query_type: QueryType, ranges: Vec<(usize, usize)>) -> RelevanceJudgment {
    RelevanceJudgment {
        id: id.to_string(),
        query_text: String::new(),
        query_type,
        fuzzy_subtype: None,
        relevant_ranges: ranges
            .into_iter()
            .map(|(s, e)| {
                (
                    cce_e2e_tests::bench_data::SourceRange {
                        file: "src/lib.rs".to_string(),
                        start_line: s,
                        end_line: e,
                    },
                    RelevanceLevel::Strong,
                )
            })
            .collect(),
    }
}

fn row<'a>(
    results: &'a [cce_e2e_tests::retrieval_method::MethodResult],
    method: &'a str,
) -> impl Iterator<Item = &'a cce_e2e_tests::retrieval_method::MethodResult> + 'a {
    results.iter().filter(move |r| {
        r.method == method
            && r.top_k == 5
            && (r.query_id == "Q1"
                || r.query_id == "Q2"
                || r.query_id == "Q3"
                || r.query_id == "Q4")
    })
}

fn recall_any(
    results: &[cce_e2e_tests::retrieval_method::MethodResult],
    method: &str,
    query: &str,
) -> f64 {
    row(results, method)
        .find(|r| r.query_id == query)
        .map(|r| r.score.range.recall_any)
        .unwrap_or(f64::NAN)
}

fn any_matches(
    results: &[cce_e2e_tests::retrieval_method::MethodResult],
    method: &str,
    query: &str,
) -> usize {
    row(results, method)
        .find(|r| r.query_id == query)
        .map(|r| r.score.range.any_matches)
        .unwrap_or(usize::MAX)
}

/// Cross-path alignment coverage on the synthetic dataset: the multi-entity
/// chunk must contribute one key per contained entity, and all 6 entity keys
/// must be common across the two paths.
#[test]
fn synthetic_data_has_full_entity_alignment() {
    let bench = synthetic_benchmark();
    let stat = collect_alignment_stat("synthetic", &bench);
    assert_eq!(stat.emb_keys, 6);
    assert_eq!(stat.bm25_keys, 6);
    assert_eq!(stat.common_keys, 6);
    assert_eq!(stat.emb_only_keys, 0);
    assert_eq!(stat.bm25_only_keys, 0);
}

/// Entity-level fused evaluation (issue 2.2): the judgment range (26,27) is
/// covered only by the BM25 chunk of entity keys 3/4. The embedding path
/// misses it; every fused row must still hit because evaluation uses the
/// union of both paths' best chunks — independent of which path's chunk won
/// the representative slot (vector-dominant α=0.9 vs bm25-dominant α=0.1).
#[test]
fn fused_evaluation_hits_entity_regardless_of_representative_chunk() {
    let bench = synthetic_benchmark();
    let judgments = vec![judgment("Q2", QueryType::Qualified, vec![(26, 27)])];
    let (results, _) = evaluate_baseline("synthetic", &bench, &judgments);

    assert_eq!(
        recall_any(&results, "emb", "Q2"),
        0.0,
        "embedding path misses (21,24) vs (26,27)"
    );
    assert_eq!(
        recall_any(&results, "bm25", "Q2"),
        1.0,
        "BM25 path hits (25,28)"
    );
    // Entity-level coverage unions both paths: the fused rows hit regardless
    // of the representative chunk (vector-dominant vs bm25-dominant).
    assert_eq!(recall_any(&results, "minmax-0.9", "Q2"), 1.0);
    assert_eq!(recall_any(&results, "minmax-0.1", "Q2"), 1.0);
    // Both contained entities are retrieved as separate entity hits.
    assert_eq!(any_matches(&results, "minmax-0.9", "Q2"), 2);
    assert_eq!(any_matches(&results, "minmax-0.1", "Q2"), 2);
}

/// Single-path rows measure the raw chunk ranking: a multi-entity chunk is
/// counted exactly once (it is a single retrieved chunk), matching the
/// no-aggregation baseline benchmark. The `emb` row for Q4 therefore reports
/// one match for the two-entity chunk `e12`, not two — the two-entity count is
/// an artifact of entity-level fusion, which expands each contained entity
/// into its own fused row.
#[test]
fn single_path_rows_count_multi_entity_chunk_once() {
    let bench = synthetic_benchmark();
    let judgments = vec![judgment("Q4", QueryType::Semantic, vec![(22, 23)])];
    let (results, _) = evaluate_baseline("synthetic", &bench, &judgments);

    // The two-entity chunk e12 (keys e:3 and e:4) is one retrieved chunk, so
    // it contributes one match and full recall for the judged range.
    assert_eq!(any_matches(&results, "emb", "Q4"), 1);
    assert_eq!(recall_any(&results, "emb", "Q4"), 1.0);
}

/// Weight response is monotonic in the aligned regime (issue 2.4): when the
/// embedding path ranks the relevant entity on top and the BM25 path ranks an
/// irrelevant entity on top, minmax-α converges to the embedding path as α→1:
/// the vector-only hit keeps rank 1 for vector-dominant weights and is
/// displaced to rank 2 for bm25-dominant weights.
#[test]
fn minmax_converges_to_embedding_when_keys_align() {
    let bench = synthetic_benchmark();
    let judgments = vec![judgment("Q3", QueryType::Semantic, vec![(50, 55)])];
    let (results, _) = evaluate_baseline("synthetic", &bench, &judgments);

    // The embedding path ranks the relevant entity 6 first (query vector
    // matches e6). The BM25 path ranks entity 7 first (its text contains
    // "prime"), so the judged range only surfaces at rank 5 — the two paths
    // genuinely disagree on the top hit.
    assert_eq!(recall_any(&results, "emb", "Q3"), 1.0);
    let first_hit = |method: &str| -> f64 {
        row(&results, method)
            .find(|r| r.query_id == "Q3")
            .and_then(|r| r.score.avg_first_hit_rank)
            .unwrap_or(f64::NAN)
    };
    assert_eq!(
        first_hit("emb"),
        1.0,
        "embedding path ranks the relevant entity 6 first"
    );
    assert_eq!(
        first_hit("bm25"),
        5.0,
        "BM25 ranks the irrelevant entity 7 first; the judged range surfaces last"
    );
    // α→1 keeps the embedding hit at rank 1 (vector-dominant fusion inherits
    // the vector ranking); α=0.1 displaces it below the BM25 top hit.
    assert_eq!(
        first_hit("minmax-0.9"),
        1.0,
        "α→1 keeps the embedding hit on top"
    );
    assert_eq!(
        first_hit("minmax-0.1"),
        2.0,
        "bm25-dominant fusion must rank the irrelevant BM25 hit above the embedding hit"
    );
}

/// The full_pipeline data generator must parse each file once so both paths
/// share entity ids (issue 2.3 root cause: double parsing with a global
/// sequential entity-id counter produced fully disjoint ids, i.e. 0%
/// alignment coverage).
#[tokio::test]
async fn full_pipeline_generation_aligns_entity_ids_across_paths() {
    init_minimal_logging();
    let generated = gen_full_pipeline_chunks(
        FixtureSpec::rust_once_cell(),
        FixtureSpec::rust_distractor(),
    )
    .await
    .expect("full_pipeline chunk generation must succeed");
    assert_cross_path_alignment(
        &generated.embedding_chunks,
        &generated.bm25_chunks,
        "full_pipeline",
    );
}

/// Same contract for the raw-source variant.
#[tokio::test]
async fn full_pipeline_raw_generation_aligns_entity_ids_across_paths() {
    init_minimal_logging();
    let generated = gen_full_pipeline_raw_source_chunks(
        FixtureSpec::rust_once_cell(),
        FixtureSpec::rust_distractor(),
    )
    .await
    .expect("full_pipeline_raw chunk generation must succeed");
    assert_cross_path_alignment(
        &generated.embedding_chunks,
        &generated.bm25_chunks,
        "full_pipeline_raw_source",
    );
}

fn assert_cross_path_alignment(emb: &[ChunkData], bm25: &[ChunkData], label: &str) {
    let emb_entities: HashSet<i64> = emb
        .iter()
        .flat_map(|c| c.entity_ids.iter().copied())
        .collect();
    let bm25_entities: HashSet<i64> = bm25
        .iter()
        .flat_map(|c| c.entity_ids.iter().copied())
        .collect();
    let common_entities = emb_entities.intersection(&bm25_entities).count();
    let smaller = emb_entities.len().min(bm25_entities.len());
    assert!(smaller > 0, "{label}: no entities on the smaller path");
    assert!(
        common_entities as f64 / smaller as f64 >= 0.9,
        "{label}: only {common_entities}/{smaller} entity ids are shared between paths \
         (double parsing breaks entity-level alignment)",
    );

    let emb_segments: HashSet<&str> = emb
        .iter()
        .map(|c| c.segment_id.as_str())
        .filter(|s| !s.is_empty())
        .collect();
    let bm25_segments: HashSet<&str> = bm25
        .iter()
        .map(|c| c.segment_id.as_str())
        .filter(|s| !s.is_empty())
        .collect();
    assert!(
        !emb_segments.is_disjoint(&bm25_segments),
        "{label}: no shared segment ids between paths",
    );

    // The benchmark's own coverage statistic must report a non-zero common
    // key set (previously 0.0% on full_pipeline).
    let bench = BenchmarkData {
        queries: Vec::new(),
        query_texts: Vec::new(),
        embedding: RetrieverDataset {
            chunks: emb.to_vec(),
            texts: Vec::new(),
            vectors: Vec::new(),
            query_vectors: Vec::new(),
            dimension: 0,
        },
        bm25: RetrieverDataset {
            chunks: bm25.to_vec(),
            texts: Vec::new(),
            vectors: Vec::new(),
            query_vectors: Vec::new(),
            dimension: 0,
        },
        bm25_documents: Vec::new(),
        call_edges: Vec::new(),
    };
    let stat = collect_alignment_stat(label, &bench);
    assert!(
        stat.common_keys > 0,
        "{label}: alignment coverage must be non-zero, got emb_keys={} bm25_keys={} common={}",
        stat.emb_keys,
        stat.bm25_keys,
        stat.common_keys,
    );
}
