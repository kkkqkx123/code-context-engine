//! Rerank benchmark tests with a mock rerank model.
//!
//! Exercises candidate construction, provider-generic score generation, the
//! sidecar round-trip, and offline control-vs-reranked evaluation on a tiny
//! in-memory dataset. No network, no model calls, no fixture files.

use cce_config::modules::search::RerankFusionStrategy;
use cce_e2e_tests::bench_data::{
    BenchmarkData, Bm25DocRecord, ChunkData, ChunkSourceRange, QueryData, QueryType,
    RelevanceJudgment, RelevanceLevel, RetrieverDataset, SourceRange,
};
use cce_e2e_tests::rerank_benchmark::{
    RerankQueryOutcome, RerankRuntime, RerankScoring, RerankTextSource, build_candidates,
    decode_sidecar, encode_sidecar, evaluate_query_rerank, generate_sidecar, score_query_outcome,
};
use cce_e2e_tests::retrieval_method::rank_recall;

/// Mock rerank model: scores candidates by ascending position, so the recall
/// order flips after fusion (the last recall candidate scores highest).
struct ReverseRerank;

impl cce_llm::RerankProvider for ReverseRerank {
    async fn rerank(
        &self,
        request: &cce_llm::RerankRequest,
    ) -> Result<cce_llm::rerank::RerankResult, cce_llm::LlmError> {
        let count = request.candidates.len().max(1) as f32;
        let reranked = request
            .candidates
            .iter()
            .enumerate()
            .map(|(position, candidate)| cce_llm::rerank::RerankedCandidate {
                id: candidate.id.clone(),
                rerank_score: (position + 1) as f32 / count,
                initial_score: candidate.initial_score,
                final_score: 0.0,
                rank_change: 0,
                reasoning: None,
            })
            .collect();
        Ok(cce_llm::rerank::RerankResult::new(reranked))
    }

    fn provider_name(&self) -> &str {
        "test-reverse"
    }

    fn is_available(&self) -> bool {
        true
    }
}

fn chunk(id: &str, start: usize, end: usize) -> ChunkData {
    ChunkData {
        chunk_id: id.to_string(),
        entity_name: id.to_string(),
        file_path: "src/lib.rs".to_string(),
        start_line: start,
        end_line: end,
        source_ranges: vec![ChunkSourceRange {
            start_line: start,
            end_line: end,
        }],
        source_span_kind: String::new(),
        test_info: cce_types::TestInfo::unknown(),
        language: None,
        entity_ids: Vec::new(),
        segment_id: String::new(),
    }
}

/// Three chunks where only the last one (C) is relevant; recall ranks A first
/// on both paths so the mock flip is observable.
fn test_bench() -> BenchmarkData {
    let chunks = vec![
        chunk("emb_a", 1, 10),
        chunk("emb_b", 11, 20),
        chunk("emb_c", 21, 30),
    ];
    let texts = vec![
        "alpha beta alpha beta".to_string(),
        "delta epsilon".to_string(),
        "zeta eta theta".to_string(),
    ];
    // Query vector equals chunk A: recall order is A, B, C.
    let vectors = vec![1.0, 0.0, 0.7, 0.7, 0.0, 1.0];
    let bm25_chunks = vec![
        chunk("bm25_a", 1, 10),
        chunk("bm25_b", 11, 20),
        chunk("bm25_c", 21, 30),
    ];
    let bm25_texts = texts.clone();
    BenchmarkData {
        queries: vec![QueryData {
            id: "TQ1".to_string(),
            text: "alpha beta".to_string(),
            query_type: QueryType::Semantic,
            relevant_names: Vec::new(),
            irrelevant_names: Vec::new(),
        }],
        query_texts: vec!["alpha beta".to_string()],
        embedding: RetrieverDataset {
            chunks,
            texts,
            vectors,
            query_vectors: vec![1.0, 0.0],
            dimension: 2,
        },
        bm25: RetrieverDataset {
            chunks: bm25_chunks,
            texts: bm25_texts,
            vectors: Vec::new(),
            query_vectors: Vec::new(),
            dimension: 0,
        },
        bm25_documents: vec![
            Bm25DocRecord {
                title: "alpha beta".to_string(),
                keywords: String::new(),
                content: "alpha beta alpha beta gamma".to_string(),
            },
            Bm25DocRecord {
                title: "delta".to_string(),
                keywords: String::new(),
                content: "delta epsilon".to_string(),
            },
            Bm25DocRecord {
                title: "zeta".to_string(),
                keywords: String::new(),
                content: "zeta eta theta".to_string(),
            },
        ],
        call_edges: Vec::new(),
    }
}

fn test_judgment() -> RelevanceJudgment {
    RelevanceJudgment {
        id: "TQ1".to_string(),
        query_text: "alpha beta".to_string(),
        query_type: QueryType::Semantic,
        fuzzy_subtype: None,
        relevant_ranges: vec![(
            SourceRange {
                file: "src/lib.rs".to_string(),
                start_line: 21,
                end_line: 30,
            },
            RelevanceLevel::Strong,
        )],
    }
}

fn test_runtime() -> RerankRuntime {
    RerankRuntime {
        model_key: "test-reranker".to_string(),
        model_name: "test-model".to_string(),
        mode: "test".to_string(),
        depth: 50,
        fusion: RerankFusionStrategy::LinearWeighted { alpha: 0.7 },
        timeout_ms: 1000,
        text_source: RerankTextSource::EmbText,
    }
}

fn test_scoring(normalize_initial: bool) -> RerankScoring {
    RerankScoring {
        model_key: "test-reranker".to_string(),
        depth: 50,
        fusion: RerankFusionStrategy::LinearWeighted { alpha: 0.7 },
        normalize_initial,
        text_source: RerankTextSource::EmbText,
    }
}

