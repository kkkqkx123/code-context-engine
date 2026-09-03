//! Post-retrieval score boosting module
//!
//! Provides unified score boosting through additive aggregation of multiple
//! boost sources (summary relevance, relation graph).
//! Each boost source contributes a capped additive value; the aggregator
//! sums all contributions and caps the total to prevent score overshoot.
//!
//! # Architecture
//!
//! ```text
//! Boost Layer (boosting)
//!     │
//!     ├── SummaryBoost (summary boost)
//!     │   └── File-level summary relevance contribution
//!     │
//!     ├── RelationBoost (relation boost)
//!     │   └── Call graph hop-decay contribution
//!     │
//!     └── UnifiedBoostAggregator (unified boost aggregation)
//!         └── Collects, caps, and applies all contributions to vector_score
//! ```
//!
//! # Boost Formula
//!
//! ```text
//! total_addition = Σ source_contributions  (each capped at max_source_boost)
//! capped_addition = min(total_addition, max_addition)
//! score = vector_score × (1.0 + capped_addition)
//! ```

use std::collections::HashMap;

use crate::query::types::SearchResult;

// Sub-modules: individual boost contributors
pub mod normalization;
pub mod relation;
pub mod summary;

// Re-exports
pub use normalization::{NormalizationStrategy, normalize_scores};
pub use relation::{RelationBoost, RelationType};
pub use summary::SummaryBoost;

/// A single boost contribution from one source for one candidate result.
#[derive(Debug, Clone)]
pub struct BoostContribution {
    /// The candidate result ID this contribution applies to
    pub candidate_id: String,
    /// Source identifier: "bm25", "summary", "relation"
    pub source: &'static str,
    /// Normalized boost addition value [0.0, max_source_boost]
    pub boost_value: f32,
    /// Original signal strength before normalization (for debugging)
    pub raw_signal: f32,
    /// Optional human-readable reason
    pub reason: Option<String>,
}

impl BoostContribution {
    pub fn new(
        candidate_id: String,
        source: &'static str,
        boost_value: f32,
        raw_signal: f32,
    ) -> Self {
        Self {
            candidate_id,
            source,
            boost_value,
            raw_signal,
            reason: None,
        }
    }

    pub fn with_reason(mut self, reason: String) -> Self {
        self.reason = Some(reason);
        self
    }
}

/// Aggregated boost result for a single candidate.
#[derive(Debug, Clone)]
pub struct AggregatedBoost {
    /// Per-source contributions
    pub contributions: Vec<BoostContribution>,
    /// Raw sum of all capped contribution values (before global cap)
    pub raw_total_addition: f32,
    /// Capped total addition (after applying max_addition)
    pub capped_addition: f32,
    /// Final multiplier = 1.0 + capped_addition
    pub effective_multiplier: f32,
}

/// Re-export BoostAggregationConfig from core config module.
pub use cce_config::modules::search::BoostAggregationConfig;

/// Apply unified boost aggregation to search results.
///
/// Takes a list of contributions from all boost sources and applies them
/// to the candidate results. Each source's contribution is independently
/// capped, then the total is globally capped before being applied to
/// `result.score = result.vector_score × (1.0 + capped_addition)`.
///
/// # Arguments
/// * `results` - Search results to boost (will be modified in place)
/// * `contributions` - Collected boost contributions from all sources
/// * `config` - Boost aggregation configuration
pub fn apply_boosts(
    results: &mut [SearchResult],
    contributions: Vec<BoostContribution>,
    config: &BoostAggregationConfig,
) {
    if !config.enabled {
        return;
    }
    if contributions.is_empty() {
        return;
    }

    // Group contributions by candidate_id
    let mut grouped: HashMap<String, Vec<BoostContribution>> = HashMap::new();
    for c in contributions {
        let id = c.candidate_id.clone();
        grouped.entry(id).or_default().push(c);
    }

    // Build per-result aggregated boosts
    let per_source_cap: HashMap<&str, f32> = {
        let mut m = HashMap::new();
        m.insert("summary", config.summary_max);
        m.insert("relation", config.relation_max);
        m
    };

    let mut aggregated_map: HashMap<String, AggregatedBoost> = HashMap::new();
    for (id, contribs) in grouped {
        // Cap each source independently
        let mut source_totals: HashMap<&str, f32> = HashMap::new();
        for c in &contribs {
            let cap = per_source_cap
                .get(c.source)
                .copied()
                .unwrap_or(config.max_source_boost);
            let entry = source_totals.entry(c.source).or_insert(0.0);
            *entry = (*entry + c.boost_value).min(cap);
        }

        // Collect capped contributions for record-keeping
        let capped_contribs: Vec<BoostContribution> = contribs
            .into_iter()
            .filter_map(|mut c| {
                let cap = per_source_cap
                    .get(c.source)
                    .copied()
                    .unwrap_or(config.max_source_boost);
                if c.boost_value > cap {
                    c.boost_value = cap;
                }
                if c.boost_value > 0.0 { Some(c) } else { None }
            })
            .collect();

        let raw_total: f32 = source_totals.values().sum();
        let capped_addition = raw_total.min(config.max_addition);

        aggregated_map.insert(
            id,
            AggregatedBoost {
                capped_addition,
                effective_multiplier: 1.0 + capped_addition,
                raw_total_addition: raw_total,
                contributions: capped_contribs,
            },
        );
    }

    // Apply to results
    for result in results.iter_mut() {
        if let Some(boost) = aggregated_map.remove(&result.id) {
            if boost.effective_multiplier <= 1.0 {
                continue;
            }

            let new_score = result.vector_score * boost.effective_multiplier;
            result.score = new_score;
            result
                .sources
                .extend(boost.contributions.iter().map(|c| c.source.to_string()));
            result.is_boosted = true;

            let reasons: Vec<String> = boost
                .contributions
                .iter()
                .map(|c| {
                    let reason = c.reason.as_deref().unwrap_or(c.source);
                    format!("{}(+{:.3})", reason, c.boost_value)
                })
                .collect();
            result.boost_reason = Some(format!(
                "agg(vector={:.3}, add={:.3}, capped={:.3}) [{}]",
                result.vector_score,
                boost.raw_total_addition,
                boost.capped_addition,
                reasons.join(", ")
            ));

            // Record individual contributions in metadata
            result.metadata.insert(
                "boost_contributions".to_string(),
                boost
                    .contributions
                    .iter()
                    .map(|c| format!("{}={:.3}", c.source, c.boost_value))
                    .collect::<Vec<_>>()
                    .join(","),
            );
        } else {
            // No boost for this result, keep base score
            result.score = result.vector_score;
        }
    }
}
