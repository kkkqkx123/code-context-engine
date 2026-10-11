//! Search result types

use std::collections::HashMap;

use cce_types::EntityId;

use super::content_reference::ContentState;

/// Unified search result item
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// Result ID
    pub id: String,
    /// All entity IDs associated with this result.
    /// A single chunk may contain multiple entities; this field captures all of them.
    /// Hybrid fusion expands multi-entity results so that after fusion each entry
    /// carries exactly one entity (code chunks). The entity a result "represents"
    /// is decided at query time by expansion + dedup scoring.
    pub entity_ids: Vec<EntityId>,
    /// Segment ID for hybrid fusion alignment when entity_ids is empty.
    /// Always populated. Two chunks from the same logical segment share the same
    /// segment_id, enabling BM25 ↔ vector matching for document/plain-text chunks.
    pub segment_id: Option<String>,
    /// Entity type (function, class, etc.)
    pub kind: String,
    /// Entity name
    pub name: String,
    /// File path
    pub file_path: String,
    /// Unified relevance score (post-fusion)
    pub score: f32,
    /// Original score from retrieval
    pub original_score: f32,
    /// Vector similarity score (dense)
    pub vector_score: f32,
    /// BM25 score (if available, from consensus fusion)
    pub bm25_score: Option<f32>,
    /// Source identifiers (e.g., "vector", "bm25")
    pub sources: Vec<String>,
    /// Code chunk content
    pub content: String,
    /// Start line
    pub start_line: u32,
    /// End line
    pub end_line: u32,
    /// Whether this result was boosted
    pub is_boosted: bool,
    /// Boost reason (if boosted)
    pub boost_reason: Option<String>,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
    /// Serialized pattern detection information (JSON) from EntityGroup
    /// Populated during retrieval from Qdrant payload.
    /// Consumers can deserialize into PatternInfo for pattern-aware processing.
    pub pattern_info: Option<String>,
    /// File category for category-aware search (e.g., "test", "config", "normal")
    pub category: Option<String>,
    /// Whether the stored content was token-budget truncated during indexing.
    pub truncated: bool,
    /// Whether `content` carries a full body or a file-and-range reference.
    pub content_state: ContentState,
}

impl SearchResult {
    /// Stable content-coordinate ordering used to break score ties.
    ///
    /// Index-assigned identifiers (entity row ids, segment ids, chunk ids)
    /// are reallocated on every rebuild, so ties must never resolve on them.
    /// File path, line range, kind and name survive rebuilds and read in the
    /// same order the renderer groups units. The chunk id stays only as a
    /// same-run uniqueness fallback once every stable field compares equal.
    pub fn cmp_stable(&self, other: &Self) -> std::cmp::Ordering {
        (
            &self.file_path,
            self.start_line,
            self.end_line,
            &self.kind,
            &self.name,
            &self.id,
        )
            .cmp(&(
                &other.file_path,
                other.start_line,
                other.end_line,
                &other.kind,
                &other.name,
                &other.id,
            ))
    }

    /// Score-descending ordering with the stable tie-break above, shared by
    /// every stage that ranks hits so equal scores read identically across
    /// rebuilds instead of following identifier allocation order.
    pub fn cmp_score_desc_then_stable(&self, other: &Self) -> std::cmp::Ordering {
        other
            .score
            .total_cmp(&self.score)
            .then_with(|| self.cmp_stable(other))
    }
}

impl Default for SearchResult {
    fn default() -> Self {
        Self {
            id: String::new(),
            entity_ids: Vec::new(),
            segment_id: None,
            kind: String::new(),
            name: String::new(),
            file_path: String::new(),
            score: 0.0,
            original_score: 0.0,
            vector_score: 0.0,
            bm25_score: None,
            sources: Vec::new(),
            content: String::new(),
            start_line: 0,
            end_line: 0,
            is_boosted: false,
            boost_reason: None,
            metadata: HashMap::new(),
            pattern_info: None,
            category: None,
            truncated: false,
            content_state: ContentState::Full,
        }
    }
}

