//! Immutable snapshot views over a relation index.
//!
//! Two layers share the same query surface:
//!
//! - [`RelationSnapshotIndex`]: the query facade. It holds either the
//!   build-time shared maps or a bounded sharded view of a published epoch.
//!   All callers keep using this one public type.
//! - [`LayeredSnapshotIndex`]: a base snapshot overlaid with an ordered chain
//!   of incremental deltas, merging them at read time.
//!
//! [`InMemoryRelationSnapshotIndex`] is the former shared-map implementation.
//! It is only used while building, for verification, and by shard assembly.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use cce_types::relation::RelationSnapshotStore;
use cce_types::{
    Entity, EntityId, ExternalCallType, FileInfo, ImportTable, RelationSnapshotState,
    ResolvedRelation, ShardQueryConfig, SnapshotBuildMetadata, StorageError,
};
use dashmap::DashMap;
use parking_lot::RwLock;
use smallvec::SmallVec;

use super::stores::FileRecord;
use super::stores::diagnostics::RelationDiagnostics;

use crate::dependency_graph::FileDependencyGraph;
use crate::error::IndexError;
use crate::index::core::{QualityReport, RelationEdgeSet, RelationIndex, SymbolKey};
use crate::index::view::{RelationIndexView, active_file_set, fingerprint_in_files_from_maps};
use crate::types::ExportInfo;

mod query_optimized;
mod sharded;
mod snapshot_builder;
#[cfg(test)]
mod tests;

pub use query_optimized::{QueryOptimizedIndex, TransitiveFileDeps};
pub use snapshot_builder::StreamingSnapshotArtifactBuilder;

use sharded::ShardedSnapshotIndex;
pub(crate) use snapshot_builder::shared_view;

/// Capacity of the sharded query index at a point in time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShardCapacityReport {
    pub shard_count: usize,
    pub directory_entries: usize,
    pub directory_estimated_bytes: u64,
    pub loaded_shards: usize,
    pub loaded_bytes: u64,
    pub lru_hits: u64,
    pub lru_evictions: u64,
    pub max_loaded_shards: usize,
    pub max_shards_per_query: usize,
    pub route_cache_entries: usize,
    pub route_cache_hits: u64,
    pub route_store_lookups: u64,
}

/// In-memory implementation used while a relation graph is being built and
/// as the assembled representation of one loaded shard.
#[derive(Debug, Clone)]
pub struct InMemoryRelationSnapshotIndex {
    pub(super) function_index: Arc<DashMap<EntityId, Entity>>,
    pub(super) name_index: Arc<DashMap<String, SmallVec<[EntityId; 2]>>>,
    pub(super) entity_file_index: Arc<DashMap<EntityId, String>>,
    pub(super) resolved_relation_index: Arc<DashMap<EntityId, RelationEdgeSet>>,
    pub(super) reverse_callee_index: Arc<DashMap<EntityId, Vec<EntityId>>>,
    pub(super) file_relation_index: Arc<DashMap<String, RelationEdgeSet>>,
    pub(super) file_callers_by_callee: Arc<DashMap<EntityId, HashSet<String>>>,
    pub(super) file_records: Arc<RwLock<HashMap<String, FileRecord>>>,
    pub(super) dependency_graph: Arc<FileDependencyGraph>,
    pub(super) symbol_key_to_entity: Arc<RwLock<HashMap<SymbolKey, EntityId>>>,
    pub(super) entity_to_symbol_key: Arc<RwLock<HashMap<EntityId, SymbolKey>>>,
    pub(super) stable_id_to_entity: Arc<RwLock<HashMap<String, EntityId>>>,
    pub(super) file_symbol_keys: Arc<RwLock<HashMap<String, Vec<SymbolKey>>>>,
    pub(super) file_entities_by_start: super::FileEntitiesByStart,
    pub(super) diagnostics: Arc<RelationDiagnostics>,
    pub(super) query_optimized: Arc<QueryOptimizedIndex>,
    pub(super) transitive_deps: Arc<TransitiveFileDeps>,
    pub(super) classification_stats_cache: Arc<RwLock<Option<HashMap<ExternalCallType, usize>>>>,
}

impl InMemoryRelationSnapshotIndex {
    pub fn from_index(index: &RelationIndex) -> Self {
        let index = index.detached_clone();
        Self::from_index_shared(&index)
    }

    pub fn from_index_owned(index: &mut RelationIndex) -> Self {
        let index = index.snapshot_take();
        Self::from_index_shared(&index)
    }

    pub fn from_index_shared(index: &RelationIndex) -> Self {
        let query_optimized = Arc::new(QueryOptimizedIndex::from_relation_index(index));
        let transitive_deps = Arc::new(TransitiveFileDeps::from_relation_index(index, 10));
        Self {
            function_index: Arc::clone(&index.function_index),
            name_index: Arc::clone(&index.name_index),
            entity_file_index: Arc::clone(&index.entity_file_index),
            resolved_relation_index: Arc::clone(&index.resolved_relation_index),
            reverse_callee_index: Arc::clone(&index.reverse_callee_index),
            file_relation_index: Arc::clone(&index.file_relation_index),
            file_callers_by_callee: Arc::clone(&index.file_callers_by_callee),
            file_records: Arc::clone(&index.file_records),
            dependency_graph: Arc::clone(&index.dependency_graph),
            symbol_key_to_entity: Arc::clone(&index.symbol_key_to_entity),
            entity_to_symbol_key: Arc::clone(&index.entity_to_symbol_key),
            stable_id_to_entity: Arc::clone(&index.stable_id_to_entity),
            file_symbol_keys: Arc::clone(&index.file_symbol_keys),
            file_entities_by_start: Arc::clone(&index.file_entities_by_start),
            diagnostics: Arc::clone(&index.diagnostics),
            query_optimized,
            transitive_deps,
            classification_stats_cache: Arc::new(RwLock::new(None)),
        }
    }

