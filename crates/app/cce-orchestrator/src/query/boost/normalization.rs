//! Score normalization utilities for boosting
//!
//! Provides various normalization strategies to make scores comparable
//! before boost aggregation. Min-max scaling delegates to the fusion
//! canonical implementation so the pipeline has a single min-max definition;
//! the Z-score strategy applies only to roughly normal score columns, never
//! to fusion key-granularity normalization.
//!
//! Single-path recall is normalized to feed the absolute `result.min_score`
//! threshold, so the default strategy is [`NormalizationStrategy::MaxRelative`]:
//! min-max always maps the worst element to exactly `0.0`, which every
//! positive threshold then drops, silently truncating one result per query.

pub use cce_config::modules::search::NormalizationStrategy;

use crate::query::retrieval::primitives::fusion::minmax_normalize as fusion_minmax;

/// Normalize scores using the specified strategy
pub fn normalize_scores(
    scores: &mut [f32],
    strategy: &NormalizationStrategy,
) -> Result<(), String> {
    match strategy {
        NormalizationStrategy::MaxRelative => {
            normalize_max_relative(scores);
            Ok(())
        }
        NormalizationStrategy::MinMax => normalize_min_max(scores),
        NormalizationStrategy::ZScore => normalize_z_score(scores),
        NormalizationStrategy::None => Ok(()),
    }
}

/// Scale every score by the largest positive score in the set.
///
/// The best candidate maps to `1.0` and every other score keeps its relative
/// distance to it, so the absolute threshold sees an interpretable
/// "share of the best hit" scale. Two properties matter for the threshold:
///
/// - nothing is pinned to `0.0`, unlike min-max, which maps the worst element
///   to zero and therefore loses it to any positive `min_score`;
/// - a tightly bunched result set stays bunched (raw `1.45` / `1.44` / `1.43`
///   normalizes to `1.00` / `0.99` / `0.99` instead of `1.0` / `0.5` / `0.0`),
///   so min-max's artificial spread no longer decides what survives.
///
/// Negative scores stay negative under the same divisor, and a set with no
/// positive score (all zero or all negative) is left untouched: there is no
/// "best hit" to scale against, and those candidates are meant to be filtered
/// by the threshold on their original scale.
fn normalize_max_relative(scores: &mut [f32]) {
    let max = scores
        .iter()
        .copied()
        .filter(|s| s.is_finite())
        .fold(f32::NEG_INFINITY, f32::max);
    if !max.is_finite() || max <= 0.0 {
        return;
    }
    for score in scores.iter_mut() {
        if score.is_finite() {
            *score /= max;
        }
    }
}

/// Min-Max normalization to [0, 1], delegating to the fusion canonical form.
///
/// Degenerate inputs (all equal, single element) map to full credit per the
/// shared contract; the shift is uniform so candidate order is preserved.
fn normalize_min_max(scores: &mut [f32]) -> Result<(), String> {
    if scores.is_empty() {
        return Ok(());
    }

    let normalized = fusion_minmax(scores);
    scores.copy_from_slice(&normalized);

    Ok(())
}

