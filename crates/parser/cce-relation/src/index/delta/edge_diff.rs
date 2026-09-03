use std::collections::HashSet;

use cce_types::{EntityId, FileRelationDiff, ResolvedRelation};

use crate::index::core::{RelationEdgeIdentity, RelationIndex, relation_identity};
use crate::index::view::RelationIndexView;

fn entity_in_scope<V: RelationIndexView>(
    index: &V,
    id: EntityId,
    affected_files: &Option<&HashSet<String>>,
) -> bool {
    match affected_files {
        None => true,
        Some(files) => index
            .entity_file_of(id)
            .is_some_and(|file| files.contains(&file)),
    }
}

fn entity_in_scope_new(
    index: &RelationIndex,
    id: EntityId,
    affected_files: &Option<&HashSet<String>>,
) -> bool {
    match affected_files {
        None => true,
        Some(files) => index
            .entity_file_index
            .get(&id)
            .is_some_and(|file| files.contains(file.value())),
    }
}

pub(super) fn compute_relation_diff<V: RelationIndexView>(
    new_index: &RelationIndex,
    old: &V,
    affected_files: Option<&HashSet<String>>,
    removed_entity_set: &HashSet<EntityId>,
) -> (Vec<ResolvedRelation>, Vec<ResolvedRelation>, u64) {
    let mut new_relation_edges: HashSet<RelationEdgeIdentity> = HashSet::new();
    for entry in new_index.resolved_relation_index.iter() {
        if !entity_in_scope_new(new_index, *entry.key(), &affected_files) {
            continue;
        }
        new_relation_edges.extend(entry.value().iter().map(relation_identity));
    }
    let mut old_relation_edges: HashSet<RelationEdgeIdentity> = HashSet::new();
    old.for_each_resolved_relation(|caller, relations| {
        if !entity_in_scope(old, caller, &affected_files) {
            return;
        }
        old_relation_edges.extend(relations.iter().map(relation_identity));
    });

    let mut removed_relations = Vec::new();
    let mut added_relations = Vec::new();

    old.for_each_resolved_relation(|caller, relations| {
        if removed_entity_set.contains(&caller) {
            return;
        }
        if !entity_in_scope(old, caller, &affected_files) {
            return;
        }
        for relation in relations.iter() {
            if !new_relation_edges.contains(&relation_identity(relation)) {
                removed_relations.push(relation.clone());
            }
        }
    });

    let mut relation_edges_dropped_unbounded: u64 = 0;
    for removed_id in removed_entity_set {
        for caller in old.callers_of(*removed_id) {
            if entity_in_scope(old, caller, &affected_files) {
                continue;
            }
            if let Some(relations) = old.relations_of(caller) {
                for relation in relations.iter() {
                    if relation.callee_id == Some(*removed_id) {
                        removed_relations.push(relation.clone());
                        relation_edges_dropped_unbounded += 1;
                    }
                }
            }
        }
    }

    for entry in new_index.resolved_relation_index.iter() {
        let caller = *entry.key();
        if !entity_in_scope_new(new_index, caller, &affected_files) {
            continue;
        }
        for relation in entry.value().iter() {
            if !old_relation_edges.contains(&relation_identity(relation)) {
                added_relations.push(relation.clone());
            }
        }
    }

    (
        removed_relations,
        added_relations,
        relation_edges_dropped_unbounded,
    )
}