    pub(super) fn relations_by_classification(
        &self,
        classification: &ExternalCallType,
    ) -> Vec<ResolvedRelation> {
        let mut result = Vec::new();
        for entry in self.resolved_relation_index.iter() {
            for relation in entry.iter() {
                if relation.external_type.as_ref() == Some(classification) {
                    result.push(relation.clone());
                }
            }
        }
        result
    }

    pub(super) fn classification_stats_memoized(&self) -> HashMap<ExternalCallType, usize> {
        if let Some(cached) = self.classification_stats_cache.read().clone() {
            return cached;
        }
        let mut stats = HashMap::new();
        for entry in self.resolved_relation_index.iter() {
            for relation in entry.iter() {
                if let Some(ref ext_type) = relation.external_type {
                    *stats.entry(ext_type.clone()).or_insert(0) += 1;
                }
            }
        }
        *self.classification_stats_cache.write() = Some(stats.clone());
        stats
    }

    pub(super) fn transitive_dependents_of(&self, file_path: &str) -> Vec<String> {
        self.transitive_deps.transitive_dependents_of(file_path)
    }

    pub(super) fn transitive_dependencies_of(&self, file_path: &str) -> Vec<String> {
        self.transitive_deps.transitive_dependencies_of(file_path)
    }

    pub(super) fn function_count(&self) -> usize {
        self.function_index.len()
    }

    pub(super) fn file_count(&self) -> usize {
        self.file_records.read().len()
    }

    pub(super) fn resolved_relation_count(&self) -> usize {
        self.resolved_relation_index
            .iter()
            .map(|entry| entry.len())
            .sum()
    }

    pub(super) fn quality_report(&self) -> QualityReport {
        let summary = self.diagnostics.summary();
        let quality_score = self.diagnostics.quality_score(self.function_index.len());
        QualityReport {
            summary,
            quality_score,
            file_count: self.file_records.read().len(),
            entity_count: self.function_index.len(),
            relation_count: self.resolved_relation_count(),
        }
    }

    pub(super) fn get_function(&self, id: EntityId) -> Option<Entity> {
        self.function_index
            .get(&id)
            .map(|entry| entry.value().clone())
    }

    pub(super) fn function_ids_by_name(&self, name: &str) -> Vec<EntityId> {
        self.name_index
            .get(name)
            .map(|ids| ids.to_vec())
            .unwrap_or_default()
    }

    pub(super) fn file_path_of(&self, id: EntityId) -> Option<String> {
        self.entity_file_index.get(&id).map(|value| value.clone())
    }

    pub(super) fn relations_of(&self, caller: EntityId) -> Option<Vec<ResolvedRelation>> {
        if let Some(callees) = self.query_optimized.get_callees(caller) {
            return Some(callees.clone());
        }
        self.resolved_relation_index
            .get(&caller)
            .map(|entry| entry.edges.clone())
    }

    pub(super) fn callers_of(&self, callee: EntityId) -> Vec<EntityId> {
        let mut result = if let Some(callers) = self.query_optimized.get_callers(callee) {
            callers.clone()
        } else if let Some(callers) = self.reverse_callee_index.get(&callee) {
            callers.clone()
        } else {
            Vec::new()
        };
        result.sort();
        result.dedup();
        result
    }

