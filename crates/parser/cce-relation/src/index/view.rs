//! Read-only view trait over relation index state.
//!
//! `RelationIndexView` abstracts the read surface of a relation index so
//! that hot-update consumers (delta computation, symbol prepopulation,
//! fingerprinting, dependent collection) can operate on either a concrete
//! materialized `RelationIndex` or a layered read-only view
//! ([`LayeredSnapshotIndex`](crate::index::LayeredSnapshotIndex),
//! base + delta chain) without copying the whole project.
//!
//! Every method is read-only. Implementations must not allocate project-scale
//! copies beyond what each individual query requires; full-project reads such
//! as `entities_by_file` / `stable_symbol_keys` are O(project) but zero-copy
//! snapshots of shared state (they are the cross-file symbol context consumed
//! by prepopulation, which was always O(project) read-only).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use parking_lot::RwLock;

use crate::index::core::{RelationEdgeSet, RelationIndex, SymbolKey};
use crate::types::ExportInfo;
use cce_types::{
    CanonicalDependency, CanonicalEntity, CanonicalExport, CanonicalFile, CanonicalRelation,
    CanonicalRelationTarget, Entity, EntityId, EntityKind, FileInfo, FingerprintComponents,
    ImportTable, ResolvedRelation, UnresolvedReason, fingerprint_from_components,
    normalize_project_path,
};
use dashmap::DashMap;

/// Read-only view over relation index state.
///
/// Implemented by [`RelationIndex`] (direct delegation to its maps), by
/// [`RelationSnapshotIndex`](crate::index::RelationSnapshotIndex)
/// (delegation to the zero-copy shared maps), and by
/// [`LayeredSnapshotIndex`](crate::index::LayeredSnapshotIndex)
/// (lazy base + delta merge with the same semantics as applying the deltas
/// onto the base).
///
/// Note: the doc-level signature used `impl FnMut` in argument position, which
/// is not allowed in trait method declarations on stable Rust; the for-each
/// methods are declared as generic methods (`F: FnMut`) instead, with identical
/// call semantics.
pub trait RelationIndexView: Send + Sync {
    // File layer
    fn file_contains(&self, path: &str) -> bool;
    fn for_each_file<F: FnMut(&str, &FileInfo)>(&self, f: F);
    // Entity layer
    fn function_contains(&self, id: EntityId) -> bool;
    fn for_each_function<F: FnMut(EntityId, &Entity)>(&self, f: F);
    fn entity_file_of(&self, id: EntityId) -> Option<String>;
    // Relation layer
    fn relations_of(&self, caller: EntityId) -> Option<Vec<ResolvedRelation>>;
    fn for_each_resolved_relation<F: FnMut(EntityId, &[ResolvedRelation])>(&self, f: F);
    fn callers_of(&self, callee: EntityId) -> Vec<EntityId>;
    /// File-level relations of a normalized file path (imports, uses,
    /// module-level calls).
    fn file_relations_of(&self, path: &str) -> Vec<ResolvedRelation>;
    fn for_each_file_relation<F: FnMut(&str, &[ResolvedRelation])>(&self, f: F);
    // Import / export layer
    fn imports_of(&self, path: &str) -> Option<ImportTable>;
    fn for_each_import<F: FnMut(&str, &ImportTable)>(&self, f: F);
    fn exports_of(&self, path: &str) -> Option<Vec<ExportInfo>>;
    fn for_each_export<F: FnMut(&str, &[ExportInfo])>(&self, f: F);
    fn symbol_key_of(&self, id: EntityId) -> Option<SymbolKey>;
    // Dependency graph layer
    fn dependency_files(&self) -> Vec<String>;
    fn dependencies_of(&self, source: &str) -> Vec<String>;
    fn dependents_of(&self, file: &str) -> Vec<String>;
    fn collect_transitive_dependents(&self, file: &str, max_depth: usize) -> Vec<String>;
    fn collect_transitive_dependencies(&self, file: &str, max_depth: usize) -> Vec<String>;
    // Symbol context (prepopulate / fingerprint / candidate ID allocation)
    fn entities_by_file(&self) -> HashMap<String, Vec<Entity>>;
    /// Live entities of a single file, via the per-file membership index.
    ///
    /// O(entities of the file) instead of the O(project) whole-project
    /// grouping of `entities_by_file`; results match
    /// `entities_by_file().remove(path)`.
    fn entities_of_file(&self, path: &str) -> Vec<Entity>;
    /// File paths holding a file-level relation edge targeting `callee`.
    ///
    /// O(1) lookup through the maintained reverse index instead of scanning
    /// every file-level edge.
    fn file_callers_of(&self, callee: EntityId) -> Vec<String>;
    fn stable_symbol_keys(&self) -> Vec<SymbolKey>;
    /// Stable symbol keys for the given file set only.
    ///
    /// Aggregates per file through the file-membership index + reverse symbol
    /// map instead of materializing every symbol key in the project and
    /// filtering  Cost is bounded by the requested files' entity count
    /// instead of the whole symbol table.
    fn stable_symbol_keys_in_files(&self, files: &HashSet<String>) -> Vec<SymbolKey>;
    /// Deterministic, file-scoped fingerprint.
    ///
    /// Canonical components (files, entities, relations, dependencies) are
    /// built from the shared maps and hashed with
    /// `fingerprint_from_components`, so the result is byte-identical to
    /// hashing the corresponding canonical snapshot — never runtime entity
    /// IDs. Entities/relations without a file membership are always included;
    /// files/entities/relations with a membership inside `files` are included
    /// and the rest excluded. Dependency edges are strictly filtered to both
    /// endpoints being in `files`.
    fn fingerprint_in_files(&self, files: &HashSet<String>) -> String;
    fn max_entity_id(&self) -> u64;
}

