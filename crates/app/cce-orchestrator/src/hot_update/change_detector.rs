//! Change detection for hot update
//!
//! This module provides file change detection functionality using SQLite-based hash comparison.
//!
//! # Design Philosophy
//!
//! - Single source of truth: SQLite is the only source for file hashes
//! - No memory cache: Always read from SQLite to ensure consistency
//! - Simple and reliable: Eliminates cache synchronization issues

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use cce_scanner::{FSScanner, FileEntry, ScanOptions};
use cce_storage_sqlite::SqliteClient;
use cce_storage_sqlite::repo::{
    FileRepository, GenerationOverrideRepository, ProjectIndexManifestRepository,
};

use super::error::HotUpdateError;
use super::exclude_rules::ExcludeRules;

/// Every N scans a full re-hash is forced to bound the staleness window of
/// the mtime-based incremental scan (a file modified twice within the mtime
/// granularity with the same size would otherwise be invisible to reuse).
const FULL_SCAN_INTERVAL: u32 = 10;

/// Depth bound of the inheritance chain consulted for stored-state lookups,
/// mirroring the GC protection window of the zero-copy generation model.
const GENERATION_VIEW_DEPTH: usize = 2;

/// In-memory state backing the incremental scan.
#[derive(Debug)]
struct ScanReuseState {
    /// Entries of the previous scan, keyed by relative path.
    previous_entries: HashMap<PathBuf, FileEntry>,
    /// Scans performed since the last full (non-reusing) scan.
    scans_since_full: u32,
}

/// Result of cache update operation
#[derive(Debug, Clone, Default)]
pub struct CacheUpdateResult {
    /// Newly added files
    pub added: Vec<PathBuf>,
    /// Modified files
    pub modified: Vec<PathBuf>,
    /// Unchanged files
    pub unchanged: Vec<PathBuf>,
    /// Removed files (in database but not on disk)
    pub removed: Vec<PathBuf>,
    /// Files that failed to process
    pub failed: Vec<(PathBuf, String)>,
}

impl CacheUpdateResult {
    /// Get total changed files (added + modified + removed)
    pub fn total_changes(&self) -> usize {
        self.added.len() + self.modified.len() + self.removed.len()
    }

    /// Check if there are any changes
    pub fn has_changes(&self) -> bool {
        self.total_changes() > 0
    }

    /// Get all affected paths (added + modified + removed)
    pub fn all_affected_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        paths.extend(self.added.clone());
        paths.extend(self.modified.clone());
        paths.extend(self.removed.clone());
        paths
    }
}

/// File change detector
///
/// Detects file changes by comparing current filesystem state with cached hash in SQLite.
/// Uses SQLite as the single source of truth for file hashes.
#[derive(Debug)]
pub struct ChangeDetector {
    /// Database connection
    db: Arc<SqliteClient>,
    /// Scan options for file discovery
    scan_options: ScanOptions,
    /// Exclude rules for filtering files
    exclude_rules: Option<ExcludeRules>,
    /// Project ID for scoped queries (must be set via set_project_id before use)
    project_id: i64,
    /// Incremental-scan reuse state (previous entries + full-scan cadence).
    scan_reuse: Mutex<ScanReuseState>,
}

impl ChangeDetector {
    /// Create a new change detector
    pub fn new(db: Arc<SqliteClient>, scan_options: ScanOptions) -> Self {
        Self {
            db,
            scan_options,
            exclude_rules: None,
            project_id: 0,
            scan_reuse: Mutex::new(ScanReuseState {
                previous_entries: HashMap::new(),
                scans_since_full: 0,
            }),
        }
    }

    /// Set the project ID for scoped file change detection
    pub fn set_project_id(&mut self, project_id: i64) {
        self.project_id = project_id;
    }

    /// Update the root path for scanning
    ///
    /// This should be called when initializing with a specific project root.
    pub fn set_root_path(&mut self, root_path: &str) {
        self.scan_options.root_path = root_path.to_string();
    }

    /// Get reference to the underlying database
    pub fn db(&self) -> &Arc<SqliteClient> {
        &self.db
    }

    /// Get the current project ID
    pub fn project_id(&self) -> i64 {
        self.project_id
    }

    /// Update scan options (for config reload)
    pub fn set_scan_options(&mut self, scan_options: ScanOptions) {
        self.scan_options = scan_options;
        // New roots/patterns invalidate the previous scan's fingerprints.
        self.scan_reuse
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .previous_entries
            .clear();
        self.scan_reuse
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .scans_since_full = 0;
    }

    /// Get scan options
    pub fn scan_options(&self) -> &ScanOptions {
        &self.scan_options
    }

