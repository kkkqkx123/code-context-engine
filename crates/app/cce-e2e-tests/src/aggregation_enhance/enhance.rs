//! Offline boost simulation for the aggregation-enhance benchmark.
//!
//! Reuses only `data/benchmark/{baseline}/{fixture}/bge-m3/bench_data.rkyv`
//! (no regeneration, no LLM calls). The summary signal mirrors the production
//! formula in `cce_orchestrator::query::boost` with the same default
//! parameters, but derives its inputs from already-materialized data:
//! file vectors mean-pooled from the existing chunk vectors; per-query cosine
//! against the existing query vector plays the role of the production
//! summary-index lookup, with the same `min_score` threshold, `top_k` file
//! cap, and threshold normalization.
//!
//! The relation signal is an offline-only proxy: a file-cohort entity graph
//! built from chunk metadata (`entity_ids` + `file_path`). The seed is the
//! top-1 base hit, expansion is BFS up to `max_hops`, and per-hit addition
//! decays with `1 / sqrt(hops)` (deepest matching hop per chunk). The graph
//! links entities that share a file (undirected); an entity present in
//! several files bridges them. Graph traversal for production queries lives
//! on the dedicated graph retrieval path, not in semantic scoring.
//!
//! Aggregation (`apply_additive_boosts`) keeps the historical production
//! capping shape: per-source caps, then a global `max_addition` cap,
//! applied as `score * (1 + capped_addition)`.

use std::collections::{HashMap, HashSet};

/// Offline-only aggregation caps.
///
/// Historical mirror of the removed production capping defaults so
/// benchmark reports stay reproducible without depending on shipped config.
#[derive(Debug, Clone)]
pub struct AggCaps {
    pub max_addition: f32,
    pub max_source_boost: f32,
}

impl Default for AggCaps {
    fn default() -> Self {
        Self {
            max_addition: 0.5,
            max_source_boost: 0.3,
        }
    }
}

impl AggCaps {
    pub fn cap_for(&self, _source: &str) -> f32 {
        self.max_source_boost
    }
}

/// Offline-only summary signal parameters.
///
/// Mirrors the removed production summary-boost defaults so historical
/// benchmark reports stay reproducible without depending on shipped config.
#[derive(Debug, Clone)]
pub struct SummaryParams {
    pub top_k: usize,
    pub min_score: f32,
}

impl Default for SummaryParams {
    fn default() -> Self {
        Self {
            top_k: 20,
            min_score: 0.4,
        }
    }
}

use crate::bench_data::{ChunkData, cosine_similarity};

/// Offline-only relation proxy parameters.
#[derive(Debug, Clone)]
pub struct RelationParams {
    pub top_n: usize,
    pub max_hops: usize,
    pub max_boost: f32,
}

impl Default for RelationParams {
    fn default() -> Self {
        Self {
            top_n: 5,
            max_hops: 2,
            max_boost: 0.15,
        }
    }
}

/// Boost parameters for the offline benchmark.
#[derive(Debug, Clone, Default)]
pub struct EnhanceParams {
    pub relation: RelationParams,
    pub summary: SummaryParams,
    pub agg: AggCaps,
}

/// Undirected file-cohort entity graph (offline proxy for the call graph).
///
/// An entity appearing in several files is a bridge: it links the member
/// entities of every file it appears in.
#[derive(Debug, Default)]
pub struct FileCohortGraph {
    entity_to_files: HashMap<i64, Vec<String>>,
    file_to_entities: HashMap<String, Vec<i64>>,
}

impl FileCohortGraph {
    /// Build the graph from the union of both recall paths' chunks.
    pub fn build(chunks: &[&ChunkData]) -> Self {
        let mut graph = Self::default();
        for chunk in chunks {
            for &entity in &chunk.entity_ids {
                graph
                    .entity_to_files
                    .entry(entity)
                    .or_default()
                    .push(chunk.file_path.clone());
                graph
                    .file_to_entities
                    .entry(chunk.file_path.clone())
                    .or_default()
                    .push(entity);
            }
        }
        for files in graph.entity_to_files.values_mut() {
            files.sort_unstable();
            files.dedup();
        }
        for entities in graph.file_to_entities.values_mut() {
            entities.sort_unstable();
            entities.dedup();
        }
        graph
    }

    pub fn entity_count(&self) -> usize {
        self.entity_to_files.len()
    }

    pub fn file_count(&self) -> usize {
        self.file_to_entities.len()
    }

