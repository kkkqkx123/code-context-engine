//! Cross sub-query aggregation helpers.
//!
//! Sub-queries reach the merge step on different score scales: recall leaves a
//! single-path sub-query min-max normalized, so its head is always 1.0, while a
//! hybrid sub-query keeps the weighted fusion sum and its head sits at the
//! vector/BM25 weight (0.5 by default). Comparing the two directly ranks hits by
//! path configuration instead of relevance, so every list is aligned to a shared
//! ceiling before the merge.
//!
//! Everything here is pure: the planning rules, the score alignment and the
//! merge key are testable without assembling a coordinator.

use cce_config::modules::search::AggregationScoreStrategy;

use super::types::{SearchResult, SubQuery};

/// Hard cap on the sub-queries one aggregated request may carry.
///
/// Each sub-query costs at least one embedding round-trip plus a full recall
/// pipeline, so an unbounded list is a denial-of-service vector for callers that
/// bypass the HTTP surface (internal evaluation, SDK, tests). The HTTP layer
/// keeps its own, stricter limit and reports it as a request-shape error.
pub const MAX_SUB_QUERIES: usize = 16;

/// Align one sub-query's candidate scores to the shared aggregation ceiling.
///
/// Scores are divided by the list's own maximum, so the best candidate of every
/// sub-query reads 1.0 and a merged score means "match strength relative to this
/// sub-query's best" regardless of recall path. In-list ordering and the spacing
/// between neighbours are preserved, which is what Min-Max would flatten: a list
/// whose worst candidate is still highly relevant must not be pushed to 0.
///
/// Empty lists and all-zero (or non-finite) lists are left untouched, so a
/// degenerate sub-query contributes nothing rather than being promoted.
pub fn align_sub_query_scores(items: &mut [SearchResult]) {
    let max = items
        .iter()
        .map(|item| item.score)
        .filter(|score| score.is_finite())
        .fold(0.0_f32, f32::max);
    if max <= 0.0 {
        return;
    }
    for item in items.iter_mut() {
        item.score = if item.score.is_finite() {
            (item.score / max).clamp(0.0, 1.0)
        } else {
            0.0
        };
    }
}

/// Validate a sub-query plan before any retrieval is spent on it.
///
/// Beyond shape checks this rejects a plan that cannot produce a result: after
/// alignment a sub-query's contribution is bounded by its weight, so under the
/// `max` strategy a weight at or below `min_score` is dropped by the global
/// threshold no matter what it recalls. Paying for that retrieval and silently
/// returning nothing is the failure mode this rule exists to prevent.
///
/// `noisy_or` keeps low weights legal: contributions on the same alignment key
/// accumulate, so a weak sub-query can still lift a key it agrees on.
pub fn validate_plan(
    sub_queries: &[SubQuery],
    min_score: f32,
    strategy: AggregationScoreStrategy,
) -> Result<(), String> {
    if sub_queries.is_empty() {
        return Err("aggregated search needs at least one sub-query".to_string());
    }
    if sub_queries.len() > MAX_SUB_QUERIES {
        return Err(format!(
            "aggregated search accepts at most {MAX_SUB_QUERIES} sub-queries, got {}",
            sub_queries.len()
        ));
    }
    // Contributions are clamped below 1.0 before they are combined, so a
    // threshold of 1.0 rejects every plan no matter how it is shaped. Say that
    // instead of blaming the weights of an otherwise valid request.
    if min_score >= 1.0 {
        return Err(format!(
            "result threshold {min_score} cannot be satisfied: merged scores stay below 1.0, lower min_score"
        ));
    }
    for sub_query in sub_queries {
        if cce_utils::text::is_blank(&sub_query.text) {
            return Err("sub-query text cannot be blank".to_string());
        }
        if !sub_query.weight.is_finite() || sub_query.weight <= 0.0 || sub_query.weight > 1.0 {
            return Err(format!(
                "invalid sub-query weight {} for '{}': expected (0, 1]",
                sub_query.weight, sub_query.text
            ));
        }
        if strategy == AggregationScoreStrategy::Max
            && min_score > 0.0
            && sub_query.weight <= min_score
        {
            return Err(format!(
                "sub-query weight {} for '{}' can never pass the result threshold {}: raise the weight above {} or lower min_score",
                sub_query.weight, sub_query.text, min_score, min_score
            ));
        }
    }
    Ok(())
}

