use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;

use crate::index::core::{RelationIndex, SymbolKey};
use crate::index::delta::RelationDeltaOps;
use cce_types::{
    AddedEntity, CanonicalDependency, CanonicalEntity, CanonicalExport, CanonicalFile,
    CanonicalRelation, CanonicalRelationSnapshot, CanonicalRelationTarget, EntityId, EntityKind,
    FileInfo, ResolvedRelation, SnapshotBuildMetadata, normalize_project_path,
};

use super::{LayeredSnapshotIndex, RelationSnapshotIndex};

impl RelationSnapshotIndex {
    /// Stable symbol key of an entity, degrading to a derived key rebuilt
    /// from the entity's own fields when the reverse map has no entry.
    ///
    /// The derived key `(file, name, kind, signature)` matches the fallback
    /// used by `compute_delta` and `fingerprint_in_files_from_maps`, so a
    /// degraded snapshot stays byte-identical to the index fingerprint.
    fn entity_key_or_derived(&self, entity_id: EntityId) -> SymbolKey {
        if let Some(key) = self.entity_to_symbol_key.read().get(&entity_id) {
            return key.clone();
        }
        self.diagnostics
            .entity_derived_key_count
            .fetch_add(1, Ordering::Relaxed);
        let file = self
            .entity_file_index
            .get(&entity_id)
            .map(|v| v.clone())
            .unwrap_or_default();
        match self.function_index.get(&entity_id) {
            Some(entity) => SymbolKey::new(
                &file,
                &entity.value().name,
                entity.value().kind,
                &entity.value().signature,
            ),
            None => SymbolKey::new(&file, "<unknown>", EntityKind::Unknown, "<unknown>"),
        }
    }

    /// Stable symbol key of a relation caller/target, degrading to a derived
    /// key when the reverse map has no entry. Counts toward
    /// `relation_derived_key_count`.
    fn relation_key_or_derived(&self, entity_id: EntityId, name_fallback: &str) -> SymbolKey {
        if let Some(key) = self.entity_to_symbol_key.read().get(&entity_id) {
            return key.clone();
        }
        self.diagnostics
            .relation_derived_key_count
            .fetch_add(1, Ordering::Relaxed);
        let file = self
            .entity_file_index
            .get(&entity_id)
            .map(|v| v.clone())
            .unwrap_or_default();
        match self.function_index.get(&entity_id) {
            Some(entity) => SymbolKey::new(
                &file,
                &entity.value().name,
                entity.value().kind,
                &entity.value().signature,
            ),
            None => SymbolKey::new(&file, name_fallback, EntityKind::Unknown, name_fallback),
        }
    }

