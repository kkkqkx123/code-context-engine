//! Score normalization utilities for boosting
//!
//! Provides various normalization strategies to make scores comparable
//! before boost aggregation.

pub use cce_config::modules::search::NormalizationStrategy;

/// Normalize scores using the specified strategy
pub fn normalize_scores(
    scores: &mut [f32],
    strategy: &NormalizationStrategy,
) -> Result<(), String> {
    match strategy {
        NormalizationStrategy::MinMax => normalize_min_max(scores),
        NormalizationStrategy::ZScore => normalize_z_score(scores),
        NormalizationStrategy::None => Ok(()),
    }
}

/// Min-Max normalization to [0, 1]
fn normalize_min_max(scores: &mut [f32]) -> Result<(), String> {
    if scores.is_empty() {
        return Ok(());
    }

    let min = scores.iter().cloned().fold(f32::INFINITY, f32::min);
    let max = scores.iter().cloned().fold(f32::NEG_INFINITY, f32::max);

    if (max - min).abs() < f32::EPSILON {
        for score in scores.iter_mut() {
            *score = 0.5;
        }
        return Ok(());
    }

    for score in scores.iter_mut() {
        *score = (*score - min) / (max - min);
    }

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
        assert!((scores[0] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_normalize_min_max_single() {
        let mut scores = vec![0.5];
        normalize_min_max(&mut scores).unwrap();
        assert!((scores[0] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_normalize_min_max_empty() {
        let mut scores: Vec<f32> = vec![];
        normalize_min_max(&mut scores).unwrap();
        assert!(scores.is_empty());
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