/// Dedup key for the cross-query merge.
///
/// Chunk grouping keeps one entry per physical chunk inside a sub-query, but the
/// representative is the highest-scoring entity, so two sub-queries can surface
/// the same chunk under different entity ids. Keying on the chunk id keeps one
/// entry per body, which is what the caller receives. Without grouping the
/// unified key (entity, then segment, then chunk) stays authoritative.
///
/// Candidates that carry no identity at all still share a physical location, so
/// the file-and-range serves as the last-resort key instead of letting the same
/// lines in through every sub-query. Hits without a real range stay keyless and
/// are appended as-is.
pub fn aggregate_dedup_key(result: &SearchResult, group_by_chunk: bool) -> Option<String> {
    if group_by_chunk && !result.id.is_empty() {
        return Some(format!("c:{}", result.id));
    }
    if let Some(key) = crate::query::retrieval::primitives::unified_dedup_key(result) {
        return Some(key);
    }
    if !result.file_path.is_empty() && result.end_line > result.start_line {
        return Some(format!(
            "l:{}:{}-{}",
            result.file_path, result.start_line, result.end_line
        ));
    }
    None
}

/// Time reserved for the post-merge stages of an aggregated request.
///
/// Sub-queries are bounded by `deadline - reserve`, so a slow recall cannot
/// consume the budget reranking, thresholding and expansion still need. Without
/// the reservation the request-level deadline fires after the merge started and
/// throws away work that already completed.
///
/// The reserve is sized generously relative to the stages it protects: reranking
/// `rerank.max_candidates` candidates is the dominant post-merge cost, and
/// cutting recall early only shortens the candidate list, while cutting the
/// merge short loses the whole answer. Scaled to the budget with a floor for
/// tiny test budgets and a ceiling so a large budget does not reserve minutes.
pub fn post_processing_reserve_ms(timeout_ms: u64) -> u64 {
    (timeout_ms / 4).clamp(25, 4_000)
}

#[cfg(test)]
mod tests {
    use super::{
        aggregate_dedup_key, align_sub_query_scores, post_processing_reserve_ms, validate_plan,
    };
    use crate::query::types::{SearchResult, SearchSources, SubQuery};
    use cce_config::modules::search::AggregationScoreStrategy;

    fn hit(id: &str, score: f32) -> SearchResult {
        SearchResult {
            id: id.to_string(),
            score,
            ..Default::default()
        }
    }

    fn sub(text: &str, weight: f32) -> SubQuery {
        SubQuery {
            text: text.to_string(),
            sources: SearchSources::default(),
            weight,
        }
    }

    #[test]
    fn alignment_puts_every_path_head_on_one_ceiling() {
        let mut hybrid = vec![hit("a", 0.5), hit("b", 0.25)];
        let mut single_path = vec![hit("c", 0.9), hit("d", 0.45)];
        align_sub_query_scores(&mut hybrid);
        align_sub_query_scores(&mut single_path);

        assert!((hybrid[0].score - 1.0).abs() < 1e-6);
        assert!((single_path[0].score - 1.0).abs() < 1e-6);
        // Relative spacing survives the scaling.
        assert!((hybrid[1].score - 0.5).abs() < 1e-6);
        assert!((single_path[1].score - 0.5).abs() < 1e-6);
    }

    #[test]
    fn alignment_leaves_degenerate_lists_untouched() {
        let mut empty: Vec<SearchResult> = Vec::new();
        align_sub_query_scores(&mut empty);
        assert!(empty.is_empty());

        let mut zeroed = vec![hit("a", 0.0), hit("b", 0.0)];
        align_sub_query_scores(&mut zeroed);
        assert_eq!(zeroed[0].score, 0.0);
        assert_eq!(zeroed[1].score, 0.0);
    }

