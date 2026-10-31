//! Caller-driven relation expansion for the query path.
//!
//! The [`RelationAnnotator`](super::annotation::RelationAnnotator) only attaches
//! pre-resolved neighbour units; graph traversal lives here so the search
//! pipeline can supply real callee/caller units on opt-in requests instead of
//! the empty vectors used by default.
//!
//! Seeds resolve through the snapshot line-range lookup (the relation index ID
//! space), not the hit's retrieval entity IDs (the SQLite row space), so
//! method groups represented by their container still surface member edges.
//! Snippets are full entity spans read from the live source tree, matching the
//! production annotation contract (the 3-line reference window is review-only).

use std::collections::HashSet;
use std::path::Path;

use cce_storage_metadb_sqlite::source_reader::{SourceFileCache, read_source_lines_cached};

use super::annotation::{
    AnnotatedResult, AnnotationMetadata, ExpandedUnit, ExpansionOrigin, RelationAnnotationConfig,
    RelationAnnotator, SearchResultInput,
};
use super::relation_searcher::{RelationQueryOptions, RelationSearcher};
use cce_codegraph::index::snapshot_query::{SnapshotEntityQueryOps, SnapshotSymbolQueryOps};

/// Maximum seeds taken from one hit's line range.
const MAX_SEEDS: usize = 4;

/// Upper bound for one expansion unit body before windowing. Oversized
/// bodies are downgraded to a head/tail window excerpt instead of being
/// delivered whole (where the budget would drop or degrade them entirely).
const MAX_UNIT_CHARS: usize = 1600;
/// Head lines kept in a window excerpt (signature + first statements).
const EXCERPT_HEAD_LINES: usize = 24;
/// Tail lines kept in a window excerpt.
const EXCERPT_TAIL_LINES: usize = 4;

/// Downgrade an oversized unit body to a head/tail window excerpt.
///
/// Returns `None` when the body fits within [`MAX_UNIT_CHARS`] or is too
/// short to window meaningfully, so the caller keeps the original text.
fn window_excerpt(code: &str) -> Option<String> {
    if code.chars().count() <= MAX_UNIT_CHARS {
        return None;
    }
    let lines: Vec<&str> = code.lines().collect();
    let total = lines.len();
    if total <= EXCERPT_HEAD_LINES + EXCERPT_TAIL_LINES + 1 {
        return None;
    }
    let head_end = EXCERPT_HEAD_LINES;
    let tail_start = total - EXCERPT_TAIL_LINES;
    let mut out = String::with_capacity(MAX_UNIT_CHARS);
    out.push_str(&lines[..head_end].join("\n"));
    out.push_str(&format!(
        "\n... excerpt: lines {}-{} of {} omitted ...\n",
        head_end + 1,
        tail_start,
        total
    ));
    out.push_str(&lines[tail_start..].join("\n"));
    Some(out)
}

/// Path filter inherited from the main query for expansion neighbours.
///
/// Neighbours whose file fails this filter never occupy budget. Mirrors the
/// main-path semantics: directory prefix, content-type exclusion and
/// include/exclude glob patterns share one implementation.
#[derive(Clone, Debug, Default)]
pub struct ExpansionPathFilter {
    pub directory_prefix: Option<String>,
    pub exclude_content_types: Vec<crate::query::types::ExcludableContentType>,
    pub include_patterns: Vec<String>,
    pub exclude_patterns: Vec<String>,
}

impl ExpansionPathFilter {
    pub fn allows(&self, file_path: &str) -> bool {
        let normalized = cce_types::normalize_project_path(file_path);
        if let Some(prefix) = &self.directory_prefix {
            let p = cce_types::normalize_project_path(prefix.trim_matches('/'));
            if !(normalized == p || normalized.starts_with(&format!("{p}/"))) {
                return false;
            }
        }
        if crate::query::types::query_options::path_excluded_by_types(
            &normalized,
            &self.exclude_content_types,
        ) {
            return false;
        }
        if !self.include_patterns.is_empty() || !self.exclude_patterns.is_empty() {
            let filter = crate::query::retrieval::post_processing::GlobFilter::new();
            let probe = crate::query::types::SearchResult {
                file_path: normalized.clone(),
                ..Default::default()
            };
            let kept = filter
                .apply(vec![probe], &self.include_patterns, &self.exclude_patterns)
                .unwrap_or_default();
            if kept.is_empty() {
                return false;
            }
        }
        true
    }
}