fn first_hit(rows: &[cce_e2e_tests::rerank_benchmark::RerankRow], method: &str) -> Option<f64> {
    rows.iter()
        .find(|row| row.method == method && row.top_k == 5)
        .and_then(|row| row.avg_first_hit_rank)
}

/// The mock model flips recall order: control first-hit rank 3 becomes 1.
#[tokio::test]
async fn test_mock_rerank_flips_first_hit() {
    let bench = test_bench();
    let judgment = test_judgment();
    let runtime = test_runtime();
    let recall = rank_recall(&bench);

    let candidates = build_candidates(
        &bench,
        &recall,
        "emb",
        RerankTextSource::EmbText,
        0,
        runtime.depth,
    );
    assert_eq!(candidates.len(), 3, "emb must rank all chunks");
    assert!(
        candidates[0].chunk_id.ends_with("_a"),
        "emb recall must rank chunk A first"
    );

    let outcome =
        score_query_outcome(&ReverseRerank, "TQ1", "alpha beta", &candidates, &runtime).await;
    assert!(!outcome.failed, "emb mock scoring must succeed");
    assert_eq!(outcome.candidates.len(), 3);

    let eval = evaluate_query_rerank(
        "test",
        "emb",
        "TQ1",
        &judgment,
        &candidates,
        &outcome,
        &test_scoring(false),
    );
    // Control + reranked rows across the five top-k cutoffs.
    assert_eq!(eval.rows.len(), 10, "emb must score both orders");
    let control_hit = first_hit(&eval.rows, "emb");
    let reranked_hit = first_hit(&eval.rows, "emb+rerank");
    assert_eq!(
        control_hit,
        Some(3.0),
        "emb control first hit must be rank 3"
    );
    // The mock scores the last recall candidate highest, so the reranked
    // first hit must improve.
    assert!(
        reranked_hit.is_some_and(|rank| rank < 3.0),
        "emb reranked first hit {reranked_hit:?} must beat control {control_hit:?}"
    );
}

/// Reranking is defined against the embedding path only: lexical and fused
/// methods must not produce rerank candidates.
#[test]
fn test_non_emb_methods_produce_no_candidates() {
    let bench = test_bench();
    let recall = rank_recall(&bench);

    for method in ["bm25", "minmax-0.5"] {
        let candidates =
            build_candidates(&bench, &recall, method, RerankTextSource::EmbText, 0, 50);
        assert!(
            candidates.is_empty(),
            "{method} must not produce rerank candidates"
        );
    }
}

/// Sidecar generation and the rkyv round-trip preserve every stored score.
#[tokio::test]
async fn test_sidecar_generation_round_trip() {
    let bench = test_bench();
    let runtime = test_runtime();
    let sidecar = generate_sidecar(&ReverseRerank, &bench, 99, "emb", &runtime).await;
    assert_eq!(sidecar.header.source_hash, 99);
    assert_eq!(sidecar.header.retrieval_method, "emb");
    assert_eq!(sidecar.queries.len(), 1);

    let bytes = encode_sidecar(&sidecar).expect("encode must succeed");
    let decoded = decode_sidecar(&bytes).expect("decode must succeed");
    assert_eq!(decoded.queries.len(), 1);
    let stored: Vec<(String, f32)> = decoded.queries[0]
        .candidates
        .iter()
        .map(|candidate| (candidate.chunk_id.clone(), candidate.rerank_score))
        .collect();
    assert_eq!(stored.len(), 3);
    // Recall order A, B, C with ascending mock scores.
    assert!(stored[0].1 < stored[1].1 && stored[1].1 < stored[2].1);
}

/// Failed samples and score drift yield no rows instead of bad numbers.
#[test]
fn test_failed_outcome_yields_no_rows() {
    let bench = test_bench();
    let judgment = test_judgment();
    let runtime = test_runtime();
    let recall = rank_recall(&bench);
    let candidates = build_candidates(
        &bench,
        &recall,
        "emb",
        RerankTextSource::EmbText,
        0,
        runtime.depth,
    );

    let failed = RerankQueryOutcome {
        query_id: "TQ1".to_string(),
        query_text: "alpha beta".to_string(),
        candidates: Vec::new(),
        elapsed_ms: 0,
        failed: true,
    };
    let eval = evaluate_query_rerank(
        "test",
        "emb",
        "TQ1",
        &judgment,
        &candidates,
        &failed,
        &test_scoring(false),
    );
    assert!(eval.rows.is_empty());
    assert!(eval.relevance.is_empty());
}

/// Normalizing initial scores makes the rerank signal dominate the blend:
/// with wide-scale raw initials the mock flip only improves the first hit,
/// while normalized initials let the mock's top candidate take rank 1.
#[tokio::test]
async fn test_normalize_initial_lets_rerank_dominate() {
    let bench = test_bench();
    let judgment = test_judgment();
    let runtime = test_runtime();
    let recall = rank_recall(&bench);
    let candidates = build_candidates(
        &bench,
        &recall,
        "emb",
        RerankTextSource::EmbText,
        0,
        runtime.depth,
    );
    let outcome =
        score_query_outcome(&ReverseRerank, "TQ1", "alpha beta", &candidates, &runtime).await;
    assert!(!outcome.failed);

    let eval = evaluate_query_rerank(
        "test",
        "emb",
        "TQ1",
        &judgment,
        &candidates,
        &outcome,
        &test_scoring(true),
    );
    assert_eq!(
        first_hit(&eval.rows, "emb+rerank"),
        Some(1.0),
        "normalized initials must let the mock top candidate take rank 1"
    );
}