    fn neighbors(&self, entity: i64) -> Vec<i64> {
        let Some(files) = self.entity_to_files.get(&entity) else {
            return Vec::new();
        };
        let mut members: Vec<i64> = Vec::new();
        for file in files {
            if let Some(entities) = self.file_to_entities.get(file.as_str()) {
                members.extend(entities.iter().copied());
            }
        }
        members.sort_unstable();
        members.dedup();
        members.retain(|&e| e != entity);
        members
    }
}

/// BFS over the cohort graph from the seed entities.
///
/// Returns `entity -> hops` for every reached non-seed entity within
/// `max_hops`. Mirrors the production BFS bound (`max_nodes`).
pub fn expand_cohort(
    graph: &FileCohortGraph,
    seeds: &[i64],
    max_hops: usize,
) -> HashMap<i64, usize> {
    const MAX_NODES: usize = 10_000;
    let mut related = HashMap::new();
    let mut visited: HashSet<i64> = seeds.iter().copied().collect();
    let mut frontier: std::collections::VecDeque<(i64, usize)> =
        seeds.iter().map(|&s| (s, 0)).collect();
    while let Some((entity, hops)) = frontier.pop_front() {
        if visited.len() > MAX_NODES || hops >= max_hops {
            continue;
        }
        for neighbor in graph.neighbors(entity) {
            if visited.insert(neighbor) {
                related.insert(neighbor, hops + 1);
                frontier.push_back((neighbor, hops + 1));
            }
        }
    }
    related
}

/// First entity id of a chunk, mirroring production (which matches
/// candidates by `entity_ids.first()` only).
pub fn primary_entity(chunk: &ChunkData) -> Option<i64> {
    chunk.entity_ids.first().copied()
}

/// Relation boost value per candidate chunk index.
///
/// The seed is the top-1 base hit's primary entity; expansion runs BFS up
/// to `max_hops`. The first `top_n` candidates are boosted: a chunk matching
/// several related entities decays with its deepest hop, so a chunk only
/// reachable through a bridge decays as the distant context it is.
pub fn relation_boosts(
    base_ranked: &[(usize, f64)],
    chunks: &[ChunkData],
    graph: &FileCohortGraph,
    params: &EnhanceParams,
) -> HashMap<usize, f64> {
    let seed = base_ranked
        .first()
        .and_then(|&(idx, _)| chunks.get(idx).and_then(primary_entity));
    let Some(seed) = seed else {
        return HashMap::new();
    };
    let related = expand_cohort(graph, &[seed], params.relation.max_hops);
    if related.is_empty() {
        return HashMap::new();
    }
    let top_n = params.relation.top_n.min(base_ranked.len());
    let mut boosts = HashMap::new();
    for (idx, _) in base_ranked.iter().take(top_n) {
        let Some(chunk) = chunks.get(*idx) else {
            continue;
        };
        if let Some(&hops) = chunk.entity_ids.iter().filter_map(|e| related.get(e)).max() {
            let decay = 1.0 / (hops as f64).sqrt();
            let value = params.relation.max_boost as f64 * decay;
            if value > 0.0 {
                boosts.insert(*idx, value);
            }
        }
    }
    boosts
}

/// Mean-pooled file vectors from the existing chunk vectors.
#[derive(Debug, Default)]
pub struct FileVectors {
    pub files: Vec<String>,
    pub vectors: Vec<Vec<f32>>,
}

impl FileVectors {
    /// Pool `flat_vectors` (aligned with `chunks[..n]`) per file.
    pub fn build(chunks: &[ChunkData], flat_vectors: &[f32], dim: usize) -> Option<Self> {
        if dim == 0 || flat_vectors.is_empty() {
            return None;
        }
        let n = (flat_vectors.len() / dim).min(chunks.len());
        if n == 0 {
            return None;
        }
        let mut sums: HashMap<&str, (Vec<f64>, usize)> = HashMap::new();
        for (i, chunk) in chunks[..n].iter().enumerate() {
            let vec = &flat_vectors[i * dim..(i + 1) * dim];
            let entry = sums
                .entry(chunk.file_path.as_str())
                .or_insert_with(|| (vec![0.0; dim], 0));
            for (acc, &v) in entry.0.iter_mut().zip(vec.iter()) {
                *acc += v as f64;
            }
            entry.1 += 1;
        }
        let mut files = Vec::with_capacity(sums.len());
        let mut vectors = Vec::with_capacity(sums.len());
        let mut names: Vec<&str> = sums.keys().copied().collect();
        names.sort_unstable();
        for name in names {
            let (sum, count) = &sums[name];
            files.push(name.to_string());
            vectors.push(
                sum.iter()
                    .map(|s| (s / *count as f64) as f32)
                    .collect::<Vec<_>>(),
            );
        }
        Some(Self { files, vectors })
    }

