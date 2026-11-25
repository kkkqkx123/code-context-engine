//! Equivalence and capacity tests for the published snapshot facade.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use cce_types::{
    CanonicalRelationSnapshot, CanonicalRelationTarget, CanonicalSnapshotArtifact, Entity,
    EntityId, EntityKind, FileInfo, FingerprintComponents, ImportKind, ImportTable,
    RelationShardComponents, RelationSnapshotManifest, RelationSnapshotState,
    RelationSnapshotStore, RelationType, ResolvedRelation, ShardDirectory, ShardEntityRef,
    ShardLocation, ShardQueryConfig, SnapshotArtifactOptions, SnapshotDelta, Span, StableSymbolKey,
    StandardizedImport, StorageError, fingerprint_from_components,
};

use crate::index::core::RelationIndex;
use crate::index::snapshot_index::RelationSnapshotIndex;
use crate::index::snapshot_query::{
    SnapshotEntityQueryOps, SnapshotFileQueryOps, SnapshotRelationQueryOps,
};
use crate::index::{EntityIndexOps, ExportIndexOps, FileIndexOps, ImportIndexOps};
use crate::types::{ExportInfo, ExportType};

struct ShardStore {
    manifest: RelationSnapshotManifest,
    snapshot: CanonicalRelationSnapshot,
    directory: ShardDirectory,
    components: HashMap<i64, RelationShardComponents>,
}

