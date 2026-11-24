//! Bounded sharded backing for the published relation snapshot facade.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use cce_types::{
    Entity, EntityId, ExternalCallType, FileInfo, ImportTable, RelationSnapshotManifest,
    ResolvedRelation, ShardDirectory, ShardQueryConfig, StorageError,
};
use parking_lot::{Mutex, RwLock};

use crate::index::core::SymbolKey;
use crate::index::snapshot_loader::RelationSnapshotLoader;
use crate::index::stores::diagnostics::RelationDiagnostics;
use crate::types::ExportInfo;

use super::InMemoryRelationSnapshotIndex;

type Store = Arc<dyn cce_types::RelationSnapshotStore>;

#[derive(Debug, Default)]
struct LruState {
    shards: HashMap<i64, Arc<InMemoryRelationSnapshotIndex>>,
    order: VecDeque<i64>,
    bytes: u64,
    hits: u64,
    evictions: u64,
    max_loaded: usize,
}

/// Read-only sharded backing of a canonical epoch.
///
/// Only the directory and a bounded number of assembled shards stay resident.
/// Assembly failures propagate as `StorageError`; query entry points convert
/// them into explicit failures instead of returning partial results.
pub(super) struct ShardedSnapshotIndex {
    store: Store,
    manifest: RelationSnapshotManifest,
    pub(super) base_epoch: i64,
    pub(super) directory: ShardDirectory,
    pub(super) config: ShardQueryConfig,
    lru: Mutex<LruState>,
    pub(super) diagnostics: Arc<RelationDiagnostics>,
    stable_id_to_entity: Arc<RwLock<HashMap<String, EntityId>>>,
    symbol_key_to_entity: Arc<RwLock<HashMap<SymbolKey, EntityId>>>,
    entity_to_symbol_key: Arc<RwLock<HashMap<EntityId, SymbolKey>>>,
    file_symbol_keys: Arc<RwLock<HashMap<String, Vec<SymbolKey>>>>,
}

impl std::fmt::Debug for ShardedSnapshotIndex {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ShardedSnapshotIndex")
            .field("project_id", &self.manifest.project_id)
            .field("base_epoch", &self.base_epoch)
            .field("shards", &self.directory.descriptors.len())
            .finish_non_exhaustive()
    }
}

impl ShardedSnapshotIndex {
    pub(super) fn new(
        store: Store,
        manifest: RelationSnapshotManifest,
        base_epoch: i64,
        directory: ShardDirectory,
        config: ShardQueryConfig,
    ) -> Result<Self, StorageError> {
        config.validate()?;
        let symbol_key_to_entity: HashMap<SymbolKey, EntityId> = directory
            .symbol_locations
            .iter()
            .map(|(key, location)| (key.clone(), EntityId(location.entity_id)))
            .collect();
        let entity_to_symbol_key: HashMap<EntityId, SymbolKey> = symbol_key_to_entity
            .iter()
            .map(|(key, id)| (*id, key.clone()))
            .collect();
        let mut file_symbol_keys: HashMap<String, Vec<SymbolKey>> = HashMap::new();
        for key in symbol_key_to_entity.keys() {
            file_symbol_keys
                .entry(key.file_path.clone())
                .or_default()
                .push(key.clone());
        }
        let stable_id_map: HashMap<String, EntityId> = directory
            .stable_id_locations
            .iter()
            .map(|(stable_id, location)| (stable_id.clone(), EntityId(location.entity_id)))
            .collect();

        Ok(Self {
            store,
            manifest,
            base_epoch,
            directory,
            config,
            lru: Mutex::new(LruState::default()),
            diagnostics: Arc::new(RelationDiagnostics::new()),
            stable_id_to_entity: Arc::new(RwLock::new(stable_id_map)),
            symbol_key_to_entity: Arc::new(RwLock::new(symbol_key_to_entity)),
            entity_to_symbol_key: Arc::new(RwLock::new(entity_to_symbol_key)),
            file_symbol_keys: Arc::new(RwLock::new(file_symbol_keys)),
        })
    }

    pub(super) fn manifest(&self) -> &RelationSnapshotManifest {
        &self.manifest
    }

    pub(super) fn capacity_report(&self) -> super::ShardCapacityReport {
        let lru = self.lru.lock();
        super::ShardCapacityReport {
            shard_count: self.directory.descriptors.len(),
            directory_entries: self.directory.entity_locations.len()
                + self.directory.file_shards.len()
                + self.directory.target_callers.len(),
            directory_estimated_bytes: self.directory.estimated_bytes(),
            loaded_shards: lru.shards.len(),
            loaded_bytes: lru.bytes,
            lru_hits: lru.hits,
            lru_evictions: lru.evictions,
            max_loaded_shards: lru.max_loaded,
            max_shards_per_query: self.config.shard_query_shards,
        }
    }

