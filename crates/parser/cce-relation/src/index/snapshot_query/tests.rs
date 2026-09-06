use super::*;
use super::{
    SnapshotEntityQueryOps, SnapshotFileQueryOps, SnapshotFrontendQueryOps,
    SnapshotHierarchyQueryOps, SnapshotRelationQueryOps, SnapshotSymbolQueryOps,
};
use crate::index::core::RelationIndex;
use crate::index::delta::RelationDeltaOps;
use crate::index::entity_index::EntityIndexOps;
use crate::index::snapshot_index::{LayeredSnapshotIndex, RelationSnapshotIndex};
use crate::types::ExportInfo;
use cce_types::relation::CallContext;
use cce_types::{
    Entity, EntityId, EntityKind, FileInfo, ImportTable, RelationType, ResolvedRelation, Span,
};
use std::collections::HashMap;
use std::sync::Arc;

fn create_test_entity(id: u32, name: &str) -> Entity {
    Entity {
        id: EntityId(id.into()),
        kind: EntityKind::Function,
        name: name.to_string(),
        signature: format!("fn {}()", name),
        parameters: Vec::new(),
        return_type: None,
        span: Span::default(),
        depth: 0,
        parent: None,
        children: Vec::new(),
        doc_comment: None,
        modifiers: Vec::new(),
        attributes: HashMap::new(),
        metadata: HashMap::new(),
        is_stdlib: false,
        subtype: None,
        stdlib_category: None,
    }
}

/// Seed an index mirroring the write-side builder behavior.
fn seed(index: &RelationIndex, entities: &[(EntityId, &str)], relations: &[ResolvedRelation]) {
    for (id, name) in entities {
        let entity = create_test_entity(id.0 as u32, name);
        index.add_function_with_path(*id, entity.clone(), "src/lib.rs".to_string());
        index.register_symbol_key("src/lib.rs", name, &entity, *id);
    }
    for relation in relations {
        index.add_resolved_relation(relation.clone());
    }
}

fn internal_edge(
    caller: EntityId,
    callee_id: EntityId,
    callee_name: &str,
    relation_type: RelationType,
) -> ResolvedRelation {
    ResolvedRelation {
        caller,
        callee_id: Some(callee_id),
        callee_name: callee_name.to_string(),
        relation_type,
        span: Span::default(),
        is_external: false,
        external_type: None,
        callee_symbol: None,
        stdlib_category: None,
        owner_type: None,
        call_context: CallContext::Direct,
        overload_signature: None,
    }
}

/// Build a `FileInfo` for tests, keyed by path.
fn file_info(path: &str, hash: &str) -> FileInfo {
    FileInfo {
        id: path.to_string(),
        path: path.to_string(),
        language: "rust".to_string(),
        file_hash: hash.to_string(),
        file_size: 5,
        modified_time: 0,
        parse_status: cce_types::entity::ParseStatus::Success,
        parse_errors: Vec::new(),
        parse_version: 1,
        entity_count: 1,
        relation_count: 1,
        export_count: 0,
        import_count: 0,
        depends_on: Vec::new(),
    }
}

#[test]
fn snapshot_reads_match_mutable_index() {
    let index = RelationIndex::new();
    seed(
        &index,
        &[(EntityId(1), "caller"), (EntityId(2), "callee")],
        &[internal_edge(
            EntityId(1),
            EntityId(2),
            "callee",
            RelationType::DirectCall,
        )],
    );

    let snapshot = RelationSnapshotIndex::from_index(&index);
    let layered = LayeredSnapshotIndex::new(Arc::new(snapshot.clone()));

    for view in [&index as &dyn QueryProbe, &snapshot, &layered] {
        assert_eq!(
            view.get_function_by_entity_id(EntityId(1)).unwrap().name,
            "caller"
        );
        assert!(view.contains_function(EntityId(2)));
        assert_eq!(view.function_count(), 2);
        assert_eq!(
            view.get_file_path_by_entity(EntityId(1)),
            Some("src/lib.rs".to_string())
        );
        assert_eq!(
            view.get_callers_by_callee_entity(EntityId(2)),
            vec![EntityId(1)]
        );
        assert_eq!(
            view.get_resolved_relations_by_caller(EntityId(1))
                .unwrap()
                .len(),
            1
        );
        assert_eq!(view.get_relations_to_entity(EntityId(2)).len(), 1);
        assert_eq!(
            view.get_callers_by_callee_and_type(EntityId(2), RelationType::DirectCall),
            vec![EntityId(1)]
        );
        assert_eq!(
            view.get_derived_classes(EntityId(2)),
            Vec::<EntityId>::new()
        );
        assert_eq!(view.get_entities_by_file("src/lib.rs").len(), 2);
        assert_eq!(view.resolved_relation_count(), 1);
    }
}