    /// Export the fully resolved graph into the only persistent relationship
    /// model (read-only; reads the shared maps directly).
    pub fn to_canonical_snapshot(
        &self,
        config_fingerprint: String,
    ) -> Result<CanonicalRelationSnapshot, String> {
        let mut snapshot = CanonicalRelationSnapshot::new(config_fingerprint);

        let fr_guard = self.file_records.read();
        for record in fr_guard.values() {
            let norm_path = normalize_project_path(&record.info.path);

            let imports = record.imports.clone();

            let mut exports = Vec::new();
            for export in record.exports.iter() {
                let symbol = self.entity_key_or_derived(export.function_id);
                exports.push(CanonicalExport {
                    symbol,
                    export_type: format!("{:?}", export.export_type).to_lowercase(),
                });
            }

            snapshot.files.push(CanonicalFile {
                path: norm_path,
                language: record.info.language.clone(),
                input_hash: record.info.file_hash.clone(),
                file_size: record.info.file_size,
                imports: imports.standardized_imports,
                exports,
            });
        }

        // Collect all entities before serializing relations so every internal
        // target can be validated within the same snapshot.
        for entity_entry in self.function_index.iter() {
            let entity_id = *entity_entry.key();
            let entity = entity_entry.value();
            let key = self.entity_key_or_derived(entity_id);
            let parent = entity
                .parent
                .map(|parent_id| self.entity_key_or_derived(parent_id));
            snapshot.entities.push(CanonicalEntity {
                key,
                entity_id: Some(entity_id.0),
                name: entity.name.clone(),
                signature: entity.signature.clone(),
                parameters: entity.parameters.clone(),
                return_type: entity.return_type.clone(),
                span: entity.span,
                depth: entity.depth,
                parent,
                doc_comment: entity.doc_comment.clone(),
                modifiers: entity.modifiers.clone(),
                attributes: entity.attributes.clone().into_iter().collect(),
                metadata: entity.metadata.clone().into_iter().collect(),
                is_stdlib: entity.is_stdlib,
                stdlib_category: entity.stdlib_category,
                subtype: entity.subtype.clone(),
            });
        }

        for relation_entry in self.resolved_relation_index.iter() {
            for relation in relation_entry.value().iter() {
                let caller = self.relation_key_or_derived(relation.caller, "");
                let target = if let Some(callee_id) = relation.callee_id {
                    let key = self.relation_key_or_derived(callee_id, &relation.callee_name);
                    CanonicalRelationTarget::Internal { key }
                } else if relation.is_external && relation.external_type.is_some() {
                    CanonicalRelationTarget::External {
                        classification: relation.external_type.clone(),
                    }
                } else {
                    CanonicalRelationTarget::Unresolved {
                        reason: cce_types::UnresolvedReason::SymbolNotFound,
                    }
                };
                snapshot.relations.push(CanonicalRelation {
                    caller,
                    target,
                    raw_target: relation.callee_name.clone(),
                    relation_type: relation.relation_type,
                    span: relation.span,
                    stdlib_category: relation.stdlib_category,
                    overload_signature: relation.overload_signature.clone(),
                });
            }
        }

        // File-scoped edges are canonicalized under the per-file placeholder
        // caller key `(path, "<file>", Module)`.
        for relation_entry in self.file_relation_index.iter() {
            let caller_key =
                SymbolKey::new(relation_entry.key(), "<file>", EntityKind::Module, "<file>");
            for relation in relation_entry.value().iter() {
                let target = if let Some(callee_id) = relation.callee_id {
                    let key = self.relation_key_or_derived(callee_id, &relation.callee_name);
                    CanonicalRelationTarget::Internal { key }
                } else if relation.is_external && relation.external_type.is_some() {
                    CanonicalRelationTarget::External {
                        classification: relation.external_type.clone(),
                    }
                } else {
                    CanonicalRelationTarget::Unresolved {
                        reason: cce_types::UnresolvedReason::SymbolNotFound,
                    }
                };
                snapshot.relations.push(CanonicalRelation {
                    caller: caller_key.clone(),
                    target,
                    raw_target: relation.callee_name.clone(),
                    relation_type: relation.relation_type,
                    span: relation.span,
                    stdlib_category: relation.stdlib_category,
                    overload_signature: relation.overload_signature.clone(),
                });
            }
        }

        for file in &snapshot.files {
            for target in self.dependency_graph.get_dependencies(&file.path) {
                snapshot.dependencies.push(CanonicalDependency {
                    source_file: file.path.clone(),
                    target_file: normalize_project_path(&target),
                    source: "resolved_or_import".to_string(),
                });
            }
        }

        snapshot.normalize();
        self.fill_build_metadata(&mut snapshot);
        Ok(snapshot)
    }

    /// Fill `snapshot.build_metadata` from this snapshot view's shared
    /// conflict diagnostics.
    fn fill_build_metadata(&self, snapshot: &mut CanonicalRelationSnapshot) {
        snapshot.build_metadata = self.build_metadata_snapshot();
    }

    /// Copy the shared conflict diagnostics into a standalone metadata struct.
    pub(crate) fn build_metadata_snapshot(&self) -> SnapshotBuildMetadata {
        let samples = if let Ok(guard) = self.diagnostics.symbol_key_conflict_samples.lock() {
            guard.iter().cloned().collect()
        } else {
            Vec::new()
        };
        SnapshotBuildMetadata {
            symbol_key_conflict_count: self
                .diagnostics
                .symbol_key_conflict_count
                .load(Ordering::Relaxed),
            symbol_key_conflict_samples: samples,
            entity_derived_key_count: self
                .diagnostics
                .entity_derived_key_count
                .load(Ordering::Relaxed),
            relation_derived_key_count: self
                .diagnostics
                .relation_derived_key_count
                .load(Ordering::Relaxed),
        }
    }
}