pub(super) fn compute_file_relation_diff<V: RelationIndexView>(
    new_index: &RelationIndex,
    old: &V,
    affected_files: Option<&HashSet<String>>,
) -> Vec<FileRelationDiff> {
    let mut file_relation_diffs = Vec::new();
    let mut all_file_relation_paths: HashSet<String> = HashSet::new();
    old.for_each_file_relation(|path, _| {
        all_file_relation_paths.insert(path.to_string());
    });
    for entry in new_index.file_relation_index.iter() {
        all_file_relation_paths.insert(entry.key().clone());
    }
    let all_file_relation_paths: Vec<String> = all_file_relation_paths
        .into_iter()
        .filter(|path| affected_files.is_none_or(|files| files.contains(path)))
        .collect();

    for path in &all_file_relation_paths {
        let old_edges: HashSet<RelationEdgeIdentity> = old
            .file_relations_of(path)
            .iter()
            .map(relation_identity)
            .collect();
        let new_relations: Vec<ResolvedRelation> = new_index
            .file_relation_index
            .get(path)
            .map(|entry| entry.edges.clone())
            .unwrap_or_default();
        let new_edges: HashSet<RelationEdgeIdentity> =
            new_relations.iter().map(relation_identity).collect();

        let removed: Vec<_> = old
            .file_relations_of(path)
            .into_iter()
            .filter(|r| !new_edges.contains(&relation_identity(r)))
            .collect();
        let added: Vec<_> = new_relations
            .into_iter()
            .filter(|r| !old_edges.contains(&relation_identity(r)))
            .collect();

        if !removed.is_empty() || !added.is_empty() {
            file_relation_diffs.push(FileRelationDiff {
                file_path: path.clone(),
                removed_relations: removed,
                added_relations: added,
            });
        }
    }

    file_relation_diffs
}

pub(super) fn apply_removed_relations(
    index: &RelationIndex,
    removed_relations: &[ResolvedRelation],
) {
    for relation in removed_relations {
        let caller = relation.caller;
        let identity = relation_identity(relation);
        if let Some(mut relations) = index.resolved_relation_index.get_mut(&caller) {
            relations.remove_by_identity(&identity);
            if relations.is_empty() {
                drop(relations);
                index.resolved_relation_index.remove(&caller);
            }
        }
        if let Some(callee_id) = relation.callee_id {
            // Reverse index: only drop caller when no remaining edge
            // from this caller to the callee survives.
            index.maybe_untrack_reverse_caller(callee_id, caller);
            // Keep embedded callers list in sync for legacy code paths.
            if let Some(mut callee_entry) = index.resolved_relation_index.get_mut(&callee_id) {
                // Check if caller still has any edge to callee before
                // removing from the embedded list.
                let still_calls = index
                    .resolved_relation_index
                    .get(&caller)
                    .is_some_and(|rels| rels.iter().any(|r| r.callee_id == Some(callee_id)));
                if !still_calls {
                    callee_entry.remove_caller(&caller);
                    if callee_entry.is_empty() && callee_entry.is_callers_empty() {
                        drop(callee_entry);
                        index.resolved_relation_index.remove(&callee_id);
                    }
                }
            }
        }
    }
}

pub(super) fn apply_added_relations(index: &RelationIndex, added_relations: &[ResolvedRelation]) {
    for relation in added_relations {
        index.add_resolved_relation(relation.clone());
    }
}

pub(super) fn apply_file_relation_diffs(index: &RelationIndex, diffs: &[FileRelationDiff]) {
    for diff in diffs {
        let mut touched_callees: Vec<EntityId> = Vec::new();
        {
            let mut edges = index
                .file_relation_index
                .entry(diff.file_path.clone())
                .or_default();
            for relation in &diff.removed_relations {
                let identity = relation_identity(relation);
                edges.remove_by_identity(&identity);
                if let Some(callee_id) = relation.callee_id
                    && !touched_callees.contains(&callee_id)
                {
                    touched_callees.push(callee_id);
                }
            }
            for relation in &diff.added_relations {
                edges.insert(relation.clone());
                if let Some(callee_id) = relation.callee_id
                    && !touched_callees.contains(&callee_id)
                {
                    touched_callees.push(callee_id);
                }
            }
            let now_empty = edges.is_empty();
            if now_empty {
                drop(edges);
                index.file_relation_index.remove(&diff.file_path);
            }
        }
        let file_path = diff.file_path.clone();
        for callee_id in touched_callees {
            index.untrack_file_caller(Some(callee_id), &file_path);
            let still_calls = index
                .file_relation_index
                .get(&file_path)
                .is_some_and(|edges| edges.iter().any(|rel| rel.callee_id == Some(callee_id)));
            if still_calls {
                index.track_file_caller(Some(callee_id), &file_path);
            }
        }
    }
}
