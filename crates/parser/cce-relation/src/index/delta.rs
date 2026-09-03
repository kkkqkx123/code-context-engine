//! Delta computation and application for [`RelationIndex`].
//!
//! Defines the edge-identity model used to diff relation indexes
//! ([`relation_identity`]) and the `RelationDeltaOps` extension trait that
//! computes a [`SnapshotDelta`] between two indexes and replays one onto an
//! index in-place.

use std::collections::HashSet;

use cce_types::SnapshotDelta;

use super::core::RelationIndex;
use super::view::RelationIndexView;

pub(crate) use super::core::relation_identity;

mod edge_diff;
mod entity_diff;
mod import_export_diff;

/// Delta computation and application over [`RelationIndex`].
///
/// Kept as an extension trait (like the other index op traits) so the delta
/// machinery lives outside the core index file; the fields it reads/writes
/// are `pub(super)` within the `relation::index` module.
pub trait RelationDeltaOps {
    /// Compute a delta between `old` and `self` (the new index).
    ///
    /// This compares the two indices and produces a `SnapshotDelta` that
    /// describes what changed. The delta can be applied to `old` to reconstruct
    /// `self`.
    ///
    /// Edges are identified by their full identity — `(caller, callee_id,
    /// relation_type)` for internal edges, with external edges additionally
    /// distinguished by callee name and classification, and unresolved
    /// edges by raw target —
    /// so multi-typed edges between the same pair are diffed independently and
    /// `callee_id = None` edges are never lost. Edges referencing an entity
    /// removed by this delta are always scheduled for removal, regardless of
    /// whether the caller was reparsed, which clears dangling references in
    /// bounded propagation scenarios.
    ///
    /// When `affected_files` is supplied, only entities/edges/files in that
    /// set are compared (hot updates know the replaced files in advance), so
    /// diff cost is bounded by the change size instead of the project size.
    /// Edges from callers OUTSIDE the set are still examined via the reverse
    /// index when they point at removed entities (dangling-reference cleanup).
    /// With `None`, every file participates — identical to a full diff.
    fn compute_delta<V: RelationIndexView>(
        &self,
        old: &V,
        epoch: i64,
        base_epoch: i64,
        config_fingerprint: String,
        affected_files: Option<&HashSet<String>>,
    ) -> SnapshotDelta;

    /// Apply an incremental delta to this index in-place.
    ///
    /// This mutates the receiver to reflect the changes described by `delta`.
    /// It is the caller's responsibility to ensure the delta's `base_epoch`
    /// matches the current state of this index.
    fn apply_delta(&self, delta: &SnapshotDelta);
}

impl RelationDeltaOps for RelationIndex {
    fn compute_delta<V: RelationIndexView>(
        &self,
        old: &V,
        epoch: i64,
        base_epoch: i64,
        config_fingerprint: String,
        affected_files: Option<&HashSet<String>>,
    ) -> SnapshotDelta {
        let mut removed_files = Vec::new();
        let mut added_files = Vec::new();

        old.for_each_file(|path, _| {
            if affected_files.is_none_or(|files| files.contains(path))
                && !self.file_records.read().contains_key(path)
            {
                removed_files.push(path.to_string());
            }
        });
        let fr_guard = self.file_records.read();
        for entry in fr_guard.iter() {
            let path = entry.0;
            if affected_files.is_none_or(|files| files.contains(path)) && !old.file_contains(path) {
                added_files.push(entry.1.info.clone());
            }
        }
        drop(fr_guard);

        let (removed_entities, added_entities, renamed_pairs, removed_entity_set) =
            entity_diff::compute_entity_diff(self, old, affected_files);

        let (removed_relations, added_relations, relation_edges_dropped_unbounded) =
            edge_diff::compute_relation_diff(self, old, affected_files, &removed_entity_set);

        let file_relation_diffs = edge_diff::compute_file_relation_diff(self, old, affected_files);

        let import_diffs = import_export_diff::compute_import_diff(self, old, affected_files);
        let export_diffs = import_export_diff::compute_export_diff(self, old, affected_files);
        let dependency_diffs =
            import_export_diff::compute_dependency_diff(self, old, affected_files);

        SnapshotDelta {
            epoch,
            base_epoch,
            config_fingerprint,
            removed_files,
            added_files,
            removed_entities,
            added_entities,
            removed_relations,
            added_relations,
            file_relation_diffs,
            import_diffs,
            export_diffs,
            dependency_diffs,
            relation_edges_dropped_unbounded,
            renamed_entities: renamed_pairs,
        }
    }

