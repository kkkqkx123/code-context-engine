//! Search result types

use std::collections::HashMap;

use cce_types::EntityId;

/// Call relation info
#[derive(Debug, Clone)]
pub struct CallInfo {
    /// Function identifier
    pub id: EntityId,
    /// Function name
    pub name: String,
    /// File path
    pub file: String,
    /// Line number
    pub line: Option<u32>,
}

/// Entity relations
#[derive(Debug, Clone, Default)]
pub struct Relations {
    /// Functions that call this entity
    pub callers: Vec<CallInfo>,
    /// Functions called by this entity
    pub callees: Vec<CallInfo>,
}

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
    /// Code snippet (raw code, if available)
    pub snippet: Option<String>,
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
    /// Call relations
    pub relations: Option<Relations>,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
    /// Serialized pattern detection information (JSON) from EntityGroup
    /// Populated during retrieval from Qdrant payload.
    /// Consumers can deserialize into PatternInfo for pattern-aware processing.
    pub pattern_info: Option<String>,
    /// File category for category-aware search (e.g., "test", "config", "normal")
    pub category: Option<String>,
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
}