/// Neighbour resolver injected into the engine search pass.
///
/// A concrete (non-dyn) carrier bundling the relation searcher and the
/// project root the caller already holds. When attached to
/// [`super::types::QueryOptions`], the unified searcher's annotation step
/// resolves real neighbours and runs the batch annotation entry,
/// so real-neighbour resolution, cross-hit shared dedup and single wrapping
/// all happen inside the engine pass. When absent, the annotation step keeps
/// its historical empty-vector semantics and default behavior is unchanged.
#[derive(Clone)]
pub struct NeighbourResolver {
    searcher: std::sync::Arc<RelationSearcher>,
    project_root: std::path::PathBuf,
    pub path_filter: ExpansionPathFilter,
    pub domains: Vec<String>,
}

impl std::fmt::Debug for NeighbourResolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NeighbourResolver")
            .field("project_root", &self.project_root)
            .finish_non_exhaustive()
    }
}

impl NeighbourResolver {
    /// Bundle a relation searcher with the project root used for source reads.
    pub fn new(
        searcher: std::sync::Arc<RelationSearcher>,
        project_root: std::path::PathBuf,
    ) -> Self {
        Self {
            searcher,
            project_root,
            path_filter: ExpansionPathFilter::default(),
            domains: vec!["call".to_string()],
        }
    }

    /// Bundle with explicit path filter and relation domains.
    pub fn with_filter(
        searcher: std::sync::Arc<RelationSearcher>,
        project_root: std::path::PathBuf,
        path_filter: ExpansionPathFilter,
        domains: Vec<String>,
    ) -> Self {
        Self {
            searcher,
            project_root,
            path_filter,
            domains,
        }
    }

    /// Resolve neighbour units for one hit (see [`resolve_expansion_units`]).
    pub fn resolve(
        &self,
        file_path: &str,
        start_line: u32,
        end_line: u32,
        forward_budget: usize,
        backward_budget: usize,
        include_callers: bool,
    ) -> (Vec<ExpandedUnit>, Vec<ExpandedUnit>) {
        let request = ExpansionRequest {
            file_path: file_path.to_string(),
            start_line,
            end_line,
            forward_budget,
            backward_budget,
            include_callers,
            domains: self.domains.clone(),
            path_filter: self.path_filter.clone(),
        };
        resolve_expansion_units_with_filter(&self.searcher, &self.project_root, request)
    }
}

/// Single-hit expansion request grouping range, budgets and filters.
///
/// Groups the scalar arguments of neighbour resolution into one owned value
/// so the resolution entry stays within the argument-count guideline and
/// future filter fields grow the struct instead of the function signature.
#[derive(Debug, Clone)]
pub struct ExpansionRequest {
    pub file_path: String,
    pub start_line: u32,
    pub end_line: u32,
    pub forward_budget: usize,
    pub backward_budget: usize,
    pub include_callers: bool,
    pub domains: Vec<String>,
    pub path_filter: ExpansionPathFilter,
}

