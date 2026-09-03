//! Rerank contract types (cross-layer contract)
//!
//! Moved from `cce_infrastructure::llm::services::rerank::types` so the plugin
//! rerank contract (`cce_core::plugin::CodePlugin::rerank`) can reference them
//! without depending on the infrastructure crate. The infrastructure crate
//! re-exports these from its original module path.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Rearrangement of candidates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RerankCandidate {
    /// Candidate ID
    pub id: String,
    /// Candidate content (code snippet or text)
    pub content: String,
    /// file path
    pub file_path: String,
    /// Initial score (from recall phase)
    pub initial_score: f32,
    /// Entity types (function/class, etc.)
    #[serde(default)]
    pub entity_type: Option<String>,
    /// Other metadata
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// Candidates after rearrangement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RerankedCandidate {
    /// Candidate ID
    pub id: String,
    /// Score after rearrangement
    pub rerank_score: f32,
    /// raw score
    pub initial_score: f32,
    /// Combined score (possibly combining raw and rearranged scores)
    pub final_score: f32,
    /// Ranking changes
    pub rank_change: i32,
    /// Rationale for rearrangement (optional, for debugging)
    #[serde(default)]
    pub reasoning: Option<String>,
}

/// Rearrangement results
///
/// The `prompt_tokens` / `total_tokens` / `elapsed_ms` fields are LLM-specific
/// accounting; plugin rerankers leave them at their defaults and only populate
/// `reranked_candidates`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RerankResult {
    /// Rearranged candidate list (sorted by new score)
    #[serde(default)]
    pub reranked_candidates: Vec<RerankedCandidate>,
    /// Number of tokens used
    #[serde(default)]
    pub prompt_tokens: u64,
    /// Total number of tokens
    #[serde(default)]
    pub total_tokens: u64,
    /// Rearrangement time (milliseconds)
    #[serde(default)]
    pub elapsed_ms: u64,
}

impl RerankResult {
    /// Create a result with only the reranked candidate list.
    pub fn new(reranked_candidates: Vec<RerankedCandidate>) -> Self {
        Self {
            reranked_candidates,
            prompt_tokens: 0,
            total_tokens: 0,
            elapsed_ms: 0,
        }
    }
}