    pub(super) fn load_shard(
        &self,
        shard_id: i64,
    ) -> Result<Arc<InMemoryRelationSnapshotIndex>, StorageError> {
        {
            let mut lru = self.lru.lock();
            let cached = lru.shards.get(&shard_id).map(Arc::clone);
            if let Some(shard) = cached {
                lru.hits += 1;
                lru.order.retain(|id| *id != shard_id);
                lru.order.push_back(shard_id);
                return Ok(shard);
            }
        }
        let exists = self
            .directory
            .descriptors
            .iter()
            .any(|d| d.shard_id == shard_id);
        if !exists {
            return Err(StorageError::validation(format!(
                "shard {shard_id} is not part of relation epoch {}",
                self.base_epoch
            )));
        }
        let components = self
            .store
            .read_shard_components(self.manifest.project_id, self.base_epoch, shard_id)?
            .ok_or_else(|| {
                StorageError::validation(format!(
                    "shard {shard_id} of relation epoch {} has no rows",
                    self.base_epoch
                ))
            })?;
        let mut index = RelationSnapshotLoader::build_shard_index(&self.manifest, &components)?;
        let shard = Arc::new(InMemoryRelationSnapshotIndex::from_index_owned(&mut index));
        let bytes = self
            .directory
            .descriptors
            .iter()
            .find(|d| d.shard_id == shard_id)
            .map(|d| d.estimated_bytes)
            .unwrap_or(0);
        let mut lru = self.lru.lock();
        lru.shards.insert(shard_id, Arc::clone(&shard));
        lru.order.retain(|id| *id != shard_id);
        lru.order.push_back(shard_id);
        lru.bytes = lru.bytes.saturating_add(bytes);
        while lru.shards.len() > self.config.shard_lru_shards
            || lru.bytes > self.config.shard_lru_bytes
        {
            let Some(evicted) = lru.order.pop_front() else {
                break;
            };
            if let Some(removed) = lru.shards.remove(&evicted) {
                let evicted_bytes = self
                    .directory
                    .descriptors
                    .iter()
                    .find(|d| d.shard_id == evicted)
                    .map(|d| d.estimated_bytes)
                    .unwrap_or(0);
                lru.bytes = lru.bytes.saturating_sub(evicted_bytes);
                lru.evictions += 1;
                drop(removed);
            }
        }
        lru.max_loaded = lru.max_loaded.max(lru.shards.len());
        Ok(shard)
    }

    pub(super) fn load_shards(
        &self,
        shard_ids: &[i64],
    ) -> Result<Vec<Arc<InMemoryRelationSnapshotIndex>>, StorageError> {
        if shard_ids.len() > self.config.shard_query_shards {
            return Err(StorageError::validation(format!(
                "query needs {} shards, limit is {}; compact the snapshot or raise max_shards_per_query",
                shard_ids.len(),
                self.config.shard_query_shards
            )));
        }
        shard_ids.iter().map(|id| self.load_shard(*id)).collect()
    }

    pub(super) fn all_shard_ids(&self) -> Vec<i64> {
        self.directory
            .descriptors
            .iter()
            .map(|descriptor| descriptor.shard_id)
            .collect()
    }

    pub(super) fn shard_ids_for_entity(&self, id: EntityId) -> Option<Vec<i64>> {
        self.directory
            .entity_locations
            .get(&id.0)
            .map(|location| vec![location.shard_id])
    }

    pub(super) fn shard_ids_for_file(&self, path: &str) -> Vec<i64> {
        self.directory
            .file_shards
            .get(path)
            .copied()
            .into_iter()
            .collect()
    }

    pub(super) fn caller_shards(&self, callee: EntityId) -> Vec<i64> {
        self.directory
            .target_callers
            .get(&callee.0)
            .cloned()
            .unwrap_or_default()
    }

    pub(super) fn classification_shards(&self, classification: &ExternalCallType) -> Vec<i64> {
        self.directory
            .classification_shards
            .get(classification)
            .cloned()
            .unwrap_or_default()
    }

    pub(super) fn classification_stats(&self) -> HashMap<ExternalCallType, usize> {
        self.directory.classification_counts.clone()
    }

    pub(super) fn stable_symbol_keys(&self, files: Option<&HashSet<String>>) -> Vec<SymbolKey> {
        self.directory.stable_symbol_keys(files)
    }

    pub(super) fn files(&self) -> Vec<String> {
        self.directory.file_shards.keys().cloned().collect()
    }

    pub(super) fn file_relations_of(
        &self,
        path: &str,
    ) -> Result<Vec<ResolvedRelation>, StorageError> {
        let shards = self.shard_ids_for_file(path);
        let shards = self.load_shards(&shards)?;
        Ok(shards
            .iter()
            .flat_map(|shard| shard.file_relations_of(path))
            .collect())
    }

