//! Disk-backed build store for full relation builds.
//!
//! A full build keeps entity routing, file metadata and resolved edges in an
//! operation-local SQLite database. The store is written once per file group
//! and read through bounded queries during resolution; the complete relation
//! graph is never materialized in memory. Canonical artifact rows are streamed
//! directly from this store after resolution.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use cce_types::{
    CanonicalDependency, CanonicalEntity, CanonicalFile, CanonicalRelation,
    CanonicalRelationTarget, Entity, EntityId, EntityKind, FileInfo, ImportTable, ParsedFile,
    ResolvedRelation, SnapshotArtifactBuilder, SnapshotArtifactHeader, SnapshotArtifactOptions,
    SnapshotBuildMetadata, StableSymbolKey, SymbolKeyConflictRecord, UnresolvedReason,
    canonical_call_context, canonical_owner_type, normalize_project_path,
};
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use serde::de::DeserializeOwned;

use super::build_view::{BuildEntityView, BuildIndexSink};
use super::core::{ExportInfo, relation_identity};
use crate::symbol_table::candidate::SymbolCandidate;
use crate::symbol_table::{LocalSymbolStore, ModuleRecord};
use cce_types::{CanonicalSnapshotArtifact, StorageError};

/// Build-time relation store backed by a temporary SQLite database.
pub struct BuildRelationStore {
    connection: Mutex<Connection>,
    path: PathBuf,
    next_sequence: AtomicU64,
    symbol_key_conflicts: AtomicU64,
    entity_derived_keys: AtomicU64,
    relation_derived_keys: AtomicU64,
    conflict_samples: Mutex<VecDeque<SymbolKeyConflictRecord>>,
    local_symbols: Arc<LocalSymbolStore>,
}