/// Uniform probe over all three queryable index surfaces.
trait QueryProbe:
    SnapshotEntityQueryOps
    + SnapshotRelationQueryOps
    + SnapshotHierarchyQueryOps
    + SnapshotFrontendQueryOps
    + SnapshotFileQueryOps
    + SnapshotSymbolQueryOps
{
}
impl<T> QueryProbe for T where
    T: SnapshotEntityQueryOps
        + SnapshotRelationQueryOps
        + SnapshotHierarchyQueryOps
        + SnapshotFrontendQueryOps
        + SnapshotFileQueryOps
        + SnapshotSymbolQueryOps
{
}

/// Delta chain where the last delta wins: delta1 adds extra.rs /
/// extra_fn, delta2 removes both. The canonical snapshot, fingerprint and
/// every query must reflect the final (removed) state.
#[test]
fn layered_snapshot_query_multi_delta_final_state() {
    use crate::index::file_index::FileIndexOps;
    use cce_types::{AddedEntity, SnapshotDelta};

    let base = RelationIndex::new();
    seed(
        &base,
        &[(EntityId(1), "caller"), (EntityId(2), "callee")],
        &[internal_edge(
            EntityId(1),
            EntityId(2),
            "callee",
            RelationType::DirectCall,
        )],
    );
    base.add_file(file_info("src/lib.rs", "lib"));

    let extra_id = EntityId(3);
    let delta1 = SnapshotDelta {
        epoch: 1,
        base_epoch: 0,
        config_fingerprint: "test-config".to_string(),
        removed_files: Vec::new(),
        added_files: vec![file_info("src/extra.rs", "extra")],
        removed_entities: Vec::new(),
        added_entities: vec![AddedEntity {
            entity: create_test_entity(3, "extra_fn"),
            symbol_key: SymbolKey::new(
                "src/extra.rs",
                "extra_fn",
                EntityKind::Function,
                "fn extra_fn()",
            ),
            file_path: "src/extra.rs".to_string(),
        }],
        removed_relations: vec![internal_edge(
            EntityId(1),
            EntityId(2),
            "callee",
            RelationType::DirectCall,
        )],
        added_relations: vec![internal_edge(
            EntityId(1),
            extra_id,
            "extra_fn",
            RelationType::DirectCall,
        )],
        import_diffs: Vec::new(),
        export_diffs: Vec::new(),
        file_relation_diffs: Vec::new(),
        relation_edges_dropped_unbounded: 0,
        dependency_diffs: Vec::new(),
        renamed_entities: Vec::new(),
    };
    let delta2 = SnapshotDelta {
        epoch: 2,
        base_epoch: 1,
        config_fingerprint: "test-config".to_string(),
        removed_files: vec!["src/extra.rs".to_string()],
        added_files: Vec::new(),
        removed_entities: vec![extra_id],
        added_entities: Vec::new(),
        removed_relations: vec![internal_edge(
            EntityId(1),
            extra_id,
            "extra_fn",
            RelationType::DirectCall,
        )],
        added_relations: Vec::new(),
        import_diffs: Vec::new(),
        export_diffs: Vec::new(),
        file_relation_diffs: Vec::new(),
        relation_edges_dropped_unbounded: 0,
        dependency_diffs: Vec::new(),
        renamed_entities: Vec::new(),
    };

    let layered = LayeredSnapshotIndex::with_deltas(
        Arc::new(RelationSnapshotIndex::from_index(&base)),
        vec![Arc::new(delta1.clone()), Arc::new(delta2.clone())],
    );

    // 1. The canonical snapshot must not contain the removed records.
    let canonical = layered
        .to_canonical_snapshot("test-config".to_string())
        .expect("canonical snapshot");
    assert!(
        canonical.files.iter().all(|f| f.path != "src/extra.rs"),
        "extra.rs must not survive a remove after add"
    );
    assert!(
        canonical.entities.iter().all(|e| e.name != "extra_fn"),
        "extra_fn must not survive a remove after add"
    );
    assert!(
        canonical.files.iter().any(|f| f.path == "src/lib.rs"),
        "the untouched base file must remain"
    );

    // 2. Fingerprint matches the fully materialized replay of the chain
    // (the `RelationSnapshotLoader::load` path applies every delta).
    let materialized = RelationIndex::new();
    seed(
        &materialized,
        &[(EntityId(1), "caller"), (EntityId(2), "callee")],
        &[internal_edge(
            EntityId(1),
            EntityId(2),
            "callee",
            RelationType::DirectCall,
        )],
    );
    materialized.add_file(file_info("src/lib.rs", "lib"));
    materialized.apply_delta(&delta1);
    materialized.apply_delta(&delta2);
    let cold = RelationSnapshotIndex::from_index(&materialized);
    assert_eq!(
        layered.compute_fingerprint(),
        cold.compute_fingerprint(),
        "the layered fingerprint must match a materialized replay"
    );

    // 3/4. Queries reflect the final state.
    assert!(
        layered.get_entity_ids_by_file("src/extra.rs").is_empty(),
        "no entities may remain in a removed file"
    );
    assert!(
        layered.get_callers_by_callee_entity(extra_id).is_empty(),
        "a removed entity must have no callers"
    );
    assert!(
        !layered.contains_file("src/extra.rs"),
        "a removed file must not be contained"
    );
    assert!(
        !layered.contains_entity(extra_id),
        "a removed entity must not be contained"
    );
}

