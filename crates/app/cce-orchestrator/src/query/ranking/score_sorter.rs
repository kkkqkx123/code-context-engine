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
    /// Ties resolve on stable content coordinates (file path, line range,
    /// kind, name), never on index-assigned identifiers, so equal scores read
    /// identically across index rebuilds.
    pub fn sort(&self, mut results: Vec<SearchResult>) -> Vec<SearchResult> {
        results.sort_by(|a, b| a.cmp_score_desc_then_stable(b));
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

    #[test]
    fn test_ties_break_by_stable_content_not_rebuild_ids() {
        use cce_types::EntityId;

        fn hit(id: &str, entity: u64, file: &str) -> SearchResult {
            SearchResult {
                id: id.to_string(),
                entity_ids: vec![EntityId(entity)],
                segment_id: Some(format!("group_{entity}")),
                file_path: file.to_string(),
                start_line: 1,
                end_line: 3,
                kind: "method".to_string(),
                name: "send_static_file".to_string(),
                score: 0.6443,
                original_score: 0.6443,
                ..Default::default()
            }
        }

        let sorter = ScoreSorter::new();
        // Identical content under reallocated identifiers, arriving in either
        // order, must sort identically: file path decides, not entity id.
        for results in [
            vec![
                hit("group_111_bm25_0", 111, "src/flask/blueprints.py"),
                hit("group_197_bm25_0", 197, "src/flask/app.py"),
            ],
            vec![
                hit("group_77_bm25_0", 77, "src/flask/app.py"),
                hit("group_224_bm25_0", 224, "src/flask/blueprints.py"),
            ],
        ] {
            let sorted = sorter.sort(results);
            assert_eq!(sorted[0].file_path, "src/flask/app.py");
            assert_eq!(sorted[1].file_path, "src/flask/blueprints.py");
        }
    }
}
