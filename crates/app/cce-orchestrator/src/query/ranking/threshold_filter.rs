//! Threshold filter for search results
//!
//! Applies minimum score thresholds and result limits.

use crate::query::error::Result;
use crate::query::types::{SearchConfig, SearchResult};

/// Threshold filter that enforces score thresholds and result limits
#[derive(Clone)]
pub struct ThresholdFilter;

impl ThresholdFilter {
    /// Create a new threshold filter
    pub fn new() -> Self {
        Self
    }

    /// Apply final thresholds and limits
    pub fn apply(
        &self,
        mut results: Vec<SearchResult>,
        config: &SearchConfig,
    ) -> Result<Vec<SearchResult>> {
        // Apply minimum score threshold
        results.retain(|r| r.score >= config.result.min_score);

        // Apply result limit
        results.truncate(config.result.limit);

        Ok(results)
    }
}

impl Default for ThresholdFilter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::types::ResultFilterConfig;

    #[test]
    fn test_apply_thresholds() {
        let filter = ThresholdFilter::new();
        let config = SearchConfig {
            result: ResultFilterConfig {
                min_score: 0.5,
                limit: 2,
                ..Default::default()
            },
            ..Default::default()
        };

        let results = vec![
            SearchResult {
                id: "1".to_string(),
                score: 0.9,
                ..Default::default()
            },
            SearchResult {
                id: "2".to_string(),
                score: 0.7,
                ..Default::default()
            },
            SearchResult {
                id: "3".to_string(),
                score: 0.6,
                ..Default::default()
            },
            SearchResult {
                id: "4".to_string(),
                score: 0.4, // Below threshold
                ..Default::default()
            },
        ];

        let filtered = filter.apply(results, &config).unwrap();
        assert_eq!(filtered.len(), 2); // Limited to 2
        assert!(filtered.iter().all(|r| r.score >= 0.5)); // All above threshold
    }
}
