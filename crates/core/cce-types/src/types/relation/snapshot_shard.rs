//! Query-side shard directory and component types for canonical snapshots.
//!
//! The directory is intentionally small: it carries IDs, counts and routing
//! maps, never entity payloads. Components are read on demand from the
//! canonical rows of one shard and are assembled into a read-only view.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::canonical::{StableSymbolKey, stable_json};
use super::snapshot_artifact::{
    CanonicalComponentBatch, CanonicalSnapshotArtifact, DEFAULT_SNAPSHOT_BATCH_ITEMS,
};
use crate::types::relation::classification::ExternalCallType;
use crate::types::{
    CanonicalDependency, CanonicalEntity, CanonicalFile, CanonicalRelation, StorageError,
};

/// Number of stable slots before bounded splitting by item budget.
pub const SHARD_SLOT_COUNT: u64 = 1024;
/// Default item budget of one shard. Explicit configuration may override it.
pub const DEFAULT_MAX_SHARD_ITEMS: usize = 4096;
/// Default number of concurrently resident shards.
pub const DEFAULT_SHARD_LRU_SHARDS: usize = 4;
/// Default estimated-byte budget of the shard LRU.
pub const DEFAULT_SHARD_LRU_BYTES: u64 = 256 * 1024 * 1024;
/// Default upper bound on shards loaded by a single query.
pub const DEFAULT_SHARD_QUERY_SHARDS: usize = 64;
/// Default number of cached routing lookups kept alongside the shard LRU.
pub const DEFAULT_ROUTE_CACHE_ENTRIES: usize = 4096;

/// Explicit capacity controls for the sharded query index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ShardQueryConfig {
    /// Maximum entities/edges in one shard. Fixed for every project size.
    pub max_shard_items: usize,
    /// Maximum number of resident shards.
    pub shard_lru_shards: usize,
    /// Maximum estimated bytes of resident shards.
    pub shard_lru_bytes: u64,
    /// Maximum shards one query may load before failing explicitly.
    pub shard_query_shards: usize,
    /// Maximum cached route lookups. Route data is read from the store on a
    /// miss, so this cache is the only resident per-entity routing memory.
    pub route_cache_entries: usize,
}

impl Default for ShardQueryConfig {
    fn default() -> Self {
        Self {
            max_shard_items: DEFAULT_MAX_SHARD_ITEMS,
            shard_lru_shards: DEFAULT_SHARD_LRU_SHARDS,
            shard_lru_bytes: DEFAULT_SHARD_LRU_BYTES,
            shard_query_shards: DEFAULT_SHARD_QUERY_SHARDS,
            route_cache_entries: DEFAULT_ROUTE_CACHE_ENTRIES,
        }
    }
}

impl ShardQueryConfig {
    pub fn validate(&self) -> Result<(), StorageError> {
        if self.max_shard_items == 0
            || self.shard_lru_shards == 0
            || self.shard_lru_bytes == 0
            || self.shard_query_shards == 0
            || self.route_cache_entries == 0
        {
            return Err(StorageError::validation(
                "shard capacity parameters must be greater than zero",
            ));
        }
        Ok(())
    }
}

/// Stable slot of a normalized project path.
pub fn shard_slot(path: &str) -> u64 {
    let digest = Sha256::digest(path.as_bytes());
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&digest[..8]);
    u64::from_le_bytes(bytes) % SHARD_SLOT_COUNT
}

/// One persisted shard and its canonical counts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationShardDescriptor {
    pub shard_id: i64,
    pub file_count: usize,
    pub entity_count: usize,
    pub relation_count: usize,
    pub dependency_count: usize,
    pub estimated_bytes: u64,
    pub path_start: String,
    pub path_end: String,
}

impl RelationShardDescriptor {
    pub fn estimated_bytes(
        file_count: usize,
        entity_count: usize,
        relation_count: usize,
        dependency_count: usize,
    ) -> u64 {
        (file_count as u64) * 256
            + (entity_count as u64) * 512
            + (relation_count as u64) * 640
            + (dependency_count as u64) * 128
    }
}

