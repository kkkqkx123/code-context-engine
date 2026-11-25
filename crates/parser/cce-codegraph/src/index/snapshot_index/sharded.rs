//! Bounded sharded backing for the published relation snapshot facade.

use std::collections::{HashMap, HashSet, VecDeque};
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use cce_types::relation::snapshot_shard::external_call_type_json;
use cce_types::{
    Entity, EntityId, ExternalCallType, FileInfo, ImportTable, RelationSnapshotManifest,
    ResolvedRelation, ShardDirectory, ShardLocation, ShardQueryConfig, StableSymbolKey,
    StorageError,
};
use lru::LruCache;
use parking_lot::Mutex;

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

/// Bounded cache of store-backed routing lookups.
///
/// The directory itself only carries shard descriptors and aggregate counts.
/// Per-entity, per-name, per-file and per-callee routes are read from the
/// store on demand and retained only up to the configured entry budget.
#[derive(Debug)]
struct RouteCache {
    entity_by_id: LruCache<u64, Option<ShardLocation>>,
    entity_by_symbol: LruCache<StableSymbolKey, Option<ShardLocation>>,
    entity_by_stable_id: LruCache<String, Option<ShardLocation>>,
    name_shards: LruCache<String, Vec<i64>>,
    file_shards: LruCache<String, Option<i64>>,
    caller_shards: LruCache<u64, Vec<i64>>,
    classification_shards: LruCache<String, Vec<i64>>,
    dependencies: LruCache<String, Vec<String>>,
    dependents: LruCache<String, Vec<String>>,
    files: Option<Vec<String>>,
    classification_counts: Option<HashMap<ExternalCallType, usize>>,
    max_entity_id: Option<u64>,
}

impl RouteCache {
    fn new(capacity: usize) -> Self {
        let capacity = NonZeroUsize::new(capacity).expect("route cache capacity is validated");
        Self {
            entity_by_id: LruCache::new(capacity),
            entity_by_symbol: LruCache::new(capacity),
            entity_by_stable_id: LruCache::new(capacity),
            name_shards: LruCache::new(capacity),
            file_shards: LruCache::new(capacity),
            caller_shards: LruCache::new(capacity),
            classification_shards: LruCache::new(capacity),
            dependencies: LruCache::new(capacity),
            dependents: LruCache::new(capacity),
            files: None,
            classification_counts: None,
            max_entity_id: None,
        }
    }

