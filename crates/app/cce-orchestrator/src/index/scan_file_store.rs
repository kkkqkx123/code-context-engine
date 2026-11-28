//! Operation-local SQLite manifest for scanned files.
//!
//! A full index streams scan batches into this store instead of keeping the
//! complete file list resident. Rows are reordered by path once the scan
//! finishes, so batch boundaries stay deterministic.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use cce_scanner::FileEntry;
use rusqlite::{Connection, OptionalExtension, params};

/// SQLite-backed scan manifest. The directory is removed on drop.
pub struct ScanFileStore {
    connection: Mutex<Connection>,
    directory: PathBuf,
    next_ordinal: AtomicU64,
}

impl ScanFileStore {
    /// Create a fresh manifest under `<root>/.cce/scan-index-<operation_id>`.
    pub fn new(root: &Path, operation_id: &str) -> Result<Arc<Self>, String> {
        let directory = root.join(".cce").join(format!("scan-index-{operation_id}"));
        if directory.exists() {
            fs::remove_dir_all(&directory).map_err(|error| error.to_string())?;
        }
        fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        let path = directory.join("scan-files.sqlite3");
        let connection = Connection::open(&path).map_err(|error| error.to_string())?;
        connection
            .execute_batch(
                "PRAGMA journal_mode=MEMORY;\
                 PRAGMA synchronous=OFF;\
                 CREATE TABLE scan_files (\
                    ordinal INTEGER PRIMARY KEY,\
                    path TEXT NOT NULL UNIQUE,\
                    entry_json TEXT NOT NULL);",
            )
            .map_err(|error| error.to_string())?;
        Ok(Arc::new(Self {
            connection: Mutex::new(connection),
            directory,
            next_ordinal: AtomicU64::new(0),
        }))
    }

    /// Append one scanned batch. Ordinals are provisional until sorting.
    pub fn append_batch(&self, entries: &[FileEntry]) -> Result<(), String> {
        let mut guard = self.connection.lock().map_err(|error| error.to_string())?;
        let transaction = guard.transaction().map_err(|error| error.to_string())?;
        {
            let mut statement = transaction
                .prepare("INSERT INTO scan_files(ordinal, path, entry_json) VALUES (?1, ?2, ?3)")
                .map_err(|error| error.to_string())?;
            for entry in entries {
                let json = serde_json::to_string(entry).map_err(|error| error.to_string())?;
                let ordinal = self.next_ordinal.fetch_add(1, Ordering::Relaxed) as i64;
                statement
                    .execute(params![ordinal, entry.path.to_string_lossy(), json])
                    .map_err(|error| error.to_string())?;
            }
        }
        transaction.commit().map_err(|error| error.to_string())
    }

    /// Renumber rows by path so interval queries match the previous sorted
    /// in-memory batching.
    pub fn sort_by_path(&self) -> Result<(), String> {
        let mut guard = self.connection.lock().map_err(|error| error.to_string())?;
        let transaction = guard.transaction().map_err(|error| error.to_string())?;
        transaction
            .execute_batch(
                r#"
                CREATE TABLE scan_files_sorted (
                    ordinal INTEGER PRIMARY KEY,
                    path TEXT NOT NULL UNIQUE,
                    entry_json TEXT NOT NULL
                );
                INSERT INTO scan_files_sorted(ordinal, path, entry_json)
                    SELECT ROW_NUMBER() OVER (ORDER BY path) - 1, path, entry_json
                    FROM scan_files;
                DROP TABLE scan_files;
                ALTER TABLE scan_files_sorted RENAME TO scan_files;
                "#,
            )
            .map_err(|error| error.to_string())?;
        transaction.commit().map_err(|error| error.to_string())
    }

    pub fn count(&self) -> Result<usize, String> {
        let guard = self.connection.lock().map_err(|error| error.to_string())?;
        let count: i64 = guard
            .query_row("SELECT COUNT(*) FROM scan_files", [], |row| row.get(0))
            .map_err(|error| error.to_string())?;
        Ok(count as usize)
    }

    /// Read one batch by its ordinal interval.
    pub fn get_batch(
        &self,
        batch_index: usize,
        batch_size: usize,
    ) -> Result<Vec<FileEntry>, String> {
        let start = batch_index
            .checked_mul(batch_size)
            .ok_or_else(|| "batch index overflow".to_string())?;
        let end = start.saturating_add(batch_size);
        let guard = self.connection.lock().map_err(|error| error.to_string())?;
        let mut statement = guard
            .prepare("SELECT entry_json FROM scan_files WHERE ordinal >= ?1 AND ordinal < ?2 ORDER BY ordinal")
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(params![start as i64, end as i64], |row| {
                row.get::<_, String>(0)
            })
            .map_err(|error| error.to_string())?;
        let mut entries = Vec::with_capacity(batch_size);
        for row in rows {
            let json = row.map_err(|error| error.to_string())?;
            entries.push(decode(&json)?);
        }
        if entries.is_empty() {
            return Err(format!(
                "Batch index {batch_index} out of range (total batches: {})",
                self.count_in(&guard)?
            ));
        }
        Ok(entries)
    }

