//! Score normalization for hybrid fusion.
//!
//! Canonical home of min-max normalization for the query pipeline: the boost
//! layer delegates to [`minmax_normalize`] instead of keeping its own copy.
//!
//! Key-granularity normalization (the one fusion actually consumes) applies
//! two extra rules on top of plain min-max: an outlier-resistant range for
//! path sets large enough to have a meaningful body, and an explicit
//! degenerate outcome so "no spread to normalize against" stays observable
//! instead of silently producing full marks.

use std::collections::HashMap;

use super::aligner::best_per_key;
use crate::query::types::SearchResult;

/// Path size from which quantile clipping becomes meaningful.
///
/// Below this the quantile estimate is dominated by the sample itself, so
/// clipping would only redistribute noise.
const ROBUST_MIN_KEYS: usize = 8;

/// Quantile trimmed from each end before min-max normalization.
///
/// An extreme outlier (a single near-duplicate hit scoring far above the
/// body) otherwise owns the whole range and compresses every other key into
/// a narrow band. Trimming 10% per side keeps the body's spread while the
/// trimmed keys still land inside `[0, 1]`.
const ROBUST_TRIM_QUANTILE: f32 = 0.10;

/// Normalize a slice of scores to [0.0, 1.0] using min-max normalization.
///
/// Returns the normalized scores. A degenerate input (all scores equal, or a
/// single score) has no spread to normalize against and maps to full credit
/// (`1.0` for every element): each key is tied for best on its path, and the
/// path weight alone controls its contribution. Uniform shifts preserve
/// ranking; only absolute score scales change. If the input is empty, returns
/// an empty vec.
pub fn minmax_normalize(scores: &[f32]) -> Vec<f32> {
    if scores.is_empty() {
        return Vec::new();
    }
    let (min, max) = range(scores);
    let range = max - min;
    if range <= f32::EPSILON {
        return vec![1.0; scores.len()];
    }
    scores.iter().map(|&s| (s - min) / range).collect()
}

/// Per alignment key: `(result index, normalized score)`.
pub type KeyScores = HashMap<String, (usize, f32)>;

/// Per-path normalization result.
#[derive(Debug, Clone)]
pub struct NormalizedPath {
    /// `key -> (result index, normalized score)`.
    pub scores: KeyScores,
    /// True when the path had no score spread (single key or all equal), so
    /// every key maps to full credit and the ordering carries no signal.
    pub degenerate: bool,
}

impl NormalizedPath {
    /// Number of keys in this path.
    pub fn len(&self) -> usize {
        self.scores.len()
    }

    /// Whether the path produced no keys at all.
    pub fn is_empty(&self) -> bool {
        self.scores.is_empty()
    }
}

/// Normalize the per-key best scores with min-max, returning the key map plus
/// whether the input was degenerate.
///
/// Keys are sorted before pairing so the order of the normalized values does
/// not depend on HashMap iteration order. When the path holds at least
/// [`ROBUST_MIN_KEYS`] keys the range is estimated on the 10th/90th
/// percentiles and raw scores are clamped into it first, so a single outlier
/// cannot flatten the body of the distribution.
pub fn normalize_by_key(map: HashMap<String, (usize, f32)>) -> NormalizedPath {
    let mut entries: Vec<(String, (usize, f32))> = map.into_iter().collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let mut raw: Vec<f32> = entries.iter().map(|(_, (_, s))| *s).collect();
    let degenerate = is_degenerate(&raw);
    if !degenerate && raw.len() >= ROBUST_MIN_KEYS {
        let (lo, hi) = trimmed_range(&raw, ROBUST_TRIM_QUANTILE);
        for score in &mut raw {
            *score = score.clamp(lo, hi);
        }
    }
    let normalized = minmax_normalize(&raw);
    let scores = entries
        .into_iter()
        .zip(normalized)
        .map(|((key, (i, _)), n)| (key, (i, n)))
        .collect();
    NormalizedPath { scores, degenerate }
}