/// Delta chain remove-then-re-add: delta1 adds A, delta2 removes A,
/// delta3 re-adds A with different content. The final state must expose
/// the delta3 version.
#[test]
fn layered_snapshot_query_remove_then_readd_restores_latest_version() {
    use crate::index::file_index::FileIndexOps;
    use cce_types::{AddedEntity, SnapshotDelta};

    let base = RelationIndex::new();
    seed(
        &base,
        &[(EntityId(1), "caller"), (EntityId(2), "callee")],
        &[internal_edge(
            EntityId(1),
            EntityId(2),
            "callee",
            RelationType::DirectCall,
        )],
    );
    base.add_file(file_info("src/lib.rs", "lib"));

    let extra_id = EntityId(3);
    let v1_key = SymbolKey::new(
        "src/extra.rs",
        "extra_fn",
        EntityKind::Function,
        "fn extra_fn()",
    );
    let v3_key = SymbolKey::new(
        "src/extra.rs",
        "extra_fn_v3",
        EntityKind::Function,
        "fn extra_fn_v3()",
    );
    let delta1 = SnapshotDelta {
        epoch: 1,
        base_epoch: 0,
        config_fingerprint: "test-config".to_string(),
        removed_files: Vec::new(),
        added_files: vec![file_info("src/extra.rs", "extra-v1")],
        removed_entities: Vec::new(),
        added_entities: vec![AddedEntity {
            entity: create_test_entity(3, "extra_fn"),
            symbol_key: v1_key,
            file_path: "src/extra.rs".to_string(),
        }],
        removed_relations: vec![internal_edge(
            EntityId(1),
            EntityId(2),
            "callee",
            RelationType::DirectCall,
        )],
        added_relations: vec![internal_edge(
            EntityId(1),
            extra_id,
            "extra_fn",
            RelationType::DirectCall,
        )],
        import_diffs: Vec::new(),
        export_diffs: Vec::new(),
        file_relation_diffs: Vec::new(),
        relation_edges_dropped_unbounded: 0,
        dependency_diffs: Vec::new(),
        renamed_entities: Vec::new(),
    };
    let delta2 = SnapshotDelta {
        epoch: 2,
        base_epoch: 1,
        config_fingerprint: "test-config".to_string(),
        removed_files: vec!["src/extra.rs".to_string()],
        added_files: Vec::new(),
        removed_entities: vec![extra_id],
        added_entities: Vec::new(),
        removed_relations: vec![internal_edge(
            EntityId(1),
            extra_id,
            "extra_fn",
            RelationType::DirectCall,
        )],
        added_relations: Vec::new(),
        import_diffs: Vec::new(),
        export_diffs: Vec::new(),
        file_relation_diffs: Vec::new(),
        relation_edges_dropped_unbounded: 0,
        dependency_diffs: Vec::new(),
        renamed_entities: Vec::new(),
    };
    let delta3 = SnapshotDelta {
        epoch: 3,
        base_epoch: 2,
        config_fingerprint: "test-config".to_string(),
        removed_files: Vec::new(),
        added_files: vec![file_info("src/extra.rs", "extra-v3")],
        removed_entities: Vec::new(),
        added_entities: vec![AddedEntity {
            entity: create_test_entity(3, "extra_fn_v3"),
            symbol_key: v3_key,
            file_path: "src/extra.rs".to_string(),
        }],
        removed_relations: Vec::new(),
        added_relations: vec![internal_edge(
            EntityId(1),
            extra_id,
            "extra_fn_v3",
            RelationType::DirectCall,
        )],
        import_diffs: Vec::new(),
        export_diffs: Vec::new(),
        file_relation_diffs: Vec::new(),
        relation_edges_dropped_unbounded: 0,
        dependency_diffs: Vec::new(),
        renamed_entities: Vec::new(),
    };

    let layered = LayeredSnapshotIndex::with_deltas(
        Arc::new(RelationSnapshotIndex::from_index(&base)),
        vec![
            Arc::new(delta1.clone()),
            Arc::new(delta2.clone()),
            Arc::new(delta3.clone()),
        ],
    );

    // The re-added file/entity must be visible with the delta3 version.
    let canonical = layered
        .to_canonical_snapshot("test-config".to_string())
        .expect("canonical snapshot");
    let restored = canonical
        .files
        .iter()
        .find(|f| f.path == "src/extra.rs")
        .expect("re-added file must be visible");
    assert_eq!(restored.input_hash, "extra-v3");
    assert!(
        canonical.entities.iter().any(|e| e.name == "extra_fn_v3"),
        "re-added entity must be visible with the latest version"
    );
    assert!(
        canonical.entities.iter().all(|e| e.name != "extra_fn"),
        "the removed version must not resurface"
    );
    assert_eq!(
        layered.get_entity_ids_by_file("src/extra.rs"),
        vec![extra_id],
        "the re-added entity must be addressable by file"
    );

    // Fingerprint matches the materialized replay of the whole chain.
    let materialized = RelationIndex::new();
    seed(
        &materialized,
        &[(EntityId(1), "caller"), (EntityId(2), "callee")],
        &[internal_edge(
            EntityId(1),
            EntityId(2),
            "callee",
            RelationType::DirectCall,
        )],
    );
    materialized.add_file(file_info("src/lib.rs", "lib"));
    materialized.apply_delta(&delta1);
    materialized.apply_delta(&delta2);
    materialized.apply_delta(&delta3);
    assert_eq!(
        layered.compute_fingerprint(),
        RelationSnapshotIndex::from_index(&materialized).compute_fingerprint(),
        "remove-then-re-add must stay consistent with a materialized replay"
    );
}