    pub fn file_count(&self) -> usize {
        self.files.len()
    }
}

/// Summary boost value per candidate chunk index.
///
/// Scores every pooled file vector against the query vector, keeps files
/// above `min_score` (top-`top_k`), then assigns chunks in matching files
/// `summary_cap * (score - min) / (1 - min)`, where `summary_cap` is the
/// configured per-source cap for `summary`.
pub fn summary_boosts(
    query_vector: &[f32],
    file_vectors: &FileVectors,
    chunks: &[ChunkData],
    base_ranked: &[(usize, f64)],
    params: &EnhanceParams,
) -> HashMap<usize, f64> {
    if query_vector.is_empty() {
        return HashMap::new();
    }
    let min_score = params.summary.min_score as f64;
    let mut file_scores: Vec<(&str, f64)> = file_vectors
        .files
        .iter()
        .zip(file_vectors.vectors.iter())
        .map(|(file, vec)| {
            (
                file.as_str(),
                cosine_similarity(&vec.to_vec(), query_vector),
            )
        })
        .filter(|(_, score)| *score >= min_score)
        .collect();
    file_scores.sort_by(|a, b| b.1.total_cmp(&a.1));
    file_scores.truncate(params.summary.top_k);
    if file_scores.is_empty() {
        return HashMap::new();
    }
    let matched: HashMap<&str, f64> = file_scores.into_iter().collect();
    let mut boosts = HashMap::new();
    for (idx, _) in base_ranked {
        let Some(chunk) = chunks.get(*idx) else {
            continue;
        };
        if let Some(&score) = matched.get(chunk.file_path.as_str()) {
            let normalized = ((score - min_score) / (1.0 - min_score)).clamp(0.0, 1.0);
            let value = params.agg.cap_for("summary") as f64 * normalized;
            if value > 0.0 {
                boosts.insert(*idx, value);
            }
        }
    }
    boosts
}