/// Routing target of an entity in the shard directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShardLocation {
    pub shard_id: i64,
    pub entity_id: u64,
    pub file_path: String,
    pub name: String,
}

/// Lightweight query directory of one canonical epoch.
#[derive(Debug, Clone, Default)]
pub struct ShardDirectory {
    pub descriptors: Vec<RelationShardDescriptor>,
    pub entity_locations: HashMap<u64, ShardLocation>,
    pub symbol_locations: HashMap<StableSymbolKey, ShardLocation>,
    pub stable_id_locations: HashMap<String, ShardLocation>,
    pub name_locations: HashMap<String, Vec<i64>>,
    pub file_shards: HashMap<String, i64>,
    /// Internal target entity ID -> shard IDs of its callers.
    pub target_callers: HashMap<u64, Vec<i64>>,
    /// External classification -> shard IDs containing such edges.
    pub classification_shards: HashMap<ExternalCallType, Vec<i64>>,
    /// External classification -> edge count, aggregated in storage.
    pub classification_counts: HashMap<ExternalCallType, usize>,
    pub dependencies: HashMap<String, Vec<String>>,
    pub dependents: HashMap<String, Vec<String>>,
}

impl ShardDirectory {
    pub fn estimated_bytes(&self) -> u64 {
        self.descriptors
            .iter()
            .map(|descriptor| descriptor.estimated_bytes)
            .sum()
    }

    pub fn counts(&self) -> (usize, usize, usize, usize) {
        self.descriptors.iter().fold(
            (0, 0, 0, 0),
            |(files, entities, relations, dependencies), descriptor| {
                (
                    files + descriptor.file_count,
                    entities + descriptor.entity_count,
                    relations + descriptor.relation_count,
                    dependencies + descriptor.dependency_count,
                )
            },
        )
    }

    pub fn stable_symbol_keys(&self, files: Option<&HashSet<String>>) -> Vec<StableSymbolKey> {
        self.symbol_locations
            .keys()
            .filter(|key| {
                files.is_none_or(|selected| {
                    selected.contains(&key.file_path) || selected.contains(key.file_path.as_str())
                })
            })
            .cloned()
            .collect()
    }
}

/// An entity of another shard referenced by a relation inside this shard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShardEntityRef {
    pub key: StableSymbolKey,
    pub entity_id: u64,
    pub file_path: String,
}

/// Canonical rows of one shard plus the cross-shard target identities needed
/// to preserve internal relation targets without loading other shards.
#[derive(Debug, Clone)]
pub struct RelationShardComponents {
    pub files: Vec<CanonicalFile>,
    pub entities: Vec<CanonicalEntity>,
    pub relations: Vec<CanonicalRelation>,
    pub dependencies: Vec<CanonicalDependency>,
    pub target_entities: Vec<ShardEntityRef>,
}

/// Deterministic shard assignment computed from an artifact without retaining
/// canonical component payloads.
#[derive(Debug, Clone)]
pub struct SnapshotShardPlan {
    pub path_shards: HashMap<String, i64>,
    pub descriptors: Vec<RelationShardDescriptor>,
}