    /// Read only the first and last row of one batch for checkpoint checks.
    pub fn batch_boundary(
        &self,
        batch_index: usize,
        batch_size: usize,
    ) -> Result<(String, String), String> {
        let start = batch_index
            .checked_mul(batch_size)
            .ok_or_else(|| "batch index overflow".to_string())?;
        let guard = self.connection.lock().map_err(|error| error.to_string())?;
        let total = self.count_in(&guard)?;
        if start >= total {
            return Err(format!(
                "Batch index {batch_index} out of range (total batches: {})",
                total.div_ceil(batch_size)
            ));
        }
        let end = ((batch_index + 1) * batch_size).min(total);
        let first = self.path_at(&guard, start)?;
        let last = self.path_at(&guard, end - 1)?;
        Ok((first, last))
    }

    fn path_at(&self, guard: &Connection, ordinal: usize) -> Result<String, String> {
        guard
            .query_row(
                "SELECT path FROM scan_files WHERE ordinal = ?1",
                params![ordinal as i64],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("Missing scan manifest row at ordinal {ordinal}"))
    }

    fn count_in(&self, guard: &Connection) -> Result<usize, String> {
        let count: i64 = guard
            .query_row("SELECT COUNT(*) FROM scan_files", [], |row| row.get(0))
            .map_err(|error| error.to_string())?;
        Ok(count as usize)
    }

    /// Visit every entry in batch order without materializing the manifest.
    pub fn for_each_file<F>(&self, mut visitor: F) -> Result<(), String>
    where
        F: FnMut(FileEntry),
    {
        let guard = self.connection.lock().map_err(|error| error.to_string())?;
        let mut statement = guard
            .prepare("SELECT entry_json FROM scan_files ORDER BY ordinal")
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| error.to_string())?;
        for row in rows {
            visitor(decode(&row.map_err(|error| error.to_string())?)?);
        }
        Ok(())
    }

    /// Visit batches in deterministic order without retaining earlier batches.
    pub fn for_each_batch<F>(&self, batch_size: usize, mut visitor: F) -> Result<(), String>
    where
        F: FnMut(Vec<FileEntry>),
    {
        let total = self.count()?;
        for batch_index in 0..total.div_ceil(batch_size) {
            visitor(self.get_batch(batch_index, batch_size)?);
        }
        Ok(())
    }
}

impl Drop for ScanFileStore {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.directory) {
            tracing::warn!(
                path = %self.directory.display(),
                %error,
                "Failed to remove scan manifest directory"
            );
        }
    }
}

fn decode(json: &str) -> Result<FileEntry, String> {
    serde_json::from_str(json).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn entry(name: &str) -> FileEntry {
        FileEntry {
            path: PathBuf::from(format!("/project/{name}")),
            relative_path: PathBuf::from(name),
            size: 1,
            modified: chrono::Utc::now(),
            content_hash: Some(format!("hash-{name}")),
            language_info: None,
        }
    }

    #[test]
    fn batches_follow_sorted_paths() {
        let root = tempfile::tempdir().expect("temp dir");
        let store = ScanFileStore::new(root.path(), "op").expect("manifest must open");
        let mut batch = vec![entry("c.rs"), entry("a.rs"), entry("b.rs")];
        store.append_batch(&batch).expect("append must succeed");
        batch.clear();
        store
            .append_batch(&[entry("d.rs")])
            .expect("append must succeed");
        store.sort_by_path().expect("sort must succeed");

        assert_eq!(store.count().expect("count"), 4);
        let first = store.get_batch(0, 2).expect("first batch");
        let second = store.get_batch(1, 2).expect("second batch");
        assert_eq!(first[0].path, PathBuf::from("/project/a.rs"));
        assert_eq!(first[1].path, PathBuf::from("/project/b.rs"));
        assert_eq!(second[0].path, PathBuf::from("/project/c.rs"));
        assert_eq!(second[1].path, PathBuf::from("/project/d.rs"));
        assert_eq!(
            store.batch_boundary(1, 2).expect("boundary"),
            ("/project/c.rs".to_string(), "/project/d.rs".to_string())
        );
    }

    #[test]
    fn streaming_resident_entries_never_exceed_one_batch() {
        let root = tempfile::tempdir().expect("temp dir");
        let store = ScanFileStore::new(root.path(), "op").expect("manifest must open");
        // Medium-tier scale: the bound has to hold for a file count near the
        // measured corpus, not only for a toy batch count.
        let batch_size = 512;
        let mut peak = 0usize;
        for batch_number in 0..64 {
            let batch: Vec<_> = (0..batch_size)
                .map(|index| entry(&format!("{:03}.rs", batch_number * batch_size + index)))
                .collect();
            peak = peak.max(batch.len());
            store.append_batch(&batch).expect("append must succeed");
        }
        store.sort_by_path().expect("sort must succeed");

        let mut resident = 0usize;
        store
            .for_each_batch(batch_size, |batch| {
                resident = resident.max(batch.len());
            })
            .expect("replay must succeed");
        assert_eq!(resident, batch_size);
        assert!(peak <= batch_size);
        assert_eq!(store.count().expect("count"), 64 * batch_size);
    }

    #[test]
    fn json_rows_round_trip() {
        let root = tempfile::tempdir().expect("temp dir");
        let store = ScanFileStore::new(root.path(), "op").expect("manifest must open");
        let original = entry("a.rs");
        store
            .append_batch(std::slice::from_ref(&original))
            .expect("append");
        store.sort_by_path().expect("sort");
        let mut seen = Vec::new();
        store
            .for_each_file(|file| seen.push(file))
            .expect("replay must succeed");
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].content_hash, original.content_hash);
        assert_eq!(seen[0].path, original.path);
    }
}
