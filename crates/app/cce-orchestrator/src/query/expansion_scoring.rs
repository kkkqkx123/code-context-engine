//! Expansion neighbour scoring for the query path.
//!
//! Pure functions turning resolved-relation metadata into ordering scores.
//! The formula is a product of three factors: edge confidence (reusing the
//! graph traversal edge load), seed weight (primary hit score scaled by seed
//! overlap and rank), and query bonus (neighbour name overlap with query
//! terms, neutral when there is nothing to match). Fan-in acts as a
//! lightweight centrality multiplier. Every factor degrades to neutral when
//! its switch is off, and the master switch restores the historical all-zero
//! scores so callers keep traversal discovery order.

use cce_types::ResolvedRelation;

/// Per-request scoring switches, derived from the annotation config.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExpansionScoring {
    /// Master switch: false forces every score to zero (legacy order).
    pub enable: bool,
    /// Query-term name bonus on/off.
    pub query_bonus: bool,
    /// Caller fan-in bonus on/off.
    pub fanin_bonus: bool,
}

impl Default for ExpansionScoring {
    fn default() -> Self {
        Self {
            enable: true,
            query_bonus: true,
            fanin_bonus: true,
        }
    }
}

impl From<&cce_config::modules::search::RelationAnnotationConfig> for ExpansionScoring {
    fn from(config: &cce_config::modules::search::RelationAnnotationConfig) -> Self {
        Self {
            enable: config.expansion_scoring,
            query_bonus: config.expansion_query_scoring,
            fanin_bonus: config.expansion_fanin_scoring,
        }
    }
}

/// Extra confidence for a disambiguated overload edge.
const OVERLOAD_BONUS: f32 = 1.1;
/// Extra confidence for a call edge carrying receiver type information.
const TYPED_CONTEXT_BONUS: f32 = 1.05;
/// Discount for edges inside a conditional-compilation branch.
const CFG_DISCOUNT: f32 = 0.7;
/// Per matched query term added to the query factor.
const QUERY_TERM_STEP: f32 = 0.2;
/// Cap on matched query terms counted toward the query factor.
const QUERY_TERM_CAP: usize = 3;
/// Per log2 unit of fan-in added to the fan-in factor.
const FANIN_STEP: f32 = 0.1;
/// Discount applied to self-evident edges that share their name with a
/// sibling edge instead of being dropped.
pub const SHARED_EVIDENT_DISCOUNT: f32 = 0.5;
/// Seed rank decay step: rank `n` keeps `1 / (1 + n * STEP)`.
const SEED_RANK_STEP: f32 = 0.1;

/// Split a query into lowercase alphanumeric terms for name matching.
pub fn split_query_terms(query: &str) -> Vec<String> {
    let mut terms = Vec::new();
    for term in query
        .split(|c: char| !c.is_alphanumeric())
        .map(|s| s.to_lowercase())
        .filter(|s| s.len() >= 2)
    {
        if !terms.contains(&term) {
            terms.push(term);
        }
    }
    terms
}

/// Whether a call context carries concrete receiver/owner type information.
///
/// Only statically dispatched calls gain confidence: direct calls report no
/// context at all, virtual dispatch resolves at runtime so its receiver
/// type does not disambiguate the edge, and empty payloads mean the type
/// was never recorded.
fn has_typed_context(context: &cce_types::relation::CallContext) -> bool {
    use cce_types::relation::CallContext as Context;
    match context {
        Context::InstanceMethod { receiver_type } => !receiver_type.is_empty(),
        Context::StaticMethod { owner_type } | Context::Constructor { owner_type } => {
            !owner_type.is_empty()
        }
        Context::Direct | Context::VirtualDispatch { .. } => false,
    }
}

/// Edge confidence from relation metadata.
///
/// Reuses the graph traversal edge load (type prior scaled by call heat),
/// then adjusts for resolution quality signals the load does not carry:
/// disambiguated overloads and statically typed call contexts gain
/// confidence, conditional-compilation edges lose it.
pub fn edge_confidence(relation: &ResolvedRelation) -> f32 {
    let mut score =
        super::graph::service::edge_load(&relation.relation_type, relation.call_frequency);
    if relation.overload_signature.is_some() {
        score *= OVERLOAD_BONUS;
    }
    if has_typed_context(&relation.call_context) {
        score *= TYPED_CONTEXT_BONUS;
    }
    if relation.cfg_condition.is_some() {
        score *= CFG_DISCOUNT;
    }
    sanitize(score)
}

/// Seed weight from the primary hit score, the seed overlap ratio and rank.
///
/// Overlap ratio is the hit-range overlap over the seed entity length, so a
/// fully covered member outranks a barely touched container. Rank decay keeps
/// the earlier (higher-overlap) seeds ahead without letting one seed starve
/// the rest.
pub fn seed_weight(primary_score: f32, overlap_ratio: f32, seed_rank: usize) -> f32 {
    sanitize(primary_score.max(0.0) * overlap_ratio.clamp(0.0, 1.0))
        / (1.0 + seed_rank as f32 * SEED_RANK_STEP)
}

/// Query factor from neighbour name overlap with query terms.
///
/// Neutral (1.0) when there are no terms or no match, so neighbours unrelated
/// to the query wording keep their structural score instead of zeroing out.
pub fn query_factor(neighbour_name: &str, query_terms: &[String]) -> f32 {
    if query_terms.is_empty() || neighbour_name.is_empty() {
        return 1.0;
    }
    let name = neighbour_name.to_lowercase();
    let hits = query_terms
        .iter()
        .filter(|term| name.contains(term.as_str()))
        .take(QUERY_TERM_CAP + 1)
        .count()
        .min(QUERY_TERM_CAP);
    1.0 + hits as f32 * QUERY_TERM_STEP
}

