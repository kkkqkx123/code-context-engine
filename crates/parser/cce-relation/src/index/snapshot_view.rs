//! `RelationIndexView` implementations for the snapshot world.
//!
//! - [`RelationSnapshotIndex`]: a read-only delegation to the shared maps it
//!   zero-copy borrows from the source `RelationIndex`, so a published
//!   snapshot answers the same view queries as the index it came from.
//! - [`LayeredSnapshotIndex`]: base + delta chain merged lazily at read
//!   time (removed entries hidden, added entries visible), with merge
//!   semantics identical to applying each delta via `RelationIndex::
//!   apply_delta` in epoch order. This is the single layered-view
//!   implementation: the runtime projection and the hot-update base cache
//!   share one type instead of maintaining parallel merge logic.

mod layered_view_impl;
mod snapshot_view_impl;

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::Arc;

    use super::super::snapshot_index::{LayeredSnapshotIndex, RelationSnapshotIndex};
    use super::super::view::active_file_set;
    use crate::index::core::RelationIndex;
    use crate::index::core::SymbolKey;
    use crate::index::delta::RelationDeltaOps;
    use crate::index::view::RelationIndexView;
    use crate::index::{EntityIndexOps, ExportIndexOps, FileIndexOps, ImportIndexOps};
    use crate::types::{ExportInfo, ExportType};
    use cce_types::{
        AddedEntity, CanonicalExport, DependencyDiff, Entity, EntityId, EntityKind, ExportDiff,
        FileInfo, ImportDiff, ImportKind, ImportTable, RelationType, ResolvedRelation,
        SnapshotDelta, Span, StableSymbolKey, StandardizedImport,
    };

    /// Signature of a resolved relation for cross-index comparison.
    type RelationSig = (u64, Option<u64>, String, bool);
    /// Signature of an entity/export for cross-index comparison.
    type EntitySig = (u64, String, String);

    fn entity(id: u64, name: &str) -> Entity {
        Entity::new(
            EntityId(id),
            EntityKind::Function,
            name.to_string(),
            Span::default(),
        )
        .with_signature(format!("fn {}()", name))
    }

    fn symbol_key(file: &str, name: &str, e: &Entity) -> SymbolKey {
        StableSymbolKey::new(file, name, EntityKind::Function, &e.signature)
    }

    fn file_info(path: &str) -> FileInfo {
        FileInfo {
            id: path.to_string(),
            path: path.to_string(),
            ..Default::default()
        }
    }

    fn import(source: &str) -> StandardizedImport {
        StandardizedImport::new(ImportKind::ModuleImport, source)
    }

    fn relation(caller: u64, callee: u64) -> ResolvedRelation {
        ResolvedRelation {
            caller: EntityId(caller),
            callee_id: Some(EntityId(callee)),
            callee_name: format!("callee{}", callee),
            relation_type: RelationType::DirectCall,
            span: Span::default(),
            is_external: false,
            external_type: None,
            callee_symbol: None,
            stdlib_category: None,
            owner_type: None,
            call_context: cce_types::relation::CallContext::Direct,
            overload_signature: None,
        }
    }

    fn export(function_id: u64, name: &str) -> ExportInfo {
        ExportInfo {
            function_id: EntityId(function_id),
            function_name: name.to_string(),
            export_type: ExportType::Named,
        }
    }

    /// Base: a.rs (alpha=1, beta=2), b.rs (gamma=3), c.rs (delta=4);
    /// relations 1->2, 2->3, 3->4; imports a=[std::fmt,std::io], b=[std::collections];
    /// exports a=[alpha], b=[gamma,delta]; deps a->b, b->c; symbols for all entities.
    fn build_base() -> RelationIndex {
        let index = RelationIndex::new();
        for path in ["a.rs", "b.rs", "c.rs"] {
            index.add_file(file_info(path));
        }
        let specs: [(u64, &str, &str); 4] = [
            (1, "alpha", "a.rs"),
            (2, "beta", "a.rs"),
            (3, "gamma", "b.rs"),
            (4, "delta", "c.rs"),
        ];
        for (id, name, file) in specs {
            let e = entity(id, name);
            index.add_function_with_path(EntityId(id), e.clone(), file.to_string());
            index.register_symbol_key(file, name, &e, EntityId(id));
        }
        index.add_resolved_relation(relation(1, 2));
        index.add_resolved_relation(relation(2, 3));
        index.add_resolved_relation(relation(3, 4));
        // A file-level edge (file caller, no owning entity) so the
        // file-caller reverse index participates in equivalence checks.
        index.add_file_relation(
            "c.rs",
            ResolvedRelation {
                caller: EntityId(0),
                callee_id: Some(EntityId(2)),
                callee_name: "beta".to_string(),
                relation_type: RelationType::DirectCall,
                span: Span::default(),
                is_external: false,
                external_type: None,
                callee_symbol: None,
                stdlib_category: None,
                owner_type: None,
                call_context: cce_types::relation::CallContext::Direct,
                overload_signature: None,
            },
        );
        index.add_import_table(
            "a.rs".to_string(),
            ImportTable {
                file_id: "a.rs".to_string(),
                standardized_imports: vec![import("std::fmt"), import("std::io")],
                ..Default::default()
            },
        );
        index.add_import_table(
            "b.rs".to_string(),
            ImportTable {
                file_id: "b.rs".to_string(),
                standardized_imports: vec![import("std::collections")],
                ..Default::default()
            },
        );
        index.add_exports("a.rs".to_string(), vec![export(1, "alpha")]);
        index.add_exports(
            "b.rs".to_string(),
            vec![export(3, "gamma"), export(4, "delta")],
        );
        index.dependency_graph.add_dependency("a.rs", "b.rs");
        index.dependency_graph.add_dependency("b.rs", "c.rs");
        index
    }

    fn delta1() -> SnapshotDelta {
        let e5 = entity(5, "epsilon");
        SnapshotDelta {
            epoch: 2,
            base_epoch: 1,
            config_fingerprint: "cfg-1".to_string(),
            removed_files: vec![],
            added_files: vec![],
            removed_entities: vec![EntityId(2)],
            added_entities: vec![AddedEntity {
                entity: e5.clone(),
                symbol_key: symbol_key("b.rs", "epsilon", &e5),
                file_path: "b.rs".to_string(),
            }],
            removed_relations: vec![relation(1, 2)],
            added_relations: vec![relation(1, 5), relation(5, 3)],
            import_diffs: vec![ImportDiff {
                file_path: "a.rs".to_string(),
                removed_imports: vec![import("std::fmt")],
                added_imports: vec![import("std::vec")],
            }],
            export_diffs: vec![ExportDiff {
                file_path: "b.rs".to_string(),
                removed_exports: vec![CanonicalExport {
                    symbol: symbol_key("b.rs", "gamma", &entity(3, "gamma")),
                    export_type: "named".to_string(),
                }],
                added_exports: vec![CanonicalExport {
                    symbol: symbol_key("b.rs", "epsilon", &e5),
                    export_type: "named".to_string(),
                }],
            }],
            file_relation_diffs: Vec::new(),
            dependency_diffs: vec![DependencyDiff {
                source_file: "c.rs".to_string(),
                removed_dependencies: vec![],
                added_dependencies: vec!["a.rs".to_string()],
            }],
            relation_edges_dropped_unbounded: 0,
            renamed_entities: Vec::new(),
        }
    }

    fn delta2() -> SnapshotDelta {
        let e6 = entity(6, "zeta");
        SnapshotDelta {
            epoch: 3,
            base_epoch: 1,
            config_fingerprint: "cfg-1".to_string(),
            removed_files: vec!["c.rs".to_string()],
            added_files: vec![],
            removed_entities: vec![EntityId(4)],
            added_entities: vec![AddedEntity {
                entity: e6.clone(),
                symbol_key: symbol_key("a.rs", "zeta", &e6),
                file_path: "a.rs".to_string(),
            }],
            removed_relations: vec![relation(3, 4)],
            added_relations: vec![relation(1, 6)],
            import_diffs: vec![ImportDiff {
                file_path: "b.rs".to_string(),
                removed_imports: vec![],
                added_imports: vec![import("std::vec")],
            }],
            export_diffs: vec![ExportDiff {
                file_path: "a.rs".to_string(),
                removed_exports: vec![],
                added_exports: vec![CanonicalExport {
                    symbol: symbol_key("a.rs", "zeta", &e6),
                    export_type: "named".to_string(),
                }],
            }],
            file_relation_diffs: Vec::new(),
            dependency_diffs: vec![DependencyDiff {
                source_file: "b.rs".to_string(),
                removed_dependencies: vec!["c.rs".to_string()],
                added_dependencies: vec![],
            }],
            relation_edges_dropped_unbounded: 0,
            renamed_entities: Vec::new(),
        }
    }

    /// Layered snapshot view over `base` with the given chain.
    fn layered_view(base: &RelationIndex, deltas: Vec<SnapshotDelta>) -> LayeredSnapshotIndex {
        let deltas = deltas.into_iter().map(Arc::new).collect();
        LayeredSnapshotIndex::with_deltas(
            Arc::new(RelationSnapshotIndex::from_index_shared(base)),
            deltas,
        )
    }

    fn sorted<T: Ord>(mut v: Vec<T>) -> Vec<T> {
        v.sort();
        v
    }

    fn rel_sig(rels: &[ResolvedRelation]) -> Vec<RelationSig> {
        let mut v: Vec<_> = rels
            .iter()
            .map(|r| {
                (
                    r.caller.0,
                    r.callee_id.map(|id| id.0),
                    r.callee_name.clone(),
                    r.is_external,
                )
            })
            .collect();
        v.sort();
        v
    }

    fn entity_sig(entities: &[Entity]) -> Vec<EntitySig> {
        let mut v: Vec<_> = entities
            .iter()
            .map(|e| (e.id.0, e.name.clone(), e.signature.clone()))
            .collect();
        v.sort();
        v
    }

    fn import_sig(table: &ImportTable) -> Vec<String> {
        let mut v: Vec<String> = table
            .standardized_imports
            .iter()
            .map(|i| format!("{:?}", i))
            .collect();
        v.sort();
        v
    }

    fn export_sig(exports: &[ExportInfo]) -> Vec<EntitySig> {
        let mut v: Vec<_> = exports
            .iter()
            .map(|e| {
                (
                    e.function_id.0,
                    e.function_name.clone(),
                    format!("{:?}", e.export_type),
                )
            })
            .collect();
        v.sort();
        v
    }

    /// Compare every `RelationIndexView` method of `view` against the
    /// materialized `mat` (sorted where iteration order is not meaningful).
    fn assert_view_matches_materialized<V: RelationIndexView>(view: &V, mat: &RelationIndex) {
        // File layer
        let mut v_files: Vec<String> = Vec::new();
        view.for_each_file(|p, _| v_files.push(p.to_string()));
        let mut m_files: Vec<String> = Vec::new();
        mat.for_each_file(|p, _| m_files.push(p.to_string()));
        assert_eq!(sorted(v_files), sorted(m_files), "for_each_file");
        for p in ["a.rs", "b.rs", "c.rs"] {
            assert_eq!(
                view.file_contains(p),
                mat.file_contains(p),
                "file_contains {p}"
            );
        }

        // Entity layer
        let mut v_fns: Vec<EntitySig> = Vec::new();
        view.for_each_function(|id, e| v_fns.push((id.0, e.name.clone(), e.signature.clone())));
        let mut m_fns: Vec<EntitySig> = Vec::new();
        mat.for_each_function(|id, e| m_fns.push((id.0, e.name.clone(), e.signature.clone())));
        assert_eq!(sorted(v_fns), sorted(m_fns), "for_each_function");
        for id in 1..=8u64 {
            assert_eq!(
                view.function_contains(EntityId(id)),
                mat.function_contains(EntityId(id)),
                "function_contains {id}"
            );
            assert_eq!(
                view.entity_file_of(EntityId(id)),
                mat.entity_file_of(EntityId(id)),
                "entity_file_of {id}"
            );
            assert_eq!(
                view.symbol_key_of(EntityId(id)).map(|k| k.sort_key()),
                mat.symbol_key_of(EntityId(id)).map(|k| k.sort_key()),
                "symbol_key_of {id}"
            );
            assert_eq!(
                view.relations_of(EntityId(id)).map(|r| rel_sig(&r)),
                mat.relations_of(EntityId(id)).map(|r| rel_sig(&r)),
                "relations_of {id}"
            );
            assert_eq!(
                sorted(view.callers_of(EntityId(id))),
                sorted(mat.callers_of(EntityId(id))),
                "callers_of {id}"
            );
        }

        // Relation layer (whole-map iteration)
        let mut v_rels: Vec<(u64, Vec<RelationSig>)> = Vec::new();
        view.for_each_resolved_relation(|c, rels| v_rels.push((c.0, rel_sig(rels))));
        let mut m_rels: Vec<(u64, Vec<RelationSig>)> = Vec::new();
        mat.for_each_resolved_relation(|c, rels| m_rels.push((c.0, rel_sig(rels))));
        v_rels.sort_by_key(|(c, _)| *c);
        m_rels.sort_by_key(|(c, _)| *c);
        assert_eq!(v_rels, m_rels, "for_each_resolved_relation");

        // File-relation layer (whole-map iteration)
        let mut v_frels: Vec<(String, Vec<RelationSig>)> = Vec::new();
        view.for_each_file_relation(|p, rels| v_frels.push((p.to_string(), rel_sig(rels))));
        let mut m_frels: Vec<(String, Vec<RelationSig>)> = Vec::new();
        mat.for_each_file_relation(|p, rels| m_frels.push((p.to_string(), rel_sig(rels))));
        v_frels.sort_by_key(|(p, _)| p.clone());
        m_frels.sort_by_key(|(p, _)| p.clone());
        assert_eq!(v_frels, m_frels, "for_each_file_relation");
        for p in ["a.rs", "b.rs", "c.rs"] {
            assert_eq!(
                rel_sig(&view.file_relations_of(p)),
                rel_sig(&mat.file_relations_of(p)),
                "file_relations_of {p}"
            );
        }

        // Import / export layer
        for p in ["a.rs", "b.rs", "c.rs"] {
            assert_eq!(
                view.imports_of(p).map(|t| import_sig(&t)),
                mat.imports_of(p).map(|t| import_sig(&t)),
                "imports_of {p}"
            );
            assert_eq!(
                view.exports_of(p).map(|e| export_sig(&e)),
                mat.exports_of(p).map(|e| export_sig(&e)),
                "exports_of {p}"
            );
        }
        let mut v_imports: Vec<(String, Vec<String>)> = Vec::new();
        view.for_each_import(|p, t| v_imports.push((p.to_string(), import_sig(t))));
        let mut m_imports: Vec<(String, Vec<String>)> = Vec::new();
        mat.for_each_import(|p, t| m_imports.push((p.to_string(), import_sig(t))));
        v_imports.sort_by_key(|(p, _)| p.clone());
        m_imports.sort_by_key(|(p, _)| p.clone());
        assert_eq!(v_imports, m_imports, "for_each_import");
        let mut v_exports: Vec<(String, Vec<EntitySig>)> = Vec::new();
        view.for_each_export(|p, e| v_exports.push((p.to_string(), export_sig(e))));
        let mut m_exports: Vec<(String, Vec<EntitySig>)> = Vec::new();
        mat.for_each_export(|p, e| m_exports.push((p.to_string(), export_sig(e))));
        v_exports.sort_by_key(|(p, _)| p.clone());
        m_exports.sort_by_key(|(p, _)| p.clone());
        assert_eq!(v_exports, m_exports, "for_each_export");

        // Dependency layer
        assert_eq!(
            sorted(view.dependency_files()),
            sorted(mat.dependency_files()),
            "dependency_files"
        );
        for p in ["a.rs", "b.rs", "c.rs"] {
            assert_eq!(
                sorted(view.dependencies_of(p)),
                sorted(mat.dependencies_of(p)),
                "dependencies_of {p}"
            );
            assert_eq!(
                sorted(view.dependents_of(p)),
                sorted(mat.dependents_of(p)),
                "dependents_of {p}"
            );
            for depth in [0usize, 2usize] {
                assert_eq!(
                    sorted(view.collect_transitive_dependents(p, depth)),
                    sorted(mat.collect_transitive_dependents(p, depth)),
                    "collect_transitive_dependents {p} depth {depth}"
                );
                assert_eq!(
                    sorted(view.collect_transitive_dependencies(p, depth)),
                    sorted(mat.collect_transitive_dependencies(p, depth)),
                    "collect_transitive_dependencies {p} depth {depth}"
                );
            }
        }

        // Symbol context
        let mut v_ebf: Vec<(String, Vec<EntitySig>)> = view
            .entities_by_file()
            .into_iter()
            .map(|(p, es)| (p, entity_sig(&es)))
            .collect();
        let mut m_ebf: Vec<(String, Vec<EntitySig>)> = mat
            .entities_by_file()
            .into_iter()
            .map(|(p, es)| (p, entity_sig(&es)))
            .collect();
        v_ebf.sort_by_key(|(p, _)| p.clone());
        m_ebf.sort_by_key(|(p, _)| p.clone());
        assert_eq!(v_ebf, m_ebf, "entities_by_file");
        for p in ["a.rs", "b.rs", "c.rs"] {
            let mut v_ef = entity_sig(&view.entities_of_file(p));
            let mut m_ef = entity_sig(&mat.entities_of_file(p));
            v_ef.sort();
            m_ef.sort();
            assert_eq!(v_ef, m_ef, "entities_of_file {p}");
        }
        let callees: [u64; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
        for callee in callees {
            assert_eq!(
                sorted(view.file_callers_of(EntityId(callee))),
                sorted(mat.file_callers_of(EntityId(callee))),
                "file_callers_of {callee}"
            );
        }
        let v_keys: Vec<String> = view
            .stable_symbol_keys()
            .iter()
            .map(|k| k.sort_key())
            .collect();
        let m_keys: Vec<String> = mat
            .stable_symbol_keys()
            .iter()
            .map(|k| k.sort_key())
            .collect();
        assert_eq!(sorted(v_keys), sorted(m_keys), "stable_symbol_keys");
        assert_eq!(view.max_entity_id(), mat.max_entity_id(), "max_entity_id");
    }

    #[test]
    fn layered_view_matches_applied_delta() {
        let base = build_base();
        let d1 = delta1();
        let d2 = delta2();

        // Materialized reference: base clone + sequential apply_delta.
        let materialized = base.detached_clone();
        materialized.apply_delta(&d1);
        materialized.apply_delta(&d2);

        // Layered view over the same base with the same delta chain.
        let view = layered_view(&base, vec![d1, d2]);

        assert_view_matches_materialized(&view, &materialized);
    }

    #[test]
    fn layered_view_empty_chain_equals_base() {
        let base = build_base();
        let view = layered_view(&base, vec![]);
        assert_view_matches_materialized(&view, &base);
    }

    #[test]
    fn max_entity_id_includes_added() {
        let base = build_base();
        let view = layered_view(&base, vec![]);
        assert_eq!(view.max_entity_id(), 4);

        let e = entity(42, "big");
        let delta = SnapshotDelta {
            epoch: 2,
            base_epoch: 1,
            config_fingerprint: "cfg-1".to_string(),
            removed_files: vec![],
            added_files: vec![],
            removed_entities: vec![],
            added_entities: vec![AddedEntity {
                entity: e.clone(),
                symbol_key: symbol_key("a.rs", "big", &e),
                file_path: "a.rs".to_string(),
            }],
            removed_relations: vec![],
            added_relations: vec![],
            import_diffs: vec![],
            export_diffs: vec![],
            file_relation_diffs: Vec::new(),
            relation_edges_dropped_unbounded: 0,
            dependency_diffs: vec![],
            renamed_entities: Vec::new(),
        };
        let view = layered_view(&base, vec![delta]);
        assert_eq!(view.max_entity_id(), 42);
    }

    #[test]
    fn stable_symbol_keys_in_files_matches_scope() {
        let base = build_base();
        let d1 = delta1();

        let materialized = base.detached_clone();
        materialized.apply_delta(&d1);

        let view = layered_view(&base, vec![d1]);
        let scope: HashSet<String> = ["a.rs".to_string()].into_iter().collect();

        let mut view_keys: Vec<String> = view
            .stable_symbol_keys_in_files(&scope)
            .iter()
            .map(|k| k.sort_key())
            .collect();
        let mut mat_keys: Vec<String> = materialized
            .stable_symbol_keys_in_files(&scope)
            .iter()
            .map(|k| k.sort_key())
            .collect();
        view_keys.sort();
        mat_keys.sort();
        assert_eq!(view_keys, mat_keys);
    }

    #[test]
    fn layered_fingerprint_in_files_matches_materialized() {
        let base = build_base();
        let d1 = delta1();
        let d2 = delta2();

        // Materialized reference: base clone + sequential apply_delta.
        let materialized = base.detached_clone();
        materialized.apply_delta(&d1);
        materialized.apply_delta(&d2);

        // Layered snapshot over the same base with the same delta chain.
        let layered = layered_view(&base, vec![d1, d2]);

        let files = active_file_set(&materialized.file_records, &materialized.entity_file_index);
        assert_eq!(
            layered.fingerprint_in_files(&files),
            materialized.fingerprint_in_files(&files),
            "layered file-scoped fingerprint must match the materialized index"
        );
        assert_eq!(
            layered.compute_fingerprint(),
            materialized.compute_fingerprint(),
            "layered full fingerprint must match the materialized index"
        );

        // Scoped form agrees with the full fingerprint over the full file set.
        assert_eq!(
            layered.fingerprint_in_files(&files),
            layered.compute_fingerprint()
        );
    }
}
