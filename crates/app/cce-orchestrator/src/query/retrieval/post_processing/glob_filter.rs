//! Glob filter for search result path filtering
//!
//! Applies include/exclude glob patterns against file paths of search results
//! as a post-retrieval filtering step. This ensures that patterns provided by
//! users via `include_patterns` / `exclude_patterns` are actually applied.
//!
//! # Behavior
//!
//! - `include_patterns` (if non-empty): result must match at least one pattern
//! - `exclude_patterns` (if non-empty): result must NOT match any pattern
//! - Both can be active simultaneously
//!
//! # Position in Pipeline
//!
//! Glob filtering runs **before** fusion and ranking. This minimizes unnecessary
//! computation by dropping excluded results as early as possible.

use crate::query::error::Result;
use crate::query::types::SearchResult;
use globset::{Glob, GlobSet, GlobSetBuilder};

/// Glob filter that filters search results by file path patterns
#[derive(Clone)]
pub struct GlobFilter;

impl GlobFilter {
    /// Create a new glob filter
    pub fn new() -> Self {
        Self
    }

    /// Apply include/exclude glob patterns to results
    ///
    /// # Arguments
    ///
    /// * `results` - Search results to filter
    /// * `include_patterns` - If non-empty, results must match at least one pattern
    /// * `exclude_patterns` - If non-empty, results must not match any pattern
    ///
    /// # Returns
    ///
    /// Filtered search results
    pub fn apply(
        &self,
        results: Vec<SearchResult>,
        include_patterns: &[String],
        exclude_patterns: &[String],
    ) -> Result<Vec<SearchResult>> {
        // Fast path: no patterns to apply
        if include_patterns.is_empty() && exclude_patterns.is_empty() {
            return Ok(results);
        }

        let include_set = if !include_patterns.is_empty() {
            Some(Self::build_glob_set(include_patterns)?)
        } else {
            None
        };

        let exclude_set = if !exclude_patterns.is_empty() {
            Some(Self::build_glob_set(exclude_patterns)?)
        } else {
            None
        };

        Ok(results
            .into_iter()
            .filter(|r| {
                let path = &r.file_path;

                // Must match at least one include pattern (if specified)
                if let Some(ref include_set) = include_set {
                    if !include_set.is_match(path) {
                        return false;
                    }
                }

                // Must NOT match any exclude pattern (if specified)
                if let Some(ref exclude_set) = exclude_set {
                    if exclude_set.is_match(path) {
                        return false;
                    }
                }

                true
            })
            .collect())
    }

    /// Compile a list of glob patterns into a GlobSet
    fn build_glob_set(patterns: &[String]) -> Result<GlobSet> {
        let mut builder = GlobSetBuilder::new();
        for pattern in patterns {
            let glob = Glob::new(pattern).map_err(|e| {
                crate::query::error::QueryError::invalid(
                    format!("Invalid glob pattern '{}': {}", pattern, e).as_str(),
                )
            })?;
            builder.add(glob);
        }
        builder.build().map_err(|e| {
            crate::query::error::QueryError::invalid(
                format!("Failed to compile glob patterns: {}", e).as_str(),
            )
        })
    }
}

impl Default for GlobFilter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_result(file_path: &str) -> SearchResult {
        SearchResult {
            id: "test".to_string(),
            entity_ids: Vec::new(),
            segment_id: None,
            kind: String::new(),
            name: String::new(),
            file_path: file_path.to_string(),
            score: 1.0,
            original_score: 1.0,
            vector_score: 1.0,
            bm25_score: None,
            sources: vec![],
            snippet: None,
            content: String::new(),
            start_line: 0,
            end_line: 0,
            is_boosted: false,
            boost_reason: None,
            relations: None,
            metadata: HashMap::new(),
            pattern_info: None,
            category: None,
        }
    }

    #[test]
    fn test_no_patterns_returns_all() {
        let filter = GlobFilter::new();
        let results = vec![make_result("src/main.rs"), make_result("tests/test.rs")];
        let filtered = filter.apply(results, &[], &[]).unwrap();
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_include_patterns() {
        let filter = GlobFilter::new();
        let results = vec![
            make_result("src/main.rs"),
            make_result("src/lib.rs"),
            make_result("tests/test.rs"),
        ];
        let filtered = filter
            .apply(results, &["src/**/*.rs".to_string()], &[])
            .unwrap();
        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().all(|r| r.file_path.starts_with("src/")));
    }

    #[test]
    fn test_exclude_patterns() {
        let filter = GlobFilter::new();
        let results = vec![make_result("src/main.rs"), make_result("tests/test.rs")];
        let filtered = filter
            .apply(results, &[], &["tests/**".to_string()])
            .unwrap();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].file_path, "src/main.rs");
    }

    #[test]
    fn test_both_include_and_exclude() {
        let filter = GlobFilter::new();
        let results = vec![
            make_result("src/main.rs"),
            make_result("src/lib.rs"),
            make_result("tests/test.rs"),
            make_result("README.md"),
        ];
        // Only `*.rs` files under `src/`, excluding `lib.rs`
        let filtered = filter
            .apply(
                results,
                &["**/*.rs".to_string()],
                &["**/lib.rs".to_string()],
            )
            .unwrap();
        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().all(|r| r.file_path.ends_with(".rs")));
        assert!(filtered.iter().all(|r| !r.file_path.contains("lib.rs")));
    }

    #[test]
    fn test_empty_results() {
        let filter = GlobFilter::new();
        let filtered = filter.apply(vec![], &["*.rs".to_string()], &[]).unwrap();
        assert!(filtered.is_empty());
    }

    #[test]
    fn test_no_match_returns_empty() {
        let filter = GlobFilter::new();
        let results = vec![make_result("src/main.rs")];
        let filtered = filter.apply(results, &["*.py".to_string()], &[]).unwrap();
        assert!(filtered.is_empty());
    }
}