/// File removed by delta1 and re-added by delta2: the three file-level
/// query methods must serve the re-added (final) state.
#[test]
fn layered_snapshot_query_file_query_methods_use_final_state() {
    use crate::index::file_index::{ExportIndexOps, FileIndexOps, ImportIndexOps};
    use crate::types::ExportType;
    use cce_types::{
        CanonicalExport, ExportDiff, ImportDiff, ImportKind, SnapshotDelta, StandardizedImport,
    };

    let base_import = StandardizedImport::new(ImportKind::SymbolImport, "old_module");
    let new_import = StandardizedImport::new(ImportKind::SymbolImport, "new_module");
    let exported_key = SymbolKey::new(
        "src/file.rs",
        "exported_fn",
        EntityKind::Function,
        "fn exported_fn()",
    );

    let base = RelationIndex::new();
    seed(&base, &[(EntityId(2), "exported_fn")], &[]);
    base.add_file(file_info("src/file.rs", "file-v1"));
    base.add_import_table(
        "src/file.rs".to_string(),
        ImportTable {
            file_id: "src/file.rs".to_string(),
            standardized_imports: vec![base_import.clone()],
            source_stats: Default::default(),
        },
    );
    base.add_exports(
        "src/file.rs".to_string(),
        vec![ExportInfo {
            function_id: EntityId(2),
            function_name: "exported_fn".to_string(),
            export_type: ExportType::Named,
        }],
    );

    let delta1 = SnapshotDelta {
        epoch: 1,
        base_epoch: 0,
        config_fingerprint: "test-config".to_string(),
        removed_files: vec!["src/file.rs".to_string()],
        added_files: Vec::new(),
        removed_entities: Vec::new(),
        added_entities: Vec::new(),
        removed_relations: Vec::new(),
        added_relations: Vec::new(),
        import_diffs: vec![ImportDiff {
            file_path: "src/file.rs".to_string(),
            removed_imports: vec![base_import],
            added_imports: Vec::new(),
        }],
        export_diffs: vec![ExportDiff {
            file_path: "src/file.rs".to_string(),
            removed_exports: vec![CanonicalExport {
                symbol: exported_key.clone(),
                export_type: "named".to_string(),
            }],
            added_exports: Vec::new(),
        }],
        file_relation_diffs: Vec::new(),
        relation_edges_dropped_unbounded: 0,
        dependency_diffs: Vec::new(),
        renamed_entities: Vec::new(),
    };
    let delta2 = SnapshotDelta {
        epoch: 2,
        base_epoch: 1,
        config_fingerprint: "test-config".to_string(),
        removed_files: Vec::new(),
        added_files: vec![file_info("src/file.rs", "file-v2")],
        removed_entities: Vec::new(),
        added_entities: Vec::new(),
        removed_relations: Vec::new(),
        added_relations: Vec::new(),
        import_diffs: vec![ImportDiff {
            file_path: "src/file.rs".to_string(),
            removed_imports: Vec::new(),
            added_imports: vec![new_import.clone()],
        }],
        export_diffs: vec![ExportDiff {
            file_path: "src/file.rs".to_string(),
            removed_exports: Vec::new(),
            added_exports: vec![CanonicalExport {
                symbol: exported_key,
                export_type: "named".to_string(),
            }],
        }],
        file_relation_diffs: Vec::new(),
        relation_edges_dropped_unbounded: 0,
        dependency_diffs: Vec::new(),
        renamed_entities: Vec::new(),
    };

    let layered = LayeredSnapshotIndex::with_deltas(
        Arc::new(RelationSnapshotIndex::from_index(&base)),
        vec![Arc::new(delta1), Arc::new(delta2)],
    );

    assert!(
        layered.is_file_active("src/file.rs"),
        "a re-added file must be active"
    );
    let table = layered
        .get_import_table("src/file.rs")
        .expect("re-added file must expose its import table");
    assert_eq!(table.standardized_imports, vec![new_import]);
    assert!(
        layered.has_imports("src/file.rs"),
        "the re-added file has imports"
    );
    let exports = layered
        .get_exports("src/file.rs")
        .expect("re-added file must expose its exports");
    assert_eq!(exports.len(), 1);
    assert_eq!(exports[0].function_name, "exported_fn");

    // A file removed and never re-added is fully gone.
    assert!(!layered.is_file_active("src/gone.rs"));
    assert!(layered.get_import_table("src/gone.rs").is_none());
    assert!(!layered.has_imports("src/gone.rs"));
    assert!(layered.get_exports("src/gone.rs").is_none());
}