    /// Scan the project, reusing the previous scan's content hashes for
    /// files whose (size, mtime) fingerprint is unchanged.
    ///
    /// Every `FULL_SCAN_INTERVAL` scans a full re-hash is forced to bound the
    /// mtime-granularity staleness window (a file modified twice within the
    /// mtime granularity with the same size would otherwise be invisible).
    async fn scan_entries(&self) -> Result<Vec<FileEntry>, HotUpdateError> {
        let (reuse_previous, force_full) = {
            let state = self
                .scan_reuse
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let force_full = state.scans_since_full >= FULL_SCAN_INTERVAL;
            let reuse_previous = if force_full {
                HashMap::new()
            } else {
                state.previous_entries.clone()
            };
            (reuse_previous, force_full)
        };

        let mut scanner = FSScanner::new();
        let entries = if reuse_previous.is_empty() {
            scanner
                .scan(&self.scan_options)
                .map_err(|e| HotUpdateError::scan(e.to_string()))?
        } else {
            scanner
                .scan_incremental(&self.scan_options, &reuse_previous)
                .map_err(|e| HotUpdateError::scan(e.to_string()))?
        };

        let mut state = self
            .scan_reuse
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.previous_entries = entries
            .iter()
            .map(|entry| (entry.relative_path.clone(), entry.clone()))
            .collect();
        state.scans_since_full = if force_full {
            0
        } else {
            state.scans_since_full.saturating_add(1)
        };
        drop(state);

        Ok(entries)
    }

    /// Check if there are file changes (without updating)
    ///
    /// Performs a quick scan to detect if any files have changed or been deleted.
    pub async fn check_changes(&self) -> bool {
        match self.scan_entries().await {
            Ok(entries) => {
                let mut disk_count = 0usize;
                let mut current_paths = HashSet::with_capacity(entries.len());
                // Entries without a content hash are invisible to the per-file
                // comparison below, so a deletion could hide behind balanced
                // counts when an unhashable file appears in the same scan.
                let mut saw_unhashable = false;
                for entry in &entries {
                    if let Some(rules) = &self.exclude_rules {
                        if rules.should_exclude(entry) {
                            continue;
                        }
                    }
                    disk_count += 1;
                    current_paths.insert(entry.relative_path.clone());

                    match &entry.content_hash {
                        Some(hash) => {
                            let stored_hash = self.get_stored_hash(&entry.relative_path).await;
                            if stored_hash.as_deref() != Some(hash.as_str()) {
                                return true;
                            }
                        }
                        None => saw_unhashable = true,
                    }
                }

                // O(1) fast negative: equal counts with every disk file
                // hashed and matched imply the path sets coincide. Any other
                // combination (count mismatch, or an unhashable file that can
                // mask a balanced add+delete batch) is settled by the exact
                // set difference used by scan_and_detect.
                if let Ok(db_count) = self.count_stored_files().await {
                    if db_count != disk_count || saw_unhashable {
                        if let Ok(removed) = self.find_removed_files(&current_paths).await {
                            if !removed.is_empty() {
                                return true;
                            }
                        }
                    }
                }

                false
            }
            Err(e) => {
                tracing::error!(error = %e, "Failed to scan for file changes");
                false
            }
        }
    }

    /// Scan files and detect changes
    ///
    /// Returns detailed information about added, modified, removed, and unchanged files.
    pub async fn scan_and_detect(&self) -> Result<CacheUpdateResult, HotUpdateError> {
        let entries = self.scan_entries().await?;

        let mut result = CacheUpdateResult::default();

        // Filter entries using exclude rules
        let filtered_entries: Vec<_> = entries
            .iter()
            .filter(|entry| {
                if let Some(rules) = &self.exclude_rules {
                    !rules.should_exclude(entry)
                } else {
                    true // No rules, include all
                }
            })
            .cloned()
            .collect();

        let current_paths: HashSet<PathBuf> = filtered_entries
            .iter()
            .map(|e| e.relative_path.clone())
            .collect();

        // Find removed files (in database but not on disk)
        let removed = self.find_removed_files(&current_paths).await?;
        for path in &removed {
            result.removed.push(path.clone());
        }

        // Load every stored hash of the current epoch in one query so the
        // per-file comparison below touches the database exactly once (and
        // resolves the epoch manifest once) instead of once per file.
        let stored_hashes = self.load_stored_hashes().await?;

        // Check for added/modified files
        for entry in &filtered_entries {
            if let Some(new_hash) = &entry.content_hash {
                match stored_hashes.get(&entry.relative_path) {
                    None => {
                        // New file
                        result.added.push(entry.relative_path.clone());
                    }
                    Some(old) if old != new_hash => {
                        // Modified file
                        result.modified.push(entry.relative_path.clone());
                    }
                    _ => {
                        // Unchanged
                        result.unchanged.push(entry.relative_path.clone());
                    }
                }
            }
        }

        Ok(result)
    }

