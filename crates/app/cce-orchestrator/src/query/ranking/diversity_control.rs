//! Diversity control for search results
//!
//! Limits the number of results per file to ensure diversity in the result set.

use std::collections::HashMap;

use crate::query::types::SearchResult;

/// Diversity controller that limits results per file
#[derive(Clone)]
pub struct DiversityControl;

impl DiversityControl {
    /// Create a new diversity controller
    pub fn new() -> Self {
        Self
    }

    /// Apply per-file diversity control
    ///
    /// Limits the number of results from each file to ensure diversity.
    pub fn apply(&self, results: Vec<SearchResult>, max_per_file: usize) -> Vec<SearchResult> {
        let mut file_counts: HashMap<String, usize> = HashMap::new();
        let mut diversified = Vec::new();

        for result in results {
            let count = file_counts.entry(result.file_path.clone()).or_insert(0);
            if *count < max_per_file {
                diversified.push(result);
                *count += 1;
            }
        }

        diversified
    }
}

impl Default for DiversityControl {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diversity_control() {
        let controller = DiversityControl::new();
        let results = vec![
            SearchResult {
                id: "1".to_string(),
                file_path: "file1.rs".to_string(),
                score: 0.9,
                ..Default::default()
            },
            SearchResult {
                id: "2".to_string(),
                file_path: "file1.rs".to_string(),
                score: 0.8,
                ..Default::default()
            },
            SearchResult {
                id: "3".to_string(),
                file_path: "file1.rs".to_string(),
                score: 0.7,
                ..Default::default()
            },
            SearchResult {
                id: "4".to_string(),
                file_path: "file2.rs".to_string(),
                score: 0.85,
                ..Default::default()
            },
        ];

        let processed = controller.apply(results, 2);

        // Should have 3 results: 2 from file1.rs, 1 from file2.rs
        assert_eq!(processed.len(), 3);
        assert_eq!(processed[0].id, "1");
        assert_eq!(processed[1].id, "2");
        assert_eq!(processed[2].id, "4");
    }
}