/// Boost statistics for search results
#[derive(Debug, Clone, Default)]
pub struct BoostStats {
    /// Total number of results
    pub total_results: usize,
    /// Number of boosted results
    pub boosted_results: usize,
    /// Boost rate (boosted / total)
    pub boost_rate: f32,
    /// Average boost amount
    pub avg_boost: f32,
}

impl BoostStats {
    /// Calculate boost statistics from search results
    pub fn from_results(results: &[SearchResult]) -> Self {
        let total_results = results.len();
        let boosted_results = results.iter().filter(|r| r.is_boosted).count();

        let boost_rate = if total_results > 0 {
            boosted_results as f32 / total_results as f32
        } else {
            0.0
        };

        let avg_boost = if boosted_results > 0 {
            let total_boost: f32 = results
                .iter()
                .filter(|r| r.is_boosted)
                .map(|r| r.score - r.original_score)
                .sum();
            total_boost / boosted_results as f32
        } else {
            0.0
        };

        BoostStats {
            total_results,
            boosted_results,
            boost_rate,
            avg_boost,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_boost_stats() {
        let results = vec![
            SearchResult {
                id: "1".to_string(),
                score: 0.96,
                original_score: 0.8,
                vector_score: 0.8,
                is_boosted: true,
                ..Default::default()
            },
            SearchResult {
                id: "2".to_string(),
                score: 0.7,
                original_score: 0.7,
                vector_score: 0.7,
                is_boosted: false,
                ..Default::default()
            },
        ];

        let stats = BoostStats::from_results(&results);

        assert_eq!(stats.total_results, 2);
        assert_eq!(stats.boosted_results, 1);
        assert_eq!(stats.boost_rate, 0.5);
        assert!((stats.avg_boost - 0.16).abs() < 0.001);
    }

    fn tied_hit(id: &str, entity: u64, file: &str, start: u32) -> SearchResult {
        SearchResult {
            id: id.to_string(),
            entity_ids: vec![EntityId(entity)],
            segment_id: Some(format!("group_{entity}")),
            file_path: file.to_string(),
            start_line: start,
            end_line: start + 20,
            kind: "method".to_string(),
            name: "send_static_file".to_string(),
            score: 0.6443,
            original_score: 0.6443,
            ..Default::default()
        }
    }

    #[test]
    fn test_stable_tie_break_ignores_rebuild_ids() {
        // Same content coordinates under reallocated identifiers must compare
        // equal on the stable prefix; the chunk id only breaks what content
        // cannot distinguish within one run.
        let before = tied_hit("group_111_bm25_0", 111, "src/flask/app.py", 392);
        let after = tied_hit("group_77_bm25_0", 77, "src/flask/app.py", 392);
        assert_eq!(before.cmp_stable(&after), std::cmp::Ordering::Less);
        let mut pair = [after.clone(), before.clone()];
        pair.sort_by(|a, b| a.cmp_score_desc_then_stable(b));
        assert_eq!(pair[0].id, "group_111_bm25_0");
        assert_eq!(pair[1].id, "group_77_bm25_0");
    }

    #[test]
    fn test_score_desc_then_stable_orders_ties_by_content() {
        let blueprints = tied_hit("group_111_bm25_0", 111, "src/flask/blueprints.py", 82);
        let app = tied_hit("group_197_bm25_0", 197, "src/flask/app.py", 392);
        let higher = SearchResult {
            score: 0.9,
            ..tied_hit("group_1_bm25_0", 1, "src/z.rs", 1)
        };
        let mut results = [blueprints, higher, app];
        results.sort_by(|a, b| a.cmp_score_desc_then_stable(b));
        assert_eq!(results[0].file_path, "src/z.rs");
        assert_eq!(results[1].file_path, "src/flask/app.py");
        assert_eq!(results[2].file_path, "src/flask/blueprints.py");
    }
}
