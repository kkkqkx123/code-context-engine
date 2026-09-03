//! Candidate selection for top-N results
//!
//! Selects the top-N candidates based on configured limits.

use crate::query::types::SearchResult;

/// Candidate selector that selects top-N results
#[derive(Clone)]
pub struct CandidateSelection;

impl CandidateSelection {
    /// Create a new candidate selector
    pub fn new() -> Self {
        Self
    }

    /// Select top-N candidates
    pub fn select(&self, results: Vec<SearchResult>, top_k: usize) -> Vec<SearchResult> {
        results.into_iter().take(top_k).collect()
    }
}

impl Default for CandidateSelection {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_select_top_k() {
        let selector = CandidateSelection::new();
        let results = vec![
            SearchResult {
                id: "1".to_string(),
                score: 0.9,
                ..Default::default()
            },
            SearchResult {
                id: "2".to_string(),
                score: 0.8,
                ..Default::default()
            },
            SearchResult {
                id: "3".to_string(),
                score: 0.7,
                ..Default::default()
            },
        ];

        let selected = selector.select(results, 2);
        assert_eq!(selected.len(), 2);
        assert_eq!(selected[0].id, "1");
        assert_eq!(selected[1].id, "2");
    }

    #[test]
    fn test_select_more_than_available() {
        let selector = CandidateSelection::new();
        let results = vec![SearchResult {
            id: "1".to_string(),
            score: 0.9,
            ..Default::default()
        }];

        let selected = selector.select(results, 5);
        assert_eq!(selected.len(), 1); // Only 1 available
    }
}