impl BuildRelationStore {
    /// Create a fresh build database at `path`.
    pub fn new(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        if path.exists() {
            fs::remove_file(&path).map_err(|error| error.to_string())?;
        }
        let connection = Connection::open(&path).map_err(|error| error.to_string())?;
        connection
            .execute_batch(
                "PRAGMA journal_mode=MEMORY;\
                 PRAGMA synchronous=OFF;\
                 CREATE TABLE entities (\
                    id INTEGER PRIMARY KEY, file TEXT NOT NULL, name TEXT NOT NULL,\
                    seq INTEGER NOT NULL, entity_json TEXT NOT NULL);\
                 CREATE INDEX entities_by_name ON entities(name, seq);\
                 CREATE INDEX entities_by_file ON entities(file);\
                 CREATE TABLE entity_files (id INTEGER PRIMARY KEY, file TEXT NOT NULL);\
                 CREATE TABLE symbol_keys (\
                    entity_id INTEGER PRIMARY KEY, key_json TEXT NOT NULL UNIQUE);\
                 CREATE TABLE entity_remaps (file TEXT PRIMARY KEY, remap_json TEXT NOT NULL);\
                 CREATE TABLE files (path TEXT PRIMARY KEY, info_json TEXT NOT NULL);\
                 CREATE TABLE imports (path TEXT PRIMARY KEY, table_json TEXT NOT NULL);\
                 CREATE TABLE exports (path TEXT, seq INTEGER, export_json TEXT NOT NULL);\
                 CREATE TABLE dependencies (\
                    seq INTEGER PRIMARY KEY, source TEXT NOT NULL, target TEXT NOT NULL,\
                    UNIQUE(source, target));\
                 CREATE INDEX dependencies_by_source ON dependencies(source);\
                 CREATE TABLE relations (\
                    caller INTEGER NOT NULL, seq INTEGER NOT NULL,\
                    identity TEXT NOT NULL, relation_json TEXT NOT NULL,\
                    UNIQUE(caller, identity));\
                 CREATE INDEX relations_by_caller ON relations(caller);\
                 CREATE TABLE file_relations (\
                    path TEXT NOT NULL, seq INTEGER NOT NULL,\
                    identity TEXT NOT NULL, relation_json TEXT NOT NULL,\
                    UNIQUE(path, identity));\
                 CREATE TABLE modules (\
                    path TEXT PRIMARY KEY, package_id TEXT NOT NULL,\
                    module_path TEXT NOT NULL, namespace TEXT NOT NULL,\
                    record_json TEXT NOT NULL);\
                 CREATE INDEX modules_by_path ON modules(module_path);\
                 CREATE TABLE module_exports (\
                    name TEXT NOT NULL, path TEXT NOT NULL,\
                    visibility TEXT NOT NULL, symbol_key TEXT NOT NULL);\
                 CREATE INDEX module_exports_by_name ON module_exports(name);\
                 CREATE TABLE module_imports (\
                    local_name TEXT NOT NULL, path TEXT NOT NULL,\
                    source TEXT NOT NULL, symbol_name TEXT,\
                    chain_depth INTEGER NOT NULL);\
                 CREATE INDEX module_imports_by_name ON module_imports(local_name);\
                 CREATE TABLE module_reexports (\
                    local_name TEXT NOT NULL, path TEXT NOT NULL,\
                    source TEXT NOT NULL, symbol_name TEXT,\
                    chain_depth INTEGER NOT NULL);\
                 CREATE INDEX module_reexports_by_name ON module_reexports(local_name);\
                 CREATE TABLE module_types (\
                    qualified_name TEXT NOT NULL, simple_name TEXT NOT NULL,\
                    member_name TEXT NOT NULL, path TEXT NOT NULL,\
                    entity_id INTEGER NOT NULL, arity_key TEXT NOT NULL);\
                 CREATE INDEX module_types_by_member ON module_types(member_name, arity_key);\
                 CREATE TABLE name_candidates (\
                    name TEXT NOT NULL, arity_key TEXT NOT NULL,\
                    sort_key TEXT NOT NULL, candidate_json TEXT NOT NULL,\
                    PRIMARY KEY(name, arity_key, candidate_json));\
                 CREATE INDEX name_candidates_by_bucket ON name_candidates(name, arity_key, sort_key);\
                 CREATE TABLE global_symbols (
                    key TEXT PRIMARY KEY, entity_id INTEGER NOT NULL);\
                 CREATE TABLE namespace_symbols (
                    key TEXT PRIMARY KEY, entity_id INTEGER NOT NULL);",
            )
            .map_err(|error| error.to_string())?;
        let local_symbols =
            Arc::new(LocalSymbolStore::new(&path).map_err(|error| error.to_string())?);
        Ok(Self {
            connection: Mutex::new(connection),
            path,
            next_sequence: AtomicU64::new(0),
            symbol_key_conflicts: AtomicU64::new(0),
            entity_derived_keys: AtomicU64::new(0),
            relation_derived_keys: AtomicU64::new(0),
            conflict_samples: Mutex::new(VecDeque::new()),
            local_symbols,
        })
    }

    /// Shared disk-backed local symbol table store for this build.
    pub fn local_symbol_store(&self) -> Arc<LocalSymbolStore> {
        Arc::clone(&self.local_symbols)
    }

    /// Persist one module record and its indexed lookup columns.
    pub fn write_module_record(
        &self,
        package_id: &str,
        record: &ModuleRecord,
    ) -> Result<(), String> {
        let json = serde_json::to_string(record).map_err(|error| error.to_string())?;
        let namespace = record
            .type_index
            .all_types()
            .first()
            .and_then(|entry| {
                entry
                    .key
                    .qualified
                    .rsplit_once("::")
                    .map(|(prefix, _)| prefix.to_string())
            })
            .unwrap_or_default();
        let guard = self.connection.lock().map_err(|error| error.to_string())?;
        guard
            .execute(
                "INSERT OR REPLACE INTO modules(path, package_id, module_path, namespace, record_json) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    record.file_path,
                    package_id,
                    record.module_path,
                    namespace,
                    json
                ],
            )
            .map_err(|error| error.to_string())?;
        for table in [
            "module_exports",
            "module_imports",
            "module_reexports",
            "module_types",
        ] {
            guard
                .execute(
                    &format!("DELETE FROM {table} WHERE path = ?1"),
                    params![record.file_path],
                )
                .map_err(|error| error.to_string())?;
        }
        for (name, _metadata, visibility) in record.export_entries() {
            guard
                .execute(
                    "INSERT INTO module_exports(name, path, visibility, symbol_key) \
                     VALUES (?1, ?2, ?3, ?4)",
                    params![
                        name,
                        record.file_path,
                        format!("{visibility:?}"),
                        format!("{}::{}::{}", record.file_path, record.module_path, name)
                    ],
                )
                .map_err(|error| error.to_string())?;
        }
        for (local_name, binding) in &record.imports {
            guard
                .execute(
                    "INSERT INTO module_imports(local_name, path, source, symbol_name, chain_depth) \
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        local_name,
                        record.file_path,
                        binding.source_path,
                        binding.symbol_name,
                        0i64
                    ],
                )
                .map_err(|error| error.to_string())?;
        }
        for (local_name, binding) in &record.reexports {
            guard
                .execute(
                    "INSERT INTO module_reexports(local_name, path, source, symbol_name, chain_depth) \
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        local_name,
                        record.file_path,
                        binding.original_module,
                        binding.original_name,
                        binding.chain_depth as i64
                    ],
                )
                .map_err(|error| error.to_string())?;
        }
        for entry in record.type_index.all_types() {
            for (member_name, members) in &entry.members {
                for member in members {
                    guard
                        .execute(
                            "INSERT INTO module_types(qualified_name, simple_name, member_name, path, entity_id, arity_key) \
                             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                            params![
                                entry.key.qualified,
                                entry.key.simple,
                                member_name,
                                record.file_path,
                                member.entity_id.0 as i64,
                                ""
                            ],
                        )
                        .map_err(|error| error.to_string())?;
                }
            }
        }
        Ok(())
    }

    pub fn module_record(&self, path: &str) -> Option<ModuleRecord> {
        let guard = self.connection.lock().ok()?;
        let json = guard
            .query_row(
                "SELECT record_json FROM modules WHERE path = ?1",
                params![path],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .ok()
            .flatten()?;
        Self::decode(&json).ok()
    }

    pub fn module_paths(&self) -> Vec<(String, String)> {
        let Ok(guard) = self.connection.lock() else {
            return Vec::new();
        };
        let Ok(mut statement) = guard.prepare("SELECT package_id, path FROM modules ORDER BY path")
        else {
            return Vec::new();
        };
        statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map(|rows| rows.filter_map(|row| row.ok()).collect())
            .unwrap_or_default()
    }

    /// Insert one name candidate, keeping only the first `limit` stable keys.
    /// Returns `true` when the candidate was retained.
    pub fn write_name_candidate(
        &self,
        candidate: &SymbolCandidate,
        limit: usize,
    ) -> Result<bool, String> {
        let arity_key = SymbolCandidate::bucket_id(&candidate.name, &candidate.arity);
        let json = serde_json::to_string(candidate).map_err(|error| error.to_string())?;
        let guard = self.connection.lock().map_err(|error| error.to_string())?;
        let count: i64 = guard
            .query_row(
                "SELECT COUNT(*) FROM name_candidates WHERE name = ?1 AND arity_key = ?2",
                params![candidate.name, arity_key],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        let existing: Option<String> = guard
            .query_row(
                "SELECT candidate_json FROM name_candidates \
                 WHERE name = ?1 AND arity_key = ?2 AND candidate_json = ?3",
                params![candidate.name, arity_key, json],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        if existing.is_some() {
            return Ok(true);
        }
        if count as usize >= limit {
            let worst: Option<String> = guard
                .query_row(
                    "SELECT candidate_json FROM name_candidates \
                     WHERE name = ?1 AND arity_key = ?2 ORDER BY sort_key DESC LIMIT 1",
                    params![candidate.name, arity_key],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|error| error.to_string())?;
            let Some(worst_json) = worst else {
                return Ok(false);
            };
            let retained = Self::decode::<SymbolCandidate>(&worst_json)
                .is_ok_and(|worst| candidate.sort_key < worst.sort_key);
            if !retained {
                return Ok(false);
            }
            guard
                .execute(
                    "DELETE FROM name_candidates \
                     WHERE name = ?1 AND arity_key = ?2 AND candidate_json = ?3",
                    params![candidate.name, arity_key, worst_json],
                )
                .map_err(|error| error.to_string())?;
        }
        guard
            .execute(
                "INSERT OR IGNORE INTO name_candidates(name, arity_key, sort_key, candidate_json) \
                 VALUES (?1, ?2, ?3, ?4)",
                params![candidate.name, arity_key, candidate.sort_key, json],
            )
            .map_err(|error| error.to_string())?;
        Ok(true)
    }

    pub fn name_candidates(&self, name: &str, arity_key: Option<&str>) -> Vec<SymbolCandidate> {
        let Ok(guard) = self.connection.lock() else {
            return Vec::new();
        };
        let rows: Vec<String> = if let Some(arity_key) = arity_key {
            let Ok(mut statement) = guard.prepare(
                "SELECT candidate_json FROM name_candidates \
                 WHERE name = ?1 AND arity_key = ?2 ORDER BY sort_key",
            ) else {
                return Vec::new();
            };
            statement
                .query_map(params![name, arity_key], |row| row.get::<_, String>(0))
                .map(|rows| rows.filter_map(|row| row.ok()).collect())
                .unwrap_or_default()
        } else {
            let Ok(mut statement) = guard.prepare(
                "SELECT candidate_json FROM name_candidates WHERE name = ?1 ORDER BY sort_key",
            ) else {
                return Vec::new();
            };
            statement
                .query_map(params![name], |row| row.get::<_, String>(0))
                .map(|rows| rows.filter_map(|row| row.ok()).collect())
                .unwrap_or_default()
        };
        rows.iter()
            .filter_map(|json| Self::decode(json).ok())
            .collect()
    }

    /// Declared arities present in a name's candidate buckets.
    pub fn name_candidate_arities(
        &self,
        name: &str,
    ) -> Vec<crate::type_inference::overload::ParamArity> {
        let Ok(guard) = self.connection.lock() else {
            return Vec::new();
        };
        let Ok(mut statement) = guard.prepare(
            "SELECT candidate_json FROM name_candidates WHERE name = ?1 GROUP BY arity_key",
        ) else {
            return Vec::new();
        };
        statement
            .query_map(params![name], |row| row.get::<_, String>(0))
            .map(|rows| {
                rows.filter_map(|row| {
                    row.ok()
                        .and_then(|json| Self::decode::<SymbolCandidate>(&json).ok())
                        .map(|candidate| candidate.arity)
                })
                .collect()
            })
            .unwrap_or_default()
    }

    /// Candidates in one stable `(name, arity)` bucket.
    pub fn name_candidates_for_arity(
        &self,
        name: &str,
        arity: &crate::type_inference::overload::ParamArity,
    ) -> Vec<SymbolCandidate> {
        let Ok(guard) = self.connection.lock() else {
            return Vec::new();
        };
        let key = SymbolCandidate::bucket_id(name, arity);
        let Ok(mut statement) = guard.prepare(
            "SELECT candidate_json FROM name_candidates \
             WHERE name = ?1 AND arity_key = ?2 ORDER BY sort_key",
        ) else {
            return Vec::new();
        };
        statement
            .query_map(params![name, key], |row| row.get::<_, String>(0))
            .map(|rows| {
                rows.filter_map(|row| {
                    row.ok()
                        .and_then(|json| Self::decode::<SymbolCandidate>(&json).ok())
                })
                .collect()
            })
            .unwrap_or_default()
    }

    /// Number of persisted module records.
    pub fn module_count(&self) -> u64 {
        let Ok(guard) = self.connection.lock() else {
            return 0;
        };
        guard
            .query_row("SELECT COUNT(*) FROM modules", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap_or(0) as u64
    }

    fn sequence(&self) -> u64 {
        self.next_sequence.fetch_add(1, Ordering::Relaxed)
    }

    fn decode<T: DeserializeOwned>(json: &str) -> Result<T, String> {
        serde_json::from_str(json).map_err(|error| error.to_string())
    }

    fn entity_json(&self, id: EntityId) -> Option<Entity> {
        let guard = self.connection.lock().ok()?;
        Self::entity_json_with(&guard, id)
    }

    fn entity_json_with(connection: &Connection, id: EntityId) -> Option<Entity> {
        connection
            .query_row(
                "SELECT entity_json FROM entities WHERE id = ?1",
                params![id.0 as i64],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .ok()
            .flatten()
            .and_then(|json| Self::decode(&json).ok())
    }

    fn file_path_with(connection: &Connection, id: EntityId) -> Option<String> {
        connection
            .query_row(
                "SELECT file FROM entities WHERE id = ?1",
                params![id.0 as i64],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .ok()
            .flatten()
    }

    /// Stable key of an entity, falling back to the same derived identity the
    /// in-memory artifact writer uses when registration had no stable key.
    fn key_or_derived_with(
        connection: &Connection,
        derived_keys: &AtomicU64,
        id: EntityId,
        name_fallback: &str,
    ) -> StableSymbolKey {
        let key_json: Option<String> = connection
            .query_row(
                "SELECT key_json FROM symbol_keys WHERE entity_id = ?1",
                params![id.0 as i64],
                |row| row.get(0),
            )
            .optional()
            .expect("symbol key query");
        if let Some(json) = key_json {
            return Self::decode(&json).expect("stored symbol key must decode");
        }
        derived_keys.fetch_add(1, Ordering::Relaxed);
        let file = Self::file_path_with(connection, id).unwrap_or_default();
        match Self::entity_json_with(connection, id) {
            Some(entity) => StableSymbolKey::for_entity(&file, &entity.name, &entity),
            None => StableSymbolKey::new(&file, name_fallback, EntityKind::Unknown, name_fallback),
        }
    }

    fn canonical_relation_with<F>(
        &self,
        caller: StableSymbolKey,
        relation: &ResolvedRelation,
        key: F,
    ) -> CanonicalRelation
    where
        F: Fn(EntityId, &str) -> StableSymbolKey,
    {
        let target = if let Some(callee_id) = relation.callee_id {
            CanonicalRelationTarget::Internal {
                key: key(callee_id, &relation.callee_name),
            }
        } else if relation.is_external && relation.external_type.is_some() {
            CanonicalRelationTarget::External {
                classification: relation.external_type.clone(),
            }
        } else {
            CanonicalRelationTarget::Unresolved {
                reason: UnresolvedReason::SymbolNotFound,
            }
        };
        CanonicalRelation {
            caller,
            target,
            raw_target: relation.callee_name.clone(),
            relation_type: relation.relation_type,
            span: relation.span,
            stdlib_category: relation.stdlib_category,
            overload_signature: relation.overload_signature.clone(),
            callee_symbol: relation.callee_symbol.clone(),
            owner_type: canonical_owner_type(&relation.owner_type),
            call_context: canonical_call_context(&relation.call_context),
            call_frequency: relation.call_frequency,
            cfg_condition: relation.cfg_condition.clone(),
        }
    }

    fn build_metadata(&self) -> SnapshotBuildMetadata {
        SnapshotBuildMetadata {
            symbol_key_conflict_count: self.symbol_key_conflicts.load(Ordering::Relaxed),
            symbol_key_conflict_samples: self
                .conflict_samples
                .lock()
                .expect("conflict sample lock")
                .iter()
                .cloned()
                .collect(),
            entity_derived_key_count: self.entity_derived_keys.load(Ordering::Relaxed),
            relation_derived_key_count: self.relation_derived_keys.load(Ordering::Relaxed),
        }
    }

    /// Look up one qualified key in the disk-backed global index.
    pub fn global_symbol(&self, key: &str) -> Option<EntityId> {
        let guard = self.connection.lock().ok()?;
        guard
            .query_row(
                "SELECT entity_id FROM global_symbols WHERE key = ?1",
                params![key],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .ok()
            .flatten()
            .map(|id| EntityId(id as u64))
    }

    pub fn write_global_symbol(&self, key: &str, entity_id: EntityId) -> Result<(), String> {
        let guard = self.connection.lock().map_err(|error| error.to_string())?;
        guard
            .execute(
                "INSERT OR REPLACE INTO global_symbols(key, entity_id) VALUES (?1, ?2)",
                params![key, entity_id.0 as i64],
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn remove_global_symbol(&self, key: &str) -> Result<(), String> {
        let guard = self.connection.lock().map_err(|error| error.to_string())?;
        guard
            .execute("DELETE FROM global_symbols WHERE key = ?1", params![key])
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn global_symbol_count(&self) -> u64 {
        let guard = self.connection.lock().ok();
        guard
            .and_then(|connection| {
                connection
                    .query_row("SELECT COUNT(*) FROM global_symbols", [], |row| {
                        row.get::<_, i64>(0)
                    })
                    .ok()
            })
            .unwrap_or(0) as u64
    }

    /// Look up one qualified key in the disk-backed namespace index.
    pub fn namespace_symbol(&self, key: &str) -> Option<EntityId> {
        let guard = self.connection.lock().ok()?;
        guard
            .query_row(
                "SELECT entity_id FROM namespace_symbols WHERE key = ?1",
                params![key],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .ok()
            .flatten()
            .map(|id| EntityId(id as u64))
    }

    pub fn write_namespace_symbol(&self, key: &str, entity_id: EntityId) -> Result<(), String> {
        let guard = self.connection.lock().map_err(|error| error.to_string())?;
        guard
            .execute(
                "INSERT OR REPLACE INTO namespace_symbols(key, entity_id) VALUES (?1, ?2)",
                params![key, entity_id.0 as i64],
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn remove_namespace_symbol(&self, key: &str) -> Result<(), String> {
        let guard = self.connection.lock().map_err(|error| error.to_string())?;
        guard
            .execute("DELETE FROM namespace_symbols WHERE key = ?1", params![key])
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn namespace_symbol_count(&self) -> u64 {
        let guard = self.connection.lock().ok();
        guard
            .and_then(|connection| {
                connection
                    .query_row("SELECT COUNT(*) FROM namespace_symbols", [], |row| {
                        row.get::<_, i64>(0)
                    })
                    .ok()
            })
            .unwrap_or(0) as u64
    }

    /// Clear both qualified indexes before a full rebuild.
    pub fn clear_qualified_symbols(&self) -> Result<(), String> {
        let guard = self.connection.lock().map_err(|error| error.to_string())?;
        guard
            .execute_batch("DELETE FROM global_symbols; DELETE FROM namespace_symbols;")
            .map_err(|error| error.to_string())
    }

    /// Read every persisted qualified key with its entity id.
    ///
    /// Used to rebuild the namespace projection from the authoritative
    /// persisted keys instead of re-deriving them from resident maps.
    pub fn global_symbol_rows(&self) -> Vec<(String, EntityId)> {
        let Ok(guard) = self.connection.lock() else {
            return Vec::new();
        };
        let Ok(mut statement) = guard.prepare("SELECT key, entity_id FROM global_symbols") else {
            return Vec::new();
        };
        statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    EntityId(row.get::<_, i64>(1)? as u64),
                ))
            })
            .map(|rows| rows.filter_map(|row| row.ok()).collect())
            .unwrap_or_default()
    }

    /// Drop the persisted namespace projection and the resident entries
    /// caching it, leaving the global projection untouched.
    pub fn clear_namespace_symbols(&self) -> Result<(), String> {
        let guard = self.connection.lock().map_err(|error| error.to_string())?;
        guard
            .execute("DELETE FROM namespace_symbols", [])
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    /// Number of resolved edges currently stored.
    pub fn relation_count(&self) -> usize {
        let guard = self.connection.lock().expect("build store lock");
        guard
            .query_row(
                "SELECT (SELECT COUNT(*) FROM relations) + (SELECT COUNT(*) FROM file_relations)",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0) as usize
    }

    /// Stage capacity report for the build store.
    pub fn capacity_counts(&self) -> (u64, u64, u64) {
        let guard = self.connection.lock().expect("build store lock");
        let entities: i64 = guard
            .query_row("SELECT COUNT(*) FROM entities", [], |row| row.get(0))
            .unwrap_or(0);
        let files: i64 = guard
            .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))
            .unwrap_or(0);
        let edges: i64 = guard
            .query_row(
                "SELECT (SELECT COUNT(*) FROM relations) + (SELECT COUNT(*) FROM file_relations)",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);
        let nodes = (entities + files) as u64;
        (nodes, edges as u64, nodes * 1024 + edges as u64 * 96)
    }

    /// Stream all canonical rows into a publication artifact.
    pub fn write_artifact(
        &self,
        config_fingerprint: String,
        options: SnapshotArtifactOptions,
    ) -> Result<CanonicalSnapshotArtifact, cce_types::StorageError> {
        let header = SnapshotArtifactHeader {
            schema_version: cce_types::RELATION_SNAPSHOT_SCHEMA_VERSION,
            parser_version: cce_types::RELATION_PARSER_VERSION,
            resolver_version: cce_types::RELATION_RESOLVER_VERSION,
            path_normalization_version: cce_types::RELATION_PATH_NORMALIZATION_VERSION,
            config_fingerprint,
            base_relation_epoch: None,
            build_metadata: self.build_metadata(),
        };
        let reader = Connection::open_with_flags(&self.path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|error| StorageError::validation(format!("artifact reader: {error}")))?;
        let entity_key = |id: EntityId, name: &str| {
            Self::key_or_derived_with(&reader, &self.entity_derived_keys, id, name)
        };
        let relation_key = |id: EntityId, name: &str| {
            Self::key_or_derived_with(&reader, &self.relation_derived_keys, id, name)
        };
        let mut builder = SnapshotArtifactBuilder::new(header, options)?;

        {
            let guard = self.connection.lock().expect("build store lock");
            let mut statement = guard
                .prepare("SELECT path, info_json FROM files ORDER BY path")
                .map_err(|error| StorageError::validation(error.to_string()))?;
            let rows = statement
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(|error| StorageError::validation(error.to_string()))?;
            for row in rows {
                let (path, info_json) =
                    row.map_err(|error| StorageError::validation(error.to_string()))?;
                let info: FileInfo = Self::decode(&info_json).map_err(StorageError::validation)?;
                let imports: Vec<String> = {
                    let mut lookup = reader
                        .prepare("SELECT table_json FROM imports WHERE path = ?1")
                        .map_err(|error| StorageError::validation(error.to_string()))?;
                    lookup
                        .query_map(params![path], |row| row.get::<_, String>(0))
                        .map_err(|error| StorageError::validation(error.to_string()))?
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(|error| StorageError::validation(error.to_string()))?
                };
                let import_table = imports
                    .first()
                    .map(|json| Self::decode::<ImportTable>(json))
                    .transpose()
                    .map_err(StorageError::validation)?
                    .unwrap_or_default();
                let exports: Vec<String> = {
                    let mut lookup = reader
                        .prepare("SELECT export_json FROM exports WHERE path = ?1 ORDER BY seq")
                        .map_err(|error| StorageError::validation(error.to_string()))?;
                    lookup
                        .query_map(params![path], |row| row.get::<_, String>(0))
                        .map_err(|error| StorageError::validation(error.to_string()))?
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(|error| StorageError::validation(error.to_string()))?
                };
                let mut canonical_exports = Vec::with_capacity(exports.len());
                for json in exports {
                    let export: ExportInfo =
                        Self::decode(&json).map_err(StorageError::validation)?;
                    canonical_exports.push(cce_types::CanonicalExport {
                        symbol: entity_key(export.function_id, &export.function_name),
                        export_type: format!("{:?}", export.export_type).to_lowercase(),
                    });
                }
                builder.push_file(CanonicalFile {
                    path: normalize_project_path(&path),
                    language: info.language,
                    input_hash: info.file_hash,
                    file_size: info.file_size,
                    imports: import_table.standardized_imports,
                    exports: canonical_exports,
                })?;
            }
        }

        {
            let guard = self.connection.lock().expect("build store lock");
            let mut statement = guard
                .prepare("SELECT id, entity_json FROM entities ORDER BY id")
                .map_err(|error| StorageError::validation(error.to_string()))?;
            let rows = statement
                .query_map([], |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(|error| StorageError::validation(error.to_string()))?;
            for row in rows {
                let (id, entity_json) =
                    row.map_err(|error| StorageError::validation(error.to_string()))?;
                let entity: Entity =
                    Self::decode(&entity_json).map_err(StorageError::validation)?;
                let parent = entity.parent.map(|parent| entity_key(parent, &entity.name));
                builder.push_entity(CanonicalEntity {
                    key: entity_key(EntityId(id as u64), &entity.name),
                    entity_id: Some(id as u64),
                    name: entity.name,
                    signature: entity.signature,
                    parameters: entity.parameters,
                    return_type: entity.return_type,
                    span: entity.span,
                    depth: entity.depth,
                    parent,
                    doc_comment: entity.doc_comment,
                    modifiers: entity.modifiers,
                    attributes: entity.attributes.into_iter().collect(),
                    metadata: entity.metadata.into_iter().collect(),
                    is_stdlib: entity.is_stdlib,
                    stdlib_category: entity.stdlib_category,
                    subtype: entity.subtype,
                })?;
            }
        }

        {
            let guard = self.connection.lock().expect("build store lock");
            let mut statement = guard
                .prepare("SELECT relation_json FROM relations ORDER BY seq")
                .map_err(|error| StorageError::validation(error.to_string()))?;
            let rows = statement
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(|error| StorageError::validation(error.to_string()))?;
            for row in rows {
                let json = row.map_err(|error| StorageError::validation(error.to_string()))?;
                let relation: ResolvedRelation =
                    Self::decode(&json).map_err(StorageError::validation)?;
                let caller = relation_key(relation.caller, "");
                builder.push_relation(self.canonical_relation_with(
                    caller,
                    &relation,
                    relation_key,
                ))?;
            }
        }

        {
            let guard = self.connection.lock().expect("build store lock");
            let mut statement = guard
                .prepare("SELECT path, relation_json FROM file_relations ORDER BY seq")
                .map_err(|error| StorageError::validation(error.to_string()))?;
            let rows = statement
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(|error| StorageError::validation(error.to_string()))?;
            for row in rows {
                let (path, json) =
                    row.map_err(|error| StorageError::validation(error.to_string()))?;
                let relation: ResolvedRelation =
                    Self::decode(&json).map_err(StorageError::validation)?;
                let key = StableSymbolKey::new(&path, "<file>", EntityKind::Module, "<file>");
                builder.push_relation(self.canonical_relation_with(
                    key,
                    &relation,
                    relation_key,
                ))?;
            }
        }

        {
            let guard = self.connection.lock().expect("build store lock");
            let mut statement = guard
                .prepare("SELECT source, target FROM dependencies ORDER BY seq")
                .map_err(|error| StorageError::validation(error.to_string()))?;
            let rows = statement
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(|error| StorageError::validation(error.to_string()))?;
            for row in rows {
                let (source, target) =
                    row.map_err(|error| StorageError::validation(error.to_string()))?;
                builder.push_dependency(CanonicalDependency {
                    source_file: source,
                    target_file: normalize_project_path(&target),
                    source: "resolved_or_import".to_string(),
                })?;
            }
        }

        builder.set_build_metadata(self.build_metadata());
        builder.finish()
    }
}
impl Drop for BuildRelationStore {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

impl BuildEntityView for BuildRelationStore {
    fn entity_id_remap_for(&self, path: &str) -> Option<HashMap<EntityId, EntityId>> {
        let guard = self.connection.lock().expect("build store lock");
        guard
            .query_row(
                "SELECT remap_json FROM entity_remaps WHERE file = ?1",
                params![path],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .ok()
            .flatten()
            .and_then(|json| Self::decode(&json).ok())
    }

    fn get_function_ids_by_name(&self, name: &str) -> Vec<EntityId> {
        let guard = self.connection.lock().expect("build store lock");
        let mut statement = guard
            .prepare("SELECT id FROM entities WHERE name = ?1 ORDER BY seq")
            .expect("name lookup statement");
        statement
            .query_map(params![name], |row| row.get::<_, i64>(0))
            .expect("name lookup")
            .filter_map(|row| row.ok())
            .map(|id| EntityId(id as u64))
            .collect()
    }

    fn get_function_by_entity_id(&self, id: EntityId) -> Option<Entity> {
        self.entity_json(id)
    }

    fn get_file_path_by_entity(&self, id: EntityId) -> Option<String> {
        let guard = self.connection.lock().expect("build store lock");
        guard
            .query_row(
                "SELECT file FROM entity_files WHERE id = ?1",
                params![id.0 as i64],
                |row| row.get(0),
            )
            .optional()
            .ok()
            .flatten()
    }
}

impl BuildIndexSink for BuildRelationStore {
    fn add_file(&self, file: FileInfo) {
        let json = serde_json::to_string(&file).expect("file info must serialize");
        let guard = self.connection.lock().expect("build store lock");
        guard
            .execute(
                "INSERT OR REPLACE INTO files(path, info_json) VALUES (?1, ?2)",
                params![file.path, json],
            )
            .expect("file insert");
    }

    fn allocate_entity_id(&self) -> EntityId {
        EntityId(self.next_sequence.fetch_add(1, Ordering::Relaxed))
    }

    fn add_entities_with_paths(&self, entities: Vec<(EntityId, Entity, String)>) {
        let guard = self.connection.lock().expect("build store lock");
        for (id, entity, path) in entities {
            let name = entity.name.clone();
            let json = serde_json::to_string(&entity).expect("entity must serialize");
            let seq = self.sequence() as i64;
            guard
                .execute(
                    "INSERT OR REPLACE INTO entities(id, file, name, seq, entity_json) \
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![id.0 as i64, path, name, seq, json],
                )
                .expect("entity insert");
            guard
                .execute(
                    "INSERT OR REPLACE INTO entity_files(id, file) VALUES (?1, ?2)",
                    params![id.0 as i64, path],
                )
                .expect("entity file insert");
        }
    }

    fn function_contains(&self, id: EntityId) -> bool {
        let guard = self.connection.lock().expect("build store lock");
        guard
            .query_row(
                "SELECT 1 FROM entities WHERE id = ?1",
                params![id.0 as i64],
                |_| Ok(()),
            )
            .optional()
            .ok()
            .flatten()
            .is_some()
    }

    fn register_symbol_key(
        &self,
        file_path: &str,
        scoped_name: &str,
        entity: &Entity,
        entity_id: EntityId,
    ) -> bool {
        let key = StableSymbolKey::for_entity(file_path, scoped_name, entity);
        let key_json = serde_json::to_string(&key).expect("symbol key must serialize");
        let guard = self.connection.lock().expect("build store lock");
        let existing: Option<(i64, String)> = guard
            .query_row(
                "SELECT entity_id, key_json FROM symbol_keys WHERE key_json = ?1",
                params![key_json],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .expect("symbol key lookup");
        let Some((existing_id, existing_json)) = existing else {
            guard
                .execute(
                    "INSERT OR REPLACE INTO symbol_keys(entity_id, key_json) VALUES (?1, ?2)",
                    params![entity_id.0 as i64, key_json],
                )
                .expect("symbol key insert");
            return true;
        };
        if existing_id as u64 == entity_id.0 {
            return true;
        }
        let owner_json: Option<String> = guard
            .query_row(
                "SELECT entity_json FROM entities WHERE id = ?1",
                params![existing_id],
                |row| row.get(0),
            )
            .optional()
            .expect("owner lookup");
        let owner_differs = owner_json
            .and_then(|json| Self::decode::<Entity>(&json).ok())
            .is_some_and(|owner| {
                owner.span.start_byte != entity.span.start_byte
                    || owner.span.end_byte != entity.span.end_byte
            });
        let span_scoped = format!(
            "{scoped_name}#{}-{}",
            entity.span.start_byte, entity.span.end_byte
        );
        let span_key = StableSymbolKey::for_entity(file_path, &span_scoped, entity);
        let span_json = serde_json::to_string(&span_key).expect("span symbol key must serialize");
        let span_taken: Option<i64> = guard
            .query_row(
                "SELECT entity_id FROM symbol_keys WHERE key_json = ?1",
                params![span_json],
                |row| row.get(0),
            )
            .optional()
            .expect("span key lookup");
        if !owner_differs || span_taken.is_some_and(|id| id as u64 != entity_id.0) {
            self.symbol_key_conflicts.fetch_add(1, Ordering::Relaxed);
            let mut samples = self.conflict_samples.lock().expect("conflict sample lock");
            if samples.len() >= cce_types::SYMBOL_KEY_CONFLICT_SAMPLE_CAP {
                samples.pop_front();
            }
            samples.push_back(SymbolKeyConflictRecord {
                file_path: key.file_path.clone(),
                scoped_name: key.scoped_name.clone(),
                kind: key.kind,
                kept_entity: existing_id as u64,
                rejected_entity: entity_id.0,
            });
            return false;
        }
        guard
            .execute(
                "INSERT OR REPLACE INTO symbol_keys(entity_id, key_json) VALUES (?1, ?2)",
                params![entity_id.0 as i64, span_json],
            )
            .expect("span symbol key insert");
        let _ = existing_json;
        true
    }

    fn set_entity_remap(&self, path: String, remap: HashMap<EntityId, EntityId>) {
        let json = serde_json::to_string(&remap).expect("entity remap must serialize");
        let guard = self.connection.lock().expect("build store lock");
        guard
            .execute(
                "INSERT OR REPLACE INTO entity_remaps(file, remap_json) VALUES (?1, ?2)",
                params![path, json],
            )
            .expect("entity remap insert");
    }

    fn add_import_table(&self, path: String, table: ImportTable) {
        let json = serde_json::to_string(&table).expect("import table must serialize");
        let guard = self.connection.lock().expect("build store lock");
        guard
            .execute(
                "INSERT OR REPLACE INTO imports(path, table_json) VALUES (?1, ?2)",
                params![path, json],
            )
            .expect("import insert");
    }

    fn add_exports(&self, path: String, exports: Vec<ExportInfo>) {
        let guard = self.connection.lock().expect("build store lock");
        for export in exports {
            let json = serde_json::to_string(&export).expect("export must serialize");
            let seq = self.sequence() as i64;
            guard
                .execute(
                    "INSERT INTO exports(path, seq, export_json) VALUES (?1, ?2, ?3)",
                    params![path, seq, json],
                )
                .expect("export insert");
        }
    }

    fn add_dependency(&self, source: &str, target: &str) {
        let guard = self.connection.lock().expect("build store lock");
        let seq = self.sequence() as i64;
        guard
            .execute(
                "INSERT OR IGNORE INTO dependencies(seq, source, target) VALUES (?1, ?2, ?3)",
                params![seq, source, normalize_project_path(target)],
            )
            .expect("dependency insert");
    }

    fn add_resolved_relation(&self, relation: ResolvedRelation) {
        let identity = format!("{:?}", relation_identity(&relation));
        let json = serde_json::to_string(&relation).expect("relation must serialize");
        let seq = self.sequence() as i64;
        let guard = self.connection.lock().expect("build store lock");
        guard
            .execute(
                "INSERT OR IGNORE INTO relations(caller, seq, identity, relation_json) \
                 VALUES (?1, ?2, ?3, ?4)",
                params![relation.caller.0 as i64, seq, identity, json],
            )
            .expect("relation insert");
    }

    fn add_file_relation(&self, path: &str, relation: ResolvedRelation) {
        let identity = format!("{:?}", relation_identity(&relation));
        let json = serde_json::to_string(&relation).expect("file relation must serialize");
        let seq = self.sequence() as i64;
        let guard = self.connection.lock().expect("build store lock");
        guard
            .execute(
                "INSERT OR IGNORE INTO file_relations(path, seq, identity, relation_json) \
                 VALUES (?1, ?2, ?3, ?4)",
                params![path, seq, identity, json],
            )
            .expect("file relation insert");
    }

    fn get_resolved_relations_by_file(&self, path: &str) -> Vec<(EntityId, Vec<ResolvedRelation>)> {
        let guard = self.connection.lock().expect("build store lock");
        let mut statement = guard
            .prepare(
                "SELECT r.caller, r.relation_json FROM relations r \
                 JOIN entities e ON e.id = r.caller WHERE e.file = ?1 ORDER BY r.seq",
            )
            .expect("file relation lookup statement");
        let rows = statement
            .query_map(params![path], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .expect("file relation lookup");
        let mut groups: HashMap<EntityId, Vec<ResolvedRelation>> = HashMap::new();
        for row in rows.flatten() {
            if let Ok(relation) = Self::decode::<ResolvedRelation>(&row.1) {
                groups
                    .entry(EntityId(row.0 as u64))
                    .or_default()
                    .push(relation);
            }
        }
        groups.into_iter().collect()
    }

    fn relations_of_caller(&self, caller: EntityId) -> Vec<ResolvedRelation> {
        let guard = self.connection.lock().expect("build store lock");
        let mut statement = guard
            .prepare("SELECT relation_json FROM relations WHERE caller = ?1 ORDER BY seq")
            .expect("caller relations statement");
        statement
            .query_map(params![caller.0 as i64], |row| row.get::<_, String>(0))
            .expect("caller relations")
            .filter_map(|row| row.ok())
            .filter_map(|json| Self::decode::<ResolvedRelation>(&json).ok())
            .collect()
    }

    fn for_each_import<F: FnMut(&str, &ImportTable)>(&self, mut visit: F) {
        let guard = self.connection.lock().expect("build store lock");
        let mut statement = guard
            .prepare("SELECT path, table_json FROM imports ORDER BY path")
            .expect("import iteration statement");
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .expect("import iteration");
        for row in rows.flatten() {
            let (path, json) = row;
            if let Ok(table) = Self::decode::<ImportTable>(&json) {
                visit(&path, &table);
            }
        }
    }

    fn entities_of_file(&self, path: &str) -> Vec<Entity> {
        let guard = self.connection.lock().expect("build store lock");
        let mut statement = guard
            .prepare("SELECT entity_json FROM entities WHERE file = ?1 ORDER BY id")
            .expect("file entities statement");
        statement
            .query_map(params![path], |row| row.get::<_, String>(0))
            .expect("file entities")
            .filter_map(|row| row.ok())
            .filter_map(|json| Self::decode::<Entity>(&json).ok())
            .collect()
    }

    fn caller_entity_ids(&self, file: &ParsedFile) -> Vec<EntityId> {
        let path = normalize_project_path(&file.path);
        let Some(remap) = self.entity_id_remap_for(&path) else {
            return Vec::new();
        };
        let mut ids: HashSet<EntityId> = remap.into_values().collect();
        let mut ids: Vec<EntityId> = ids.drain().collect();
        ids.sort_by_key(|id| id.0);
        ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::build_view::BuildIndexSink;
    use cce_types::{
        CanonicalRelation, CanonicalRelationSnapshot, CanonicalRelationTarget, EntityKind,
        RelationType, ResolvedRelation, Span,
    };

    fn sample_entity(id: EntityId, name: &str) -> Entity {
        Entity {
            id,
            kind: EntityKind::Function,
            name: name.to_string(),
            signature: format!("fn {name}()"),
            ..Entity::default()
        }
    }

    #[test]
    fn disk_build_artifact_matches_canonical_fingerprints() {
        let root = tempfile::tempdir().expect("temp dir");
        let store = BuildRelationStore::new(root.path().join("build.sqlite3"))
            .expect("build store must open");
        let path = "src/lib.rs".to_string();
        let caller = EntityId(1);
        let callee = EntityId(2);
        let caller_entity = sample_entity(caller, "caller");
        let callee_entity = sample_entity(callee, "callee");
        let file = FileInfo {
            id: path.clone(),
            path: path.clone(),
            language: "rust".to_string(),
            file_hash: "hash".to_string(),
            file_size: 42,
            ..FileInfo::default()
        };
        store.add_file(file.clone());
        store.add_entities_with_paths(vec![
            (caller, caller_entity.clone(), path.clone()),
            (callee, callee_entity.clone(), path.clone()),
        ]);
        let caller_key = StableSymbolKey::for_entity(&path, "caller", &caller_entity);
        let callee_key = StableSymbolKey::for_entity(&path, "callee", &callee_entity);
        assert!(store.register_symbol_key(&path, "caller", &caller_entity, caller));
        assert!(store.register_symbol_key(&path, "callee", &callee_entity, callee));
        let relation = ResolvedRelation {
            caller,
            callee_id: Some(callee),
            callee_name: "callee".to_string(),
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
        };
        store.add_resolved_relation(relation.clone());
        store.add_dependency(&path, "src/other.rs");

        let artifact = store
            .write_artifact("config".to_string(), SnapshotArtifactOptions::default())
            .expect("artifact must be written");

        let mut snapshot = CanonicalRelationSnapshot::new("config".to_string());
        snapshot.files.push(CanonicalFile {
            path: path.clone(),
            language: file.language,
            input_hash: file.file_hash,
            file_size: file.file_size,
            imports: Vec::new(),
            exports: Vec::new(),
        });
        for entity in [&caller_entity, &callee_entity] {
            let key = if entity.id == caller {
                caller_key.clone()
            } else {
                callee_key.clone()
            };
            snapshot.entities.push(CanonicalEntity {
                key,
                entity_id: Some(entity.id.0),
                name: entity.name.clone(),
                signature: entity.signature.clone(),
                parameters: entity.parameters.clone(),
                return_type: entity.return_type.clone(),
                span: entity.span,
                depth: entity.depth,
                parent: None,
                doc_comment: entity.doc_comment.clone(),
                modifiers: entity.modifiers.clone(),
                attributes: entity.attributes.clone().into_iter().collect(),
                metadata: entity.metadata.clone().into_iter().collect(),
                is_stdlib: entity.is_stdlib,
                stdlib_category: entity.stdlib_category,
                subtype: entity.subtype.clone(),
            });
        }
        snapshot.relations.push(CanonicalRelation {
            caller: caller_key,
            target: CanonicalRelationTarget::Internal { key: callee_key },
            raw_target: relation.callee_name.clone(),
            relation_type: relation.relation_type,
            span: relation.span,
            stdlib_category: relation.stdlib_category,
            overload_signature: relation.overload_signature.clone(),
            callee_symbol: relation.callee_symbol.clone(),
            owner_type: None,
            call_context: relation.call_context.clone(),
            call_frequency: relation.call_frequency,
            cfg_condition: relation.cfg_condition.clone(),
        });
        snapshot.dependencies.push(CanonicalDependency {
            source_file: path,
            target_file: "src/other.rs".to_string(),
            source: "resolved_or_import".to_string(),
        });

        let reference =
            CanonicalSnapshotArtifact::from_snapshot(&snapshot, SnapshotArtifactOptions::default())
                .expect("reference artifact must be written");
        assert_eq!(artifact.counts(), reference.counts());
        assert_eq!(
            artifact.snapshot_fingerprint(),
            reference.snapshot_fingerprint()
        );
        assert_eq!(artifact.input_fingerprint(), reference.input_fingerprint());
    }
}