/// Resolve neighbour units for one search hit with explicit budgets, domains
/// and path filtering.
///
/// Seeds are ordered by overlap with the hit range so container members win
/// over unrelated siblings. Neighbours failing the path filter never occupy
/// budget. Returns `(forward, backward)` capped by their own budgets.
pub fn resolve_expansion_units_with_filter(
    searcher: &RelationSearcher,
    project_root: &Path,
    request: ExpansionRequest,
) -> (Vec<ExpandedUnit>, Vec<ExpandedUnit>) {
    let ExpansionRequest {
        file_path,
        start_line,
        end_line,
        forward_budget,
        backward_budget,
        include_callers,
        domains,
        path_filter,
    } = request;
    if forward_budget == 0 && (!include_callers || backward_budget == 0) {
        return (Vec::new(), Vec::new());
    }
    let snapshot = searcher.query().index();
    let domains = if domains.is_empty() {
        vec!["call".to_string()]
    } else {
        domains.to_vec()
    };
    let fetch = forward_budget
        .max(if include_callers { backward_budget } else { 0 })
        .max(1);
    let options = RelationQueryOptions {
        relation_domains: domains,
        include_external: false,
        limit: fetch.max(8),
        ..RelationQueryOptions::default()
    };

    let first_row = start_line.min(end_line).saturating_sub(1) as usize;
    let last_row = start_line.max(end_line).saturating_sub(1) as usize;
    let (first_row, last_row) = (first_row.min(last_row), first_row.max(last_row));
    let mut seeds = snapshot.get_entities_in_line_range(file_path.as_str(), first_row, last_row);
    seeds.sort_by(|a, b| {
        overlap_len(*a, first_row, last_row, snapshot)
            .cmp(&overlap_len(*b, first_row, last_row, snapshot))
            .reverse()
            .then_with(|| a.0.cmp(&b.0))
    });
    seeds.truncate(MAX_SEEDS);
    if seeds.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let seed_set: HashSet<cce_types::EntityId> = seeds.iter().copied().collect();

    let mut cache = SourceFileCache::new();
    let mut forward = Vec::new();
    let mut seen_forward = HashSet::new();
    for entity_id in &seeds {
        if forward.len() >= forward_budget {
            break;
        }
        for relation in searcher.get_callees_paginated(*entity_id, &options) {
            let Some(callee) = relation.callee_id else {
                continue;
            };
            if seed_set.contains(&callee) {
                continue;
            }
            if forward.len() >= forward_budget || !seen_forward.insert(callee.0) {
                continue;
            }
            if let Some(neighbour_file) = searcher.query().index().get_file_path_by_entity(callee)
                && !path_filter.allows(&neighbour_file)
            {
                continue;
            }
            if let Some(unit) = snippet_unit(
                searcher,
                project_root,
                &mut cache,
                callee,
                ExpansionOrigin::Forward,
                "calls",
                &relation,
            ) {
                forward.push(unit);
            }
        }
    }

    let mut backward = Vec::new();
    if include_callers && backward_budget > 0 {
        let mut seen_backward = HashSet::new();
        for entity_id in &seeds {
            if backward.len() >= backward_budget {
                break;
            }
            for relation in searcher.get_callers_paginated(*entity_id, &options) {
                let caller = relation.caller;
                if seed_set.contains(&caller) {
                    continue;
                }
                if backward.len() >= backward_budget || !seen_backward.insert(caller.0) {
                    continue;
                }
                if let Some(neighbour_file) =
                    searcher.query().index().get_file_path_by_entity(caller)
                    && !path_filter.allows(&neighbour_file)
                {
                    continue;
                }
                let edge_label = if relation.relation_type.is_constructor_call() {
                    "constructed by"
                } else {
                    "called by"
                };
                if let Some(unit) = snippet_unit(
                    searcher,
                    project_root,
                    &mut cache,
                    caller,
                    ExpansionOrigin::Backward,
                    edge_label,
                    &relation,
                ) {
                    backward.push(unit);
                }
            }
        }
    }
    (forward, backward)
}

/// Overlap length between an entity span and the hit row range.
fn overlap_len(
    entity_id: cce_types::EntityId,
    first_row: usize,
    last_row: usize,
    snapshot: &cce_codegraph::LayeredSnapshotIndex,
) -> usize {
    use cce_codegraph::index::snapshot_query::SnapshotEntityQueryOps;
    let Some(entity) = snapshot.get_function_by_entity_id(entity_id) else {
        return 0;
    };
    let start = entity.span.start_position.row;
    let end = entity.span.end_position.row.max(start);
    let overlap_start = start.max(first_row);
    let overlap_end = end.min(last_row);
    if overlap_end >= overlap_start {
        overlap_end - overlap_start + 1
    } else {
        0
    }
}