/// Z-score normalization (standardization)
fn normalize_z_score(scores: &mut [f32]) -> Result<(), String> {
    if scores.is_empty() {
        return Ok(());
    }

    let mean = scores.iter().sum::<f32>() / scores.len() as f32;
    let variance = scores.iter().map(|s| (s - mean).powi(2)).sum::<f32>() / scores.len() as f32;
    let std_dev = variance.sqrt();

    if std_dev < f32::EPSILON {
        for score in scores.iter_mut() {
            *score = 0.0;
        }
        return Ok(());
    }

    for score in scores.iter_mut() {
        *score = (*score - mean) / std_dev;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_min_max() {
        let mut scores = vec![0.1, 0.5, 0.9];
        normalize_min_max(&mut scores).unwrap();
        assert!((scores[0] - 0.0).abs() < 1e-6);
        assert!((scores[1] - 0.5).abs() < 1e-6);
        assert!((scores[2] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_normalize_min_max_constant() {
        let mut scores = vec![0.5, 0.5, 0.5];
        normalize_min_max(&mut scores).unwrap();
        assert!((scores[0] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_normalize_min_max_single() {
        let mut scores = vec![0.5];
        normalize_min_max(&mut scores).unwrap();
        assert!((scores[0] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_normalize_min_max_empty() {
        let mut scores: Vec<f32> = vec![];
        normalize_min_max(&mut scores).unwrap();
        assert!(scores.is_empty());
    }

    #[test]
    fn max_relative_keeps_every_candidate_above_a_positive_threshold() {
        // Raw BM25 scores for a small corpus. Min-max turned this into
        // [1.0, 0.0] and the default `min_score = 0.25` dropped the second
        // hit, so every query lost its last result.
        let mut scores = vec![1.4477, 1.2698];
        normalize_scores(&mut scores, &NormalizationStrategy::MaxRelative).unwrap();
        assert!((scores[0] - 1.0).abs() < 1e-6);
        assert!((scores[1] - 1.2698 / 1.4477).abs() < 1e-6);
        assert!(
            scores.iter().all(|s| *s >= 0.25),
            "no candidate may fall under the default threshold: {scores:?}"
        );
    }

    #[test]
    fn max_relative_keeps_a_tight_cluster_tight() {
        // Min-max would stretch this cluster to [1.0, 0.5, 0.0] and lose two
        // of three hits; the relative scale keeps the true closeness.
        let mut scores = vec![1.45, 1.44, 1.43];
        normalize_scores(&mut scores, &NormalizationStrategy::MaxRelative).unwrap();
        assert!((scores[0] - 1.0).abs() < 1e-6);
        assert!(scores.iter().all(|s| *s > 0.98), "{scores:?}");
    }

    #[test]
    fn max_relative_preserves_negative_scores() {
        // A negative cosine similarity stays negative (and is filtered on its
        // own merits) instead of being shifted into the positive range.
        let mut scores = vec![0.5, -0.25];
        normalize_scores(&mut scores, &NormalizationStrategy::MaxRelative).unwrap();
        assert!((scores[0] - 1.0).abs() < 1e-6);
        assert!((scores[1] + 0.5).abs() < 1e-6);
    }

    #[test]
    fn max_relative_leaves_sets_without_a_positive_score_untouched() {
        // No "best hit" to scale against: keep the raw scale so the threshold
        // still sees the real values.
        let mut scores = vec![0.0, -0.4, -0.9];
        normalize_scores(&mut scores, &NormalizationStrategy::MaxRelative).unwrap();
        assert_eq!(scores, vec![0.0, -0.4, -0.9]);

        let mut empty: Vec<f32> = vec![];
        normalize_scores(&mut empty, &NormalizationStrategy::MaxRelative).unwrap();
        assert!(empty.is_empty());
    }

    #[test]
    fn default_strategy_does_not_drop_the_worst_candidate() {
        // The default must be the strategy that survives the default
        // threshold; min-max is opt-in only.
        assert_eq!(
            NormalizationStrategy::default(),
            NormalizationStrategy::MaxRelative
        );
        let mut scores = vec![3.0, 1.0];
        normalize_scores(&mut scores, &NormalizationStrategy::default()).unwrap();
        assert!(scores[1] > 0.0, "worst candidate must not become zero");
    }

    #[test]
    fn test_normalize_z_score() {
        let mut scores = vec![1.0, 2.0, 3.0];
        normalize_z_score(&mut scores).unwrap();
        // Population std dev for [1,2,3] = sqrt(2/3)
        let expected_z0 = (1.0 - 2.0) / (2.0_f32 / 3.0_f32).sqrt();
        let expected_z2 = (3.0 - 2.0) / (2.0_f32 / 3.0_f32).sqrt();
        assert!((scores[0] - expected_z0).abs() < 1e-6);
        assert!((scores[1] - 0.0).abs() < 1e-6);
        assert!((scores[2] - expected_z2).abs() < 1e-6);
    }

    #[test]
    fn test_normalize_z_score_constant() {
        let mut scores = vec![0.5, 0.5, 0.5];
        normalize_z_score(&mut scores).unwrap();
        for s in scores {
            assert!((s - 0.0).abs() < 1e-6);
        }
    }

    #[test]
    fn test_normalize_empty() {
        let mut scores: Vec<f32> = vec![];
        normalize_z_score(&mut scores).unwrap();
        assert!(scores.is_empty());
    }

    #[test]
    fn test_normalize_strategy_dispatch() {
        let mut scores = vec![0.1, 0.5, 0.9];
        normalize_scores(&mut scores, &NormalizationStrategy::MinMax).unwrap();
        assert!((scores[0] - 0.0).abs() < 1e-6);
        assert!((scores[2] - 1.0).abs() < 1e-6);

        let mut scores = vec![0.1, 0.5, 0.9];
        normalize_scores(&mut scores, &NormalizationStrategy::None).unwrap();
        assert!((scores[0] - 0.1).abs() < 1e-6);
        assert!((scores[2] - 0.9).abs() < 1e-6);
    }
}
