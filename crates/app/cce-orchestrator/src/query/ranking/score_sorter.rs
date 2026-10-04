//! Score sorter for search results
//!
//! Handles deterministic sorting of results by score.

use crate::query::types::SearchResult;

/// Score sorter that sorts results by score in descending order
#[derive(Clone)]
pub struct ScoreSorter;

impl ScoreSorter {
    /// Create a new score sorter
    pub fn new() -> Self {
        Self
    }

    /// Sort results by score (descending) with deterministic tie-breaking.
    ///
    /// Ties are broken by (entity_id, segment_id, chunk_id) — the same key
    /// chain used by the fusion merger — so reranker output order is stable
    /// across requests.
    pub fn sort(&self, mut results: Vec<SearchResult>) -> Vec<SearchResult> {
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    let a_key = (
                        a.entity_ids.first().map(|e| e.0),
                        a.segment_id.clone().unwrap_or_default(),
                        a.id.clone(),
                    );
                    let b_key = (
                        b.entity_ids.first().map(|e| e.0),
                        b.segment_id.clone().unwrap_or_default(),
                        b.id.clone(),
                    );
                    a_key.cmp(&b_key)
                })
        });
        results
    }
}

impl Default for ScoreSorter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sort_by_score() {
        let sorter = ScoreSorter::new();
        let results = vec![
            SearchResult {
                id: "1".to_string(),
                score: 0.7,
                ..Default::default()
            },
            SearchResult {
                id: "2".to_string(),
                score: 0.9,
                ..Default::default()
            },
            SearchResult {
                id: "3".to_string(),
                score: 0.8,
                ..Default::default()
            },
        ];

        let sorted = sorter.sort(results);
        assert_eq!(sorted[0].id, "2"); // Highest score first
        assert_eq!(sorted[1].id, "3");
        assert_eq!(sorted[2].id, "1");
    }
}