/// Single-budget entry grouping range, budget and filters into one request.
///
/// Keeps the historical seven-argument call shape for direct callers by
/// building the request value internally.
pub fn resolve_expansion_units(
    searcher: &RelationSearcher,
    project_root: &Path,
    file_path: &str,
    start_line: u32,
    end_line: u32,
    per_direction: usize,
    include_callers: bool,
) -> (Vec<ExpandedUnit>, Vec<ExpandedUnit>) {
    let request = ExpansionRequest {
        file_path: file_path.to_string(),
        start_line,
        end_line,
        forward_budget: per_direction,
        backward_budget: if include_callers { per_direction } else { 0 },
        include_callers,
        domains: Vec::new(),
        path_filter: ExpansionPathFilter::default(),
    };
    resolve_expansion_units_with_filter(searcher, project_root, request)
}

/// Read one entity's full definition span as an annotator unit.
///
/// Returns `None` when the entity has no snapshot metadata or its source
/// cannot be read; vanished files are the annotator's downgrade path, not an
/// error here.
fn snippet_unit(
    searcher: &RelationSearcher,
    project_root: &Path,
    cache: &mut SourceFileCache,
    entity_id: cce_types::EntityId,
    origin: ExpansionOrigin,
    edge_label: &str,
    relation: &cce_types::ResolvedRelation,
) -> Option<ExpandedUnit> {
    let snapshot = searcher.query().index();
    let entity = snapshot.get_function_by_entity_id(entity_id)?;
    let file_path = snapshot.get_file_path_by_entity(entity_id)?;
    let start_row = entity.span.start_position.row as u32;
    let end_row = entity
        .span
        .end_position
        .row
        .max(entity.span.start_position.row) as u32;
    let code = read_source_lines_cached(cache, Some(project_root), &file_path, start_row, end_row)?;
    if code.trim().is_empty() {
        return None;
    }
    // Oversized bodies become window excerpts; the whole-body fallback would
    // only crowd out the rest of the expansion budget or degrade entirely.
    let code = window_excerpt(&code).unwrap_or(code);
    Some(
        ExpandedUnit::new(
            code,
            file_path,
            start_row + 1,
            end_row + 1,
            entity.name.clone(),
        )
        .with_expansion(origin, edge_label)
        .with_entity_id(entity_id)
        .with_relation_type(relation.relation_type)
        .with_external(relation.is_external)
        .with_stdlib(relation.is_stdlib()),
    )
}

/// Annotate the top-N hits with real expansion units in place.
///
/// Resolution runs per hit, then one batch annotation pass attaches the
/// units so cross-hit dedup stays active: a neighbour already attached to an
/// earlier hit is not concatenated again. Only body-bearing hits with a valid
/// line range take part; reference and empty hits keep their content and no
/// longer consume annotation budget slots. Surviving expansion counts are
/// recorded into each hit's `metadata` (`expanded`, `expanded_nodes`,
/// `forward_nodes`, `backward_nodes`, `omitted_self_evident`) so API
/// converters can expose them without changing the orchestrator result shape.
///
/// Runs after ranking so ordering is never touched. A batch failure leaves
/// every hit unannotated; single-hit extraction cannot fail here because
/// degenerate ranges are screened out before the batch is built.
///
/// Callers keep the engine recall pass unannotated
/// (`annotation.enable_annotation = false`) and pass an enabled config here,
/// so content is wrapped exactly once.
pub async fn annotate_hits_with_expansion(
    searcher: &RelationSearcher,
    project_root: &Path,
    items: &mut [super::types::SearchResult],
    config: &RelationAnnotationConfig,
) {
    let resolver = NeighbourResolver::new(
        std::sync::Arc::new(RelationSearcher::clone(searcher)),
        project_root.to_path_buf(),
    );
    annotate_hits_with_resolver(&resolver, items, config).await;
}