/// Compute the active file set for a full-index fingerprint: every file-index
/// key (raw and normalized) plus every entity file membership.
pub(super) fn active_file_set(
    file_records: &RwLock<HashMap<String, super::stores::FileRecord>>,
    entity_file_index: &DashMap<EntityId, String>,
) -> HashSet<String> {
    let mut files = HashSet::new();
    let fr_guard = file_records.read();
    for (key, record) in fr_guard.iter() {
        files.insert(key.clone());
        files.insert(normalize_project_path(key));
        files.insert(normalize_project_path(&record.info.path));
    }
    for entry in entity_file_index.iter() {
        files.insert(entry.value().clone());
    }
    files
}

/// Stable symbol key for an entity, falling back to a key rebuilt from the
/// entity's own fields when the reverse map has no entry (mirroring the
/// `compute_delta` fallback so fingerprints never degrade to entity IDs).
pub(super) fn symbol_key_of_id(
    entity_to_symbol_key: &RwLock<HashMap<EntityId, SymbolKey>>,
    function_index: &DashMap<EntityId, Entity>,
    entity_file_index: &DashMap<EntityId, String>,
    id: EntityId,
    name_fallback: &str,
) -> SymbolKey {
    if let Some(key) = entity_to_symbol_key.read().get(&id) {
        return key.clone();
    }
    let file = entity_file_index
        .get(&id)
        .map(|v| v.clone())
        .unwrap_or_default();
    match function_index.get(&id) {
        Some(entity) => SymbolKey::new(
            &file,
            &entity.value().name,
            entity.value().kind,
            &entity.value().signature,
        ),
        None => SymbolKey::new(&file, name_fallback, EntityKind::Function, name_fallback),
    }
}

/// Stable symbol key of a file-level relation caller: a per-file placeholder
/// identity `(file, "<file>", Module)` shared by every canonical export of a
/// file-scoped edge. Deterministic and never backed by a runtime entity ID.
pub(super) fn file_caller_key(path: &str) -> SymbolKey {
    SymbolKey::new(path, "<file>", EntityKind::Module, "<file>")
}