impl LayeredSnapshotIndex {
    /// Materialize the merged state (base maps + applied deltas) into a
    /// concrete index. Used only by verification paths; never on the
    /// per-request query path.
    ///
    /// Copies every map (including `symbol_key_to_entity` /
    /// `stable_id_to_entity`, which `apply_delta`'s export-diff resolution
    /// reads) into a fresh `RelationIndex` and replays the delta chain.
    pub(crate) fn materialize_merged_index(&self) -> RelationIndex {
        let base = &self.base;
        let merged = RelationIndex::new();
        for entry in base.function_index.iter() {
            merged.insert_function(*entry.key(), entry.value().clone());
        }
        for entry in base.entity_file_index.iter() {
            merged
                .entity_file_index
                .insert(*entry.key(), entry.value().clone());
        }
        for entry in base.resolved_relation_index.iter() {
            merged
                .resolved_relation_index
                .insert(*entry.key(), entry.value().clone());
        }
        for entry in base.file_relation_index.iter() {
            merged
                .file_relation_index
                .insert(entry.key().clone(), entry.value().clone());
        }
        for (k, v) in base.file_records.read().iter() {
            merged.file_records.write().insert(k.clone(), v.clone());
            for target in base.dependency_graph.get_dependencies(k) {
                merged.dependency_graph.add_dependency(k, &target);
            }
        }
        for (k, v) in base.symbol_key_to_entity.read().iter() {
            merged.symbol_key_to_entity.write().insert(k.clone(), *v);
        }
        for (k, v) in base.entity_to_symbol_key.read().iter() {
            merged.entity_to_symbol_key.write().insert(*k, v.clone());
        }
        for (k, v) in base.stable_id_to_entity.read().iter() {
            merged.stable_id_to_entity.write().insert(k.clone(), *v);
        }
        for delta in &self.deltas {
            merged.apply_delta(delta);
        }
        merged
    }