    /// Initialize detector by scanning files and storing their hashes in SQLite.
    ///
    /// Should be called during initialization to establish the baseline.
    pub async fn initialize(&self) -> Result<usize, HotUpdateError> {
        let entries = self.scan_entries().await?;

        // Apply exclude rules
        let filtered_entries: Vec<_> = entries
            .iter()
            .filter(|entry| {
                if let Some(rules) = &self.exclude_rules {
                    !rules.should_exclude(entry)
                } else {
                    true
                }
            })
            .cloned()
            .collect();

        // Store every file's hash in SQLite so subsequent scan_and_detect()
        // sees them as "unchanged" rather than "added".
        let conn = self
            .db
            .write_connection()
            .map_err(|e| HotUpdateError::hot_update(format!("Failed to get connection: {}", e)))?;
        let tx = conn.unchecked_transaction().map_err(|e| {
            HotUpdateError::hot_update(format!("Failed to start transaction: {}", e))
        })?;

        let project_id = self.project_id;

        // Ensure project exists before inserting file records (FK constraint)
        let root_path = &self.scan_options.root_path;
        tx.execute(
            "INSERT OR IGNORE INTO projects (id, name, root_path, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?4)",
            rusqlite::params![
                project_id,
                format!("project_{}", project_id),
                root_path,
                chrono::Utc::now().timestamp(),
            ],
        )
        .map_err(|e| {
            HotUpdateError::hot_update(format!("Failed to ensure project exists: {}", e))
        })?;

        for entry in &filtered_entries {
            if let Some(hash) = &entry.content_hash {
                FileRepository::upsert_or_update_hash(&tx, &entry.relative_path, hash, project_id)
                    .map_err(|e| {
                        HotUpdateError::hot_update(format!("Failed to store hash: {}", e))
                    })?;
            }
        }

        tx.commit().map_err(|e| {
            HotUpdateError::hot_update(format!("Failed to commit transaction: {}", e))
        })?;

        tracing::info!(
            count = filtered_entries.len(),
            "Change detector initialized"
        );

        Ok(filtered_entries.len())
    }

    /// Update file cache with new hashes after processing
    ///
    /// This method should be called after files are successfully processed
    /// to update their hashes in the database. This prevents the same files
    /// from being re-processed in subsequent scans.
    ///
    /// # Arguments
    ///
    /// * `entries` - FileEntry items with content hashes to update in the database
    pub async fn update_cache_with_hashes(
        &self,
        entries: &[cce_scanner::FileEntry],
    ) -> Result<(), HotUpdateError> {
        if entries.is_empty() {
            return Ok(());
        }

        let conn = self
            .db
            .write_connection()
            .map_err(|e| HotUpdateError::hot_update(format!("Failed to get connection: {}", e)))?;
        // Baseline hashes are recorded against the active generation's own
        // epoch: unchanged files keep resolving through the inheritance chain.
        let epoch = Self::generation_view(&conn, self.project_id)?.0;
        let tx = conn.unchecked_transaction().map_err(|e| {
            HotUpdateError::hot_update(format!("Failed to start transaction: {}", e))
        })?;

        let project_id = self.project_id;
        for entry in entries {
            if let Some(hash) = &entry.content_hash {
                FileRepository::insert_hash_for_epoch(
                    &tx,
                    &entry.relative_path,
                    hash,
                    project_id,
                    epoch,
                )
                .map_err(|e| HotUpdateError::hot_update(format!("Failed to update hash: {}", e)))?;
            }
        }

        tx.commit().map_err(|e| {
            HotUpdateError::hot_update(format!("Failed to commit transaction: {}", e))
        })?;

        tracing::trace!(
            count = entries.len(),
            "Updated {} file hashes in cache",
            entries.len()
        );
        Ok(())
    }

