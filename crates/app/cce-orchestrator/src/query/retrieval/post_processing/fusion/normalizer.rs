//! Score normalization for hybrid fusion.

use std::collections::HashMap;

use super::aligner::best_per_key;
use crate::query::types::SearchResult;

/// Normalize a slice of scores to [0.0, 1.0] using min-max normalization.
///
/// Returns the normalized scores. If all scores are equal, returns all 1.0.
/// If the input is empty, returns an empty vec.
pub fn minmax_normalize(scores: &[f32]) -> Vec<f32> {
    if scores.is_empty() {
        return Vec::new();
    }

    let min = scores.iter().copied().fold(f32::MAX, f32::min);
    let max = scores.iter().copied().fold(f32::MIN, f32::max);

    let range = max - min;
    if range <= f32::EPSILON {
        return vec![1.0; scores.len()];
    }

    scores.iter().map(|&s| (s - min) / range).collect()
}

/// Normalize the per-key best scores with min-max, returning
/// `key -> (result index, normalized score)`.
///
/// Keys are sorted before pairing so the order of the normalized values does
/// not depend on HashMap iteration order.
pub fn normalize_by_key(
    map: HashMap<String, (usize, f32)>,
) -> HashMap<String, (usize, f32)> {
    let mut entries: Vec<(String, (usize, f32))> = map.into_iter().collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let raw: Vec<f32> = entries.iter().map(|(_, (_, s))| *s).collect();
    let normalized = minmax_normalize(&raw);
    entries
        .into_iter()
        .zip(normalized)
        .map(|((key, (i, _)), n)| (key, (i, n)))
        .collect()
}

/// Normalize scores within each path's results by alignment key.
///
/// Returns `key -> (result index, normalized score)` for both paths.
pub fn normalize_path_scores(
    vector_results: &[SearchResult],
    bm25_results: &[SearchResult],
) -> (
    HashMap<String, (usize, f32)>,
    HashMap<String, (usize, f32)>,
) {
    let vector_by_key = normalize_by_key(best_per_key(vector_results, |r| r.vector_score));
    let bm25_by_key = normalize_by_key(best_per_key(bm25_results, |r| r.bm25_score.unwrap_or(0.0)));
    (vector_by_key, bm25_by_key)
}
