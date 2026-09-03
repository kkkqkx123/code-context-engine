use std::collections::HashSet;

use cce_types::{AddedEntity, EntityId, EntityKind, normalize_project_path};

use crate::index::core::{RelationIndex, SymbolKey, relation_identity};
use crate::index::view::RelationIndexView;

type EntityDiffResult = (
    Vec<EntityId>,
    Vec<AddedEntity>,
    Vec<(EntityId, EntityId, String, String)>,
    HashSet<EntityId>,
);

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

pub(super) fn compute_entity_diff<V: RelationIndexView>(
    new_index: &RelationIndex,
    old: &V,
    affected_files: Option<&HashSet<String>>,
) -> EntityDiffResult {
    let mut removed_entities = Vec::new();
    let mut removed_entity_meta: Vec<(EntityId, String, EntityKind, String, i64)> = Vec::new();
    let mut added_entities = Vec::new();

    old.for_each_function(|id, entity| {
        if entity_in_scope(old, id, &affected_files) && !new_index.function_index.contains_key(&id)
        {
            let file = old.entity_file_of(id).unwrap_or_default();
            let line = entity.span.start_position.row as i64;
            removed_entity_meta.push((id, entity.name.clone(), entity.kind, file, line));
            removed_entities.push(id);
        }
    });
    for entry in new_index.function_index.iter() {
        let id = *entry.key();
        if entity_in_scope_new(new_index, id, &affected_files) && !old.function_contains(id) {
            let (symbol_key, file_path) = match (
                new_index.get_symbol_key_by_entity_id(id),
                new_index.entity_file_index.get(&id).map(|v| v.clone()),
            ) {
                (Some(key), Some(path)) => (key, path),
                _ => {
                    let entity = entry.value();
                    let fallback_path = new_index
                        .entity_file_index
                        .get(&id)
                        .map(|v| v.clone())
                        .unwrap_or_default();
                    (
                        SymbolKey::new(
                            &fallback_path,
                            &entity.name,
                            entity.kind,
                            &entity.signature,
                        ),
                        fallback_path,
                    )
                }
            };
            added_entities.push(AddedEntity {
                entity: entry.value().clone(),
                symbol_key,
                file_path,
            });
        }
    }

    let mut renamed_pairs: Vec<(EntityId, EntityId, String, String)> = Vec::new();
    let mut matched_added: Vec<usize> = Vec::new();

    for &(removed_id, ref removed_name, removed_kind, ref removed_file, removed_line) in
        &removed_entity_meta
    {
        for (ai, added) in added_entities.iter().enumerate() {
            if matched_added.contains(&ai) {
                continue;
            }
            let added_line = added.entity.span.start_position.row as i64;

            if *removed_file == added.file_path
                && removed_kind == added.entity.kind
                && (removed_line - added_line).abs() <= 2
                && *removed_name != added.entity.name
            {
                renamed_pairs.push((
                    removed_id,
                    added.entity.id,
                    removed_name.clone(),
                    added.entity.name.clone(),
                ));
                matched_added.push(ai);
                break;
            }
        }
    }

    if !matched_added.is_empty() {
        let matched_added_set: HashSet<usize> = matched_added.iter().copied().collect();
        added_entities = added_entities
            .into_iter()
            .enumerate()
            .filter(|(i, _)| !matched_added_set.contains(i))
            .map(|(_, e)| e)
            .collect();
    }
    if !renamed_pairs.is_empty() {
        let renamed_old_ids: HashSet<EntityId> =
            renamed_pairs.iter().map(|(old, _, _, _)| *old).collect();
        removed_entities.retain(|id| !renamed_old_ids.contains(id));
    }
    let removed_entity_set: HashSet<EntityId> = removed_entities.iter().copied().collect();

    (
        removed_entities,
        added_entities,
        renamed_pairs,
        removed_entity_set,
    )
}