/// Resolver-injected variant of [`annotate_hits_with_expansion`].
///
/// This is the entry the engine's annotation step calls when a
/// [`NeighbourResolver`] is attached to the query options: identical
/// semantics, but the searcher and project root come from the injected
/// resolver instead of being re-resolved by the handler layer.
pub async fn annotate_hits_with_resolver(
    resolver: &NeighbourResolver,
    items: &mut [super::types::SearchResult],
    config: &RelationAnnotationConfig,
) {
    let mut config = config.clone();
    config.enable_annotation = true;
    config.expansion_enabled = true;
    config.workspace_root = Some(resolver.project_root.clone());
    if !resolver.domains.is_empty() {
        config.expansion_domains = resolver.domains.clone();
    }
    let forward_budget = config.forward_max_expanded_units.clamp(0, 8);
    let backward_budget = if config.expansion_include_callers {
        config.backward_max_expanded_units.clamp(0, 8)
    } else {
        0
    };
    if forward_budget == 0 && backward_budget == 0 {
        return;
    }
    let annotator = RelationAnnotator::new(config.clone());
    let mut batch = Vec::new();
    let mut positions = Vec::new();
    for (index, item) in items.iter().enumerate() {
        if item.content_state.is_reference()
            || item.content.trim().is_empty()
            || item.start_line == 0
            || item.end_line == 0
            || item.start_line > item.end_line
        {
            continue;
        }
        let (forward, backward) = resolver.resolve(
            &item.file_path,
            item.start_line,
            item.end_line,
            forward_budget,
            backward_budget,
            config.expansion_include_callers,
        );
        if forward.is_empty() && backward.is_empty() {
            continue;
        }
        batch.push((
            SearchResultInput {
                id: item.id.clone(),
                entity_id: item.entity_ids.first().copied(),
                name: item.name.clone(),
                kind: item.kind.clone(),
                file_path: item.file_path.clone(),
                start_line: item.start_line,
                end_line: item.end_line,
                content: item.content.clone(),
                score: item.score,
            },
            forward,
            backward,
        ));
        positions.push(index);
    }
    if batch.is_empty() {
        return;
    }
    match annotator.annotate_batch(batch).await {
        Ok(annotated) => apply_annotated(items, &positions, &annotated),
        Err(error) => {
            tracing::warn!(
                %error,
                "Batch expansion annotation failed; keeping unannotated bodies"
            );
        }
    }
}

/// Copy batch annotation results back onto their source hits.
///
/// The batch preserves input order (top-N annotated first, the rest appended
/// unchanged), so results map back onto `positions` one to one. Tail hits
/// past the annotation budget come back unexpanded and are left untouched.
fn apply_annotated(
    items: &mut [super::types::SearchResult],
    positions: &[usize],
    annotated: &[AnnotatedResult],
) {
    for (position, result) in positions.iter().zip(annotated.iter()) {
        if !result.metadata.expanded {
            continue;
        }
        let Some(item) = items.get_mut(*position) else {
            continue;
        };
        item.content = result.annotated_content.clone();
        record_expansion_metadata(item, &result.metadata);
    }
}

/// Record surviving expansion counts on a hit for API converters.
fn record_expansion_metadata(item: &mut super::types::SearchResult, metadata: &AnnotationMetadata) {
    item.metadata
        .insert("expanded".to_string(), metadata.expanded.to_string());
    item.metadata.insert(
        "expanded_nodes".to_string(),
        metadata.expanded_nodes.to_string(),
    );
    item.metadata.insert(
        "forward_nodes".to_string(),
        metadata.forward_nodes.to_string(),
    );
    item.metadata.insert(
        "backward_nodes".to_string(),
        metadata.backward_nodes.to_string(),
    );
    item.metadata.insert(
        "omitted_self_evident".to_string(),
        metadata.omitted_self_evident.to_string(),
    );
}