/// Normalize scores within each path's results by alignment key.
///
/// Returns the key map for both paths, each carrying its own degenerate flag.
pub fn normalize_path_scores(
    vector_results: &[SearchResult],
    bm25_results: &[SearchResult],
) -> (NormalizedPath, NormalizedPath) {
    let vector_by_key = normalize_by_key(best_per_key(vector_results, |r| r.vector_score));
    let bm25_by_key = normalize_by_key(best_per_key(bm25_results, |r| r.bm25_score.unwrap_or(0.0)));
    (vector_by_key, bm25_by_key)
}

/// Min and max of a non-empty score slice.
fn range(scores: &[f32]) -> (f32, f32) {
    let min = scores.iter().copied().fold(f32::MAX, f32::min);
    let max = scores.iter().copied().fold(f32::MIN, f32::max);
    (min, max)
}

/// Whether a score slice has no spread to normalize against.
fn is_degenerate(scores: &[f32]) -> bool {
    if scores.len() <= 1 {
        return true;
    }
    let (min, max) = range(scores);
    max - min <= f32::EPSILON
}

/// Range after trimming `quantile` of the mass from each end.
///
/// Uses linear interpolation on the sorted copy, matching the percentile
/// definition used by the benchmark tooling so thresholds documented against
/// it stay comparable.
fn trimmed_range(scores: &[f32], quantile: f32) -> (f32, f32) {
    let mut sorted = scores.to_vec();
    sorted.sort_by(f32::total_cmp);
    let last = sorted.len() - 1;
    let lo_index = (quantile * last as f32).round() as usize;
    let hi_index = ((1.0 - quantile) * last as f32).round() as usize;
    let lo = sorted[lo_index.min(last)];
    let hi = sorted[hi_index.min(last)];
    if hi - lo <= f32::EPSILON {
        (sorted[0], sorted[last])
    } else {
        (lo, hi)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keyed(raw: &[(&str, f32)]) -> HashMap<String, (usize, f32)> {
        raw.iter()
            .enumerate()
            .map(|(i, (key, score))| (key.to_string(), (i, *score)))
            .collect()
    }

    #[test]
    fn single_key_is_flagged_degenerate() {
        let path = normalize_by_key(keyed(&[("e:1", 0.9)]));
        assert!(path.degenerate);
        assert!((path.scores["e:1"].1 - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn equal_scores_are_flagged_degenerate() {
        let path = normalize_by_key(keyed(&[("e:1", 0.5), ("e:2", 0.5), ("e:3", 0.5)]));
        assert!(path.degenerate);
        assert!(
            path.scores
                .values()
                .all(|(_, n)| (*n - 1.0).abs() < f32::EPSILON)
        );
    }

    #[test]
    fn spread_input_is_not_degenerate() {
        let path = normalize_by_key(keyed(&[("e:1", 0.2), ("e:2", 0.6)]));
        assert!(!path.degenerate);
        assert!((path.scores["e:1"].1 - 0.0).abs() < 1e-6);
        assert!((path.scores["e:2"].1 - 1.0).abs() < 1e-6);
    }

    #[test]
    fn outlier_is_trimmed_so_the_body_keeps_its_spread() {
        // 8 keys in the body plus one extreme outlier: without trimming the
        // body would be squeezed into [0, 0.1].
        let mut raw: Vec<(String, f32)> = (0..8)
            .map(|i| (format!("e:{i}"), 0.30 + i as f32 * 0.01))
            .collect();
        raw.push(("e:99".to_string(), 5.0));
        let path = normalize_by_key(keyed(
            &raw.iter()
                .map(|(k, s)| (k.as_str(), *s))
                .collect::<Vec<_>>(),
        ));
        let body_top = path.scores["e:7"].1;
        assert!(
            body_top > 0.5,
            "body keys must keep spread after trimming, got {body_top}"
        );
        // Trimmed keys still land inside the normalized interval.
        assert!((0.0..=1.0).contains(&path.scores["e:99"].1));
    }

    #[test]
    fn small_paths_are_not_trimmed() {
        // Three keys: trimming would collapse the extremes onto each other.
        let path = normalize_by_key(keyed(&[("e:1", 0.1), ("e:2", 0.2), ("e:3", 0.3)]));
        assert!((path.scores["e:1"].1 - 0.0).abs() < 1e-6);
        assert!((path.scores["e:3"].1 - 1.0).abs() < 1e-6);
    }
}