/// Build the canonical components visible under `files` from the shared map
/// set and hash them with `fingerprint_from_components`.
///
/// Scoping rules (mirroring `to_canonical_snapshot`): a file is included when
/// its membership is in `files`; an entity/relation is included when it has no
/// membership or its membership is in `files`; dependency edges are included
/// only when both endpoints are in `files`. Missing symbol keys are rebuilt
/// from the function index instead of failing, so this path never degrades to
/// runtime entity IDs.
#[allow(clippy::too_many_arguments)]
pub(super) fn fingerprint_in_files_from_maps(
    function_index: &Arc<DashMap<EntityId, Entity>>,
    entity_file_index: &Arc<DashMap<EntityId, String>>,
    entity_to_symbol_key: &Arc<RwLock<HashMap<EntityId, SymbolKey>>>,
    resolved_relation_index: &Arc<DashMap<EntityId, RelationEdgeSet>>,
    file_relation_index: &Arc<DashMap<String, RelationEdgeSet>>,
    file_records: &Arc<RwLock<HashMap<String, super::stores::FileRecord>>>,
    dependency_graph: &Arc<crate::dependency_graph::FileDependencyGraph>,
    files: &HashSet<String>,
) -> String {
    let mut canonical_files: Vec<CanonicalFile> = Vec::new();
    let fr_guard = file_records.read();
    let es_guard = entity_to_symbol_key.read();
    for record in fr_guard.values() {
        let norm_path = normalize_project_path(&record.info.path);
        if !files.contains(&norm_path) {
            continue;
        }
        let imports = record.imports.standardized_imports.clone();
        let exports = record
            .exports
            .iter()
            .map(|export| CanonicalExport {
                symbol: es_guard
                    .get(&export.function_id)
                    .cloned()
                    .unwrap_or_else(|| {
                        SymbolKey::new(
                            &norm_path,
                            &export.function_name,
                            EntityKind::Function,
                            &export.function_name,
                        )
                    }),
                export_type: format!("{:?}", export.export_type).to_lowercase(),
            })
            .collect();
        canonical_files.push(CanonicalFile {
            path: norm_path,
            language: record.info.language.clone(),
            input_hash: record.info.file_hash.clone(),
            file_size: record.info.file_size,
            imports,
            exports,
        });
    }

    let mut canonical_entities: Vec<CanonicalEntity> = Vec::new();
    for entity_entry in function_index.iter() {
        let entity_id = *entity_entry.key();
        let entity = entity_entry.value();
        let file = entity_file_index.get(&entity_id).map(|v| v.clone());
        if file.as_ref().is_some_and(|f| !files.contains(f)) {
            continue;
        }
        let key = symbol_key_of_id(
            entity_to_symbol_key,
            function_index,
            entity_file_index,
            entity_id,
            &entity.name,
        );
        let parent = entity.parent.map(|parent_id| {
            symbol_key_of_id(
                entity_to_symbol_key,
                function_index,
                entity_file_index,
                parent_id,
                &entity.name,
            )
        });
        canonical_entities.push(CanonicalEntity {
            key,
            entity_id: None,
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

    let mut canonical_relations: Vec<CanonicalRelation> = Vec::new();
    for relation_entry in resolved_relation_index.iter() {
        let caller_id = *relation_entry.key();
        let caller_file = entity_file_index.get(&caller_id).map(|v| v.clone());
        if caller_file.as_ref().is_some_and(|f| !files.contains(f)) {
            continue;
        }
        let caller_key = symbol_key_of_id(
            entity_to_symbol_key,
            function_index,
            entity_file_index,
            caller_id,
            "",
        );
        for relation in relation_entry.value().iter() {
            let target = if let Some(callee_id) = relation.callee_id {
                let key = symbol_key_of_id(
                    entity_to_symbol_key,
                    function_index,
                    entity_file_index,
                    callee_id,
                    &relation.callee_name,
                );
                CanonicalRelationTarget::Internal { key }
            } else if relation.is_external && relation.external_type.is_some() {
                CanonicalRelationTarget::External {
                    classification: relation.external_type.clone(),
                }
            } else {
                CanonicalRelationTarget::Unresolved {
                    reason: UnresolvedReason::SymbolNotFound,
                }
            };
            canonical_relations.push(CanonicalRelation {
                caller: caller_key.clone(),
                target,
                raw_target: relation.callee_name.clone(),
                relation_type: relation.relation_type,
                span: relation.span,
                stdlib_category: relation.stdlib_category,
            });
        }
    }
    // File-scoped edges are canonicalized under the per-file placeholder
    // caller key `(path, "<file>", Module)`; internal targets resolve through
    // the symbol maps like entity-level edges.
    for relation_entry in file_relation_index.iter() {
        let path = relation_entry.key();
        if !files.contains(path) {
            continue;
        }
        let caller_key = file_caller_key(path);
        for relation in relation_entry.value().iter() {
            let target = if let Some(callee_id) = relation.callee_id {
                let key = symbol_key_of_id(
                    entity_to_symbol_key,
                    function_index,
                    entity_file_index,
                    callee_id,
                    &relation.callee_name,
                );
                CanonicalRelationTarget::Internal { key }
            } else if relation.is_external && relation.external_type.is_some() {
                CanonicalRelationTarget::External {
                    classification: relation.external_type.clone(),
                }
            } else {
                CanonicalRelationTarget::Unresolved {
                    reason: UnresolvedReason::SymbolNotFound,
                }
            };
            canonical_relations.push(CanonicalRelation {
                caller: caller_key.clone(),
                target,
                raw_target: relation.callee_name.clone(),
                relation_type: relation.relation_type,
                span: relation.span,
                stdlib_category: relation.stdlib_category,
            });
        }
    }

    let mut canonical_dependencies: Vec<CanonicalDependency> = Vec::new();
    for file in &canonical_files {
        for target in dependency_graph.get_dependencies(&file.path) {
            let target = normalize_project_path(&target);
            if files.contains(&target) {
                canonical_dependencies.push(CanonicalDependency {
                    source_file: file.path.clone(),
                    target_file: target,
                    source: "resolved_or_import".to_string(),
                });
            }
        }
    }

    fingerprint_from_components(&FingerprintComponents {
        schema_version: cce_types::RELATION_SNAPSHOT_SCHEMA_VERSION,
        parser_version: cce_types::RELATION_PARSER_VERSION,
        resolver_version: cce_types::RELATION_RESOLVER_VERSION,
        path_normalization_version: cce_types::RELATION_PATH_NORMALIZATION_VERSION,
        config_fingerprint: "",
        base_relation_epoch: None,
        files: &canonical_files,
        entities: &canonical_entities,
        relations: &canonical_relations,
        dependencies: &canonical_dependencies,
    })
}

impl RelationIndexView for RelationIndex {
    fn file_contains(&self, path: &str) -> bool {
        self.file_records.read().contains_key(path)
    }

    fn for_each_file<F: FnMut(&str, &FileInfo)>(&self, mut f: F) {
        let guard = self.file_records.read();
        for (key, record) in guard.iter() {
            f(key, &record.info);
        }
    }

    fn function_contains(&self, id: EntityId) -> bool {
        self.function_index.contains_key(&id)
    }

    fn for_each_function<F: FnMut(EntityId, &Entity)>(&self, mut f: F) {
        for entry in self.function_index.iter() {
            f(*entry.key(), entry.value());
        }
    }

    fn entity_file_of(&self, id: EntityId) -> Option<String> {
        self.entity_file_index.get(&id).map(|v| v.clone())
    }

    fn relations_of(&self, caller: EntityId) -> Option<Vec<ResolvedRelation>> {
        self.resolved_relation_index
            .get(&caller)
            .map(|v| v.edges.clone())
    }

    fn for_each_resolved_relation<F: FnMut(EntityId, &[ResolvedRelation])>(&self, mut f: F) {
        for entry in self.resolved_relation_index.iter() {
            f(*entry.key(), &entry.value().edges);
        }
    }

    fn file_relations_of(&self, path: &str) -> Vec<ResolvedRelation> {
        self.file_relation_index
            .get(path)
            .map(|v| v.edges.clone())
            .unwrap_or_default()
    }

    fn for_each_file_relation<F: FnMut(&str, &[ResolvedRelation])>(&self, mut f: F) {
        for entry in self.file_relation_index.iter() {
            f(entry.key(), &entry.value().edges);
        }
    }

    fn callers_of(&self, callee: EntityId) -> Vec<EntityId> {
        if let Some(callers) = self.reverse_callee_index.get(&callee) {
            return callers.clone();
        }
        if let Some(entry) = self.resolved_relation_index.get(&callee) {
            let callers = entry.callers();
            if !callers.is_empty() {
                return callers.to_vec();
            }
        }
        let mut result: Vec<EntityId> = self
            .resolved_relation_index
            .iter()
            .filter(|entry| entry.value().iter().any(|r| r.callee_id == Some(callee)))
            .map(|entry| *entry.key())
            .collect();
        result.sort();
        result.dedup();
        result
    }

    fn imports_of(&self, path: &str) -> Option<ImportTable> {
        self.file_records
            .read()
            .get(path)
            .map(|r| r.imports.clone())
    }

    fn for_each_import<F: FnMut(&str, &ImportTable)>(&self, mut f: F) {
        let guard = self.file_records.read();
        for (key, record) in guard.iter() {
            f(key, &record.imports);
        }
    }

    fn exports_of(&self, path: &str) -> Option<Vec<ExportInfo>> {
        self.file_records
            .read()
            .get(path)
            .map(|r| r.exports.iter().cloned().collect())
    }

    fn for_each_export<F: FnMut(&str, &[ExportInfo])>(&self, mut f: F) {
        let guard = self.file_records.read();
        for (key, record) in guard.iter() {
            f(key, &record.exports);
        }
    }

    fn symbol_key_of(&self, id: EntityId) -> Option<SymbolKey> {
        self.get_symbol_key_by_entity_id(id)
    }

    fn dependency_files(&self) -> Vec<String> {
        self.dependency_graph.get_all_files()
    }

    fn dependencies_of(&self, source: &str) -> Vec<String> {
        self.dependency_graph.get_dependencies(source)
    }

    fn dependents_of(&self, file: &str) -> Vec<String> {
        self.dependency_graph.get_dependents(file)
    }

    fn collect_transitive_dependents(&self, file: &str, max_depth: usize) -> Vec<String> {
        self.dependency_graph
            .collect_transitive_dependents(file, max_depth)
    }

    fn collect_transitive_dependencies(&self, file: &str, max_depth: usize) -> Vec<String> {
        self.dependency_graph
            .collect_transitive_dependencies(file, max_depth)
    }

    fn entities_by_file(&self) -> HashMap<String, Vec<Entity>> {
        // Must stay byte-identical to the grouping logic inside
        // `IndexBuilder::prepopulate_index_symbols`: iterate the
        // entity_file_index, skip entities no longer live in the function
        // index (stale stable mappings from deleted files must not become
        // symbol-table entries), and group the remaining entities by file.
        let mut entities_by_file: HashMap<String, Vec<Entity>> = HashMap::new();
        for entry in self.entity_file_index.iter() {
            let entity_id = *entry.key();
            let file_path = entry.value();
            if !self.function_index.contains_key(&entity_id) {
                continue;
            }
            if let Some(entity) = self.function_index.get(&entity_id) {
                entities_by_file
                    .entry(file_path.clone())
                    .or_default()
                    .push(entity.clone());
            }
        }
        entities_by_file
    }

    fn entities_of_file(&self, path: &str) -> Vec<Entity> {
        // The per-file index may carry stale IDs (e.g. external name
        // registrations); joining against `function_index` keeps only live
        // entities, matching the `entities_by_file` grouping contract.
        let fe_guard = self.file_entities_by_start.read();
        let Some(rows) = fe_guard.get(path) else {
            return Vec::new();
        };
        rows.iter()
            .filter_map(|(_, id)| self.function_index.get(id).map(|e| e.clone()))
            .collect()
    }

    fn file_callers_of(&self, callee: EntityId) -> Vec<String> {
        self.file_callers_by_callee
            .get(&callee)
            .map(|v| v.iter().cloned().collect())
            .unwrap_or_default()
    }

    fn stable_symbol_keys(&self) -> Vec<SymbolKey> {
        self.symbol_key_to_entity.read().keys().cloned().collect()
    }

    fn stable_symbol_keys_in_files(&self, files: &HashSet<String>) -> Vec<SymbolKey> {
        // O(scope) via the file reverse index instead of scanning the full
        // `symbol_key_to_entity` map.
        let mut keys = Vec::new();
        let fsk_guard = self.file_symbol_keys.read();
        for file in files {
            if let Some(vec) = fsk_guard.get(file) {
                keys.extend(vec.iter().cloned());
            }
        }
        keys
    }

    fn max_entity_id(&self) -> u64 {
        crate::index::core::RelationIndex::max_entity_id(self)
    }

    fn fingerprint_in_files(&self, files: &HashSet<String>) -> String {
        fingerprint_in_files_from_maps(
            &self.function_index,
            &self.entity_file_index,
            &self.entity_to_symbol_key,
            &self.resolved_relation_index,
            &self.file_relation_index,
            &self.file_records,
            &self.dependency_graph,
            files,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::file_index::{ExportIndexOps, FileIndexOps, ImportIndexOps};
    use crate::index::test_support::{
        external_edge, internal_edge, seed_index_in, seed_multi_file_index,
    };
    use crate::types::{ExportInfo, ExportType};
    use cce_types::{
        EntityId, FileInfo, ImportKind, ImportTable, RelationType, StandardizedImport,
    };
    use std::collections::HashSet;

    fn file_set(index: &RelationIndex) -> HashSet<String> {
        active_file_set(&index.file_records, &index.entity_file_index)
    }

    #[test]
    fn fingerprint_in_files_matches_canonical_snapshot() {
        let index = seed_multi_file_index(
            &[
                (
                    "src/lib.rs",
                    &[(EntityId(1), "caller"), (EntityId(2), "callee")],
                ),
                ("src/util.rs", &[(EntityId(3), "helper")]),
            ],
            &[
                internal_edge(EntityId(1), EntityId(2), "callee", RelationType::DirectCall),
                internal_edge(EntityId(3), EntityId(2), "callee", RelationType::DirectCall),
            ],
        );

        let canonical = index
            .to_canonical_snapshot(String::new())
            .expect("snapshot must build");
        assert_eq!(
            index.compute_fingerprint(),
            canonical.fingerprint(),
            "index fingerprint must be byte-identical to the snapshot fingerprint"
        );
    }

    #[test]
    fn fingerprint_in_files_subset_is_stable_and_scoped() {
        let index = seed_multi_file_index(
            &[
                (
                    "src/lib.rs",
                    &[(EntityId(1), "caller"), (EntityId(2), "callee")],
                ),
                ("src/util.rs", &[(EntityId(3), "helper")]),
            ],
            &[
                internal_edge(EntityId(1), EntityId(2), "callee", RelationType::DirectCall),
                internal_edge(EntityId(3), EntityId(2), "callee", RelationType::DirectCall),
            ],
        );

        let all = file_set(&index);
        let full = index.fingerprint_in_files(&all);
        let lib_only = HashSet::from(["src/lib.rs".to_string()]);
        let lib_fp = index.fingerprint_in_files(&lib_only);

        assert_ne!(
            full, lib_fp,
            "a strict subset must not equal the full fingerprint"
        );
        assert_eq!(
            index.fingerprint_in_files(&lib_only),
            lib_fp,
            "re-computation of the same subset must be stable"
        );

        // An in-scope mutation changes the subset fingerprint.
        index.add_resolved_relation(external_edge(EntityId(1), "printf"));
        let lib_fp_after_mutation = index.fingerprint_in_files(&lib_only);
        assert_ne!(lib_fp_after_mutation, lib_fp);

        // An out-of-scope mutation leaves the subset fingerprint untouched.
        index.add_resolved_relation(external_edge(EntityId(3), "malloc"));
        assert_eq!(index.fingerprint_in_files(&lib_only), lib_fp_after_mutation);

        // ... but does change the full fingerprint.
        assert_ne!(index.fingerprint_in_files(&file_set(&index)), full);
    }

    #[test]
    fn fingerprint_in_files_is_entity_id_independent() {
        let a = seed_multi_file_index(
            &[
                (
                    "src/lib.rs",
                    &[(EntityId(1), "caller"), (EntityId(2), "callee")],
                ),
                ("src/util.rs", &[(EntityId(3), "helper")]),
            ],
            &[
                internal_edge(EntityId(1), EntityId(2), "callee", RelationType::DirectCall),
                internal_edge(EntityId(3), EntityId(2), "callee", RelationType::DirectCall),
            ],
        );
        let b = seed_multi_file_index(
            &[
                (
                    "src/lib.rs",
                    &[(EntityId(10), "caller"), (EntityId(20), "callee")],
                ),
                ("src/util.rs", &[(EntityId(30), "helper")]),
            ],
            &[
                internal_edge(
                    EntityId(10),
                    EntityId(20),
                    "callee",
                    RelationType::DirectCall,
                ),
                internal_edge(
                    EntityId(30),
                    EntityId(20),
                    "callee",
                    RelationType::DirectCall,
                ),
            ],
        );

        assert_eq!(
            a.compute_fingerprint(),
            b.compute_fingerprint(),
            "identical structure with different entity IDs must produce identical fingerprints"
        );
        let snapshot_a = a
            .to_canonical_snapshot(String::new())
            .expect("snapshot must build");
        let snapshot_b = b
            .to_canonical_snapshot(String::new())
            .expect("snapshot must build");
        assert_eq!(snapshot_a.fingerprint(), snapshot_b.fingerprint());
    }

    #[test]
    fn fingerprint_in_files_covers_imports_exports_and_deps() {
        let plain = seed_index_in("src/lib.rs", &[(EntityId(1), "caller")], &[]);

        let rich = seed_index_in("src/lib.rs", &[(EntityId(1), "caller")], &[]);
        let file = FileInfo {
            id: "src/lib.rs".to_string(),
            path: "src/lib.rs".to_string(),
            ..Default::default()
        };
        rich.add_file(file);
        let mut table = ImportTable {
            file_id: "src/lib.rs".to_string(),
            ..Default::default()
        };
        table.standardized_imports.push(StandardizedImport::new(
            ImportKind::ModuleImport,
            "other_module",
        ));
        rich.add_import_table("src/lib.rs".to_string(), table);
        rich.add_export(
            "src/lib.rs",
            ExportInfo {
                function_id: EntityId(1),
                function_name: "caller".to_string(),
                export_type: ExportType::Named,
            },
        );
        rich.dependency_graph
            .add_dependency("src/lib.rs", "other_file.rs");

        assert_eq!(file_set(&plain), file_set(&rich));
        assert_ne!(
            plain.compute_fingerprint(),
            rich.compute_fingerprint(),
            "imports, exports, and dependencies must be covered by the fingerprint"
        );
        assert_eq!(
            rich.compute_fingerprint(),
            rich.compute_fingerprint(),
            "fingerprint must be stable for the same data"
        );

        // File-level data is attributed per file: a subset containing only a
        // different file ignores it.
        let other_only = HashSet::from(["other_file.rs".to_string()]);
        assert_eq!(
            plain.fingerprint_in_files(&other_only),
            rich.fingerprint_in_files(&other_only)
        );
    }
}