    #[test]
    fn plan_rejects_weight_that_cannot_clear_the_threshold() {
        let strategy = AggregationScoreStrategy::Max;
        let error = validate_plan(&[sub("auth", 0.2)], 0.25, strategy)
            .expect_err("weight below threshold must be rejected");
        assert!(error.contains("0.2"));
        assert!(error.contains("0.25"));

        assert!(validate_plan(&[sub("auth", 0.5)], 0.25, strategy).is_ok());
        assert!(validate_plan(&[sub("auth", 0.5)], 0.0, strategy).is_ok());
    }

    #[test]
    fn plan_keeps_low_weights_under_evidence_accumulation() {
        let strategy = AggregationScoreStrategy::NoisyOr;
        assert!(validate_plan(&[sub("auth", 0.2)], 0.25, strategy).is_ok());
    }

    #[test]
    fn plan_rejects_blank_text_and_out_of_range_weight() {
        let strategy = AggregationScoreStrategy::Max;
        assert!(validate_plan(&[sub("   ", 1.0)], 0.0, strategy).is_err());
        assert!(validate_plan(&[sub("auth", 0.0)], 0.0, strategy).is_err());
        assert!(validate_plan(&[sub("auth", 1.5)], 0.0, strategy).is_err());
        assert!(validate_plan(&[], 0.0, strategy).is_err());
    }

    #[test]
    fn plan_rejects_an_unsatisfiable_threshold_once() {
        let strategy = AggregationScoreStrategy::Max;
        // Blame the threshold, not the weight: 1.0 is unreachable for any plan.
        let error = validate_plan(&[sub("auth", 1.0)], 1.0, strategy)
            .expect_err("threshold 1.0 must be rejected");
        assert!(error.contains("result threshold"));
        assert!(!error.contains("weight"));
    }

    #[test]
    fn plan_caps_the_sub_query_count() {
        let strategy = AggregationScoreStrategy::Max;
        let many: Vec<SubQuery> = (0..super::MAX_SUB_QUERIES + 1)
            .map(|index| sub(&format!("q{index}"), 1.0))
            .collect();
        assert!(validate_plan(&many, 0.0, strategy).is_err());
    }

    #[test]
    fn merge_key_prefers_the_chunk_under_grouping() {
        let mut result = hit("chunk-1", 0.5);
        result.entity_ids = vec![cce_types::EntityId(7)];
        assert_eq!(
            aggregate_dedup_key(&result, true).as_deref(),
            Some("c:chunk-1")
        );
        assert_eq!(aggregate_dedup_key(&result, false).as_deref(), Some("e:7"));
    }

    #[test]
    fn merge_key_falls_back_to_the_location_for_identity_less_hits() {
        let mut result = hit("", 0.5);
        result.file_path = "src/lib.rs".to_string();
        result.start_line = 10;
        result.end_line = 20;
        assert_eq!(
            aggregate_dedup_key(&result, true).as_deref(),
            Some("l:src/lib.rs:10-20")
        );

        // Without a real range the hit stays keyless instead of collapsing
        // every such hit in the file onto one entry.
        result.end_line = result.start_line;
        assert!(aggregate_dedup_key(&result, true).is_none());
    }

    #[test]
    fn post_merge_reserve_scales_with_the_request_budget() {
        // A quarter of the budget protects the merge stages; the floor keeps a
        // usable slice on tiny test budgets and the ceiling stops a large budget
        // from reserving minutes for a bounded rerank.
        assert_eq!(post_processing_reserve_ms(300), 75);
        assert_eq!(post_processing_reserve_ms(30_000), 4_000);
        assert_eq!(post_processing_reserve_ms(60_000), 4_000);
        assert_eq!(post_processing_reserve_ms(0), 25);
    }
}