    pub(super) fn relations_of(
        &self,
        caller: EntityId,
    ) -> Result<Option<Vec<ResolvedRelation>>, StorageError> {
        let Some(shards) = self.shard_ids_for_entity(caller) else {
            return Ok(None);
        };
        let shards = self.load_shards(&shards)?;
        Ok(shards.first().and_then(|shard| shard.relations_of(caller)))
    }

    pub(super) fn relations_to_entity(
        &self,
        callee: EntityId,
    ) -> Result<Vec<ResolvedRelation>, StorageError> {
        let shards = self.caller_shards(callee);
        let shards = self.load_shards(&shards)?;
        let mut result = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for shard in &shards {
            for relation in shard.relations_to_entity(callee) {
                if seen.insert(crate::index::delta::relation_identity(&relation)) {
                    result.push(relation);
                }
            }
        }
        Ok(result)
    }

    pub(super) fn callers_of(&self, callee: EntityId) -> Result<Vec<EntityId>, StorageError> {
        let shards = self.caller_shards(callee);
        let shards = self.load_shards(&shards)?;
        let mut callers = Vec::new();
        for shard in &shards {
            for caller in shard.callers_of(callee) {
                if !callers.contains(&caller) {
                    callers.push(caller);
                }
            }
        }
        callers.sort();
        Ok(callers)
    }

    pub(super) fn get_function(&self, id: EntityId) -> Result<Option<Entity>, StorageError> {
        let Some(shards) = self.shard_ids_for_entity(id) else {
            return Ok(None);
        };
        let shards = self.load_shards(&shards)?;
        Ok(shards.iter().find_map(|shard| shard.get_function(id)))
    }

    pub(super) fn file_path_of(&self, id: EntityId) -> Option<String> {
        self.directory
            .entity_locations
            .get(&id.0)
            .map(|location| location.file_path.clone())
    }

    pub(super) fn file_callers_of(&self, callee: EntityId) -> Result<Vec<String>, StorageError> {
        let shards = self.caller_shards(callee);
        let shards = self.load_shards(&shards)?;
        let mut result = Vec::new();
        for shard in &shards {
            for path in shard.file_callers_of(callee) {
                if !result.contains(&path) {
                    result.push(path);
                }
            }
        }
        Ok(result)
    }

    pub(super) fn get_file_info(&self, path: &str) -> Result<Option<FileInfo>, StorageError> {
        let shards = self.shard_ids_for_file(path);
        let shards = self.load_shards(&shards)?;
        Ok(shards.iter().find_map(|shard| shard.get_file_info(path)))
    }

    pub(super) fn get_exports(&self, path: &str) -> Result<Option<Vec<ExportInfo>>, StorageError> {
        let shards = self.shard_ids_for_file(path);
        let shards = self.load_shards(&shards)?;
        Ok(shards.iter().find_map(|shard| shard.get_exports(path)))
    }

    pub(super) fn get_import_table(&self, path: &str) -> Result<Option<ImportTable>, StorageError> {
        let shards = self.shard_ids_for_file(path);
        let shards = self.load_shards(&shards)?;
        Ok(shards.iter().find_map(|shard| shard.get_import_table(path)))
    }

    pub(super) fn entity_ids_of_file(&self, path: &str) -> Result<Vec<EntityId>, StorageError> {
        let shards = self.shard_ids_for_file(path);
        let shards = self.load_shards(&shards)?;
        Ok(shards
            .iter()
            .flat_map(|shard| shard.entity_ids_of_file(path))
            .collect())
    }

    pub(super) fn entities_in_line_range(
        &self,
        path: &str,
        start: usize,
        end: usize,
    ) -> Result<Vec<EntityId>, StorageError> {
        let shards = self.shard_ids_for_file(path);
        let shards = self.load_shards(&shards)?;
        Ok(shards
            .iter()
            .flat_map(|shard| shard.entities_in_line_range(path, start, end))
            .collect())
    }

    pub(super) fn relation_count(&self) -> usize {
        self.directory.counts().2
    }

    pub(super) fn function_count(&self) -> usize {
        self.directory.counts().1
    }

    pub(super) fn file_count(&self) -> usize {
        self.directory.file_shards.len()
    }

    pub(super) fn max_entity_id(&self) -> u64 {
        self.directory
            .entity_locations
            .keys()
            .copied()
            .max()
            .unwrap_or(0)
    }

    pub(super) fn dependencies_of(&self, source: &str) -> Vec<String> {
        self.directory
            .dependencies
            .get(source)
            .cloned()
            .unwrap_or_default()
    }

    pub(super) fn dependents_of(&self, file: &str) -> Vec<String> {
        self.directory
            .dependents
            .get(file)
            .cloned()
            .unwrap_or_default()
    }

    pub(super) fn symbol_key_of(&self, id: EntityId) -> Option<SymbolKey> {
        self.entity_to_symbol_key.read().get(&id).cloned()
    }

    pub(super) fn entity_id_by_symbol_key(&self, key: &SymbolKey) -> Option<EntityId> {
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
}