impl SnapshotShardPlan {
    pub fn shard_of(&self, path: &str) -> Result<i64, StorageError> {
        self.path_shards.get(path).copied().ok_or_else(|| {
            StorageError::validation(format!("snapshot shard for {path} was not planned"))
        })
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct PathCounts {
    files: usize,
    entities: usize,
    relations: usize,
    file_relations: usize,
    dependencies: usize,
}

impl PathCounts {
    fn budget(self) -> usize {
        self.files + self.entities + self.relations + self.file_relations + self.dependencies
    }
}

/// Stream an artifact twice to derive bounded shard assignments.
///
/// Paths are ordered by stable slot then path, so the plan is reproducible
/// across machines and independent of the artifact's row order. A slot itself
/// never grows past `max_shard_items`: overflow appends the next shard ID.
pub fn plan_snapshot_shards(
    artifact: &CanonicalSnapshotArtifact,
    max_shard_items: usize,
) -> Result<SnapshotShardPlan, StorageError> {
    if max_shard_items == 0 {
        return Err(StorageError::validation(
            "max_shard_items must be greater than zero",
        ));
    }
    let mut counts: HashMap<String, PathCounts> = HashMap::new();
    for batch in artifact.batches(DEFAULT_SNAPSHOT_BATCH_ITEMS) {
        match batch.map_err(|error| StorageError::transaction(error.to_string()))? {
            CanonicalComponentBatch::Files(files) => {
                for file in files {
                    counts.entry(file.path).or_default().files += 1;
                }
            }
            CanonicalComponentBatch::Entities(entities) => {
                for entity in entities {
                    counts.entry(entity.key.file_path).or_default().entities += 1;
                }
            }
            CanonicalComponentBatch::Relations(relations) => {
                for relation in relations {
                    let is_file_caller = relation.caller.is_file_placeholder();
                    let entry = counts.entry(relation.caller.file_path).or_default();
                    if is_file_caller {
                        entry.file_relations += 1;
                    } else {
                        entry.relations += 1;
                    }
                }
            }
            CanonicalComponentBatch::Dependencies(dependencies) => {
                for dependency in dependencies {
                    counts
                        .entry(dependency.source_file)
                        .or_default()
                        .dependencies += 1;
                }
            }
        }
    }

    let mut paths: Vec<String> = counts.keys().cloned().collect();
    paths.sort_by(|left, right| {
        shard_slot(left)
            .cmp(&shard_slot(right))
            .then_with(|| left.cmp(right))
    });

    let mut plan = SnapshotShardPlan {
        path_shards: HashMap::with_capacity(paths.len()),
        descriptors: Vec::new(),
    };
    let mut shard_id = 0i64;
    let mut used = 0usize;
    let mut current = RelationShardDescriptor {
        shard_id,
        file_count: 0,
        entity_count: 0,
        relation_count: 0,
        dependency_count: 0,
        estimated_bytes: 0,
        path_start: String::new(),
        path_end: String::new(),
    };
    for path in paths {
        let count = counts
            .get(&path)
            .copied()
            .ok_or_else(|| StorageError::validation("snapshot shard plan lost a path"))?;
        let cost = count.budget();
        if used > 0 && used.saturating_add(cost) > max_shard_items {
            current.estimated_bytes = RelationShardDescriptor::estimated_bytes(
                current.file_count,
                current.entity_count,
                current.relation_count,
                current.dependency_count,
            );
            plan.descriptors.push(std::mem::replace(
                &mut current,
                RelationShardDescriptor {
                    shard_id: shard_id + 1,
                    file_count: 0,
                    entity_count: 0,
                    relation_count: 0,
                    dependency_count: 0,
                    estimated_bytes: 0,
                    path_start: String::new(),
                    path_end: String::new(),
                },
            ));
            shard_id += 1;
            used = 0;
        }
        if current.file_count == 0
            && current.entity_count == 0
            && current.relation_count == 0
            && current.dependency_count == 0
        {
            current.path_start = path.clone();
        }
        current.path_end = path.clone();
        current.file_count += count.files;
        current.entity_count += count.entities;
        current.relation_count += count.relations;
        current.dependency_count += count.dependencies;
        used += cost;
        plan.path_shards.insert(path, shard_id);
    }
    if used > 0 || !plan.path_shards.is_empty() {
        current.estimated_bytes = RelationShardDescriptor::estimated_bytes(
            current.file_count,
            current.entity_count,
            current.relation_count,
            current.dependency_count,
        );
        plan.descriptors.push(current);
    }
    Ok(plan)
}

/// JSON encoding shared by storage-backed classification aggregations.
pub fn external_call_type_json(value: &ExternalCallType) -> String {
    stable_json(value)
}