    fn entries(&self) -> usize {
        self.entity_by_id.len()
            + self.entity_by_symbol.len()
            + self.entity_by_stable_id.len()
            + self.name_shards.len()
            + self.file_shards.len()
            + self.caller_shards.len()
            + self.classification_shards.len()
            + self.dependencies.len()
            + self.dependents.len()
    }
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
    routes: Mutex<RouteCache>,
    route_cache_hits: AtomicU64,
    route_store_lookups: AtomicU64,
    pub(super) diagnostics: Arc<RelationDiagnostics>,
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
        Ok(Self {
            store,
            manifest,
            base_epoch,
            directory,
            routes: Mutex::new(RouteCache::new(config.route_cache_entries)),
            config,
            lru: Mutex::new(LruState::default()),
            route_cache_hits: AtomicU64::new(0),
            route_store_lookups: AtomicU64::new(0),
            diagnostics: Arc::new(RelationDiagnostics::new()),
        })
    }

    pub(super) fn manifest(&self) -> &RelationSnapshotManifest {
        &self.manifest
    }

    pub(super) fn capacity_report(&self) -> super::ShardCapacityReport {
        let lru = self.lru.lock();
        let routes = self.routes.lock();
        super::ShardCapacityReport {
            route_cache_entries: routes.entries(),
            route_cache_hits: self.route_cache_hits.load(Ordering::Relaxed),
            route_store_lookups: self.route_store_lookups.load(Ordering::Relaxed),
            shard_count: self.directory.descriptors.len(),
            directory_entries: self.directory.descriptors.len() + routes.entries(),
            directory_estimated_bytes: self.directory.estimated_bytes(),
            loaded_shards: lru.shards.len(),
            loaded_bytes: lru.bytes,
            lru_hits: lru.hits,
            lru_evictions: lru.evictions,
            max_loaded_shards: lru.max_loaded,
            max_shards_per_query: self.config.shard_query_shards,
        }
    }

    fn project_id(&self) -> i64 {
        self.manifest.project_id
    }

    pub(crate) fn entity_location_by_id(
        &self,
        id: EntityId,
    ) -> Result<Option<ShardLocation>, StorageError> {
        if let Some(cached) = self.routes.lock().entity_by_id.get(&id.0).cloned() {
            self.route_cache_hits.fetch_add(1, Ordering::Relaxed);
            return Ok(cached);
        }
        self.route_store_lookups.fetch_add(1, Ordering::Relaxed);
        let location =
            self.store
                .entity_location_by_id(self.project_id(), self.base_epoch, id.0)?;
        self.routes.lock().entity_by_id.put(id.0, location.clone());
        Ok(location)
    }

    pub(super) fn entity_location_by_symbol(
        &self,
        key: &SymbolKey,
    ) -> Result<Option<ShardLocation>, StorageError> {
        let stable = key.clone();
        if let Some(cached) = self.routes.lock().entity_by_symbol.get(&stable).cloned() {
            self.route_cache_hits.fetch_add(1, Ordering::Relaxed);
            return Ok(cached);
        }
        self.route_store_lookups.fetch_add(1, Ordering::Relaxed);
        let location =
            self.store
                .entity_location_by_symbol(self.project_id(), self.base_epoch, &stable)?;
        self.routes
            .lock()
            .entity_by_symbol
            .put(stable, location.clone());
        Ok(location)
    }

    pub(super) fn entity_location_by_stable_id(
        &self,
        stable_id: &str,
    ) -> Result<Option<ShardLocation>, StorageError> {
        if let Some(cached) = self
            .routes
            .lock()
            .entity_by_stable_id
            .get(stable_id)
            .cloned()
        {
            self.route_cache_hits.fetch_add(1, Ordering::Relaxed);
            return Ok(cached);
        }
        self.route_store_lookups.fetch_add(1, Ordering::Relaxed);
        let location = self.store.entity_location_by_stable_id(
            self.project_id(),
            self.base_epoch,
            stable_id,
        )?;
        self.routes
            .lock()
            .entity_by_stable_id
            .put(stable_id.to_string(), location.clone());
        Ok(location)
    }

    pub(super) fn name_shards(&self, name: &str) -> Result<Vec<i64>, StorageError> {
        if let Some(cached) = self.routes.lock().name_shards.get(name).cloned() {
            self.route_cache_hits.fetch_add(1, Ordering::Relaxed);
            return Ok(cached);
        }
        self.route_store_lookups.fetch_add(1, Ordering::Relaxed);
        let shards = self
            .store
            .name_shards(self.project_id(), self.base_epoch, name)?;
        self.routes
            .lock()
            .name_shards
            .put(name.to_string(), shards.clone());
        Ok(shards)
    }

    pub(super) fn file_shard(&self, path: &str) -> Result<Option<i64>, StorageError> {
        if let Some(cached) = self.routes.lock().file_shards.get(path).cloned() {
            self.route_cache_hits.fetch_add(1, Ordering::Relaxed);
            return Ok(cached);
        }
        self.route_store_lookups.fetch_add(1, Ordering::Relaxed);
        let shard = self
            .store
            .file_shard(self.project_id(), self.base_epoch, path)?;
        self.routes.lock().file_shards.put(path.to_string(), shard);
        Ok(shard)
    }

    pub(super) fn caller_shards(&self, callee: EntityId) -> Result<Vec<i64>, StorageError> {
        if let Some(cached) = self.routes.lock().caller_shards.get(&callee.0).cloned() {
            self.route_cache_hits.fetch_add(1, Ordering::Relaxed);
            return Ok(cached);
        }
        self.route_store_lookups.fetch_add(1, Ordering::Relaxed);
        let shards = self
            .store
            .caller_shards(self.project_id(), self.base_epoch, callee.0)?;
        self.routes
            .lock()
            .caller_shards
            .put(callee.0, shards.clone());
        Ok(shards)
    }

    pub(super) fn classification_shards(
        &self,
        classification: &ExternalCallType,
    ) -> Result<Vec<i64>, StorageError> {
        let key = external_call_type_json(classification);
        if let Some(cached) = self.routes.lock().classification_shards.get(&key).cloned() {
            return Ok(cached);
        }
        self.route_store_lookups.fetch_add(1, Ordering::Relaxed);
        let shards =
            self.store
                .classification_shards(self.project_id(), self.base_epoch, classification)?;
        self.routes
            .lock()
            .classification_shards
            .put(key, shards.clone());
        Ok(shards)
    }

    pub(super) fn classification_stats(&self) -> HashMap<ExternalCallType, usize> {
        let mut routes = self.routes.lock();
        if let Some(cached) = &routes.classification_counts {
            return cached.clone();
        }
        let counts = self
            .store
            .classification_counts(self.project_id(), self.base_epoch)
            .unwrap_or_default();
        routes.classification_counts = Some(counts.clone());
        counts
    }

    pub(super) fn stable_symbol_keys(
        &self,
        files: Option<&HashSet<String>>,
    ) -> Result<Vec<SymbolKey>, StorageError> {
        let stable = self
            .store
            .stable_symbol_keys(self.project_id(), self.base_epoch, files)?;
        Ok(stable)
    }

    pub(super) fn files(&self) -> Vec<String> {
        let mut routes = self.routes.lock();
        if let Some(cached) = &routes.files {
            return cached.clone();
        }
        let files = self
            .store
            .directory_files(self.project_id(), self.base_epoch)
            .unwrap_or_default();
        routes.files = Some(files.clone());
        files
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
            .read_shard_components(self.project_id(), self.base_epoch, shard_id)?
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

    pub(super) fn shard_ids_for_entity(
        &self,
        id: EntityId,
    ) -> Result<Option<Vec<i64>>, StorageError> {
        Ok(self
            .entity_location_by_id(id)?
            .map(|location| vec![location.shard_id]))
    }

    pub(super) fn shard_ids_for_file(&self, path: &str) -> Result<Vec<i64>, StorageError> {
        Ok(self.file_shard(path)?.into_iter().collect())
    }

    pub(super) fn classification_shards_for_query(
        &self,
        classification: &ExternalCallType,
    ) -> Result<Vec<i64>, StorageError> {
        self.classification_shards(classification)
    }

    pub(super) fn file_relations_of(
        &self,
        path: &str,
    ) -> Result<Vec<ResolvedRelation>, StorageError> {
        let shards = self.shard_ids_for_file(path)?;
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
        let Some(shards) = self.shard_ids_for_entity(caller)? else {
            return Ok(None);
        };
        let shards = self.load_shards(&shards)?;
        Ok(shards.first().and_then(|shard| shard.relations_of(caller)))
    }

    pub(super) fn relations_to_entity(
        &self,
        callee: EntityId,
    ) -> Result<Vec<ResolvedRelation>, StorageError> {
        let shards = self.caller_shards(callee)?;
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
        let shards = self.caller_shards(callee)?;
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
        let Some(shards) = self.shard_ids_for_entity(id)? else {
            return Ok(None);
        };
        let shards = self.load_shards(&shards)?;
        Ok(shards.iter().find_map(|shard| shard.get_function(id)))
    }

    pub(super) fn file_path_of(&self, id: EntityId) -> Result<Option<String>, StorageError> {
        Ok(self
            .entity_location_by_id(id)?
            .map(|location| location.file_path))
    }

    pub(super) fn file_callers_of(&self, callee: EntityId) -> Result<Vec<String>, StorageError> {
        let shards = self.caller_shards(callee)?;
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
        let shards = self.shard_ids_for_file(path)?;
        let shards = self.load_shards(&shards)?;
        Ok(shards.iter().find_map(|shard| shard.get_file_info(path)))
    }

    pub(super) fn get_exports(&self, path: &str) -> Result<Option<Vec<ExportInfo>>, StorageError> {
        let shards = self.shard_ids_for_file(path)?;
        let shards = self.load_shards(&shards)?;
        Ok(shards.iter().find_map(|shard| shard.get_exports(path)))
    }

    pub(super) fn get_import_table(&self, path: &str) -> Result<Option<ImportTable>, StorageError> {
        let shards = self.shard_ids_for_file(path)?;
        let shards = self.load_shards(&shards)?;
        Ok(shards.iter().find_map(|shard| shard.get_import_table(path)))
    }

    pub(super) fn entity_ids_of_file(&self, path: &str) -> Result<Vec<EntityId>, StorageError> {
        let shards = self.shard_ids_for_file(path)?;
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
        let shards = self.shard_ids_for_file(path)?;
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
        self.directory.counts().0
    }

    pub(super) fn max_entity_id(&self) -> Result<u64, StorageError> {
        {
            let routes = self.routes.lock();
            if let Some(cached) = routes.max_entity_id {
                return Ok(cached);
            }
        }
        let max = self
            .store
            .max_entity_id(self.project_id(), self.base_epoch)?;
        self.routes.lock().max_entity_id = Some(max);
        Ok(max)
    }

    pub(super) fn dependencies_of(&self, source: &str) -> Result<Vec<String>, StorageError> {
        if let Some(cached) = self.routes.lock().dependencies.get(source).cloned() {
            return Ok(cached);
        }
        self.route_store_lookups.fetch_add(1, Ordering::Relaxed);
        let targets = self
            .store
            .dependencies_of(self.project_id(), self.base_epoch, source)?;
        self.routes
            .lock()
            .dependencies
            .put(source.to_string(), targets.clone());
        Ok(targets)
    }

    pub(super) fn dependents_of(&self, file: &str) -> Result<Vec<String>, StorageError> {
        if let Some(cached) = self.routes.lock().dependents.get(file).cloned() {
            return Ok(cached);
        }
        self.route_store_lookups.fetch_add(1, Ordering::Relaxed);
        let sources = self
            .store
            .dependents_of(self.project_id(), self.base_epoch, file)?;
        self.routes
            .lock()
            .dependents
            .put(file.to_string(), sources.clone());
        Ok(sources)
    }

    pub(super) fn all_dependencies(&self) -> Result<Vec<(String, String)>, StorageError> {
        self.store
            .all_dependencies(self.project_id(), self.base_epoch)
    }

    pub(super) fn symbol_key_of(&self, id: EntityId) -> Result<Option<SymbolKey>, StorageError> {
        self.store
            .symbol_key_by_entity_id(self.project_id(), self.base_epoch, id.0)
    }

    pub(super) fn entity_id_by_symbol_key(
        &self,
        key: &SymbolKey,
    ) -> Result<Option<EntityId>, StorageError> {
        Ok(self
            .entity_location_by_symbol(key)?
            .map(|location| EntityId(location.entity_id)))
    }

    pub(super) fn entity_id_by_stable_id(
        &self,
        stable_id: &str,
    ) -> Result<Option<EntityId>, StorageError> {
        Ok(self
            .entity_location_by_stable_id(stable_id)?
            .map(|location| EntityId(location.entity_id)))
    }

    pub(super) fn symbol_keys_in_files(
        &self,
        files: &HashSet<String>,
    ) -> Result<Vec<SymbolKey>, StorageError> {
        self.stable_symbol_keys(Some(files))
    }
}
