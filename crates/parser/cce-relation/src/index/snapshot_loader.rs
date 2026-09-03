//! Strict loader for canonical relationship epochs.

use std::collections::HashMap;

use cce_types::entity::ParseStatus;
use cce_types::relation::CallContext;
use cce_types::relation::RelationSnapshotStore;
use cce_types::{
    CanonicalEntity, CanonicalRelationSnapshot, CanonicalRelationTarget, Entity, EntityId,
    FileInfo, RelationSnapshotManifest, RelationSnapshotState, ResolvedRelation, StableSymbolKey,
    StorageError,
};

use super::core::{ExportInfo, ExportType, RelationIndex};
use super::delta::RelationDeltaOps;
use super::entity_index::EntityIndexOps;
use super::file_index::{ExportIndexOps, FileIndexOps, ImportIndexOps};

pub struct RelationSnapshotLoader;

impl RelationSnapshotLoader {
    /// Load a manifest-backed epoch, optionally replaying delta chains.
    ///
    /// If the epoch's manifest state is `Delta`, this walks the delta chain
    /// backwards to find the nearest `Active` base, loads it, then replays
    /// all deltas sequentially to reconstruct the full index.
    pub fn load(
        store: &dyn RelationSnapshotStore,
        project_id: i64,
        relation_epoch: i64,
    ) -> Result<RelationIndex, StorageError> {
        let manifest = store
            .get_manifest(project_id, relation_epoch)?
            .ok_or_else(|| {
                StorageError::Validation(format!("relation epoch {relation_epoch} has no manifest"))
            })?;

        match manifest.state {
            RelationSnapshotState::Ready | RelationSnapshotState::Active => {
                let snapshot = store.read_snapshot(&manifest)?;
                Self::validate_manifest(&manifest, &snapshot)?;
                Self::build_index(&snapshot)
            }
            RelationSnapshotState::Delta => {
                let base_epoch = store
                    .find_base_epoch(project_id, relation_epoch)?
                    .ok_or_else(|| {
                        StorageError::Validation(format!(
                            "no base epoch found for delta chain at epoch {relation_epoch}"
                        ))
                    })?;

                if base_epoch == relation_epoch {
                    return Err(StorageError::Validation(format!(
                        "epoch {relation_epoch} is marked Delta but has no delta row"
                    )));
                }

                let base_manifest =
                    store.get_manifest(project_id, base_epoch)?.ok_or_else(|| {
                        StorageError::Validation(format!(
                            "base epoch {base_epoch} referenced by delta chain is missing"
                        ))
                    })?;

                let base_snapshot = store.read_snapshot(&base_manifest)?;
                let index = Self::build_index(&base_snapshot)?;

                let deltas = store.get_delta_chain(project_id, base_epoch, relation_epoch)?;
                const DELTA_CHAIN_COMPACTION_THRESHOLD: usize = 10;
                if deltas.len() > DELTA_CHAIN_COMPACTION_THRESHOLD {
                    tracing::warn!(
                        project_id,
                        relation_epoch,
                        delta_len = deltas.len(),
                        threshold = DELTA_CHAIN_COMPACTION_THRESHOLD,
                        "delta chain exceeds compaction threshold; background compaction should be scheduled"
                    );
                }

                for delta in &deltas {
                    index.apply_delta(delta);
                }

                Ok(index)
            }
            _ => Err(StorageError::Validation(format!(
                "relation epoch {relation_epoch} is not loadable ({:?})",
                manifest.state
            ))),
        }
    }

    pub fn load_canonical(
        snapshot: &CanonicalRelationSnapshot,
    ) -> Result<RelationIndex, StorageError> {
        snapshot
            .validate_versions()
            .map_err(StorageError::Validation)?;
        Self::build_index(snapshot)
    }