    /// Export the fully merged graph (base + deltas) into the persistent
    /// relationship model. Read-only; no index materialization.
    pub fn to_canonical_snapshot(
        &self,
        config_fingerprint: String,
    ) -> Result<CanonicalRelationSnapshot, String> {
        use crate::index::snapshot_query::{
            SnapshotFileQueryOps, SnapshotRelationQueryOps, SnapshotSymbolQueryOps,
        };

        let mut snapshot = CanonicalRelationSnapshot::new(config_fingerprint);

        // A file's final visibility is decided by the LAST delta that operates
        // on it: any removal masks the base copy, and a delta-added file is
        // visible only when its most recent operation was an add (a later
        // removal hides it; a later re-add restores the re-added version).
        let mut removed_files_ever: HashSet<String> = HashSet::new();
        let mut final_added_files: HashMap<String, &FileInfo> = HashMap::new();
        for d in &self.deltas {
            for removed in &d.removed_files {
                removed_files_ever.insert(removed.clone());
                final_added_files.remove(removed);
            }
            for added in &d.added_files {
                final_added_files.insert(added.path.clone(), added);
            }
        }

        for (file_id, record) in self.base.file_records.read().iter() {
            let path = normalize_project_path(&record.info.path);
            if removed_files_ever.contains(file_id.as_str()) {
                continue;
            }
            let imports = record.imports.standardized_imports.clone();
            let mut exports = Vec::new();
            for export in record.exports.iter() {
                let symbol = self
                    .get_symbol_key_by_entity_id(export.function_id)
                    .ok_or_else(|| {
                        format!(
                            "export {} in {} references an unknown symbol",
                            export.function_name, path
                        )
                    })?;
                exports.push(CanonicalExport {
                    symbol,
                    export_type: format!("{:?}", export.export_type).to_lowercase(),
                });
            }
            snapshot.files.push(CanonicalFile {
                path,
                language: record.info.language.clone(),
                input_hash: record.info.file_hash.clone(),
                file_size: record.info.file_size,
                imports,
                exports,
            });
        }
        // Add files from all deltas, keeping only the final version of each
        // path (the last add not superseded by a later removal).
        for file in final_added_files.values() {
            let path = normalize_project_path(&file.path);
            let imports = self
                .get_import_table(&file.path)
                .map(|table| table.standardized_imports)
                .unwrap_or_default();
            let mut exports = Vec::new();
            if let Some(file_exports) = self.get_exports(&file.path) {
                for export in file_exports.iter() {
                    let symbol = self
                        .get_symbol_key_by_entity_id(export.function_id)
                        .ok_or_else(|| {
                            format!(
                                "export {} in {} references an unknown symbol",
                                export.function_name, path
                            )
                        })?;
                    exports.push(CanonicalExport {
                        symbol,
                        export_type: format!("{:?}", export.export_type).to_lowercase(),
                    });
                }
            }
            snapshot.files.push(CanonicalFile {
                path,
                language: file.language.clone(),
                input_hash: file.file_hash.clone(),
                file_size: file.file_size,
                imports,
                exports,
            });
        }

        // Entity final visibility follows the same last-operation-wins rule:
        // any removal masks the base copy; a delta-added entity is visible
        // only when its most recent operation was an add.
        let removed_entities: HashSet<EntityId> = self
            .deltas
            .iter()
            .flat_map(|d| d.removed_entities.iter().copied())
            .collect();
        let mut final_added_entities: HashMap<EntityId, &AddedEntity> = HashMap::new();
        for d in &self.deltas {
            for removed in &d.removed_entities {
                final_added_entities.remove(removed);
            }
            for added in &d.added_entities {
                final_added_entities.insert(added.entity.id, added);
            }
        }
        for entity_entry in self.base.function_index.iter() {
            let entity_id = *entity_entry.key();
            if removed_entities.contains(&entity_id) {
                continue;
            }
            let entity = entity_entry.value();
            let key = self
                .get_symbol_key_by_entity_id(entity_id)
                .unwrap_or_else(|| self.base.entity_key_or_derived(entity_id));
            let parent = entity.parent.map(|parent_id| {
                self.get_symbol_key_by_entity_id(parent_id)
                    .unwrap_or_else(|| self.base.entity_key_or_derived(parent_id))
            });
            snapshot.entities.push(CanonicalEntity {
                key,
                entity_id: Some(entity_id.0),
                name: entity.name.clone(),
                signature: entity.signature.clone(),
                parameters: entity.parameters.clone(),
                return_type: entity.return_type.clone(),
                span: entity.span,
                depth: entity.depth,
                parent,
                doc_comment: entity.doc_comment.clone(),
                modifiers: entity.modifiers.clone(),
                attributes: entity.attributes.clone().into_iter().collect(),
                metadata: entity.metadata.clone().into_iter().collect(),
                is_stdlib: entity.is_stdlib,
                stdlib_category: entity.stdlib_category,
                subtype: entity.subtype.clone(),
            });
        }
        // Add entities from all deltas, keeping only the final version of each
        // entity ID (the last add not superseded by a later removal).
        for added in final_added_entities.values() {
            let parent = added.entity.parent.map(|parent_id| {
                self.get_symbol_key_by_entity_id(parent_id)
                    .unwrap_or_else(|| self.base.entity_key_or_derived(parent_id))
            });
            snapshot.entities.push(CanonicalEntity {
                key: added.symbol_key.clone(),
                entity_id: Some(added.entity.id.0),
                name: added.entity.name.clone(),
                signature: added.entity.signature.clone(),
                parameters: added.entity.parameters.clone(),
                return_type: added.entity.return_type.clone(),
                span: added.entity.span,
                depth: added.entity.depth,
                parent,
                doc_comment: added.entity.doc_comment.clone(),
                modifiers: added.entity.modifiers.clone(),
                attributes: added.entity.attributes.clone().into_iter().collect(),
                metadata: added.entity.metadata.clone().into_iter().collect(),
                is_stdlib: added.entity.is_stdlib,
                stdlib_category: added.entity.stdlib_category,
                subtype: added.entity.subtype.clone(),
            });
        }

        let mut callers: Vec<EntityId> = self
            .base
            .resolved_relation_index
            .iter()
            .map(|e| *e.key())
            .collect();
        for d in &self.deltas {
            callers.retain(|id| !removed_entities.contains(id));
            callers.extend(d.added_entities.iter().map(|added| added.entity.id));
        }
        callers.sort();
        callers.dedup();
        for caller in callers {
            let Some(relations) = self.get_resolved_relations_by_caller(caller) else {
                continue;
            };
            for relation in relations {
                let caller_key = self
                    .get_symbol_key_by_entity_id(caller)
                    .unwrap_or_else(|| self.base.relation_key_or_derived(caller, ""));
                let target = if let Some(callee_id) = relation.callee_id {
                    let key = self
                        .get_symbol_key_by_entity_id(callee_id)
                        .unwrap_or_else(|| {
                            self.base
                                .relation_key_or_derived(callee_id, &relation.callee_name)
                        });
                    CanonicalRelationTarget::Internal { key }
                } else if relation.is_external && relation.external_type.is_some() {
                    CanonicalRelationTarget::External {
                        classification: relation.external_type.clone(),
                    }
                } else {
                    CanonicalRelationTarget::Unresolved {
                        reason: cce_types::UnresolvedReason::SymbolNotFound,
                    }
                };
                snapshot.relations.push(CanonicalRelation {
                    caller: caller_key,
                    target,
                    raw_target: relation.callee_name.clone(),
                    relation_type: relation.relation_type,
                    span: relation.span,
                    stdlib_category: relation.stdlib_category,
                    overload_signature: relation.overload_signature.clone(),
                });
            }
        }

        // File-scoped edges merge the base map with the delta file-relation
        // diffs (removed edges dropped, added edges appended).
        let mut file_relation_diffs_by_path: std::collections::HashMap<
            &str,
            (&Vec<ResolvedRelation>, &Vec<ResolvedRelation>),
        > = std::collections::HashMap::new();
        for d in &self.deltas {
            for diff in &d.file_relation_diffs {
                file_relation_diffs_by_path.insert(
                    &diff.file_path,
                    (&diff.removed_relations, &diff.added_relations),
                );
            }
        }
        for file in &snapshot.files {
            let caller_key = SymbolKey::new(&file.path, "<file>", EntityKind::Module, "<file>");
            let mut edges: Vec<ResolvedRelation> = self
                .base
                .file_relation_index
                .get(&file.path)
                .map(|entry| entry.edges.clone())
                .unwrap_or_default();
            if let Some((removed, added)) = file_relation_diffs_by_path.get(file.path.as_str()) {
                for relation in removed.iter() {
                    edges.retain(|candidate| {
                        crate::index::delta::relation_identity(candidate)
                            != crate::index::delta::relation_identity(relation)
                    });
                }
                edges.extend(added.iter().cloned());
            }
            for relation in edges {
                let target = if let Some(callee_id) = relation.callee_id {
                    let key = self
                        .get_symbol_key_by_entity_id(callee_id)
                        .unwrap_or_else(|| {
                            self.base
                                .relation_key_or_derived(callee_id, &relation.callee_name)
                        });
                    CanonicalRelationTarget::Internal { key }
                } else if relation.is_external && relation.external_type.is_some() {
                    CanonicalRelationTarget::External {
                        classification: relation.external_type.clone(),
                    }
                } else {
                    CanonicalRelationTarget::Unresolved {
                        reason: cce_types::UnresolvedReason::SymbolNotFound,
                    }
                };
                snapshot.relations.push(CanonicalRelation {
                    caller: caller_key.clone(),
                    target,
                    raw_target: relation.callee_name.clone(),
                    relation_type: relation.relation_type,
                    span: relation.span,
                    stdlib_category: relation.stdlib_category,
                    overload_signature: relation.overload_signature.clone(),
                });
            }
        }

        for file in &snapshot.files {
            let mut targets: Vec<String> = self.base.dependency_graph.get_dependencies(&file.path);
            for d in &self.deltas {
                targets.retain(|t| !d.removed_files.contains(t));
                if let Some(diff) = d
                    .dependency_diffs
                    .iter()
                    .find(|diff| diff.source_file == file.path)
                {
                    targets.retain(|t| !diff.removed_dependencies.contains(t));
                    for added in &diff.added_dependencies {
                        if !targets.contains(added) {
                            targets.push(added.clone());
                        }
                    }
                }
            }
            for target in targets {
                snapshot.dependencies.push(CanonicalDependency {
                    source_file: file.path.clone(),
                    target_file: normalize_project_path(&target),
                    source: "resolved_or_import".to_string(),
                });
            }
        }

        snapshot.normalize();
        // Merged-view diagnostics: propagate the base snapshot's build
        // metadata (deltas do not carry conflict records).
        snapshot.build_metadata = self.base.build_metadata_snapshot();
        Ok(snapshot)
    }
}