/// Apply per-source boost maps to a base ranking with production capping.
///
/// `sources` carries one map per boost source in the same order as `caps`;
/// the total per candidate is capped at `max_addition`, applied as
/// `score * (1 + capped)`. Output is re-sorted descending with a chunk-id
/// tiebreak for determinism.
pub fn apply_additive_boosts(
    base_ranked: &[(usize, f64)],
    sources: &[(&HashMap<usize, f64>, f32)],
    max_addition: f32,
    chunk_id_of: &dyn Fn(usize) -> String,
) -> Vec<(usize, f64)> {
    let mut out: Vec<(usize, f64)> = base_ranked
        .iter()
        .map(|&(idx, score)| {
            let mut total = 0.0;
            for (map, cap) in sources {
                let capped = map.get(&idx).copied().unwrap_or(0.0).min(*cap as f64);
                total += capped;
            }
            let capped = total.min(max_addition as f64);
            (idx, score * (1.0 + capped))
        })
        .collect();
    out.sort_by(|a, b| {
        b.1.total_cmp(&a.1)
            .then_with(|| chunk_id_of(a.0).cmp(&chunk_id_of(b.0)))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bench_data::{ChunkSourceRange, QueryType};

    fn chunk(id: &str, file: &str, entities: Vec<i64>) -> ChunkData {
        ChunkData {
            chunk_id: id.to_string(),
            entity_name: String::new(),
            file_path: file.to_string(),
            start_line: 1,
            end_line: 10,
            source_ranges: vec![ChunkSourceRange {
                start_line: 1,
                end_line: 10,
            }],
            source_span_kind: String::new(),
            test_info: cce_types::TestInfo::unknown(),
            language: None,
            entity_ids: entities,
            segment_id: String::new(),
        }
    }

    fn params() -> EnhanceParams {
        EnhanceParams::default()
    }

    #[test]
    fn cohort_graph_groups_entities_by_file() {
        let chunks = [
            chunk("a", "src/lib.rs", vec![1, 2]),
            chunk("b", "src/lib.rs", vec![3]),
            chunk("c", "src/race.rs", vec![4]),
        ];
        let refs: Vec<&ChunkData> = chunks.iter().collect();
        let graph = FileCohortGraph::build(&refs);
        assert_eq!(graph.entity_count(), 4);
        assert_eq!(graph.file_count(), 2);
        let mut neighbors = graph.neighbors(1);
        neighbors.sort_unstable();
        assert_eq!(neighbors, vec![2, 3]);
        assert!(graph.neighbors(4).is_empty());
    }

    #[test]
    fn expand_cohort_respects_max_hops() {
        let chunks = [
            chunk("a", "f1", vec![1]),
            chunk("b", "f1", vec![2]),
            chunk("c", "f2", vec![2, 3]),
        ];
        let refs: Vec<&ChunkData> = chunks.iter().collect();
        let graph = FileCohortGraph::build(&refs);
        // Entity 2 bridges f1 and f2, so 3 is two hops from 1.
        let one_hop = expand_cohort(&graph, &[1], 1);
        assert_eq!(one_hop.get(&2), Some(&1));
        assert!(!one_hop.contains_key(&3));
        let two_hop = expand_cohort(&graph, &[1], 2);
        assert_eq!(two_hop.get(&3), Some(&2));
        // Seeds themselves are never reported as related.
        assert!(!two_hop.contains_key(&1));
    }

    #[test]
    fn relation_boost_applies_hop_decay() {
        let chunks = vec![
            chunk("seed", "f1", vec![1]),
            chunk("near", "f1", vec![2]),
            chunk("far", "f2", vec![2, 3]),
        ];
        let refs: Vec<&ChunkData> = chunks.iter().collect();
        let graph = FileCohortGraph::build(&refs);
        let p = params();
        let base = vec![(0usize, 0.9), (1usize, 0.8), (2usize, 0.7)];
        let boosts = relation_boosts(&base, &chunks, &graph, &p);
        // Seed chunk itself gets no boost; 1-hop gets full max_boost.
        assert!(!boosts.contains_key(&0));
        let one_hop = p.relation.max_boost as f64;
        assert!((boosts[&1] - one_hop).abs() < 1e-9);
        // Entity 3 is two hops away via the bridge entity 2.
        let two_hop = p.relation.max_boost as f64 / 2.0_f64.sqrt();
        assert!((boosts[&2] - two_hop).abs() < 1e-9);
    }

    #[test]
    fn summary_boost_threshold_and_normalization() {
        // Two files with orthogonal unit vectors; the query matches f1.
        let chunks = vec![chunk("a", "f1", vec![1]), chunk("b", "f2", vec![2])];
        let vectors = vec![1.0, 0.0, 0.0, 1.0];
        let files = FileVectors::build(&chunks, &vectors, 2).expect("file vectors");
        assert_eq!(files.file_count(), 2);
        let p = params();
        let base = vec![(0usize, 0.5), (1usize, 0.6)];
        let boosts = summary_boosts(&[1.0, 0.0], &files, &chunks, &base, &p);
        // f1 scores 1.0 -> full summary cap; f2 scores 0.0 < min_score.
        assert!((boosts[&0] - p.agg.cap_for("summary") as f64).abs() < 1e-9);
        assert!(!boosts.contains_key(&1));
    }

    #[test]
    fn additive_boosts_cap_and_resort() {
        let base = vec![(0usize, 1.0), (1usize, 0.9)];
        let rel: HashMap<usize, f64> = [(1usize, 0.15)].into_iter().collect();
        let sum: HashMap<usize, f64> = [(1usize, 0.15)].into_iter().collect();
        let id_of = |i: usize| format!("c{i}");
        // Global cap binds: 0.15 + 0.15 = 0.30 < 0.5, no capping here.
        let out = apply_additive_boosts(&base, &[(&rel, 0.15), (&sum, 0.15)], 0.5, &id_of);
        assert!((out[0].1 - 0.9 * 1.30).abs() < 1e-9);
        assert_eq!(out[0].0, 1);
        // Tight global cap clamps the multiplier. The boosted candidate no
        // longer outranks the untouched 1.0 base score, so look it up by id;
        // the expected value mirrors the f32 cap's f64 widening exactly.
        let out = apply_additive_boosts(&base, &[(&rel, 0.15), (&sum, 0.15)], 0.10, &id_of);
        let boosted = out.iter().find(|(idx, _)| *idx == 1).expect("c1 present");
        assert!((boosted.1 - 0.9 * (1.0 + f64::from(0.10f32))).abs() < 1e-9);
    }

    #[test]
    fn production_defaults_match_expected_values() {
        let p = params();
        assert_eq!(p.relation.top_n, 5);
        assert_eq!(p.relation.max_hops, 2);
        assert!((p.relation.max_boost - 0.15).abs() < f32::EPSILON);
        assert!((p.agg.cap_for("summary") - 0.3).abs() < f32::EPSILON);
        assert!((p.agg.max_addition - 0.5).abs() < f32::EPSILON);
        assert_eq!(p.summary.top_k, 20);
        assert!((p.summary.min_score - 0.4).abs() < f32::EPSILON);
        let _ = QueryType::Qualified;
    }
}