    fn validate_manifest(
        manifest: &RelationSnapshotManifest,
        snapshot: &CanonicalRelationSnapshot,
    ) -> Result<(), StorageError> {
        snapshot
            .validate_versions()
            .map_err(StorageError::Validation)?;
        if manifest.schema_version != snapshot.schema_version
            || manifest.parser_version != snapshot.parser_version
            || manifest.resolver_version != snapshot.resolver_version
            || manifest.path_normalization_version != snapshot.path_normalization_version
            || manifest.config_fingerprint != snapshot.config_fingerprint
        {
            return Err(StorageError::Validation(
                "relation manifest version/config does not match its payload".to_string(),
            ));
        }
        let expected_counts = (
            Some(snapshot.files.len()),
            Some(snapshot.entities.len()),
            Some(snapshot.relations.len()),
            Some(snapshot.dependencies.len()),
        );
        let manifest_counts = (
            manifest.file_count,
            manifest.entity_count,
            manifest.relation_count,
            manifest.dependency_count,
        );
        if expected_counts != manifest_counts {
            return Err(StorageError::Validation(
                "relation manifest counts do not match its payload".to_string(),
            ));
        }
        if manifest.input_fingerprint.as_deref() != Some(snapshot.input_fingerprint().as_str()) {
            return Err(StorageError::Validation(
                "relation input fingerprint mismatch".to_string(),
            ));
        }
        if manifest.snapshot_fingerprint.as_deref() != Some(snapshot.fingerprint().as_str()) {
            return Err(StorageError::Validation(
                "relation snapshot fingerprint mismatch".to_string(),
            ));
        }
        Ok(())
    }

