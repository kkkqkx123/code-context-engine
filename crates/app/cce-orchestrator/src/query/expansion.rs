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

/// Resolve call-domain neighbour units for one search hit.
///
/// `start_line`/`end_line` are one-based presentation lines; the snapshot index
/// compares zero-based tree-sitter rows, so the range is translated back before
/// the lookup. Returns `(forward_callees, backward_callers)`, each capped at
/// `per_direction`.
pub fn resolve_expansion_units(
    searcher: &RelationSearcher,
    project_root: &Path,
    file_path: &str,
    start_line: u32,
    end_line: u32,
    per_direction: usize,
    include_callers: bool,
) -> (Vec<ExpandedUnit>, Vec<ExpandedUnit>) {
    if per_direction == 0 {
        return (Vec::new(), Vec::new());
    }
    let snapshot = searcher.query().index();
    let options = RelationQueryOptions {
        relation_domains: vec!["call".to_string()],
        include_external: false,
        limit: per_direction,
        ..RelationQueryOptions::default()
    };

    let first_row = start_line.min(end_line).saturating_sub(1) as usize;
    let last_row = start_line.max(end_line).saturating_sub(1) as usize;
    let (first_row, last_row) = (first_row.min(last_row), first_row.max(last_row));
    let mut seeds = snapshot.get_entities_in_line_range(file_path, first_row, last_row);
    seeds.sort_by_key(|id| id.0);
    seeds.truncate(MAX_SEEDS);
    if seeds.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let seed_set: HashSet<cce_types::EntityId> = seeds.iter().copied().collect();

    let mut cache = SourceFileCache::new();
    let mut forward = Vec::new();
    let mut seen_forward = HashSet::new();
    for entity_id in &seeds {
        if forward.len() >= per_direction {
            break;
        }
        for relation in searcher.get_callees_paginated(*entity_id, &options) {
            let Some(callee) = relation.callee_id else {
                continue;
            };
            if seed_set.contains(&callee) {
                continue;
            }
            if forward.len() >= per_direction || !seen_forward.insert(callee.0) {
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
    if include_callers {
        let mut seen_backward = HashSet::new();
        for entity_id in &seeds {
            if backward.len() >= per_direction {
                break;
            }
            for relation in searcher.get_callers_paginated(*entity_id, &options) {
                let caller = relation.caller;
                if seed_set.contains(&caller) {
                    continue;
                }
                if backward.len() >= per_direction || !seen_backward.insert(caller.0) {
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
    let mut config = config.clone();
    config.enable_annotation = true;
    config.expansion_enabled = true;
    config.workspace_root = Some(project_root.to_path_buf());
    if config.max_expanded_units == 0 {
        return;
    }
    let per_direction = config.max_expanded_units.clamp(1, 8);
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
        let (forward, backward) = resolve_expansion_units(
            searcher,
            project_root,
            &item.file_path,
            item.start_line,
            item.end_line,
            per_direction,
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
}