    pub(super) fn relations_to_entity(&self, callee: EntityId) -> Vec<ResolvedRelation> {
        self.callers_of(callee)
            .into_iter()
            .flat_map(|caller| {
                self.relations_of(caller)
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|relation| relation.callee_id == Some(callee))
            })
            .collect()
    }

    pub(super) fn file_relations_of(&self, path: &str) -> Vec<ResolvedRelation> {
        self.file_relation_index
            .get(path)
            .map(|entry| entry.edges.clone())
            .unwrap_or_default()
    }

    pub(super) fn file_callers_of(&self, callee: EntityId) -> Vec<String> {
        self.file_callers_by_callee
            .get(&callee)
            .map(|value| value.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub(super) fn get_file_info(&self, path: &str) -> Option<FileInfo> {
        self.file_records.read().get(path).map(|r| r.info.clone())
    }

    pub(super) fn get_import_table(&self, path: &str) -> Option<ImportTable> {
        self.file_records
            .read()
            .get(path)
            .map(|r| r.imports.clone())
    }

    pub(super) fn get_exports(&self, path: &str) -> Option<Vec<ExportInfo>> {
        self.file_records
            .read()
            .get(path)
            .map(|r| r.exports.iter().cloned().collect())
    }

    pub(super) fn entity_ids_of_file(&self, path: &str) -> Vec<EntityId> {
        if let Some(rows) = self.file_entities_by_start.read().get(path) {
            return rows.iter().map(|(_, id)| *id).collect();
        }
        self.entity_file_index
            .iter()
            .filter(|entry| entry.value().as_str() == path)
            .map(|entry| *entry.key())
            .collect()
    }

    pub(super) fn entities_in_line_range(
        &self,
        path: &str,
        start_line: usize,
        end_line: usize,
    ) -> Vec<EntityId> {
        self.entity_file_index
            .iter()
            .filter(|entry| entry.value().as_str() == path)
            .map(|entry| *entry.key())
            .filter(|id| {
                self.function_index.get(id).is_some_and(|entity| {
                    entity.span.start_position.row <= end_line
                        && entity.span.end_position.row >= start_line
                })
            })
            .collect()
    }

    pub(super) fn symbol_key_of(&self, id: EntityId) -> Option<SymbolKey> {
        self.entity_to_symbol_key.read().get(&id).cloned()
    }

    pub(super) fn entity_id_by_symbol(&self, key: &SymbolKey) -> Option<EntityId> {
        self.symbol_key_to_entity.read().get(key).copied()
    }

    pub(super) fn entity_id_by_stable_id(&self, stable_id: &str) -> Option<EntityId> {
        self.stable_id_to_entity.read().get(stable_id).copied()
    }

    pub(super) fn symbol_keys_in_files(&self, files: &HashSet<String>) -> Vec<SymbolKey> {
        let guard = self.file_symbol_keys.read();
        files
            .iter()
            .flat_map(|file| guard.get(file).cloned().unwrap_or_default())
            .collect()
    }

    pub(super) fn max_entity_id(&self) -> u64 {
        self.function_index
            .iter()
            .map(|entry| entry.key().0)
            .max()
            .unwrap_or(0)
    }

    pub(super) fn fingerprint_in_files(&self, files: &HashSet<String>) -> String {
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

#[derive(Debug, Clone)]
enum SnapshotBacking {
    InMemory(Arc<InMemoryRelationSnapshotIndex>),
    Sharded(Arc<ShardedSnapshotIndex>),
}

/// Immutable, query-only snapshot of a relation index.
///
/// The public type stays stable for every caller. Its backing is either the
/// in-memory build graph or a bounded sharded projection of a published epoch.
#[derive(Debug, Clone)]
pub struct RelationSnapshotIndex {
    backing: SnapshotBacking,
}

impl RelationSnapshotIndex {
    /// Empty immutable view used for valid zero-record projects.
    pub fn empty() -> Self {
        Self::from_memory(shared_view(&RelationIndex::new()))
    }

    pub fn from_index(index: &RelationIndex) -> Self {
        Self::from_memory(InMemoryRelationSnapshotIndex::from_index(index))
    }

    pub fn from_index_owned(index: &mut RelationIndex) -> Self {
        Self::from_memory(InMemoryRelationSnapshotIndex::from_index_owned(index))
    }

    pub fn from_index_shared(index: &RelationIndex) -> Self {
        Self::from_memory(InMemoryRelationSnapshotIndex::from_index_shared(index))
    }

    fn from_memory(index: InMemoryRelationSnapshotIndex) -> Self {
        Self {
            backing: SnapshotBacking::InMemory(Arc::new(index)),
        }
    }

    /// Construct the sharded query view of a published epoch.
    pub fn from_store<S>(
        store: Arc<S>,
        project_id: i64,
        relation_epoch: i64,
        config: ShardQueryConfig,
    ) -> Result<Self, StorageError>
    where
        S: RelationSnapshotStore + 'static,
    {
        Self::from_store_dyn(store, project_id, relation_epoch, config)
    }

    pub fn from_store_dyn(
        store: Arc<dyn RelationSnapshotStore>,
        project_id: i64,
        relation_epoch: i64,
        config: ShardQueryConfig,
    ) -> Result<Self, StorageError> {
        let manifest = store
            .get_manifest(project_id, relation_epoch)?
            .ok_or_else(|| {
                StorageError::validation(format!("relation epoch {relation_epoch} has no manifest"))
            })?;
        let base_epoch = if manifest.state == RelationSnapshotState::Delta {
            store
                .find_base_epoch(project_id, relation_epoch)?
                .ok_or_else(|| {
                    StorageError::validation(format!(
                        "delta epoch {relation_epoch} has no base epoch"
                    ))
                })?
        } else {
            relation_epoch
        };
        let base_manifest = if base_epoch == relation_epoch {
            manifest.clone()
        } else {
            store.get_manifest(project_id, base_epoch)?.ok_or_else(|| {
                StorageError::validation(format!(
                    "base epoch {base_epoch} of relation epoch {relation_epoch} is missing"
                ))
            })?
        };
        let directory = store.read_shard_directory(project_id, base_epoch)?;
        let sharded =
            ShardedSnapshotIndex::new(store, base_manifest, base_epoch, directory, config)?;
        Ok(Self {
            backing: SnapshotBacking::Sharded(Arc::new(sharded)),
        })
    }

    /// Epoch whose shard directory backs this view (for observability).
    pub fn base_epoch(&self) -> i64 {
        match &self.backing {
            SnapshotBacking::Sharded(sharded) => sharded.base_epoch,
            SnapshotBacking::InMemory(_) => 0,
        }
    }

    pub fn capacity_report(&self) -> Option<ShardCapacityReport> {
        match &self.backing {
            SnapshotBacking::Sharded(sharded) => Some(sharded.capacity_report()),
            SnapshotBacking::InMemory(_) => None,
        }
    }

    fn shard_read<T>(
        &self,
        operation: &str,
        read: impl FnOnce(&ShardedSnapshotIndex) -> Result<T, StorageError>,
    ) -> T {
        match &self.backing {
            SnapshotBacking::Sharded(sharded) => read(sharded).unwrap_or_else(|error| {
                panic!("sharded relation snapshot {operation} failed: {error}")
            }),
            SnapshotBacking::InMemory(_) => {
                panic!("sharded relation snapshot {operation} called on an in-memory view")
            }
        }
    }

    pub fn diagnostics(&self) -> &RelationDiagnostics {
        match &self.backing {
            SnapshotBacking::InMemory(index) => &index.diagnostics,
            SnapshotBacking::Sharded(sharded) => &sharded.diagnostics,
        }
    }

    pub fn transitive_dependents_of(&self, file_path: &str) -> Vec<String> {
        match &self.backing {
            SnapshotBacking::InMemory(index) => index.transitive_dependents_of(file_path),
            SnapshotBacking::Sharded(_) => self.collect_transitive_dependents(file_path, 10),
        }
    }

    pub fn transitive_dependencies_of(&self, file_path: &str) -> Vec<String> {
        match &self.backing {
            SnapshotBacking::InMemory(index) => index.transitive_dependencies_of(file_path),
            SnapshotBacking::Sharded(_) => self.collect_transitive_dependencies(file_path, 10),
        }
    }

    pub fn collect_transitive_dependents(&self, file: &str, max_depth: usize) -> Vec<String> {
        match &self.backing {
            SnapshotBacking::InMemory(index) => index
                .dependency_graph
                .collect_transitive_dependents(file, max_depth),
            SnapshotBacking::Sharded(_) => transitive_walk(file, max_depth, |path| {
                self.shard_read("transitive_dependents", |s| s.dependents_of(path))
            }),
        }
    }

    pub fn collect_transitive_dependencies(&self, file: &str, max_depth: usize) -> Vec<String> {
        match &self.backing {
            SnapshotBacking::InMemory(index) => index
                .dependency_graph
                .collect_transitive_dependencies(file, max_depth),
            SnapshotBacking::Sharded(_) => transitive_walk(file, max_depth, |path| {
                self.shard_read("transitive_dependencies", |s| s.dependencies_of(path))
            }),
        }
    }

    /// Count diagnostics counters without exposing the underlying maps.
    pub fn build_metadata_snapshot(&self) -> SnapshotBuildMetadata {
        let diagnostics = self.diagnostics();
        SnapshotBuildMetadata {
            symbol_key_conflict_count: diagnostics
                .symbol_key_conflict_count
                .load(std::sync::atomic::Ordering::Relaxed),
            symbol_key_conflict_samples: diagnostics.symbol_key_conflict_samples(),
            entity_derived_key_count: diagnostics
                .entity_derived_key_count
                .load(std::sync::atomic::Ordering::Relaxed),
            relation_derived_key_count: diagnostics
                .relation_derived_key_count
                .load(std::sync::atomic::Ordering::Relaxed),
        }
    }

    pub fn function_count(&self) -> usize {
        match &self.backing {
            SnapshotBacking::InMemory(index) => index.function_count(),
            SnapshotBacking::Sharded(sharded) => sharded.function_count(),
        }
    }

    pub fn resolved_relation_count(&self) -> usize {
        match &self.backing {
            SnapshotBacking::InMemory(index) => index.resolved_relation_count(),
            SnapshotBacking::Sharded(sharded) => sharded.relation_count(),
        }
    }

    pub fn file_count(&self) -> usize {
        match &self.backing {
            SnapshotBacking::InMemory(index) => index.file_count(),
            SnapshotBacking::Sharded(sharded) => sharded.file_count(),
        }
    }

    pub fn quality_report(&self) -> QualityReport {
        match &self.backing {
            SnapshotBacking::InMemory(index) => index.quality_report(),
            SnapshotBacking::Sharded(sharded) => QualityReport {
                summary: sharded.diagnostics.summary(),
                quality_score: sharded.diagnostics.quality_score(sharded.function_count()),
                file_count: sharded.file_count(),
                entity_count: sharded.function_count(),
                relation_count: sharded.relation_count(),
            },
        }
    }

    pub fn compute_fingerprint(&self) -> String {
        let files = match &self.backing {
            SnapshotBacking::InMemory(index) => {
                active_file_set(&index.file_records, &index.entity_file_index)
            }
            SnapshotBacking::Sharded(_) => self.shard_read("active_file_set", |s| {
                Ok(s.files().into_iter().collect::<HashSet<_>>())
            }),
        };
        self.fingerprint_in_files(&files)
    }

    /// Canonical export of the base view. Verification and storage
    /// publication use it; it is never called on a request path.
    pub fn to_canonical_snapshot(
        &self,
        config_fingerprint: String,
    ) -> Result<cce_types::CanonicalRelationSnapshot, String> {
        match &self.backing {
            SnapshotBacking::InMemory(index) => index.to_canonical_snapshot(config_fingerprint),
            SnapshotBacking::Sharded(sharded) => {
                let shards = sharded
                    .load_shards(&sharded.all_shard_ids())
                    .map_err(|error| error.to_string())?;
                let mut merged =
                    cce_types::CanonicalRelationSnapshot::new(config_fingerprint.clone());
                merged.schema_version = sharded.manifest().schema_version;
                merged.parser_version = sharded.manifest().parser_version;
                merged.resolver_version = sharded.manifest().resolver_version;
                merged.path_normalization_version = sharded.manifest().path_normalization_version;
                for shard in shards {
                    let snapshot = shard.to_canonical_snapshot(config_fingerprint.clone())?;
                    merged.files.extend(snapshot.files);
                    merged.entities.extend(snapshot.entities);
                    merged.relations.extend(snapshot.relations);
                    merged.dependencies.extend(snapshot.dependencies);
                }
                merged.normalize();
                merged.build_metadata = SnapshotBuildMetadata {
                    symbol_key_conflict_count: sharded.manifest().symbol_key_conflict_count,
                    symbol_key_conflict_samples: sharded
                        .manifest()
                        .symbol_key_conflict_samples
                        .clone(),
                    ..SnapshotBuildMetadata::default()
                };
                Ok(merged)
            }
        }
    }

    pub fn fingerprint_in_files(&self, files: &HashSet<String>) -> String {
        match &self.backing {
            SnapshotBacking::InMemory(index) => index.fingerprint_in_files(files),
            SnapshotBacking::Sharded(_) => {
                let mut snapshot =
                    self.to_canonical_snapshot(String::new())
                        .unwrap_or_else(|error| {
                            panic!("sharded canonical snapshot for fingerprint failed: {error}")
                        });
                snapshot.files.retain(|file| files.contains(&file.path));
                snapshot
                    .entities
                    .retain(|entity| files.contains(&entity.key.file_path));
                snapshot
                    .relations
                    .retain(|relation| files.contains(&relation.caller.file_path));
                snapshot.dependencies.retain(|dependency| {
                    files.contains(&dependency.source_file)
                        && files.contains(&dependency.target_file)
                });
                snapshot.fingerprint()
            }
        }
    }

    pub(super) fn get_function(&self, id: EntityId) -> Option<Entity> {
        match &self.backing {
            SnapshotBacking::InMemory(index) => index.get_function(id),
            SnapshotBacking::Sharded(_) => self.shard_read("get_function", |s| s.get_function(id)),
        }
    }

    pub(super) fn function_ids_by_name(&self, name: &str) -> Vec<EntityId> {
        match &self.backing {
            SnapshotBacking::InMemory(index) => index.function_ids_by_name(name),
            SnapshotBacking::Sharded(_) => {
                let shard_ids = self.shard_read("name_shards", |s| s.name_shards(name));
                if shard_ids.is_empty() {
                    return Vec::new();
                }
                self.shard_read("function_ids_by_name", |s| {
                    let shards = s.load_shards(&shard_ids)?;
                    Ok(shards
                        .iter()
                        .flat_map(|shard| shard.function_ids_by_name(name))
                        .collect())
                })
            }
        }
    }

    pub(super) fn file_path_of(&self, id: EntityId) -> Option<String> {
        match &self.backing {
            SnapshotBacking::InMemory(index) => index.file_path_of(id),
            SnapshotBacking::Sharded(_) => self.shard_read("file_path_of", |s| s.file_path_of(id)),
        }
    }

    pub(super) fn entities_in_line_range(
        &self,
        path: &str,
        start: usize,
        end: usize,
    ) -> Vec<EntityId> {
        match &self.backing {
            SnapshotBacking::InMemory(index) => index.entities_in_line_range(path, start, end),
            SnapshotBacking::Sharded(_) => self.shard_read("entities_in_line_range", |s| {
                s.entities_in_line_range(path, start, end)
            }),
        }
    }

    pub(super) fn relations_of(&self, caller: EntityId) -> Option<Vec<ResolvedRelation>> {
        match &self.backing {
            SnapshotBacking::InMemory(index) => index.relations_of(caller),
            SnapshotBacking::Sharded(_) => {
                self.shard_read("relations_of", |s| s.relations_of(caller))
            }
        }
    }

    pub(super) fn relations_of_checked(
        &self,
        caller: EntityId,
    ) -> Result<Vec<ResolvedRelation>, IndexError> {
        if !self.contains_function(caller) {
            return Err(IndexError::entity_not_found(caller));
        }
        Ok(self.relations_of(caller).unwrap_or_default())
    }

    pub(super) fn callers_of(&self, callee: EntityId) -> Vec<EntityId> {
        match &self.backing {
            SnapshotBacking::InMemory(index) => index.callers_of(callee),
            SnapshotBacking::Sharded(_) => self.shard_read("callers_of", |s| s.callers_of(callee)),
        }
    }

    pub(super) fn callers_of_checked(&self, callee: EntityId) -> Result<Vec<EntityId>, IndexError> {
        if !self.contains_function(callee) {
            return Err(IndexError::entity_not_found(callee));
        }
        Ok(self.callers_of(callee))
    }

    pub(super) fn relations_to_entity(&self, callee: EntityId) -> Vec<ResolvedRelation> {
        match &self.backing {
            SnapshotBacking::InMemory(index) => index.relations_to_entity(callee),
            SnapshotBacking::Sharded(_) => {
                self.shard_read("relations_to_entity", |s| s.relations_to_entity(callee))
            }
        }
    }

    pub(super) fn relations_from_entity_by_type(
        &self,
        caller: EntityId,
        relation_type: cce_types::RelationType,
    ) -> Vec<ResolvedRelation> {
        self.relations_of(caller)
            .unwrap_or_default()
            .into_iter()
            .filter(|relation| relation.relation_type == relation_type)
            .collect()
    }

    pub(super) fn relations_by_classification(
        &self,
        classification: &ExternalCallType,
    ) -> Vec<ResolvedRelation> {
        match &self.backing {
            SnapshotBacking::InMemory(index) => index.relations_by_classification(classification),
            SnapshotBacking::Sharded(_) => self.shard_read("relations_by_classification", |s| {
                let shard_ids = s.classification_shards_for_query(classification)?;
                let shards = s.load_shards(&shard_ids)?;
                Ok(shards
                    .iter()
                    .flat_map(|shard| shard.relations_by_classification(classification))
                    .collect())
            }),
        }
    }

    pub(super) fn classification_stats(&self) -> HashMap<ExternalCallType, usize> {
        match &self.backing {
            SnapshotBacking::InMemory(index) => index.classification_stats_memoized(),
            SnapshotBacking::Sharded(sharded) => sharded.classification_stats(),
        }
    }

    pub(super) fn get_file_info(&self, path: &str) -> Option<FileInfo> {
        match &self.backing {
            SnapshotBacking::InMemory(index) => index.get_file_info(path),
            SnapshotBacking::Sharded(_) => {
                self.shard_read("get_file_info", |s| s.get_file_info(path))
            }
        }
    }

    pub(super) fn contains_file(&self, path: &str) -> bool {
        match &self.backing {
            SnapshotBacking::InMemory(index) => index.file_records.read().contains_key(path),
            SnapshotBacking::Sharded(_) => {
                self.shard_read("contains_file", |s| Ok(s.file_shard(path)?.is_some()))
            }
        }
    }

    pub(super) fn get_import_table(&self, path: &str) -> Option<ImportTable> {
        match &self.backing {
            SnapshotBacking::InMemory(index) => index.get_import_table(path),
            SnapshotBacking::Sharded(_) => {
                self.shard_read("get_import_table", |s| s.get_import_table(path))
            }
        }
    }

    pub(super) fn get_exports(&self, path: &str) -> Option<Vec<ExportInfo>> {
        match &self.backing {
            SnapshotBacking::InMemory(index) => index.get_exports(path),
            SnapshotBacking::Sharded(_) => self.shard_read("get_exports", |s| s.get_exports(path)),
        }
    }

    pub(super) fn entity_ids_of_file(&self, path: &str) -> Vec<EntityId> {
        match &self.backing {
            SnapshotBacking::InMemory(index) => index.entity_ids_of_file(path),
            SnapshotBacking::Sharded(_) => {
                self.shard_read("entity_ids_of_file", |s| s.entity_ids_of_file(path))
            }
        }
    }

    pub(super) fn file_relations_of(&self, path: &str) -> Vec<ResolvedRelation> {
        match &self.backing {
            SnapshotBacking::InMemory(index) => index.file_relations_of(path),
            SnapshotBacking::Sharded(_) => {
                self.shard_read("file_relations_of", |s| s.file_relations_of(path))
            }
        }
    }

    pub(super) fn file_callers_of(&self, callee: EntityId) -> Vec<String> {
        match &self.backing {
            SnapshotBacking::InMemory(index) => index.file_callers_of(callee),
            SnapshotBacking::Sharded(_) => {
                self.shard_read("file_callers_of", |s| s.file_callers_of(callee))
            }
        }
    }

    pub(super) fn symbol_key_of(&self, id: EntityId) -> Option<SymbolKey> {
        match &self.backing {
            SnapshotBacking::InMemory(index) => index.symbol_key_of(id),
            SnapshotBacking::Sharded(_) => {
                self.shard_read("symbol_key_of", |s| s.symbol_key_of(id))
            }
        }
    }

    pub(super) fn entity_id_by_symbol_key(&self, key: &SymbolKey) -> Option<EntityId> {
        match &self.backing {
            SnapshotBacking::InMemory(index) => index.entity_id_by_symbol(key),
            SnapshotBacking::Sharded(_) => self.shard_read("entity_id_by_symbol_key", |s| {
                s.entity_id_by_symbol_key(key)
            }),
        }
    }

    pub(super) fn entity_id_by_stable_symbol_id(&self, stable_id: &str) -> Option<EntityId> {
        match &self.backing {
            SnapshotBacking::InMemory(index) => index.entity_id_by_stable_id(stable_id),
            SnapshotBacking::Sharded(_) => self.shard_read("entity_id_by_stable_id", |s| {
                s.entity_id_by_stable_id(stable_id)
            }),
        }
    }

    pub(super) fn stable_symbol_keys(&self) -> Vec<SymbolKey> {
        match &self.backing {
            SnapshotBacking::InMemory(index) => {
                index.symbol_key_to_entity.read().keys().cloned().collect()
            }
            SnapshotBacking::Sharded(_) => {
                self.shard_read("stable_symbol_keys", |s| s.stable_symbol_keys(None))
            }
        }
    }

    pub(super) fn stable_symbol_keys_in_files(&self, files: &HashSet<String>) -> Vec<SymbolKey> {
        match &self.backing {
            SnapshotBacking::InMemory(index) => index.symbol_keys_in_files(files),
            SnapshotBacking::Sharded(_) => {
                self.shard_read("symbol_keys_in_files", |s| s.symbol_keys_in_files(files))
            }
        }
    }

    pub(super) fn max_entity_id(&self) -> u64 {
        match &self.backing {
            SnapshotBacking::InMemory(index) => index.max_entity_id(),
            SnapshotBacking::Sharded(_) => self.shard_read("max_entity_id", |s| s.max_entity_id()),
        }
    }

    pub(super) fn dependency_files(&self) -> Vec<String> {
        match &self.backing {
            SnapshotBacking::InMemory(index) => index.dependency_graph.get_all_files(),
            SnapshotBacking::Sharded(_) => self.shard_read("dependency_files", |s| Ok(s.files())),
        }
    }

    pub(super) fn dependencies_of(&self, source: &str) -> Vec<String> {
        match &self.backing {
            SnapshotBacking::InMemory(index) => index.dependency_graph.get_dependencies(source),
            SnapshotBacking::Sharded(_) => {
                self.shard_read("dependencies_of", |s| s.dependencies_of(source))
            }
        }
    }

    pub(super) fn dependents_of(&self, file: &str) -> Vec<String> {
        match &self.backing {
            SnapshotBacking::InMemory(index) => index.dependency_graph.get_dependents(file),
            SnapshotBacking::Sharded(_) => {
                self.shard_read("dependents_of", |s| s.dependents_of(file))
            }
        }
    }

    pub(super) fn for_each_file_record<F: FnMut(&str, &FileRecord)>(&self, mut visit: F) {
        match &self.backing {
            SnapshotBacking::InMemory(index) => {
                for (path, record) in index.file_records.read().iter() {
                    visit(path, record);
                }
            }
            SnapshotBacking::Sharded(_) => self.shard_read("for_each_file_record", |s| {
                for shard_id in s.all_shard_ids() {
                    let shard = s.load_shard(shard_id)?;
                    for (path, record) in shard.file_records.read().iter() {
                        visit(path, record);
                    }
                }
                Ok(())
            }),
        }
    }

    pub(super) fn for_each_entity<F: FnMut(EntityId, &Entity)>(&self, mut visit: F) {
        match &self.backing {
            SnapshotBacking::InMemory(index) => {
                for entry in index.function_index.iter() {
                    visit(*entry.key(), entry.value());
                }
            }
            SnapshotBacking::Sharded(_) => self.shard_read("for_each_entity", |s| {
                for shard_id in s.all_shard_ids() {
                    let shard = s.load_shard(shard_id)?;
                    for entry in shard.function_index.iter() {
                        visit(*entry.key(), entry.value());
                    }
                }
                Ok(())
            }),
        }
    }

    pub(super) fn for_each_relation_group<F: FnMut(EntityId, &[ResolvedRelation])>(
        &self,
        mut visit: F,
    ) {
        match &self.backing {
            SnapshotBacking::InMemory(index) => {
                for entry in index.resolved_relation_index.iter() {
                    visit(*entry.key(), &entry.value().edges);
                }
            }
            SnapshotBacking::Sharded(_) => self.shard_read("for_each_relation_group", |s| {
                for shard_id in s.all_shard_ids() {
                    let shard = s.load_shard(shard_id)?;
                    for entry in shard.resolved_relation_index.iter() {
                        visit(*entry.key(), &entry.value().edges);
                    }
                }
                Ok(())
            }),
        }
    }

    pub(super) fn for_each_file_relation<F: FnMut(&str, &[ResolvedRelation])>(&self, mut visit: F) {
        match &self.backing {
            SnapshotBacking::InMemory(index) => {
                for entry in index.file_relation_index.iter() {
                    visit(entry.key(), &entry.value().edges);
                }
            }
            SnapshotBacking::Sharded(_) => self.shard_read("for_each_file_relation", |s| {
                for shard_id in s.all_shard_ids() {
                    let shard = s.load_shard(shard_id)?;
                    for entry in shard.file_relation_index.iter() {
                        visit(entry.key(), &entry.value().edges);
                    }
                }
                Ok(())
            }),
        }
    }

    pub(super) fn for_each_dependency<F: FnMut(&str, &str)>(&self, mut visit: F) {
        match &self.backing {
            SnapshotBacking::InMemory(index) => {
                for file in index.dependency_graph.get_all_files() {
                    for target in index.dependency_graph.get_dependencies(&file) {
                        visit(&file, &target);
                    }
                }
            }
            SnapshotBacking::Sharded(sharded) => {
                let pairs = sharded
                    .all_dependencies()
                    .expect("sharded relation dependency scan failed");
                for (source, target) in &pairs {
                    visit(source, target);
                }
            }
        }
    }

    pub(super) fn contains_function(&self, id: EntityId) -> bool {
        match &self.backing {
            SnapshotBacking::InMemory(index) => index.function_index.contains_key(&id),
            SnapshotBacking::Sharded(_) => self.shard_read("contains_function", |s| {
                Ok(s.entity_location_by_id(id)?.is_some())
            }),
        }
    }

    pub(super) fn entity_key_or_derived(&self, id: EntityId) -> SymbolKey {
        if let Some(key) = self.symbol_key_of(id) {
            return key;
        }
        self.diagnostics()
            .entity_derived_key_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let file = self.file_path_of(id).unwrap_or_default();
        self.get_function(id).map_or_else(
            || {
                SymbolKey::new(
                    &file,
                    "<unknown>",
                    cce_types::EntityKind::Unknown,
                    "<unknown>",
                )
            },
            |entity| SymbolKey::for_entity(&file, &entity.name, &entity),
        )
    }

    pub(super) fn relation_key_or_derived(&self, id: EntityId, name_fallback: &str) -> SymbolKey {
        if let Some(key) = self.symbol_key_of(id) {
            return key;
        }
        self.diagnostics()
            .relation_derived_key_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let file = self.file_path_of(id).unwrap_or_default();
        self.get_function(id).map_or_else(
            || {
                SymbolKey::new(
                    &file,
                    name_fallback,
                    cce_types::EntityKind::Function,
                    name_fallback,
                )
            },
            |entity| SymbolKey::for_entity(&file, &entity.name, &entity),
        )
    }
}

fn transitive_walk<F>(file: &str, max_depth: usize, neighbors: F) -> Vec<String>
where
    F: Fn(&str) -> Vec<String>,
{
    let mut result = HashSet::new();
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    for dependent in neighbors(file) {
        if visited.insert(dependent.clone()) {
            queue.push_back((dependent, 1));
        }
    }
    while let Some((current, depth)) = queue.pop_front() {
        if max_depth > 0 && depth > max_depth {
            continue;
        }
        result.insert(current.clone());
        for next in neighbors(&current) {
            if visited.insert(next.clone()) {
                queue.push_back((next, depth + 1));
            }
        }
    }
    result.into_iter().collect()
}

/// Recommended upper bound for a [`LayeredSnapshotIndex`] delta chain.
pub const LAYERED_CHAIN_COMPACTION_THRESHOLD: usize = 20;

/// Immutable base snapshot overlaid with an ordered chain of incremental deltas.
#[derive(Debug, Clone)]
pub struct LayeredSnapshotIndex {
    pub base: Arc<RelationSnapshotIndex>,
    pub deltas: Vec<Arc<cce_types::SnapshotDelta>>,
}

impl LayeredSnapshotIndex {
    pub fn new(base: Arc<RelationSnapshotIndex>) -> Self {
        Self {
            base,
            deltas: Vec::new(),
        }
    }

    pub fn with_delta(
        base: Arc<RelationSnapshotIndex>,
        delta: Arc<cce_types::SnapshotDelta>,
    ) -> Self {
        Self {
            base,
            deltas: vec![delta],
        }
    }

    pub fn with_deltas(
        base: Arc<RelationSnapshotIndex>,
        deltas: Vec<Arc<cce_types::SnapshotDelta>>,
    ) -> Self {
        if deltas.len() > LAYERED_CHAIN_COMPACTION_THRESHOLD {
            tracing::warn!(
                chain_len = deltas.len(),
                threshold = LAYERED_CHAIN_COMPACTION_THRESHOLD,
                "layered delta chain exceeds the compaction threshold; compact into a fresh base snapshot"
            );
        }
        Self { base, deltas }
    }

    pub fn chain_len(&self) -> usize {
        self.deltas.len()
    }

    pub fn should_compact(&self) -> bool {
        self.deltas.len() > LAYERED_CHAIN_COMPACTION_THRESHOLD
    }

    pub fn empty() -> Self {
        Self::new(Arc::new(RelationSnapshotIndex::empty()))
    }

    pub fn function_count(&self) -> usize {
        let mut count = self.base.function_count() as i64;
        for delta in &self.deltas {
            count += delta.added_entities.len() as i64;
            count -= delta.removed_entities.len() as i64;
        }
        count.max(0) as usize
    }

    pub fn resolved_relation_count(&self) -> usize {
        let mut count = self.base.resolved_relation_count() as i64;
        for delta in &self.deltas {
            let owned_by_removed: usize = delta
                .removed_entities
                .iter()
                .filter_map(|id| self.base.relations_of(*id))
                .map(|relations| relations.len())
                .sum();
            count -= owned_by_removed as i64;
            count -= delta.removed_relations.len() as i64;
            count += delta.added_relations.len() as i64;
        }
        count.max(0) as usize
    }

    pub fn contains_entity(&self, id: EntityId) -> bool {
        for delta in self.deltas.iter().rev() {
            if delta.removed_entities.contains(&id) {
                return false;
            }
            if delta
                .added_entities
                .iter()
                .any(|added| added.entity.id == id)
            {
                return true;
            }
        }
        self.base.contains_function(id)
    }

    pub(crate) fn is_file_active(&self, file_id: &str) -> bool {
        let mut active = self.base.contains_file(file_id);
        for delta in &self.deltas {
            if delta.removed_files.iter().any(|file| file == file_id) {
                active = false;
            }
            if delta
                .added_files
                .iter()
                .any(|file| file.path == file_id || file.id == file_id)
            {
                active = true;
            }
        }
        active
    }

    pub fn compute_fingerprint(&self) -> String {
        if let Ok(snapshot) = self.to_canonical_snapshot(String::new()) {
            return snapshot.fingerprint();
        }
        let merged = self.materialize_merged_index();
        let files = active_file_set(&merged.file_records, &merged.entity_file_index);
        merged.fingerprint_in_files(&files)
    }

    pub fn fingerprint_in_files(&self, files: &HashSet<String>) -> String {
        let Ok(mut snapshot) = self.to_canonical_snapshot(String::new()) else {
            let merged = self.materialize_merged_index();
            return fingerprint_in_files_from_maps(
                &merged.function_index,
                &merged.entity_file_index,
                &merged.entity_to_symbol_key,
                &merged.resolved_relation_index,
                &merged.file_relation_index,
                &merged.file_records,
                &merged.dependency_graph,
                files,
            );
        };
        snapshot.files.retain(|file| files.contains(&file.path));
        snapshot
            .entities
            .retain(|entity| files.contains(&entity.key.file_path));
        snapshot
            .relations
            .retain(|relation| files.contains(&relation.caller.file_path));
        snapshot.dependencies.retain(|dependency| {
            files.contains(&dependency.source_file) && files.contains(&dependency.target_file)
        });
        snapshot.fingerprint()
    }

    pub fn quality_report(&self) -> QualityReport {
        self.base.quality_report()
    }
}