impl RelationSnapshotStore for ShardStore {
    fn get_manifest(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Option<RelationSnapshotManifest>, StorageError> {
        if project_id == self.manifest.project_id && epoch == self.manifest.relation_epoch {
            Ok(Some(self.manifest.clone()))
        } else {
            Ok(None)
        }
    }

    fn read_snapshot(
        &self,
        _manifest: &RelationSnapshotManifest,
    ) -> Result<CanonicalRelationSnapshot, StorageError> {
        Ok(self.snapshot.clone())
    }

    fn read_shard_directory(
        &self,
        _project_id: i64,
        _relation_epoch: i64,
    ) -> Result<ShardDirectory, StorageError> {
        Ok(self.directory.clone())
    }

    fn read_shard_components(
        &self,
        _project_id: i64,
        _relation_epoch: i64,
        shard_id: i64,
    ) -> Result<Option<RelationShardComponents>, StorageError> {
        Ok(self.components.get(&shard_id).cloned())
    }

    fn find_base_epoch(
        &self,
        _project_id: i64,
        _delta_epoch: i64,
    ) -> Result<Option<i64>, StorageError> {
        Err(StorageError::validation("test store has no delta chain"))
    }

    fn get_delta_chain(
        &self,
        _project_id: i64,
        _after_epoch: i64,
        _up_to_epoch: i64,
    ) -> Result<Vec<SnapshotDelta>, StorageError> {
        Err(StorageError::validation("test store has no delta chain"))
    }
}

fn entity(id: u64, name: &str, file: &str) -> (Entity, StableSymbolKey) {
    let entity = Entity::new(
        EntityId(id),
        EntityKind::Function,
        name.to_string(),
        Span::default(),
    )
    .with_signature(format!("fn {name}()"));
    let key = StableSymbolKey::for_entity(file, name, &entity);
    (entity, key)
}

fn relation(caller: u64, callee: u64) -> ResolvedRelation {
    ResolvedRelation {
        caller: EntityId(caller),
        callee_id: Some(EntityId(callee)),
        callee_name: format!("fn{callee}"),
        relation_type: RelationType::DirectCall,
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

fn fixture() -> RelationIndex {
    let index = RelationIndex::new();
    for path in ["src/a.rs", "src/b.rs", "src/c.rs", "src/d.rs"] {
        index.add_file(FileInfo {
            id: path.to_string(),
            path: path.to_string(),
            ..Default::default()
        });
    }
    let specs = [
        (1, "alpha", "src/a.rs"),
        (2, "beta", "src/b.rs"),
        (3, "gamma", "src/c.rs"),
        (4, "delta", "src/d.rs"),
    ];
    for (id, name, file) in specs {
        let (value, key) = entity(id, name, file);
        index.add_function_with_path(EntityId(id), value.clone(), file.to_string());
        assert!(
            index.register_symbol_key(file, name, &value, EntityId(id)),
            "fixture symbol should register"
        );
        let _ = key;
    }
    index.add_resolved_relation(relation(1, 2));
    index.add_resolved_relation(relation(2, 3));
    index.add_resolved_relation(relation(3, 4));
    index.add_resolved_relation(relation(1, 4));
    index.add_file_relation(
        "src/d.rs",
        ResolvedRelation {
            caller: EntityId(0),
            callee_id: Some(EntityId(2)),
            callee_name: "beta".to_string(),
            relation_type: RelationType::DirectCall,
            ..relation(0, 2)
        },
    );
    index.add_import_table(
        "src/a.rs".to_string(),
        ImportTable {
            file_id: "src/a.rs".to_string(),
            standardized_imports: vec![StandardizedImport::new(
                ImportKind::ModuleImport,
                "std::io",
            )],
        },
    );
    index.add_exports(
        "src/a.rs".to_string(),
        vec![ExportInfo {
            function_id: EntityId(1),
            function_name: "alpha".to_string(),
            export_type: ExportType::Named,
        }],
    );
    index
        .dependency_graph
        .add_dependency("src/a.rs", "src/b.rs");
    index
        .dependency_graph
        .add_dependency("src/b.rs", "src/c.rs");
    index
        .dependency_graph
        .add_dependency("src/c.rs", "src/d.rs");
    index
}

fn stable_ids(snapshot: &CanonicalRelationSnapshot) -> HashMap<StableSymbolKey, u64> {
    let mut result = HashMap::new();
    let mut used: HashSet<u64> = snapshot
        .entities
        .iter()
        .filter_map(|entity| entity.entity_id)
        .collect();
    let mut next = used.iter().copied().max().map(|id| id + 1).unwrap_or(1);
    for item in &snapshot.entities {
        let id = match item.entity_id {
            Some(id) => id,
            None => {
                while used.contains(&next) {
                    next += 1;
                }
                used.insert(next);
                let id = next;
                next += 1;
                id
            }
        };
        result.insert(item.key.clone(), id);
    }
    result
}

fn shard_store(index: &RelationIndex, max_items: usize) -> ShardStore {
    let snapshot = index
        .to_canonical_snapshot("cfg".to_string())
        .expect("canonical snapshot should build");
    let artifact = CanonicalSnapshotArtifact::from_snapshot(
        &snapshot,
        SnapshotArtifactOptions::default().with_shard_items(max_items),
    )
    .expect("artifact should build");
    let plan =
        cce_types::plan_snapshot_shards(&artifact, max_items).expect("shard plan should build");
    let ids = stable_ids(&snapshot);
    let mut directory = ShardDirectory::default();
    let mut components: HashMap<i64, RelationShardComponents> = HashMap::new();
    for file in &snapshot.files {
        let shard = plan.shard_of(&file.path).expect("file should have a shard");
        directory.file_shards.insert(file.path.clone(), shard);
        components
            .entry(shard)
            .or_insert_with(|| RelationShardComponents {
                files: Vec::new(),
                entities: Vec::new(),
                relations: Vec::new(),
                dependencies: Vec::new(),
                target_entities: Vec::new(),
            });
    }
    for shard_id in plan
        .descriptors
        .iter()
        .map(|descriptor| descriptor.shard_id)
    {
        components
            .entry(shard_id)
            .or_insert_with(|| RelationShardComponents {
                files: Vec::new(),
                entities: Vec::new(),
                relations: Vec::new(),
                dependencies: Vec::new(),
                target_entities: Vec::new(),
            });
    }
    let local_keys: HashSet<StableSymbolKey> =
        snapshot.entities.iter().map(|e| e.key.clone()).collect();
    for entity in &snapshot.entities {
        let shard = plan
            .shard_of(&entity.key.file_path)
            .expect("entity should have a shard");
        let id = *ids.get(&entity.key).expect("entity should have an id");
        let location = ShardLocation {
            shard_id: shard,
            entity_id: id,
            file_path: entity.key.file_path.clone(),
            name: "stub".to_string(),
        };
        directory.entity_locations.insert(id, location.clone());
        directory
            .symbol_locations
            .insert(entity.key.clone(), location.clone());
        directory
            .stable_id_locations
            .insert(entity.key.stable_id().0, location);
        components
            .get_mut(&shard)
            .expect("shard should exist")
            .entities
            .push(entity.clone());
    }
    for relation in &snapshot.relations {
        let shard = plan
            .shard_of(&relation.caller.file_path)
            .expect("relation should have a shard");
        components
            .get_mut(&shard)
            .expect("shard should exist")
            .relations
            .push(relation.clone());
        if let CanonicalRelationTarget::Internal { key } = &relation.target {
            if !local_keys.contains(key) {
                continue;
            }
            let target = *ids.get(key).expect("target should have an id");
            let shards = directory.target_callers.entry(target).or_default();
            if !shards.contains(&shard) {
                shards.push(shard);
            }
        }
        if let CanonicalRelationTarget::External { classification } = &relation.target {
            let classification = classification.clone().expect("external edge needs a class");
            let shards = directory
                .classification_shards
                .entry(classification.clone())
                .or_default();
            if !shards.contains(&shard) {
                shards.push(shard);
            }
            *directory
                .classification_counts
                .entry(classification)
                .or_default() += 1;
        }
    }
    for dependency in &snapshot.dependencies {
        let shard = plan
            .shard_of(&dependency.source_file)
            .expect("dependency should have a shard");
        components
            .get_mut(&shard)
            .expect("shard should exist")
            .dependencies
            .push(dependency.clone());
        directory
            .dependencies
            .entry(dependency.source_file.clone())
            .or_default()
            .push(dependency.target_file.clone());
        directory
            .dependents
            .entry(dependency.target_file.clone())
            .or_default()
            .push(dependency.source_file.clone());
    }
    for file in &snapshot.files {
        let shard = plan.shard_of(&file.path).expect("file should have a shard");
        components
            .get_mut(&shard)
            .expect("shard should exist")
            .files
            .push(file.clone());
    }
    for component in components.values_mut() {
        let local: HashSet<&StableSymbolKey> = component.entities.iter().map(|e| &e.key).collect();
        component.target_entities = ids
            .iter()
            .filter(|(key, _)| !local.contains(key))
            .map(|(key, id)| ShardEntityRef {
                key: key.clone(),
                entity_id: *id,
                file_path: key.file_path.clone(),
            })
            .collect();
    }
    directory.descriptors = plan.descriptors;
    let manifest = RelationSnapshotManifest {
        project_id: 7,
        relation_epoch: 11,
        operation_id: "test".to_string(),
        state: RelationSnapshotState::Active,
        schema_version: snapshot.schema_version,
        parser_version: snapshot.parser_version,
        resolver_version: snapshot.resolver_version,
        path_normalization_version: snapshot.path_normalization_version,
        config_fingerprint: snapshot.config_fingerprint.clone(),
        input_fingerprint: Some(snapshot.input_fingerprint()),
        snapshot_fingerprint: Some(snapshot.fingerprint()),
        file_count: Some(snapshot.files.len()),
        entity_count: Some(snapshot.entities.len()),
        relation_count: Some(snapshot.relations.len()),
        dependency_count: Some(snapshot.dependencies.len()),
        failure_reason: None,
        symbol_key_conflict_count: 0,
        symbol_key_conflict_samples: Vec::new(),
    };
    ShardStore {
        manifest,
        snapshot,
        directory,
        components,
    }
}

fn assert_same_queries(memory: &RelationSnapshotIndex, sharded: &RelationSnapshotIndex) {
    for id in [
        EntityId(1),
        EntityId(2),
        EntityId(3),
        EntityId(4),
        EntityId(99),
    ] {
        assert_eq!(
            memory
                .get_function_by_entity_id(id)
                .map(|entity| entity.name),
            sharded
                .get_function_by_entity_id(id)
                .map(|entity| entity.name),
            "entity lookup {id:?}"
        );
        assert_eq!(
            memory.get_resolved_relations_by_caller(id).map(|v| v.len()),
            sharded
                .get_resolved_relations_by_caller(id)
                .map(|v| v.len()),
            "relations of {id:?}"
        );
        assert_eq!(
            memory.get_callers_by_callee_entity(id),
            sharded.get_callers_by_callee_entity(id),
            "callers of {id:?}"
        );
    }
    assert_eq!(memory.function_count(), sharded.function_count());
    assert_eq!(
        memory.resolved_relation_count(),
        sharded.resolved_relation_count()
    );
    assert_eq!(memory.file_count(), sharded.file_count());
    assert_eq!(
        memory.stable_symbol_keys().len(),
        sharded.stable_symbol_keys().len()
    );
    for file in ["src/a.rs", "src/b.rs", "src/c.rs", "src/d.rs"] {
        assert_eq!(
            memory.get_file(file).map(|f| f.path),
            sharded.get_file(file).map(|f| f.path)
        );
        assert_eq!(
            memory.get_entity_ids_by_file(file).len(),
            sharded.get_entity_ids_by_file(file).len()
        );
        assert_eq!(
            memory.file_relations_of(file).len(),
            sharded.file_relations_of(file).len()
        );
    }
    let mut memory_dependents = memory.transitive_dependents_of("src/a.rs");
    memory_dependents.sort();
    let mut sharded_dependents = sharded.transitive_dependents_of("src/a.rs");
    sharded_dependents.sort();
    assert_eq!(memory_dependents, sharded_dependents);
    let mut memory_dependencies = memory.transitive_dependencies_of("src/a.rs");
    memory_dependencies.sort();
    let mut sharded_dependencies = sharded.transitive_dependencies_of("src/a.rs");
    sharded_dependencies.sort();
    assert_eq!(memory_dependencies, sharded_dependencies);
    assert_eq!(memory.compute_fingerprint(), sharded.compute_fingerprint());
    let mut files = HashSet::new();
    files.insert("src/a.rs".to_string());
    files.insert("src/b.rs".to_string());
    assert_eq!(
        memory.fingerprint_in_files(&files),
        sharded.fingerprint_in_files(&files),
        "file-scoped fingerprint must stay byte-identical"
    );
}

#[test]
fn sharded_facade_matches_in_memory_queries_and_fingerprints() {
    let index = fixture();
    let memory = RelationSnapshotIndex::from_index(&index);
    let store = Arc::new(shard_store(&index, 2));
    let sharded = RelationSnapshotIndex::from_store(
        store,
        7,
        11,
        ShardQueryConfig {
            max_shard_items: 2,
            shard_lru_shards: 2,
            shard_lru_bytes: u64::MAX,
            shard_query_shards: 8,
            route_cache_entries: 16,
        },
    )
    .expect("sharded facade should build");
    assert_same_queries(&memory, &sharded);
    let memory_snapshot = memory
        .to_canonical_snapshot(String::new())
        .expect("memory canonical export");
    let shard_snapshot = sharded
        .to_canonical_snapshot(String::new())
        .expect("shard canonical export");
    assert_eq!(shard_snapshot.fingerprint(), memory_snapshot.fingerprint());
    assert_eq!(shard_snapshot.files.len(), memory_snapshot.files.len());
    assert_eq!(
        shard_snapshot.entities.len(),
        memory_snapshot.entities.len()
    );
    assert_eq!(
        shard_snapshot.relations.len(),
        memory_snapshot.relations.len()
    );
    assert_eq!(
        shard_snapshot.dependencies.len(),
        memory_snapshot.dependencies.len()
    );
}

#[test]
fn shard_lru_is_bounded_by_configuration() {
    let index = fixture();
    let store = Arc::new(shard_store(&index, 1));
    let sharded = RelationSnapshotIndex::from_store(
        store,
        7,
        11,
        ShardQueryConfig {
            max_shard_items: 1,
            shard_lru_shards: 1,
            shard_lru_bytes: u64::MAX,
            shard_query_shards: 8,
            route_cache_entries: 16,
        },
    )
    .expect("sharded facade should build");
    for file in ["src/a.rs", "src/b.rs", "src/c.rs", "src/d.rs"] {
        let _ = sharded.get_file(file);
        assert!(
            sharded
                .capacity_report()
                .expect("shard capacity")
                .loaded_shards
                <= 1,
            "only one shard may stay resident"
        );
    }
    let report = sharded.capacity_report().expect("shard capacity");
    assert_eq!(report.max_loaded_shards, 1);
    assert!(report.lru_evictions > 0);
}

#[test]
fn shard_query_limit_fails_explicitly_instead_of_truncating_edges() {
    let index = fixture();
    let store = Arc::new(shard_store(&index, 1));
    let sharded = RelationSnapshotIndex::from_store(
        store,
        7,
        11,
        ShardQueryConfig {
            max_shard_items: 1,
            shard_lru_shards: 4,
            shard_lru_bytes: u64::MAX,
            shard_query_shards: 1,
            route_cache_entries: 16,
        },
    )
    .expect("sharded facade should build");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = sharded.get_callers_by_callee_entity(EntityId(4));
    }));
    assert!(
        result.is_err(),
        "multi-shard query over the limit must fail"
    );
}