/// Fan-in factor from the caller count of the neighbour.
///
/// Logarithmic so a heavily referenced utility outranks a leaf without one
/// hub swamping every budget.
pub fn fanin_factor(fan_in: usize) -> f32 {
    1.0 + ((1 + fan_in) as f32).log2() * FANIN_STEP
}

/// Full expansion score for one neighbour unit.
///
/// Returns zero for every input when the master switch is off, preserving the
/// historical first-come budget order.
pub fn expansion_score(
    relation: &ResolvedRelation,
    seed_weight: f32,
    neighbour_name: &str,
    query_terms: &[String],
    fan_in: usize,
    scoring: ExpansionScoring,
) -> f32 {
    if !scoring.enable {
        return 0.0;
    }
    let query = if scoring.query_bonus {
        query_factor(neighbour_name, query_terms)
    } else {
        1.0
    };
    let fanin = if scoring.fanin_bonus {
        fanin_factor(fan_in)
    } else {
        1.0
    };
    sanitize(edge_confidence(relation) * seed_weight.max(0.0) * query * fanin)
}

/// Clamp a score to a finite non-negative value.
fn sanitize(score: f32) -> f32 {
    if score.is_finite() {
        score.max(0.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::{EntityId, RelationType, Span};

    fn relation(relation_type: RelationType) -> ResolvedRelation {
        ResolvedRelation {
            caller: EntityId(1),
            callee_id: Some(EntityId(2)),
            callee_name: "target".to_string(),
            relation_type,
            span: Span::default(),
            is_external: false,
            external_type: None,
            callee_symbol: None,
            stdlib_category: None,
            owner_type: None,
            call_context: cce_types::relation::CallContext::Direct,
            overload_signature: None,
            call_frequency: 1,
            cfg_condition: None,
        }
    }

    fn scoring() -> ExpansionScoring {
        ExpansionScoring::default()
    }

    #[test]
    fn direct_call_outranks_type_reference() {
        let direct = edge_confidence(&relation(RelationType::DirectCall));
        let reference = edge_confidence(&relation(RelationType::TypeReference));
        assert!(direct > reference);
    }

    #[test]
    fn repeated_edge_outranks_single_site() {
        let mut repeated = relation(RelationType::DirectCall);
        repeated.call_frequency = 4;
        assert!(edge_confidence(&repeated) > edge_confidence(&relation(RelationType::DirectCall)));
    }

    #[test]
    fn overload_and_typed_context_gain_cfg_loses() {
        use cce_types::relation::CallContext as Context;
        let mut rich = relation(RelationType::DirectCall);
        rich.overload_signature = Some("f(i32)".to_string());
        let plain = edge_confidence(&relation(RelationType::DirectCall));
        assert!(edge_confidence(&rich) > plain);
        let mut typed = relation(RelationType::DirectCall);
        typed.call_context = Context::InstanceMethod {
            receiver_type: "Widget".to_string(),
        };
        assert!(edge_confidence(&typed) > plain);
        let mut guarded = relation(RelationType::DirectCall);
        guarded.cfg_condition = Some("test".to_string());
        assert!(edge_confidence(&guarded) < plain);
    }

    #[test]
    fn untyped_context_gains_nothing() {
        use cce_types::relation::CallContext as Context;
        let plain = edge_confidence(&relation(RelationType::DirectCall));
        // Virtual dispatch resolves at runtime: the receiver type does not
        // disambiguate the edge.
        let mut virtual_dispatch = relation(RelationType::DirectCall);
        virtual_dispatch.call_context = Context::VirtualDispatch {
            receiver_type: "dyn Trait".to_string(),
        };
        assert_eq!(edge_confidence(&virtual_dispatch), plain);
        // Empty payloads mean the type was never recorded.
        let mut empty_owner = relation(RelationType::DirectCall);
        empty_owner.call_context = Context::StaticMethod {
            owner_type: String::new(),
        };
        assert_eq!(edge_confidence(&empty_owner), plain);
    }

    #[test]
    fn master_switch_forces_zero() {
        let off = ExpansionScoring {
            enable: false,
            ..scoring()
        };
        assert_eq!(
            expansion_score(
                &relation(RelationType::DirectCall),
                0.9,
                "target",
                &["target".to_string()],
                8,
                off
            ),
            0.0
        );
    }

    #[test]
    fn query_factor_neutral_without_terms_or_match() {
        assert_eq!(query_factor("target", &[]), 1.0);
        assert_eq!(query_factor("target", &["zzz".to_string()]), 1.0);
        assert!(query_factor("parse_query", &["query".to_string()]) > 1.0);
    }

    #[test]
    fn fanin_factor_grows_logarithmically() {
        assert_eq!(fanin_factor(0), 1.0);
        assert!(fanin_factor(7) > fanin_factor(1));
        assert!(fanin_factor(1_000_000) < 3.0);
    }

    #[test]
    fn seed_weight_prefers_full_overlap_and_early_rank() {
        assert!(seed_weight(0.9, 1.0, 0) > seed_weight(0.9, 0.2, 0));
        assert!(seed_weight(0.9, 1.0, 0) > seed_weight(0.9, 1.0, 3));
        assert_eq!(seed_weight(-1.0, 1.0, 0), 0.0);
    }

    #[test]
    fn split_query_terms_dedupes_and_drops_noise() {
        assert_eq!(
            split_query_terms("parse Query parse a!"),
            vec!["parse", "query"]
        );
    }
}