pub(super) fn apply_removed_entities(index: &RelationIndex, removed_entities: &[EntityId]) {
    for entity_id in removed_entities {
        index.remove_function(entity_id);
        index.entity_file_index.remove(entity_id);

        if let Some(key) = index.entity_to_symbol_key.write().remove(entity_id) {
            index.symbol_key_to_entity.write().remove(&key);
            index.stable_id_to_entity.write().remove(&key.stable_id().0);
            {
                let mut fsk = index.file_symbol_keys.write();
                if let Some(vec) = fsk.get_mut(&key.file_path) {
                    vec.retain(|k| k != &key);
                    if vec.is_empty() {
                        fsk.remove(&key.file_path);
                    }
                }
            }
        }

        let callee_ids: Vec<EntityId> = index
            .resolved_relation_index
            .get(entity_id)
            .map(|entry| entry.iter().filter_map(|r| r.callee_id).collect())
            .unwrap_or_default();
        index.resolved_relation_index.remove(entity_id);
        for callee_id in callee_ids {
            index.untrack_reverse_caller(callee_id, *entity_id);
            if let Some(mut callee_entry) = index.resolved_relation_index.get_mut(&callee_id) {
                callee_entry.remove_caller(entity_id);
                if callee_entry.is_empty() && callee_entry.is_callers_empty() {
                    drop(callee_entry);
                    index.resolved_relation_index.remove(&callee_id);
                }
            }
        }
        // Reverse index entry where this entity is the callee (incoming
        // callers). The forward edges from those callers to this callee
        // are removed via `removed_relations`, but the reverse entry itself
        // must also be cleared.
        index.reverse_callee_index.remove(entity_id);

        let keys: Vec<EntityId> = index
            .resolved_relation_index
            .iter()
            .filter(|entry| entry.value().callers().contains(entity_id))
            .map(|entry| *entry.key())
            .collect();
        for key in keys {
            index.untrack_reverse_caller(key, *entity_id);
            if let Some(mut entry) = index.resolved_relation_index.get_mut(&key) {
                entry.remove_caller(entity_id);
                if entry.is_empty() && entry.is_callers_empty() {
                    drop(entry);
                    index.resolved_relation_index.remove(&key);
                }
            }
        }
        // Also clean any remaining reverse entries where the removed entity
        // appears as caller. This covers the O(1) reverse path where the
        // forward edges may have already been scheduled as `removed_relations`
        // but the reverse map still holds the caller.
        let reverse_keys: Vec<EntityId> = index
            .reverse_callee_index
            .iter()
            .filter(|entry| entry.value().contains(entity_id))
            .map(|entry| *entry.key())
            .collect();
        for callee in reverse_keys {
            index.untrack_reverse_caller(callee, *entity_id);
        }
    }
}

pub(super) fn apply_renamed_entities(
    index: &RelationIndex,
    renamed_entities: &[(EntityId, EntityId, String, String)],
) {
    for (old_id, new_id, _old_name, new_name) in renamed_entities {
        let caller_ids: Vec<EntityId> = index
            .resolved_relation_index
            .get(old_id)
            .map(|entry| entry.callers().to_vec())
            .unwrap_or_default();

        for caller_id in caller_ids {
            if let Some(mut relations) = index.resolved_relation_index.get_mut(&caller_id) {
                let edges_to_migrate: Vec<_> = relations
                    .iter()
                    .filter(|rel| rel.callee_id == Some(*old_id))
                    .cloned()
                    .collect();

                for old_edge in edges_to_migrate {
                    let old_identity = relation_identity(&old_edge);
                    relations.remove_by_identity(&old_identity);

                    let mut new_edge = old_edge;
                    new_edge.callee_id = Some(*new_id);
                    new_edge.callee_name = new_name.clone();
                    relations.insert(new_edge);
                }
            }
        }

        if let Some((_, old_entry)) = index.resolved_relation_index.remove(old_id) {
            let _ = old_entry;
        }
        if let Some(mut new_entry) = index.resolved_relation_index.get_mut(new_id) {
            new_entry.callers.clear();
            let caller_ids: Vec<EntityId> = new_entry
                .edges
                .iter()
                .filter(|r| r.callee_id == Some(*new_id))
                .map(|r| r.caller)
                .collect();
            for caller in caller_ids {
                new_entry.add_caller(caller);
            }
        }

        if let Some((_, entity)) = index.function_index.remove(old_id) {
            index.function_index.insert(*new_id, entity);
        }

        let file_path = index.entity_file_index.remove(old_id).map(|(_, v)| v);
        if let Some(file_path) = file_path {
            index.entity_file_index.insert(*new_id, file_path);
        }

        let key = index.entity_to_symbol_key.write().remove(old_id);
        if let Some(key) = key {
            let mut new_key = key.clone();
            new_key.scoped_name = new_name.clone();
            new_key.file_path = normalize_project_path(&key.file_path);
            index.symbol_key_to_entity.write().remove(&key);
            index
                .entity_to_symbol_key
                .write()
                .insert(*new_id, new_key.clone());
            index
                .symbol_key_to_entity
                .write()
                .insert(new_key.clone(), *new_id);
            index.stable_id_to_entity.write().remove(&key.stable_id().0);
            index
                .stable_id_to_entity
                .write()
                .insert(new_key.stable_id().0, *new_id);
        }
    }
    // Renaming touches forward edges and symbol tables in ways that are
    // easier to fully reconcile than to patch incrementally; rebuild the
    // reverse index once after the batch.
    if !renamed_entities.is_empty() {
        index.rebuild_reverse_callee_index();
    }
}

pub(super) fn apply_added_entities(index: &RelationIndex, added_entities: &[AddedEntity]) {
    for added in added_entities {
        index.insert_function(added.entity.id, added.entity.clone());
        index
            .entity_file_index
            .insert(added.entity.id, added.file_path.clone());
        index.register_symbol_key(
            &added.symbol_key.file_path,
            &added.symbol_key.scoped_name,
            &added.entity,
            added.entity.id,
        );
        index.track_file_entity(
            &added.file_path,
            added.entity.span.start_position.row as u32,
            added.entity.id,
        );
    }
}
