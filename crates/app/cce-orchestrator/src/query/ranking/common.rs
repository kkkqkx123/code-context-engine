//! Shared candidate selection / merge-back logic for rerankers.
//!
//! Both the LLM reranker and the plugin reranker use the same dynamic
//! candidate selection, candidate projection and result merge-back; keeping
//! them here avoids duplicated drift-prone copies.

use std::collections::HashMap;

use cce_types::{RerankCandidate, RerankResult, RerankedCandidate};

use crate::query::types::SearchResult;

/// Dynamically select candidate count based on score distribution.
pub fn select_candidate_count(
    results: &[SearchResult],
    max_candidates: usize,
    min_candidates: usize,
    score_drop_threshold: f32,
    min_score: f32,
    drop_detection_start: f32,
) -> usize {
    if results.is_empty() {
        return 0;
    }

    let valid_count = results.iter().filter(|r| r.score >= min_score).count();

    if valid_count == 0 {
        return 0;
    }

    if valid_count <= min_candidates {
        return valid_count;
    }

    let search_limit = valid_count.min(max_candidates);
    let mut found_drop = false;
    let mut drop_point = search_limit;

    for i in 1..search_limit {
        let current_score = results[i].score;

        if current_score < drop_detection_start && !found_drop {
            let gap = results[i - 1].score - current_score;

            if gap > score_drop_threshold {
                drop_point = i;
                found_drop = true;
                break;
            }
        }
    }

    if found_drop {
        return drop_point.max(min_candidates);
    }

    search_limit
}

/// Project the top `candidate_count` search results into rerank candidates.
pub fn build_candidates(results: &[SearchResult], candidate_count: usize) -> Vec<RerankCandidate> {
    results
        .iter()
        .take(candidate_count)
        .map(|r| RerankCandidate {
            id: r.id.clone(),
            content: r.content.clone(),
            file_path: r.file_path.clone(),
            initial_score: r.score,
            entity_type: Some(r.kind.clone()),
            metadata: HashMap::new(),
        })
        .collect()
}

/// Merge rerank results back into the original search results and re-sort by
/// the fused final score. Unmatched results keep their original scores.
pub fn merge_rerank_results(
    original_results: Vec<SearchResult>,
    rerank_result: RerankResult,
) -> Vec<SearchResult> {
    let rerank_map: HashMap<String, RerankedCandidate> = rerank_result
        .reranked_candidates
        .into_iter()
        .map(|r| (r.id.clone(), r))
        .collect();

    let mut updated_results = original_results;
    for result in &mut updated_results {
        if let Some(reranked) = rerank_map.get(&result.id) {
            result.score = reranked.final_score;
            result.original_score = reranked.initial_score;
            result.metadata.insert(
                "rerank_score".to_string(),
                reranked.rerank_score.to_string(),
            );
            result
                .metadata
                .insert("rank_change".to_string(), reranked.rank_change.to_string());
            if let Some(ref reasoning) = reranked.reasoning {
                result
                    .metadata
                    .insert("rerank_reasoning".to_string(), reasoning.clone());
            }
        }
    }

    updated_results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    updated_results
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_result(id: &str, score: f32) -> SearchResult {
        SearchResult {
            id: id.to_string(),
            score,
            original_score: score,
            vector_score: score,
            kind: "function".to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn test_select_candidate_count_empty_results() {
        assert_eq!(select_candidate_count(&[], 50, 3, 0.05, 0.3, 0.6), 0);
    }

    #[test]
    fn test_select_candidate_count_all_below_min_score() {
        let results = vec![make_result("1", 0.25), make_result("2", 0.2)];
        assert_eq!(select_candidate_count(&results, 50, 3, 0.05, 0.3, 0.6), 0);
    }

    #[test]
    fn test_select_candidate_count_few_results() {
        let results = vec![make_result("1", 0.9), make_result("2", 0.8)];
        assert_eq!(select_candidate_count(&results, 50, 3, 0.05, 0.3, 0.6), 2);
    }

    #[test]
    fn test_select_candidate_count_max_candidates() {
        let results: Vec<SearchResult> = (0..10)
            .map(|i| make_result(&i.to_string(), 0.9 - i as f32 * 0.01))
            .collect();
        assert_eq!(select_candidate_count(&results, 4, 1, 0.05, 0.3, 0.6), 4);
    }

    #[test]
    fn test_select_candidate_count_detects_score_drop() {
        // A clear gap below the drop-detection start truncates candidates.
        let results = vec![
            make_result("1", 0.9),
            make_result("2", 0.88),
            make_result("3", 0.5),
            make_result("4", 0.45),
        ];
        let count = select_candidate_count(&results, 50, 1, 0.1, 0.3, 0.8);
        assert_eq!(count, 2, "drop at index 2 must cut the candidate list");
    }

    #[test]
    fn test_build_candidates_projects_top_n() {
        let results = vec![make_result("a", 0.9), make_result("b", 0.8)];
        let candidates = build_candidates(&results, 1);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].id, "a");
        assert!((candidates[0].initial_score - 0.9).abs() < f32::EPSILON);
        assert_eq!(candidates[0].entity_type.as_deref(), Some("function"));
    }

    #[test]
    fn test_merge_rerank_results_updates_scores_and_sorts() {
        let results = vec![make_result("a", 0.5), make_result("b", 0.4)];
        let reranked = RerankResult::new(vec![
            RerankedCandidate {
                id: "a".to_string(),
                rerank_score: 0.1,
                initial_score: 0.5,
                final_score: 0.2,
                rank_change: -1,
                reasoning: Some("less relevant".to_string()),
            },
            RerankedCandidate {
                id: "b".to_string(),
                rerank_score: 0.9,
                initial_score: 0.4,
                final_score: 0.8,
                rank_change: 1,
                reasoning: None,
            },
        ]);

        let merged = merge_rerank_results(results, reranked);
        assert_eq!(merged[0].id, "b");
        assert!((merged[0].score - 0.8).abs() < 1e-6);
        assert_eq!(merged[0].metadata["rerank_score"], "0.9");
        assert_eq!(merged[1].metadata["rerank_reasoning"], "less relevant");
    }

    #[test]
    fn test_merge_rerank_results_keeps_unmatched_results() {
        let results = vec![make_result("unknown", 0.7)];
        let reranked = RerankResult::new(vec![RerankedCandidate {
            id: "other".to_string(),
            rerank_score: 0.9,
            initial_score: 0.4,
            final_score: 0.9,
            rank_change: 0,
            reasoning: None,
        }]);

        let merged = merge_rerank_results(results, reranked);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].id, "unknown");
        assert!((merged[0].score - 0.7).abs() < 1e-6);
    }
}