    /// Get stored hash for a file path from SQLite
    /// Load every visible `(path, content_hash)` pair of the active
    /// generation view in a single pass.
    ///
    /// Under zero-copy inheritance the visible set is `own rows ∪ ancestor
    /// rows − overridden files`: nearer generations overwrite ancestor rows
    /// for the same path, and files registered as replaced/deleted in the own
    /// generation never resolve against an ancestor. Used by
    /// [`Self::scan_and_detect`] so per-file comparison touches the database
    /// once per round (and resolves the epoch view once) instead of once per
    /// file. Rows without a content hash are skipped: a file whose stored hash
    /// is NULL is indistinguishable from an absent record and is treated as
    /// "added", matching the single-point lookup semantics.
    async fn load_stored_hashes(&self) -> Result<HashMap<PathBuf, String>, HotUpdateError> {
        let conn = self
            .db
            .read_connection()
            .map_err(|e| HotUpdateError::hot_update(format!("Failed to get connection: {}", e)))?;

        let project_id = self.project_id;
        let (own_epoch, ancestors, excluded_files) = Self::generation_view(&conn, project_id)?;

        let mut stored = HashMap::new();
        // Ancestor generations oldest-first so nearer generations win for
        // paths that exist in more than one generation of the chain.
        for epoch in ancestors.iter().rev() {
            let mut stmt = conn
                .prepare(
                    "SELECT path, content_hash FROM files
                     WHERE project_id = ?1 AND epoch = ?2 AND content_hash IS NOT NULL",
                )
                .map_err(|e| {
                    HotUpdateError::hot_update(format!("Failed to prepare statement: {}", e))
                })?;

            let rows = stmt
                .query_map(rusqlite::params![project_id, epoch], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(|e| HotUpdateError::hot_update(format!("Failed to query files: {}", e)))?;

            for row in rows {
                let (path, hash) = row.map_err(|e| {
                    HotUpdateError::hot_update(format!("Failed to read file row: {}", e))
                })?;
                stored.insert(PathBuf::from(path), hash);
            }
        }

        // Overridden files' *inherited* rows are invisible; this only ever
        // drops entries resolved from an ancestor. A replaced file always
        // owns newer rows in its own generation.
        for path in &excluded_files {
            stored.remove(&PathBuf::from(path));
        }

        // Own-generation rows are applied last and are never masked by
        // overrides: an override means "do not resolve against ancestors",
        // not "this file is invisible in its own generation".
        let mut stmt = conn
            .prepare(
                "SELECT path, content_hash FROM files
                 WHERE project_id = ?1 AND epoch = ?2 AND content_hash IS NOT NULL",
            )
            .map_err(|e| {
                HotUpdateError::hot_update(format!("Failed to prepare statement: {}", e))
            })?;
        let rows = stmt
            .query_map(rusqlite::params![project_id, own_epoch], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| HotUpdateError::hot_update(format!("Failed to query files: {}", e)))?;
        for row in rows {
            let (path, hash) = row.map_err(|e| {
                HotUpdateError::hot_update(format!("Failed to read file row: {}", e))
            })?;
            stored.insert(PathBuf::from(path), hash);
        }

        Ok(stored)
    }

    /// Get the visible stored hash for a file path.
    ///
    /// Resolves the generation view "own first, miss → ancestor": a hit in
    /// the own generation always wins; an own-generation override hides the
    /// ancestors (replaced without own rows must not surface, and deleted
    /// files are invisible everywhere).
    async fn get_stored_hash(&self, path: &std::path::Path) -> Option<String> {
        let conn = match self.db.read_connection() {
            Ok(c) => c,
            Err(_) => return None,
        };

        let project_id = self.project_id;
        let path_str = path.to_str()?;
        let Ok((own_epoch, ancestors, excluded_files)) = Self::generation_view(&conn, project_id)
        else {
            return None;
        };
        let mut epochs = Vec::with_capacity(ancestors.len() + 1);
        if !excluded_files.iter().any(|excluded| excluded == path_str) {
            epochs.extend(ancestors);
        }
        epochs.push(own_epoch);

        for epoch in epochs {
            match FileRepository::get_content_hash_by_path_at_epoch(
                &conn, path_str, project_id, epoch,
            ) {
                Ok(Some(hash)) => return Some(hash),
                Ok(None) => continue,
                Err(e) => {
                    tracing::trace!(path = %path.display(), error = %e, "Failed to get stored hash");
                    return None;
                }
            }
        }
        None
    }

    /// Count file records visible from the active generation view.
    pub async fn count_stored_files(&self) -> Result<usize, HotUpdateError> {
        let conn = self
            .db
            .read_connection()
            .map_err(|e| HotUpdateError::hot_update(format!("Failed to get connection: {}", e)))?;
        let (own_epoch, ancestors, excluded_files) = Self::generation_view(&conn, self.project_id)?;

        // Own rows win over inherited rows for duplicated paths; overridden
        // files' ancestor rows are hidden, their own rows never are.
        let mut visible: HashSet<String> = HashSet::new();
        for epoch in ancestors.iter().rev() {
            let mut stmt = conn
                .prepare("SELECT DISTINCT path FROM files WHERE project_id = ?1 AND epoch = ?2")
                .map_err(|e| {
                    HotUpdateError::hot_update(format!("Failed to prepare statement: {}", e))
                })?;
            let rows = stmt
                .query_map(rusqlite::params![self.project_id, epoch], |row| {
                    row.get::<_, String>(0)
                })
                .map_err(|e| {
                    HotUpdateError::hot_update(format!("Failed to count stored files: {}", e))
                })?;
            for path in rows.into_iter().flatten() {
                visible.insert(path);
            }
        }
        for path in &excluded_files {
            visible.remove(path);
        }
        let mut stmt = conn
            .prepare("SELECT DISTINCT path FROM files WHERE project_id = ?1 AND epoch = ?2")
            .map_err(|e| {
                HotUpdateError::hot_update(format!("Failed to prepare statement: {}", e))
            })?;
        let rows = stmt
            .query_map(rusqlite::params![self.project_id, own_epoch], |row| {
                row.get::<_, String>(0)
            })
            .map_err(|e| {
                HotUpdateError::hot_update(format!("Failed to count stored files: {}", e))
            })?;
        for path in rows.into_iter().flatten() {
            visible.insert(path);
        }

        Ok(visible.len())
    }

    /// Find removed files (visible in the generation view but not on disk)
    async fn find_removed_files(
        &self,
        current_paths: &HashSet<PathBuf>,
    ) -> Result<Vec<PathBuf>, HotUpdateError> {
        let conn = self
            .db
            .read_connection()
            .map_err(|e| HotUpdateError::hot_update(format!("Failed to get connection: {}", e)))?;

        let project_id = self.project_id;
        let (own_epoch, ancestors, excluded_files) = Self::generation_view(&conn, project_id)?;

        // Visible path set: ancestor rows − overridden files, then the own
        // generation's rows on top (overrides never mask own rows).
        let mut visible: HashSet<PathBuf> = HashSet::new();
        for epoch in ancestors.iter().rev() {
            let mut stmt = conn
                .prepare("SELECT path FROM files WHERE project_id = ?1 AND epoch = ?2")
                .map_err(|e| {
                    HotUpdateError::hot_update(format!("Failed to prepare statement: {}", e))
                })?;

            let rows = stmt
                .query_map(rusqlite::params![project_id, epoch], |row| {
                    row.get::<_, String>(0)
                })
                .map_err(|e| HotUpdateError::hot_update(format!("Failed to query files: {}", e)))?;

            for path_str in rows.into_iter().flatten() {
                visible.insert(PathBuf::from(path_str));
            }
        }
        for path in &excluded_files {
            visible.remove(&PathBuf::from(path));
        }
        let mut stmt = conn
            .prepare("SELECT path FROM files WHERE project_id = ?1 AND epoch = ?2")
            .map_err(|e| {
                HotUpdateError::hot_update(format!("Failed to prepare statement: {}", e))
            })?;
        let rows = stmt
            .query_map(rusqlite::params![project_id, own_epoch], |row| {
                row.get::<_, String>(0)
            })
            .map_err(|e| HotUpdateError::hot_update(format!("Failed to query files: {}", e)))?;
        for path_str in rows.into_iter().flatten() {
            visible.insert(PathBuf::from(path_str));
        }

        let mut removed = Vec::new();
        for path in visible {
            if !current_paths.contains(&path) {
                removed.push(path);
            }
        }

        Ok(removed)
    }

    /// Resolve the visible generations of the active publication:
    /// `(own epoch, ancestor epochs oldest-first, overridden file paths)`.
    ///
    /// Under zero-copy inheritance the published generation owns rows only
    /// for its changed files; unchanged files stay in its ancestors, and the
    /// files registered in `generation_overrides` must not resolve against
    /// them. Projects without a manifest fall back to the legacy
    /// `active_epoch` meta key as a parent-free full generation.
    fn generation_view(
        conn: &rusqlite::Connection,
        project_id: i64,
    ) -> Result<(i64, Vec<i64>, Vec<String>), HotUpdateError> {
        let manifest =
            ProjectIndexManifestRepository::get_active(conn, project_id).map_err(|error| {
                HotUpdateError::hot_update(format!(
                    "Failed to read active project manifest: {error}"
                ))
            })?;
        let Some(manifest) = manifest else {
            // No manifest means the data generation was never published; a
            // missing legacy meta row is the legitimate default 0, while real
            // DB failures are propagated instead of silently operating on epoch 0.
            let epoch = cce_storage_sqlite::ProjectRepository::meta_get_int_optional(
                conn,
                project_id,
                "active_epoch",
            )
            .map_err(|error| {
                HotUpdateError::hot_update(format!("Failed to read active_epoch meta: {error}"))
            })
            .map(|value| value.unwrap_or(0))?;
            return Ok((epoch, Vec::new(), Vec::new()));
        };

        let own_epoch = manifest.data_epoch;
        let mut ancestors = Vec::new();
        let mut current = manifest.parent_data_epoch;
        while ancestors.len() < GENERATION_VIEW_DEPTH
            && let Some(epoch) = current
            && epoch > 0
        {
            ancestors.push(epoch);
            current = ProjectIndexManifestRepository::parent_data_epoch_of(conn, project_id, epoch)
                .map_err(|error| {
                    HotUpdateError::hot_update(format!("Failed to walk inheritance chain: {error}"))
                })?;
        }
        let excluded_files =
            GenerationOverrideRepository::list_for_generation(conn, project_id, own_epoch)
                .map_err(|error| {
                    HotUpdateError::hot_update(format!(
                        "Failed to read generation overrides: {error}"
                    ))
                })?
                .into_iter()
                .map(|entry| entry.file_path)
                .collect();
        Ok((own_epoch, ancestors, excluded_files))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_storage_sqlite::SqliteClient;
    use cce_storage_sqlite::repo::file_repo::FileRepository;

    #[test]
    fn test_change_detector_creation() {
        let db = Arc::new(SqliteClient::in_memory().unwrap());
        let _detector = ChangeDetector::new(db, ScanOptions::default());
    }

    #[tokio::test]
    async fn test_check_changes_detects_pure_deletion() {
        // Create a temp directory that acts as the "project root" with no files.
        let tmp = std::env::temp_dir().join(format!("cce_test_deletion_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let db = Arc::new(SqliteClient::in_memory().unwrap());
        let mut detector = ChangeDetector::new(
            db.clone(),
            ScanOptions {
                root_path: tmp.to_string_lossy().to_string(),
                ..Default::default()
            },
        );
        detector.set_project_id(1);

        // Insert a file record directly into the DB so the detector believes
        // a file exists that is no longer on disk.
        {
            let conn = db.write_connection().unwrap();
            let tx = conn.unchecked_transaction().unwrap();
            // Ensure project exists (FK constraint)
            tx.execute(
                "INSERT OR IGNORE INTO projects (id, name, root_path, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?4)",
                rusqlite::params![1, "test", &tmp.to_string_lossy().to_string(), 0],
            )
            .unwrap();
            FileRepository::upsert_or_update_hash(&tx, &PathBuf::from("deleted.rs"), "hash1", 1)
                .unwrap();
            tx.commit().unwrap();
        }

        // check_changes should detect the deletion (DB has 1 file, disk has 0).
        assert!(detector.check_changes().await, "must detect pure deletion");

        // Cleanup
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn test_delete_by_path_removes_hash_record() {
        let db = Arc::new(SqliteClient::in_memory().unwrap());
        {
            let conn = db.write_connection().unwrap();
            let tx = conn.unchecked_transaction().unwrap();
            // Insert project record (FK constraint)
            tx.execute(
                "INSERT OR IGNORE INTO projects (id, name, root_path, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?4)",
                rusqlite::params![1, "test", "/tmp", 0],
            )
            .unwrap();
            FileRepository::upsert_or_update_hash(
                &tx,
                &std::path::PathBuf::from("deleted.rs"),
                "hash1",
                1,
            )
            .unwrap();
            tx.commit().unwrap();
        }

        // Verify the record exists.
        let before = {
            let conn = db.write_connection().unwrap();
            FileRepository::get_content_hash_by_path(&conn, "deleted.rs", 1).unwrap()
        };
        assert!(before.is_some(), "hash must exist before deletion");

        // Delete via FileRepository.
        {
            let conn = db.write_connection().unwrap();
            let tx = conn.unchecked_transaction().unwrap();
            FileRepository::delete_by_path(&tx, "deleted.rs", 1).unwrap();
            tx.commit().unwrap();
        }

        // Verify the record is gone (new scope to release MutexGuard).
        let after = {
            let conn = db.write_connection().unwrap();
            FileRepository::get_content_hash_by_path(&conn, "deleted.rs", 1).unwrap()
        };
        assert!(after.is_none(), "hash must be removed after deletion");
    }

    #[tokio::test]
    async fn test_check_changes_returns_false_when_no_changes() {
        let tmp = std::env::temp_dir().join(format!("cce_test_nochange_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        // Create a file that the scanner will find.
        std::fs::write(tmp.join("main.rs"), "fn main() {}").unwrap();

        let db = Arc::new(SqliteClient::in_memory().unwrap());
        let mut detector = ChangeDetector::new(
            db.clone(),
            ScanOptions {
                root_path: tmp.to_string_lossy().to_string(),
                ..Default::default()
            },
        );
        detector.set_project_id(1);

        // Initialize the detector so it stores the hash for main.rs.
        detector.initialize().await.unwrap();

        // No changes: same file, same content → false.
        assert!(!detector.check_changes().await, "no changes expected");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn test_delete_file_then_scan_twice_returns_no_change_on_second_scan() {
        let tmp = std::env::temp_dir().join(format!("cce_test_deltwice_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let file_path = tmp.join("main.rs");
        std::fs::write(&file_path, "fn main() {}").unwrap();

        let db = Arc::new(SqliteClient::in_memory().unwrap());
        let mut detector = ChangeDetector::new(
            db.clone(),
            ScanOptions {
                root_path: tmp.to_string_lossy().to_string(),
                ..Default::default()
            },
        );
        detector.set_project_id(1);

        // Initialize → stores hash in DB.
        detector.initialize().await.unwrap();

        // Delete the file from disk.
        std::fs::remove_file(&file_path).unwrap();

        // First scan: must detect the deletion.
        assert!(
            detector.check_changes().await,
            "first scan must detect deletion"
        );

        // Simulate commit_file_hashes: remove the hash record from DB.
        {
            let conn = db.write_connection().unwrap();
            let tx = conn.unchecked_transaction().unwrap();
            // Also ensure project exists
            tx.execute(
                "INSERT OR IGNORE INTO projects (id, name, root_path, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?4)",
                rusqlite::params![1, "test", &tmp.to_string_lossy().to_string(), 0],
            )
            .unwrap();
            // Delete the hash that was stored during initialize().
            // The detector keys on the project-relative path.
            FileRepository::delete_by_path(&tx, "main.rs", 1).unwrap();
            tx.commit().unwrap();
        }

        // Second scan: no changes expected (file is gone and hash is removed).
        assert!(
            !detector.check_changes().await,
            "second scan must return no changes after hash is removed"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn test_check_changes_detects_deletion_with_balanced_counts() {
        // A deletion plus an addition in the same batch balances the counts:
        // the naive "DB count > disk count" check misses the removal. The
        // addition here is unhashable (oversized file), so the per-file hash
        // loop cannot catch it either — only the exact set difference does.
        let tmp = std::env::temp_dir().join(format!("cce_test_balanced_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("kept.rs"), "ab").unwrap();
        // Larger than max_file_size -> scanner reports no content hash.
        std::fs::write(tmp.join("big.bin"), "01234567").unwrap();

        let opts = ScanOptions {
            root_path: tmp.to_string_lossy().to_string(),
            max_file_size: Some(4),
            ..Default::default()
        };

        let db = Arc::new(SqliteClient::in_memory().unwrap());
        let mut detector = ChangeDetector::new(db.clone(), opts.clone());
        detector.set_project_id(1);

        // Hash of kept.rs as the scanner computes it (big.bin stays unhashable).
        let mut scanner = FSScanner::new();
        let scanned = scanner.scan(&opts).unwrap();
        let kept_hash = scanned
            .iter()
            .find(|e| e.path.ends_with("kept.rs"))
            .expect("kept.rs must be scanned")
            .content_hash
            .clone()
            .expect("kept.rs must be hashed");
        assert!(
            scanned
                .iter()
                .find(|e| e.path.ends_with("big.bin"))
                .expect("big.bin must be scanned")
                .content_hash
                .is_none(),
            "big.bin must be unhashable under max_file_size = 4"
        );

        // DB holds kept.rs (matching) and deleted.rs (no longer on disk).
        {
            let conn = db.write_connection().unwrap();
            let tx = conn.unchecked_transaction().unwrap();
            tx.execute(
                "INSERT OR IGNORE INTO projects (id, name, root_path, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?4)",
                rusqlite::params![1, "test", &tmp.to_string_lossy().to_string(), 0],
            )
            .unwrap();
            FileRepository::upsert_or_update_hash(&tx, &PathBuf::from("kept.rs"), &kept_hash, 1)
                .unwrap();
            FileRepository::upsert_or_update_hash(&tx, &PathBuf::from("deleted.rs"), "h1", 1)
                .unwrap();
            tx.commit().unwrap();
        }

        // disk_count (2) == db_count (2), so a count-only deletion check
        // would return false and miss the removal of deleted.rs.
        assert!(
            detector.check_changes().await,
            "balanced add+delete batch must still detect the deletion"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn test_scan_and_detect_detects_modification_across_incremental_scans() {
        // A file modified between two scans must be classified as "modified"
        // even though the second scan reuses the first scan's hashes (its
        // size changes, so the reuse fingerprint misses and the file is
        // re-hashed with the fresh content).
        let tmp = std::env::temp_dir().join(format!("cce_test_incr_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("main.rs"), "fn main() {}").unwrap();

        let opts = ScanOptions {
            root_path: tmp.to_string_lossy().to_string(),
            ..Default::default()
        };

        let db = Arc::new(SqliteClient::in_memory().unwrap());
        let mut detector = ChangeDetector::new(db.clone(), opts.clone());
        detector.set_project_id(1);
        detector.initialize().await.unwrap();

        // First detection round: nothing changed.
        let first = detector.scan_and_detect().await.unwrap();
        assert!(first.added.is_empty() && first.modified.is_empty());
        assert_eq!(first.unchanged, vec![PathBuf::from("main.rs")]);

        // Modify the file (size changes -> fingerprint differs -> re-hash).
        std::fs::write(tmp.join("main.rs"), "fn main() {}\nfn extra() {}").unwrap();

        let second = detector.scan_and_detect().await.unwrap();
        assert!(second.added.is_empty());
        assert_eq!(second.modified, vec![PathBuf::from("main.rs")]);
        assert!(second.unchanged.is_empty());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn test_incremental_reuse_preserves_unchanged_classification() {
        // Repeated scans of an untouched tree stay "unchanged": the reuse
        // path (same size + mtime) must produce the same classification as
        // the full scan it replaces.
        let tmp = std::env::temp_dir().join(format!("cce_test_reuse_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("a.rs"), "fn a() {}").unwrap();
        std::fs::write(tmp.join("b.rs"), "fn b() {}").unwrap();

        let opts = ScanOptions {
            root_path: tmp.to_string_lossy().to_string(),
            ..Default::default()
        };

        let db = Arc::new(SqliteClient::in_memory().unwrap());
        let mut detector = ChangeDetector::new(db.clone(), opts.clone());
        detector.set_project_id(1);
        detector.initialize().await.unwrap();

        for _ in 0..3 {
            let result = detector.scan_and_detect().await.unwrap();
            assert!(result.added.is_empty(), "no additions: {:?}", result.added);
            assert!(
                result.modified.is_empty(),
                "no modifications: {:?}",
                result.modified
            );
            assert_eq!(result.unchanged.len(), 2);
            assert!(result.removed.is_empty());
        }

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn test_scan_and_detect_batch_read_matches_pointwise_results() {
        // The batch hash load must produce the same added/modified/unchanged
        // classification as the old per-file lookups, including epoch
        // isolation: records in a non-active epoch must be invisible.
        let tmp = std::env::temp_dir().join(format!("cce_test_batch_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("new.rs"), "fn new() {}").unwrap();
        std::fs::write(tmp.join("mod.rs"), "fn kept() {}").unwrap();
        std::fs::write(tmp.join("changed.rs"), "fn old() {}").unwrap();

        let opts = ScanOptions {
            root_path: tmp.to_string_lossy().to_string(),
            ..Default::default()
        };

        let db = Arc::new(SqliteClient::in_memory().unwrap());
        let mut detector = ChangeDetector::new(db.clone(), opts.clone());
        detector.set_project_id(1);

        let mut scanner = FSScanner::new();
        let scanned = scanner.scan(&opts).unwrap();
        let hash_of = |name: &str| {
            scanned
                .iter()
                .find(|e| e.path.ends_with(name))
                .unwrap_or_else(|| panic!("{name} must be scanned"))
                .content_hash
                .clone()
                .expect("file must be hashed")
        };
        let mod_hash = hash_of("mod.rs");

        {
            let conn = db.write_connection().unwrap();
            let tx = conn.unchecked_transaction().unwrap();
            tx.execute(
                "INSERT OR IGNORE INTO projects (id, name, root_path, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?4)",
                rusqlite::params![1, "test", &tmp.to_string_lossy().to_string(), 0],
            )
            .unwrap();
            // Unchanged: hash matches the disk content.
            FileRepository::upsert_or_update_hash(&tx, &PathBuf::from("mod.rs"), &mod_hash, 1)
                .unwrap();
            // Modified: stored hash differs from the disk content.
            FileRepository::upsert_or_update_hash(
                &tx,
                &PathBuf::from("changed.rs"),
                "stale-hash",
                1,
            )
            .unwrap();
            // new.rs exists only in a non-active epoch (1): the active epoch
            // (0) snapshot must not see it, so it stays "added".
            FileRepository::insert_hash_for_epoch(
                &tx,
                &PathBuf::from("new.rs"),
                &hash_of("new.rs"),
                1,
                1,
            )
            .unwrap();
            tx.commit().unwrap();
        }

        let result = detector.scan_and_detect().await.unwrap();
        assert!(
            result.removed.is_empty(),
            "no files removed: {:?}",
            result.removed
        );
        assert_eq!(
            result.added,
            vec![PathBuf::from("new.rs")],
            "new.rs must be added (epoch-1 record is isolated)"
        );
        assert_eq!(result.modified, vec![PathBuf::from("changed.rs")]);
        assert_eq!(result.unchanged, vec![PathBuf::from("mod.rs")]);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Seed an inherited publication: generation 1 is full, generation 2
    /// inherits from it and owns only `src/changed.rs`, with `src/gone.rs`
    /// registered as deleted.
    fn seed_inherited_view(db: &SqliteClient) {
        use cce_storage_sqlite::GenerationOverrideRepository;
        use cce_storage_sqlite::OverrideDisposition;
        use cce_storage_sqlite::ProjectIndexManifestRepository;

        let conn = db.write_connection().unwrap();
        let tx = conn.unchecked_transaction().unwrap();
        tx.execute(
            "INSERT OR IGNORE INTO projects (id, name, root_path, created_at, updated_at)
             VALUES (1, 'test', '/tmp', 0, 0)",
            [],
        )
        .unwrap();
        ProjectIndexManifestRepository::activate(&tx, 1, 1, 0, "gen-1", None).unwrap();
        for path in ["src/keep.rs", "src/gone.rs", "src/removed.rs"] {
            FileRepository::insert_hash_for_epoch(
                &tx,
                &PathBuf::from(path),
                &format!("hash-{path}"),
                1,
                1,
            )
            .unwrap();
        }
        ProjectIndexManifestRepository::begin_building(&tx, 1, 2, "gen-2", None).unwrap();
        ProjectIndexManifestRepository::set_parent_data_epoch(&tx, 1, "gen-2", Some(1)).unwrap();
        ProjectIndexManifestRepository::activate(&tx, 1, 2, 0, "gen-2", None).unwrap();
        FileRepository::insert_hash_for_epoch(
            &tx,
            &PathBuf::from("src/changed.rs"),
            "hash-changed",
            1,
            2,
        )
        .unwrap();
        GenerationOverrideRepository::upsert(
            &tx,
            1,
            2,
            "src/gone.rs",
            OverrideDisposition::Deleted,
        )
        .unwrap();
        tx.commit().unwrap();
    }

    #[tokio::test]
    async fn stored_hashes_merge_parent_chain_and_honor_overrides() {
        use std::path::Path;

        let db = Arc::new(SqliteClient::in_memory().unwrap());
        seed_inherited_view(&db);
        let mut detector = ChangeDetector::new(db.clone(), ScanOptions::default());
        detector.set_project_id(1);

        let stored = detector.load_stored_hashes().await.unwrap();
        assert_eq!(
            stored.get(Path::new("src/keep.rs")).map(String::as_str),
            Some("hash-src/keep.rs"),
            "unchanged files must resolve against the inherited parent"
        );
        assert_eq!(
            stored.get(Path::new("src/changed.rs")).map(String::as_str),
            Some("hash-changed"),
            "own-generation rows win"
        );
        assert!(
            !stored.contains_key(Path::new("src/gone.rs")),
            "deleted files' parent rows must stay hidden"
        );
    }

    #[tokio::test]
    async fn removed_detection_covers_parent_resident_files() {
        let db = Arc::new(SqliteClient::in_memory().unwrap());
        seed_inherited_view(&db);
        let mut detector = ChangeDetector::new(db.clone(), ScanOptions::default());
        detector.set_project_id(1);

        // Disk only still has the changed file: both parent-resident files
        // are reported removed; the overridden `src/gone.rs` must never be
        // reported because its visibility is already settled by the override.
        let current: HashSet<PathBuf> = [PathBuf::from("src/changed.rs")].into_iter().collect();
        let mut removed = detector.find_removed_files(&current).await.unwrap();
        removed.sort();
        assert_eq!(
            removed,
            vec![
                PathBuf::from("src/keep.rs"),
                PathBuf::from("src/removed.rs")
            ]
        );
    }
}