/// Human-readable stable ID for an entity, for joinable responses.
///
/// Falls back to the epoch-local `entity:{id}` form when the snapshot carries
/// no symbol key for the ID.
pub fn stable_id_of(searcher: &RelationSearcher, entity_id: cce_types::EntityId) -> String {
    searcher
        .query()
        .index()
        .get_symbol_key_by_entity_id(entity_id)
        .map(|key| key.stable_id().0)
        .unwrap_or_else(|| format!("entity:{}", entity_id.0))
}

#[cfg(test)]
mod tests {
    use super::super::annotation::AnnotationMetadata;
    use super::*;

    fn hit(id: &str) -> super::super::types::SearchResult {
        super::super::types::SearchResult {
            id: id.to_string(),
            content: "fn a() {}".to_string(),
            start_line: 1,
            end_line: 1,
            ..Default::default()
        }
    }

    fn annotated(
        id: &str,
        expanded: bool,
        forward: usize,
        backward: usize,
        omitted: usize,
    ) -> AnnotatedResult {
        AnnotatedResult {
            id: id.to_string(),
            entity_id: None,
            name: String::new(),
            kind: String::new(),
            file_path: String::new(),
            score: 0.0,
            start_line: 1,
            end_line: 1,
            annotated_content: format!("annotated {id}"),
            involved_files: Vec::new(),
            metadata: AnnotationMetadata {
                expanded,
                expanded_nodes: forward + backward,
                forward_nodes: forward,
                backward_nodes: backward,
                omitted_self_evident: omitted,
                ..Default::default()
            },
            original_content: "fn a() {}".to_string(),
        }
    }

    #[test]
    fn test_apply_annotated_maps_back_by_position() {
        let mut items = vec![hit("a"), hit("b"), hit("c")];
        let results = vec![
            annotated("a", true, 2, 1, 1),
            annotated("c", false, 0, 0, 0),
        ];
        apply_annotated(&mut items, &[0, 2], &results);
        assert_eq!(items[0].content, "annotated a");
        assert_eq!(
            items[0].metadata.get("expanded").map(String::as_str),
            Some("true")
        );
        assert_eq!(
            items[0].metadata.get("expanded_nodes").map(String::as_str),
            Some("3")
        );
        assert_eq!(
            items[0].metadata.get("forward_nodes").map(String::as_str),
            Some("2")
        );
        assert_eq!(
            items[0].metadata.get("backward_nodes").map(String::as_str),
            Some("1")
        );
        assert_eq!(
            items[0]
                .metadata
                .get("omitted_self_evident")
                .map(String::as_str),
            Some("1")
        );
        assert_eq!(items[1].content, "fn a() {}");
        assert!(items[1].metadata.is_empty());
        assert_eq!(items[2].content, "fn a() {}");
        assert!(items[2].metadata.is_empty());
    }

    #[test]
    fn test_window_excerpt_keeps_head_and_tail() {
        // Long enough lines to cross MAX_UNIT_CHARS (60 lines x ~40 chars).
        let body: String = (0..60)
            .map(|i| format!("line {i}: {}", "x".repeat(30)))
            .collect::<Vec<_>>()
            .join("\n");
        let excerpt = window_excerpt(&body).expect("oversized body must be windowed");
        assert!(excerpt.starts_with("line 0"));
        assert!(excerpt.contains("line 59"));
        assert!(excerpt.contains("excerpt: lines 25-56 of 60 omitted"));
        // Head and tail content is preserved around the omission marker.
        assert!(excerpt.contains("line 23"));
        assert!(excerpt.contains("line 57"));
    }

    #[test]
    fn test_window_excerpt_passes_short_body_through() {
        assert_eq!(window_excerpt("fn a() {}"), None);
        let short: String = (0..10)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(window_excerpt(&short), None);
    }
}