#[test]
fn shard_fingerprint_reuses_canonical_hash_entry_point() {
    let index = fixture();
    let snapshot = index
        .to_canonical_snapshot("cfg".to_string())
        .expect("canonical snapshot should build");
    let direct = fingerprint_from_components(&FingerprintComponents {
        schema_version: snapshot.schema_version,
        parser_version: snapshot.parser_version,
        resolver_version: snapshot.resolver_version,
        path_normalization_version: snapshot.path_normalization_version,
        config_fingerprint: "cfg",
        base_relation_epoch: None,
        files: &snapshot.files,
        entities: &snapshot.entities,
        relations: &snapshot.relations,
        dependencies: &snapshot.dependencies,
    });
    let store = Arc::new(shard_store(&index, 2));
    let sharded = RelationSnapshotIndex::from_store(store, 7, 11, ShardQueryConfig::default())
        .expect("sharded facade should build");
    assert_eq!(
        sharded
            .to_canonical_snapshot("cfg".to_string())
            .expect("canonical export")
            .fingerprint(),
        direct
    );
}

#[test]
fn streaming_artifact_matches_full_graph_artifact() {
    let full = fixture();
    let expected = full
        .build_snapshot_artifact("config".to_string(), SnapshotArtifactOptions::default())
        .expect("full artifact");

    let streamed_index = fixture();
    let mut streamed = super::StreamingSnapshotArtifactBuilder::new(
        &streamed_index,
        "config".to_string(),
        SnapshotArtifactOptions::default(),
    )
    .expect("streaming artifact");
    for (path, ids) in [
        ("src/a.rs", vec![EntityId(1)]),
        ("src/b.rs", vec![EntityId(2)]),
        ("src/c.rs", vec![EntityId(3)]),
        ("src/d.rs", vec![EntityId(4)]),
    ] {
        streamed
            .push_registered_file(path, &ids)
            .expect("registered file should stream");
    }
    for (path, callers) in [
        ("src/a.rs", vec![EntityId(1)]),
        ("src/b.rs", vec![EntityId(2)]),
        ("src/c.rs", vec![EntityId(3)]),
        ("src/d.rs", vec![EntityId(4)]),
    ] {
        streamed
            .push_resolved_file(path, &callers)
            .expect("resolved file should stream");
    }
    let actual = streamed.finish().expect("streaming artifact should finish");

    assert_eq!(
        actual.snapshot_fingerprint(),
        expected.snapshot_fingerprint()
    );
    assert_eq!(actual.input_fingerprint(), expected.input_fingerprint());
    assert_eq!(actual.counts().files, expected.counts().files);
    assert_eq!(actual.counts().entities, expected.counts().entities);
    assert_eq!(actual.counts().relations, expected.counts().relations);
    assert_eq!(actual.counts().dependencies, expected.counts().dependencies);
}