    fn apply_delta(&self, delta: &SnapshotDelta) {
        let affected: Vec<String> = delta
            .removed_files
            .iter()
            .chain(delta.added_files.iter().map(|f| &f.path))
            .cloned()
            .collect();
        self.record_affected_files(affected);

        for path in &delta.removed_files {
            self.file_records.write().remove(path.as_str());
            self.dependency_graph.remove_file(path);
            self.take_file_relations(path);
            self.entity_id_remaps.write().remove(path.as_str());
            self.file_symbol_keys.write().remove(path.as_str());
            self.file_entities_by_start.write().remove(path.as_str());
        }

        for file_info in &delta.added_files {
            self.file_records
                .write()
                .entry(file_info.path.clone())
                .or_default()
                .info = file_info.clone();
        }

        entity_diff::apply_removed_entities(self, &delta.removed_entities);
        entity_diff::apply_renamed_entities(self, &delta.renamed_entities);
        entity_diff::apply_added_entities(self, &delta.added_entities);

        edge_diff::apply_removed_relations(self, &delta.removed_relations);
        edge_diff::apply_added_relations(self, &delta.added_relations);
        edge_diff::apply_file_relation_diffs(self, &delta.file_relation_diffs);

        import_export_diff::apply_import_diffs(self, &delta.import_diffs);
        import_export_diff::apply_export_diffs(self, &delta.export_diffs);
        import_export_diff::apply_dependency_diffs(self, &delta.dependency_diffs);

        self.bump_version();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::relation_query::RelationQueryOps;
    use crate::index::test_support::{
        create_test_function_entity, edge_identities, external_edge, internal_edge, seed_index,
        seed_multi_file_index, unresolved_edge,
    };
    use cce_types::{EntityId, EntityKind, RelationType, ResolvedRelation, Span, StableSymbolKey};

    #[test]
    fn delta_round_trip_preserves_sibling_edges_of_same_pair() {
        let old = seed_index(
            &[(EntityId(1), "caller"), (EntityId(2), "callee")],
            &[
                internal_edge(EntityId(1), EntityId(2), "callee", RelationType::DirectCall),
                internal_edge(
                    EntityId(1),
                    EntityId(2),
                    "callee",
                    RelationType::TypeReference,
                ),
            ],
        );
        let new = seed_index(
            &[(EntityId(1), "caller"), (EntityId(2), "callee")],
            &[internal_edge(
                EntityId(1),
                EntityId(2),
                "callee",
                RelationType::DirectCall,
            )],
        );

        let delta = new.compute_delta(&old, 2, 1, "config".to_string(), None);
        assert_eq!(delta.removed_relations.len(), 1);
        assert_eq!(
            delta.removed_relations[0].relation_type,
            RelationType::TypeReference
        );
        assert!(delta.added_relations.is_empty());

        let replayed = old.detached_clone();
        replayed.apply_delta(&delta);
        assert_eq!(edge_identities(&replayed), edge_identities(&new));
    }

    #[test]
    fn external_edges_to_distinct_symbols_stay_independent() {
        let edges = [
            external_edge(EntityId(1), "printf"),
            external_edge(EntityId(1), "malloc"),
        ];
        let old = seed_index(&[(EntityId(1), "caller")], &[]);
        let new = seed_index(&[(EntityId(1), "caller")], &edges);

        assert_eq!(new.relations_of(EntityId(1)).map(|r| r.len()), Some(2));

        let delta = new.compute_delta(&old, 2, 1, "config".to_string(), None);
        assert_eq!(delta.added_relations.len(), 2);

        let replayed = old.detached_clone();
        replayed.apply_delta(&delta);
        assert_eq!(edge_identities(&replayed), edge_identities(&new));
        let names: Vec<String> = replayed
            .relations_of(EntityId(1))
            .expect("caller keeps its edges")
            .iter()
            .map(|r| r.callee_name.clone())
            .collect();
        assert!(names.contains(&"printf".to_string()) && names.contains(&"malloc".to_string()));

        let removed_one = seed_index(
            &[(EntityId(1), "caller")],
            &[external_edge(EntityId(1), "malloc")],
        );
        let removal = removed_one.compute_delta(&new, 3, 2, "config".to_string(), None);
        assert_eq!(removal.removed_relations.len(), 1);
        assert_eq!(removal.removed_relations[0].callee_name, "printf");

        let after_removal = new.detached_clone();
        after_removal.apply_delta(&removal);
        assert_eq!(
            edge_identities(&after_removal),
            edge_identities(&removed_one)
        );
    }

    #[test]
    fn delta_includes_external_and_unresolved_edges() {
        let old = seed_index(
            &[(EntityId(1), "caller")],
            &[
                external_edge(EntityId(1), "printf"),
                unresolved_edge(EntityId(1), "unknown_fn", RelationType::DirectCall),
            ],
        );
        let new = seed_index(
            &[(EntityId(1), "caller")],
            &[
                ResolvedRelation {
                    caller: EntityId(1),
                    callee_id: None,
                    callee_name: "printf".to_string(),
                    relation_type: RelationType::DirectCall,
                    span: Span::default(),
                    is_external: true,
                    external_type: Some(cce_types::ExternalCallType::StandardLibrary {
                        library: "std".to_string(),
                    }),
                    callee_symbol: None,
                    stdlib_category: None,
                    owner_type: None,
                    call_context: cce_types::relation::CallContext::Direct,
                },
                unresolved_edge(EntityId(1), "renamed_fn", RelationType::DirectCall),
            ],
        );

        let delta = new.compute_delta(&old, 2, 1, "config".to_string(), None);
        assert_eq!(
            delta.removed_relations.len(),
            2,
            "removed edges: {:?}",
            delta.removed_relations
        );
        assert_eq!(
            delta.added_relations.len(),
            2,
            "added edges: {:?}",
            delta.added_relations
        );

        let replayed = old.detached_clone();
        replayed.apply_delta(&delta);
        assert_eq!(edge_identities(&replayed), edge_identities(&new));
    }

    #[test]
    fn delta_removes_dangling_edges_to_deleted_entities() {
        let old = seed_index(
            &[(EntityId(1), "caller"), (EntityId(2), "callee")],
            &[internal_edge(
                EntityId(1),
                EntityId(2),
                "callee",
                RelationType::DirectCall,
            )],
        );
        let new = seed_index(&[(EntityId(1), "caller")], &[]);

        let delta = new.compute_delta(&old, 2, 1, "config".to_string(), None);
        assert_eq!(delta.removed_entities, vec![EntityId(2)]);
        assert_eq!(delta.removed_relations.len(), 1);
        assert_eq!(delta.removed_relations[0].callee_id, Some(EntityId(2)));

        let replayed = old.detached_clone();
        replayed.apply_delta(&delta);
        assert_eq!(edge_identities(&replayed), edge_identities(&new));
        assert!(
            replayed.validate_snapshot().is_ok(),
            "replayed index must contain no dangling references"
        );
    }

    #[test]
    fn apply_delta_registers_added_entity_identity_and_file() {
        let old = seed_index(&[(EntityId(1), "caller")], &[]);
        let new = seed_index(
            &[(EntityId(1), "caller"), (EntityId(3), "added_fn")],
            &[internal_edge(
                EntityId(1),
                EntityId(3),
                "added_fn",
                RelationType::DirectCall,
            )],
        );
        new.register_symbol_key(
            "src/lib.rs",
            "added_fn",
            &create_test_function_entity(3, "added_fn"),
            EntityId(3),
        );

        let delta = new.compute_delta(&old, 2, 1, "config".to_string(), None);
        assert_eq!(delta.added_entities.len(), 1);
        let added = &delta.added_entities[0];
        assert_eq!(added.entity.id, EntityId(3));
        assert_eq!(added.file_path, "src/lib.rs");
        assert_eq!(added.symbol_key.scoped_name, "added_fn");

        let replayed = old.detached_clone();
        replayed.apply_delta(&delta);

        assert!(
            super::super::entity_index::EntityIndexOps::contains_function(&replayed, EntityId(3))
        );
        assert_eq!(
            super::super::entity_index::EntityIndexOps::get_file_path_by_entity(
                &replayed,
                EntityId(3)
            ),
            Some("src/lib.rs".to_string())
        );
        let key = StableSymbolKey::new(
            "src/lib.rs",
            "added_fn",
            EntityKind::Function,
            "fn added_fn()",
        );
        assert_eq!(
            replayed.get_entity_id_by_symbol_key(&key),
            Some(EntityId(3))
        );
        assert_eq!(
            replayed.get_entity_id_by_stable_symbol_id(&key.stable_id().0),
            Some(EntityId(3))
        );
        assert_eq!(
            replayed.get_callers_by_callee_entity(EntityId(3)),
            vec![EntityId(1)]
        );
    }

    #[test]
    fn scoped_delta_matches_full_delta_and_skips_untouched_files() {
        let old = seed_multi_file_index(
            &[
                ("caller.rs", &[(EntityId(1), "caller")]),
                ("callee.rs", &[(EntityId(2), "callee")]),
            ],
            &[internal_edge(
                EntityId(1),
                EntityId(2),
                "callee",
                RelationType::DirectCall,
            )],
        );
        let new = seed_multi_file_index(
            &[
                ("caller.rs", &[(EntityId(1), "caller")]),
                ("callee.rs", &[(EntityId(2), "callee")]),
            ],
            &[internal_edge(
                EntityId(1),
                EntityId(2),
                "callee",
                RelationType::TypeReference,
            )],
        );

        let affected = HashSet::from(["caller.rs".to_string()]);
        let scoped = new.compute_delta(&old, 2, 1, "config".to_string(), Some(&affected));
        let full = new.compute_delta(&old, 2, 1, "config".to_string(), None);

        assert_eq!(scoped.removed_relations.len(), 1);
        assert_eq!(
            scoped.removed_relations[0].relation_type,
            RelationType::DirectCall
        );
        assert_eq!(scoped.added_relations.len(), 1);
        assert_eq!(
            scoped.added_relations[0].relation_type,
            RelationType::TypeReference
        );
        assert!(scoped.removed_entities.is_empty());
        assert!(scoped.added_entities.is_empty());

        let replayed = old.detached_clone();
        replayed.apply_delta(&scoped);
        assert_eq!(edge_identities(&replayed), edge_identities(&new));
        assert!(replayed.validate_snapshot().is_ok());

        assert_eq!(full.removed_relations.len(), scoped.removed_relations.len());
        assert_eq!(full.added_relations.len(), scoped.added_relations.len());
    }

    #[test]
    fn scoped_delta_cleans_dangling_edges_from_untouched_callers() {
        let old = seed_multi_file_index(
            &[
                ("caller.rs", &[(EntityId(1), "caller")]),
                ("callee.rs", &[(EntityId(2), "callee")]),
            ],
            &[internal_edge(
                EntityId(1),
                EntityId(2),
                "callee",
                RelationType::DirectCall,
            )],
        );
        let new = seed_multi_file_index(&[("caller.rs", &[(EntityId(1), "caller")])], &[]);

        let affected = HashSet::from(["callee.rs".to_string()]);
        let scoped = new.compute_delta(&old, 2, 1, "config".to_string(), Some(&affected));

        assert_eq!(scoped.removed_entities, vec![EntityId(2)]);
        assert_eq!(scoped.removed_relations.len(), 1);
        assert_eq!(scoped.removed_relations[0].callee_id, Some(EntityId(2)));
        assert_eq!(
            scoped.relation_edges_dropped_unbounded, 1,
            "the caller is outside the scope, so the dropped edge is unbounded"
        );

        let replayed = old.detached_clone();
        replayed.apply_delta(&scoped);
        assert_eq!(edge_identities(&replayed), edge_identities(&new));
        assert!(replayed.validate_snapshot().is_ok());
    }
}
