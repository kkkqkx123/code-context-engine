//! Relation graph boost contributor
//!
//! Computes additive boost contributions from call graph relationships.
//! Entities related to high-scoring seed entities via call graph edges
//! receive a contribution that decays with hop distance.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::query::boost::{BoostAggregationConfig, BoostContribution};
use crate::query::error::Result;
use crate::query::relation_searcher::{RelationQueryOptions, RelationSearcher};
use crate::query::types::{QueryOptions, SearchResult};
use cce_types::EntityId;

/// Relation type for tracking how entities are related
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationType {
    Callee,
    Caller,
}

impl std::fmt::Display for RelationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RelationType::Callee => write!(f, "callee"),
            RelationType::Caller => write!(f, "caller"),
        }
    }
}

/// Relation graph boost contributor
///
/// Expands call graph from seed entities and computes additive boost
/// contributions for related entities with hop-distance decay.
pub struct RelationBoost {
    relation_searcher: Arc<RelationSearcher>,
}

impl RelationBoost {
    pub fn new(relation_searcher: Arc<RelationSearcher>) -> Self {
        Self { relation_searcher }
    }

    /// Collect relation graph boost contributions for the given candidates.
    ///
    /// 1. Selects top-N results as seeds
    /// 2. Expands call graph via BFS
    /// 3. Returns contributions for candidates matching related entities
    pub async fn collect(
        &self,
        candidates: &[SearchResult],
        options: &QueryOptions,
        boost_config: &BoostAggregationConfig,
    ) -> Result<Vec<BoostContribution>> {
        let config = &options.config;

        if !options.sources.relation || candidates.is_empty() {
            return Ok(Vec::new());
        }

        let top_n = config.relation.top_n.min(candidates.len());
        let seed_entities: Vec<_> = candidates[..top_n]
            .iter()
            .filter_map(|r| r.entity_ids.first().copied())
            .collect();

        if seed_entities.is_empty() {
            tracing::trace!("Relation boost: no seed entities found");
            return Ok(Vec::new());
        }

        tracing::trace!(
            seed_count = seed_entities.len(),
            "Relation boost: selected seeds"
        );

        // Expand relations
        let local_config = RelationBoostConfig {
            max_hops: config.relation.max_hops,
            include_callees: config.relation.include_callees,
            include_callers: config.relation.include_callers,
            max_nodes: 10000,
        };

        let related_entities =
            expand_relations(&self.relation_searcher, &seed_entities, &local_config).await?;

        if related_entities.is_empty() {
            return Ok(Vec::new());
        }

        tracing::trace!(
            related_count = related_entities.len(),
            "Relation boost: expanded"
        );

        // Build contributions
        let mut contributions = Vec::new();
        for candidate in candidates {
            if let Some(entity_id) = candidate.entity_ids.first().copied() {
                if let Some(&(hops, relation_type)) = related_entities.get(&entity_id) {
                    let hop_decay = 1.0 / (hops as f32).sqrt();
                    let boost_value = boost_config.relation_max * hop_decay;

                    if boost_value > 0.0 {
                        contributions.push(
                            BoostContribution::new(
                                candidate.id.clone(),
                                "relation",
                                boost_value,
                                hops as f32,
                            )
                            .with_reason(format!(
                                "relation({} hops, {}, decay={:.3})",
                                hops, relation_type, hop_decay
                            )),
                        );
                    }
                }
            }
        }

        Ok(contributions)
    }
}

/// Internal config for relation expansion logic
#[derive(Debug, Clone)]
struct RelationBoostConfig {
    max_hops: usize,
    include_callees: bool,
    include_callers: bool,
    max_nodes: usize,
}

/// Expand relations from seed entities using BFS (iterative)
async fn expand_relations(
    relation_searcher: &RelationSearcher,
    seed_entities: &[EntityId],
    config: &RelationBoostConfig,
) -> Result<HashMap<EntityId, (usize, RelationType)>> {
    let mut related = HashMap::new();
    let mut visited = HashSet::new();

    for &seed_id in seed_entities {
        visited.insert(seed_id);
    }

    let mut queue: Vec<(EntityId, usize, RelationType)> = Vec::new();

    if config.include_callees {
        for &seed_id in seed_entities {
            queue.push((seed_id, 0, RelationType::Callee));
        }
    }
    if config.include_callers {
        for &seed_id in seed_entities {
            queue.push((seed_id, 0, RelationType::Caller));
        }
    }

    while let Some((entity_id, current_hop, relation_type)) = queue.pop() {
        if visited.len() > config.max_nodes {
            tracing::warn!(
                visited = visited.len(),
                max_nodes = config.max_nodes,
                "Relation boost BFS exceeded max_nodes, truncating"
            );
            break;
        }
        if current_hop >= config.max_hops {
            continue;
        }

        let next_hop = current_hop + 1;
        let options = RelationQueryOptions::new().with_max_depth(1).with_limit(20);

        let neighbors = match relation_type {
            RelationType::Callee => relation_searcher
                .query_forward(entity_id, &options)
                .unwrap_or_default(),
            RelationType::Caller => relation_searcher
                .query_backward(entity_id, &options)
                .unwrap_or_default(),
        };

        for neighbor in neighbors {
            let neighbor_id = neighbor.function_id;
            if visited.insert(neighbor_id) {
                related.insert(neighbor_id, (next_hop, relation_type));
                if next_hop < config.max_hops {
                    queue.push((neighbor_id, next_hop, relation_type));
                }
            }
        }
    }

    Ok(related)
}