    fn build_index(snapshot: &CanonicalRelationSnapshot) -> Result<RelationIndex, StorageError> {
        let index = RelationIndex::new();
        let mut symbol_ids: HashMap<StableSymbolKey, EntityId> = HashMap::new();

        // Aggregate per-file counts and parent→children relations once instead of
        // rescanning the whole snapshot per file/entity (O(F×E) + O(F×R) + O(E²) →
        // O(E) + O(R)).
        let mut entity_count_by_file: HashMap<&str, usize> = HashMap::new();
        for entity in &snapshot.entities {
            *entity_count_by_file
                .entry(entity.key.file_path.as_str())
                .or_default() += 1;
        }
        let mut relation_count_by_file: HashMap<&str, usize> = HashMap::new();
        for relation in &snapshot.relations {
            *relation_count_by_file
                .entry(relation.caller.file_path.as_str())
                .or_default() += 1;
        }
        let mut dependencies_by_file: HashMap<&str, Vec<String>> = HashMap::new();
        for dependency in &snapshot.dependencies {
            dependencies_by_file
                .entry(dependency.source_file.as_str())
                .or_default()
                .push(dependency.target_file.clone());
        }
        let mut children_by_parent: HashMap<&StableSymbolKey, Vec<&CanonicalEntity>> =
            HashMap::new();
        for entity in &snapshot.entities {
            if let Some(parent) = &entity.parent {
                children_by_parent.entry(parent).or_default().push(entity);
            }
        }

        for file in &snapshot.files {
            index.add_file(FileInfo {
                id: file.path.clone(),
                path: file.path.clone(),
                language: file.language.clone(),
                file_hash: file.input_hash.clone(),
                file_size: file.file_size,
                modified_time: 0,
                parse_status: ParseStatus::Success,
                parse_errors: Vec::new(),
                parse_version: snapshot.parser_version as u64,
                entity_count: entity_count_by_file
                    .get(file.path.as_str())
                    .copied()
                    .unwrap_or(0),
                relation_count: relation_count_by_file
                    .get(file.path.as_str())
                    .copied()
                    .unwrap_or(0),
                export_count: file.exports.len(),
                import_count: file.imports.len(),
                depends_on: dependencies_by_file
                    .remove(file.path.as_str())
                    .unwrap_or_default(),
            });
            index.add_import_table(
                file.path.clone(),
                cce_types::ImportTable {
                    file_id: file.path.clone(),
                    standardized_imports: file.imports.clone(),
                    source_stats: Default::default(),
                },
            );
        }

        for (offset, canonical) in snapshot.entities.iter().enumerate() {
            let entity_id = canonical
                .entity_id
                .map(EntityId)
                .unwrap_or_else(|| EntityId((offset + 1) as u64));
            if symbol_ids
                .insert(canonical.key.clone(), entity_id)
                .is_some()
            {
                return Err(StorageError::Validation(format!(
                    "duplicate stable symbol {}",
                    canonical.key.scoped_name
                )));
            }
        }

        for canonical in &snapshot.entities {
            let entity_id = required_id(&symbol_ids, &canonical.key, "entity")?;
            let parent = canonical
                .parent
                .as_ref()
                .map(|key| required_id(&symbol_ids, key, "parent"))
                .transpose()?;
            let children = children_by_parent
                .get(&canonical.key)
                .map(|candidates| {
                    candidates
                        .iter()
                        .map(|candidate| required_id(&symbol_ids, &candidate.key, "child"))
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?
                .unwrap_or_default();
            let entity = Entity {
                id: entity_id,
                kind: canonical.key.kind,
                name: canonical.name.clone(),
                signature: canonical.signature.clone(),
                parameters: canonical.parameters.clone(),
                return_type: canonical.return_type.clone(),
                span: canonical.span,
                depth: canonical.depth,
                parent,
                children,
                doc_comment: canonical.doc_comment.clone(),
                modifiers: canonical.modifiers.clone(),
                attributes: canonical.attributes.clone().into_iter().collect(),
                metadata: canonical.metadata.clone().into_iter().collect(),
                is_stdlib: canonical.is_stdlib,
                stdlib_category: canonical.stdlib_category,
                subtype: canonical.subtype.clone(),
            };
            index.add_function_with_path(
                entity_id,
                entity.clone(),
                canonical.key.file_path.clone(),
            );
            index.register_symbol_key(
                &canonical.key.file_path,
                &canonical.key.scoped_name,
                &entity,
                entity_id,
            );
        }

        for file in &snapshot.files {
            let exports = file
                .exports
                .iter()
                .map(|export| {
                    Ok(ExportInfo {
                        function_id: required_id(&symbol_ids, &export.symbol, "export")?,
                        function_name: export.symbol.scoped_name.clone(),
                        export_type: match export.export_type.as_str() {
                            "named" => ExportType::Named,
                            "default" => ExportType::Default,
                            "wildcard" => ExportType::Wildcard,
                            value => {
                                return Err(StorageError::Validation(format!(
                                    "invalid canonical export type: {value}"
                                )));
                            }
                        },
                    })
                })
                .collect::<Result<Vec<_>, StorageError>>()?;
            if !exports.is_empty() {
                index.add_exports(file.path.clone(), exports);
            }
        }

        for canonical in &snapshot.relations {
            let (callee_id, is_external, external_type) = match &canonical.target {
                CanonicalRelationTarget::Internal { key } => (
                    Some(required_id(&symbol_ids, key, "relation target")?),
                    false,
                    None,
                ),
                CanonicalRelationTarget::External { classification } => {
                    (None, true, classification.clone())
                }
                CanonicalRelationTarget::Unresolved { .. } => (None, false, None),
            };
            if canonical.caller.is_file_placeholder() {
                // File-scoped edge: attributed to the file itself, never to an
                // entity. Restore it into `file_relation_index` so entity-level
                // queries and `function_index` stay unpolluted.
                index.add_file_relation(
                    &canonical.caller.file_path,
                    ResolvedRelation {
                        // File-scoped edge: attributed to the file itself, never to an
                        // entity. The caller is not used for file-level edges; it is
                        // stored as a placeholder and ignored by queries.
                        caller: EntityId(0),
                        callee_id,
                        callee_name: canonical.raw_target.clone(),
                        relation_type: canonical.relation_type,
                        span: canonical.span,
                        is_external,
                        external_type,
                        callee_symbol: None,
                        stdlib_category: canonical.stdlib_category,
                        owner_type: None,
                        call_context: CallContext::Direct,
                    },
                );
                continue;
            }
            let caller = required_id(&symbol_ids, &canonical.caller, "relation caller")?;
            index.add_resolved_relation(ResolvedRelation {
                caller,
                callee_id,
                callee_name: canonical.raw_target.clone(),
                relation_type: canonical.relation_type,
                span: canonical.span,
                is_external,
                external_type,
                callee_symbol: None,
                stdlib_category: canonical.stdlib_category,
                owner_type: None,
                call_context: CallContext::Direct,
            });
        }

        // Initialize entity_id_counter to prevent hot-update EntityId collisions.
        let max_id = index
            .function_index
            .iter()
            .map(|e| e.key().0)
            .max()
            .unwrap_or(0);
        index
            .entity_id_counter
            .store(max_id + 1, std::sync::atomic::Ordering::Relaxed);

        for dependency in &snapshot.dependencies {
            index
                .dependency_graph
                .add_dependency(&dependency.source_file, &dependency.target_file);
        }
        Ok(index)
    }
}

fn required_id(
    ids: &HashMap<StableSymbolKey, EntityId>,
    key: &StableSymbolKey,
    role: &str,
) -> Result<EntityId, StorageError> {
    ids.get(key).copied().ok_or_else(|| {
        StorageError::Validation(format!(
            "{role} references missing stable symbol {}::{}",
            key.file_path, key.scoped_name
        ))
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::index::entity_index::EntityIndexOps;
    use crate::index::file_index::FileLevelOps;
    use crate::index::relation_query::RelationQueryOps;
    use cce_types::{
        CanonicalEntity, CanonicalFile, CanonicalRelation, EntityId, EntityKind, RelationType,
        SnapshotDelta, Span,
    };

    /// In-memory `RelationSnapshotStore` test double (no SQLite). Holds
    /// manifests / snapshots keyed by epoch, plus delta chains.
    struct StubSnapshotStore {
        project_id: i64,
        manifests: HashMap<i64, RelationSnapshotManifest>,
        snapshots: HashMap<i64, CanonicalRelationSnapshot>,
        bases: HashMap<i64, i64>,
        deltas: HashMap<i64, Vec<SnapshotDelta>>,
    }

    impl StubSnapshotStore {
        fn active_epoch(
            manifest: RelationSnapshotManifest,
            snapshot: CanonicalRelationSnapshot,
        ) -> Self {
            let epoch = manifest.relation_epoch;
            Self {
                project_id: manifest.project_id,
                manifests: HashMap::from([(epoch, manifest)]),
                snapshots: HashMap::from([(epoch, snapshot)]),
                bases: HashMap::new(),
                deltas: HashMap::new(),
            }
        }

        fn delta_epoch(
            delta_manifest: RelationSnapshotManifest,
            base_manifest: RelationSnapshotManifest,
            base_snapshot: CanonicalRelationSnapshot,
            deltas: Vec<SnapshotDelta>,
        ) -> Self {
            let delta_epoch = delta_manifest.relation_epoch;
            let base_epoch = base_manifest.relation_epoch;
            Self {
                project_id: delta_manifest.project_id,
                manifests: HashMap::from([
                    (delta_epoch, delta_manifest),
                    (base_epoch, base_manifest),
                ]),
                snapshots: HashMap::from([(base_epoch, base_snapshot)]),
                bases: HashMap::from([(delta_epoch, base_epoch)]),
                deltas: HashMap::from([(delta_epoch, deltas)]),
            }
        }
    }

    impl RelationSnapshotStore for StubSnapshotStore {
        fn get_manifest(
            &self,
            project_id: i64,
            epoch: i64,
        ) -> Result<Option<RelationSnapshotManifest>, StorageError> {
            Ok((project_id == self.project_id)
                .then(|| self.manifests.get(&epoch).cloned())
                .flatten())
        }

        fn read_snapshot(
            &self,
            manifest: &RelationSnapshotManifest,
        ) -> Result<CanonicalRelationSnapshot, StorageError> {
            self.snapshots
                .get(&manifest.relation_epoch)
                .cloned()
                .ok_or_else(|| {
                    StorageError::Query(format!(
                        "stub store has no snapshot for epoch {}",
                        manifest.relation_epoch
                    ))
                })
        }

        fn find_base_epoch(
            &self,
            project_id: i64,
            delta_epoch: i64,
        ) -> Result<Option<i64>, StorageError> {
            if project_id == self.project_id {
                Ok(self.bases.get(&delta_epoch).copied())
            } else {
                Ok(None)
            }
        }

        fn get_delta_chain(
            &self,
            project_id: i64,
            after_epoch: i64,
            up_to_epoch: i64,
        ) -> Result<Vec<SnapshotDelta>, StorageError> {
            if project_id == self.project_id {
                Ok(self
                    .deltas
                    .get(&up_to_epoch)
                    .map(|chain| {
                        chain
                            .iter()
                            .filter(|delta| delta.epoch > after_epoch)
                            .cloned()
                            .collect()
                    })
                    .unwrap_or_default())
            } else {
                Ok(Vec::new())
            }
        }
    }

    fn valid_manifest(
        snapshot: &CanonicalRelationSnapshot,
        epoch: i64,
        state: RelationSnapshotState,
    ) -> RelationSnapshotManifest {
        RelationSnapshotManifest {
            project_id: 1,
            relation_epoch: epoch,
            operation_id: "operation".to_string(),
            state,
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
            symbol_key_conflict_count: snapshot.build_metadata.symbol_key_conflict_count,
            symbol_key_conflict_samples: snapshot
                .build_metadata
                .symbol_key_conflict_samples
                .clone(),
        }
    }

    #[test]
    fn empty_canonical_snapshot_loads() {
        let snapshot = CanonicalRelationSnapshot::new("config".to_string());
        let index = RelationSnapshotLoader::load_canonical(&snapshot)
            .expect("empty canonical snapshot should be valid");
        assert_eq!(index.function_count(), 0);
        assert_eq!(index.resolved_relation_count(), 0);
    }

    #[test]
    fn cold_load_round_trip_preserves_overloads_and_fingerprint() {
        let snapshot = overloaded_snapshot();

        let index = RelationSnapshotLoader::load_canonical(&snapshot)
            .expect("canonical snapshot should load");
        let reexported = index
            .to_canonical_snapshot("config".to_string())
            .expect("loaded index should export canonically");

        assert_eq!(reexported.entities.len(), 2);
        assert_eq!(snapshot.fingerprint(), reexported.fingerprint());
    }

    #[test]
    fn active_epoch_loads_without_source_files() {
        let snapshot = overloaded_snapshot();
        let store = StubSnapshotStore::active_epoch(
            valid_manifest(&snapshot, 7, RelationSnapshotState::Active),
            snapshot.clone(),
        );

        let index = RelationSnapshotLoader::load(&store, 1, 7)
            .expect("active epoch should load through the store port");
        let reexported = index
            .to_canonical_snapshot("config".to_string())
            .expect("loaded epoch should export canonically");
        assert_eq!(index.function_count(), 2);
        assert_eq!(snapshot.fingerprint(), reexported.fingerprint());
    }

    #[test]
    fn loader_rejects_manifest_fingerprint_mismatch() {
        let snapshot = overloaded_snapshot();
        let mut manifest = valid_manifest(&snapshot, 7, RelationSnapshotState::Active);
        manifest.snapshot_fingerprint = Some("tampered".to_string());
        let store = StubSnapshotStore::active_epoch(manifest, snapshot);

        assert!(RelationSnapshotLoader::load(&store, 1, 7).is_err());
    }

    #[test]
    fn delta_chain_replays_deltas_over_base_epoch() {
        let base = overloaded_snapshot();
        let store = StubSnapshotStore::delta_epoch(
            valid_manifest(&base, 42, RelationSnapshotState::Delta),
            valid_manifest(&base, 7, RelationSnapshotState::Active),
            base,
            vec![SnapshotDelta {
                epoch: 42,
                base_epoch: 7,
                config_fingerprint: "config".to_string(),
                removed_files: Vec::new(),
                added_files: Vec::new(),
                removed_entities: vec![EntityId(2)],
                added_entities: Vec::new(),
                removed_relations: Vec::new(),
                added_relations: Vec::new(),
                import_diffs: Vec::new(),
                export_diffs: Vec::new(),
                file_relation_diffs: Vec::new(),
                relation_edges_dropped_unbounded: 0,
                dependency_diffs: Vec::new(),
                renamed_entities: Vec::new(),
            }],
        );

        let index = RelationSnapshotLoader::load(&store, 1, 42)
            .expect("delta chain should replay over the base epoch");
        assert_eq!(index.function_count(), 1);
        assert!(index.file_records().read().contains_key("src/lib.rs"));
    }

    #[test]
    fn delta_chain_without_base_epoch_is_rejected() {
        let base = overloaded_snapshot();
        let mut store = StubSnapshotStore::delta_epoch(
            valid_manifest(&base, 42, RelationSnapshotState::Delta),
            valid_manifest(&base, 7, RelationSnapshotState::Active),
            base,
            vec![],
        );
        // Drop the base manifest to simulate a broken chain: the load must be
        // rejected with a validation error instead of panicking.
        store.manifests.remove(&7);

        assert!(RelationSnapshotLoader::load(&store, 1, 42).is_err());
    }

    /// Replaying a delta chain must be equivalent to a full publish.
    ///
    /// The delta adds an entity in a new file; after replay the entity must
    /// carry its file membership, stable symbol mapping (including stable-ID
    /// lookup), and file-level queryability.
    #[test]
    fn delta_replay_is_equivalent_to_full_publish() {
        let base = overloaded_snapshot();
        let added_key = StableSymbolKey::new(
            "src/extra.rs",
            "extra_fn",
            EntityKind::Function,
            "fn extra_fn()",
        );
        let delta = SnapshotDelta {
            epoch: 42,
            base_epoch: 7,
            config_fingerprint: "config".to_string(),
            removed_files: Vec::new(),
            added_files: vec![cce_types::FileInfo {
                id: "src/extra.rs".to_string(),
                path: "src/extra.rs".to_string(),
                language: "rust".to_string(),
                file_hash: "extra".to_string(),
                file_size: 5,
                modified_time: 0,
                parse_status: ParseStatus::Success,
                parse_errors: Vec::new(),
                parse_version: 1,
                entity_count: 1,
                relation_count: 1,
                export_count: 0,
                import_count: 0,
                depends_on: Vec::new(),
            }],
            removed_entities: Vec::new(),
            added_entities: vec![cce_types::AddedEntity {
                entity: Entity {
                    id: EntityId(10),
                    kind: EntityKind::Function,
                    name: "extra_fn".to_string(),
                    signature: "fn extra_fn()".to_string(),
                    parameters: Vec::new(),
                    return_type: None,
                    span: Span::default(),
                    depth: 0,
                    parent: None,
                    children: Vec::new(),
                    doc_comment: None,
                    modifiers: Vec::new(),
                    attributes: Default::default(),
                    metadata: Default::default(),
                    is_stdlib: false,
                    stdlib_category: None,
                    subtype: None,
                },
                symbol_key: added_key.clone(),
                file_path: "src/extra.rs".to_string(),
            }],
            removed_relations: Vec::new(),
            added_relations: vec![ResolvedRelation {
                caller: EntityId(1),
                callee_id: Some(EntityId(10)),
                callee_name: "extra_fn".to_string(),
                relation_type: cce_types::RelationType::DirectCall,
                span: Span::default(),
                is_external: false,
                external_type: None,
                callee_symbol: None,
                stdlib_category: None,
                owner_type: None,
                call_context: CallContext::Direct,
            }],
            import_diffs: Vec::new(),
            export_diffs: Vec::new(),
            file_relation_diffs: Vec::new(),
            relation_edges_dropped_unbounded: 0,
            dependency_diffs: Vec::new(),
            renamed_entities: Vec::new(),
        };
        let store = StubSnapshotStore::delta_epoch(
            valid_manifest(&base, 42, RelationSnapshotState::Delta),
            valid_manifest(&base, 7, RelationSnapshotState::Active),
            base,
            vec![delta],
        );

        let index = RelationSnapshotLoader::load(&store, 1, 42)
            .expect("delta chain should replay over the base epoch");

        // Entity exists with its file membership.
        assert!(index.contains_function(EntityId(10)));
        assert_eq!(
            index.get_file_path_by_entity(EntityId(10)),
            Some("src/extra.rs".to_string())
        );
        // File-level queryability.
        let entities = index.get_entities_by_file("src/extra.rs");
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].0, EntityId(10));
        // Stable symbol mapping incl. stable-ID lookup.
        assert_eq!(
            index.get_entity_id_by_symbol_key(&added_key),
            Some(EntityId(10))
        );
        assert_eq!(
            index.get_entity_id_by_stable_symbol_id(&added_key.stable_id().0),
            Some(EntityId(10))
        );
        // The added edge is present in both directions.
        assert_eq!(
            index.get_callers_by_callee_entity(EntityId(10)),
            vec![EntityId(1)]
        );
        // Re-exporting the replayed index must round-trip a valid canonical
        // snapshot (no dangling references, complete identity).
        let reexported = index
            .to_canonical_snapshot("config".to_string())
            .expect("replayed index should export canonically");
        assert_eq!(reexported.entities.len(), 3);
        assert_eq!(reexported.relations.len(), 2);
        assert!(index.validate_snapshot().is_ok());
    }

    fn overloaded_snapshot() -> CanonicalRelationSnapshot {
        let first_key = StableSymbolKey::new(
            "src/lib.rs",
            "service::run",
            EntityKind::Function,
            "fn run(value: u32)",
        );
        let second_key = StableSymbolKey::new(
            "src/lib.rs",
            "service::run",
            EntityKind::Function,
            "fn run(value: String)",
        );
        let entity = |key: StableSymbolKey, signature: &str| CanonicalEntity {
            key,
            entity_id: None,
            name: "run".to_string(),
            signature: signature.to_string(),
            parameters: Vec::new(),
            return_type: None,
            span: Span::default(),
            depth: 0,
            parent: None,
            doc_comment: None,
            modifiers: Vec::new(),
            attributes: BTreeMap::new(),
            metadata: BTreeMap::new(),
            is_stdlib: false,
            stdlib_category: None,
            subtype: None,
        };
        let mut snapshot = CanonicalRelationSnapshot::new("config".to_string());
        snapshot.files.push(CanonicalFile {
            path: "src/lib.rs".to_string(),
            language: "rust".to_string(),
            input_hash: "source-hash".to_string(),
            file_size: 42,
            imports: Vec::new(),
            exports: Vec::new(),
        });
        snapshot
            .entities
            .push(entity(first_key.clone(), "fn run(value: u32)"));
        snapshot
            .entities
            .push(entity(second_key.clone(), "fn run(value: String)"));
        snapshot.relations.push(CanonicalRelation {
            caller: first_key,
            target: CanonicalRelationTarget::Internal { key: second_key },
            raw_target: "run".to_string(),
            relation_type: RelationType::DirectCall,
            span: Span::default(),
            stdlib_category: None,
        });
        snapshot.normalize();
        snapshot
    }
}
