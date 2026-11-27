//! Operation-local disk storage for per-file local symbol tables.
//!
//! Full relation builds replay thousands of files in several passes. Keeping
//! every file's local table resident makes the symbol phase linear in project
//! size. This store keeps one durable copy in the build SQLite database and
//! materialises only a bounded working set during resolution.

use std::path::Path;
use std::sync::Arc;

use lru::LruCache;
use parking_lot::Mutex;
use rusqlite::{Connection, OptionalExtension, params};
use std::num::NonZeroUsize;

use super::local::{LocalSymbolTable, LocalSymbolTableRecord};

/// Maximum number of materialised local tables kept across lookups.
const MAX_RESIDENT_TABLES: usize = 128;
/// Soft cap on the estimated bytes held by the materialised working set.
const MAX_RESIDENT_BYTES: u64 = 96 * 1024 * 1024;

/// Disk-backed local symbol table store shared by a single full build.
#[derive(Debug)]
pub struct LocalSymbolStore {
    connection: Mutex<Connection>,
    resident: Mutex<ResidentTables>,
}

#[derive(Debug)]
struct ResidentTables {
    cache: LruCache<String, (Arc<LocalSymbolTable>, u64)>,
    bytes: u64,
}

impl LocalSymbolStore {
    /// Open (or create) the local-table section of the build database.
    pub fn new(path: &Path) -> Result<Self, String> {
        let connection = Connection::open(path).map_err(|error| error.to_string())?;
        connection
            .execute_batch("PRAGMA journal_mode=OFF; PRAGMA synchronous=OFF;")
            .map_err(|error| error.to_string())?;
        connection
            .execute(
                "CREATE TABLE IF NOT EXISTS local_symbols (\n                 path TEXT PRIMARY KEY, record_json TEXT NOT NULL);",
                [],
            )
            .map_err(|error| error.to_string())?;
        Ok(Self {
            connection: Mutex::new(connection),
            resident: Mutex::new(ResidentTables {
                cache: LruCache::new(
                    NonZeroUsize::new(MAX_RESIDENT_TABLES).expect("resident table cap is non-zero"),
                ),
                bytes: 0,
            }),
        })
    }

    /// Persist one file's local table. Writes replace the previous value for
    /// the same path so a replayed file cannot duplicate resident state.
    pub fn insert(&self, path: &str, table: &LocalSymbolTable) -> Result<(), String> {
        let record = table.to_record();
        let json = serde_json::to_string(&record).map_err(|error| error.to_string())?;
        let guard = self.connection.lock();
        let mut statement = guard
            .prepare_cached(
                "INSERT OR REPLACE INTO local_symbols(path, record_json) VALUES (?1, ?2)",
            )
            .map_err(|error| error.to_string())?;
        statement
            .execute(params![path, json])
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    /// Load one file's local table, reusing the bounded working set when the
    /// same file is revisited by a later resolution pass.
    pub fn get(&self, path: &str) -> Option<Arc<LocalSymbolTable>> {
        if let Some((table, _)) = self.resident.lock().cache.get(path).cloned() {
            return Some(table);
        }
        let json = {
            let guard = self.connection.lock();
            let mut statement = guard
                .prepare_cached("SELECT record_json FROM local_symbols WHERE path = ?1")
                .ok()?;
            statement
                .query_row(params![path], |row| row.get::<_, String>(0))
                .optional()
                .ok()
                .flatten()
        }?;
        let record: LocalSymbolTableRecord = serde_json::from_str(&json).ok()?;
        let table = Arc::new(LocalSymbolTable::from_record(record));
        let estimated = json.len() as u64;
        let mut resident = self.resident.lock();
        while resident.bytes.saturating_add(estimated) > MAX_RESIDENT_BYTES
            && !resident.cache.is_empty()
        {
            if let Some((_, (_, evicted_bytes))) = resident.cache.pop_lru() {
                resident.bytes = resident.bytes.saturating_sub(evicted_bytes);
            }
        }
        resident.bytes = resident.bytes.saturating_add(estimated);
        resident
            .cache
            .put(path.to_string(), (Arc::clone(&table), estimated));
        Some(table)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::entity::EntityId;
    use cce_types::{Entity, EntityKind, ParsedFile, Span};

    fn sample_file() -> ParsedFile {
        let mut file = ParsedFile {
            path: "src/lib.rs".to_string(),
            language: cce_types::Language::Rust,
            entities: vec![
                Entity::new(
                    EntityId(1),
                    EntityKind::Function,
                    "outer".to_string(),
                    Span::default(),
                ),
                Entity::new(
                    EntityId(2),
                    EntityKind::Function,
                    "inner".to_string(),
                    Span::default(),
                ),
            ],
            ..Default::default()
        };
        file.entities[1].parent = Some(EntityId(1));
        file
    }

    #[test]
    fn local_table_roundtrip_preserves_scope_lookup() {
        let root = tempfile::tempdir().expect("temp dir");
        let store = LocalSymbolStore::new(&root.path().join("symbols.sqlite3"))
            .expect("local symbol store must open");
        let file = sample_file();
        let memory = LocalSymbolTable::from_parsed_file(&file);
        store
            .insert(&file.path, &memory)
            .expect("insert must succeed");

        let disk = store.get(&file.path).expect("stored table must load");
        assert_eq!(disk.len(), memory.len());
        assert_eq!(
            disk.get_entity(EntityId(2))
                .map(|entity| entity.name.as_str()),
            Some("inner")
        );
        let chain = disk.build_scope_chain_with_limit(EntityId(2), 8);
        assert_eq!(chain, vec![EntityId(1), EntityId(2)]);
    }
}
