//! Project-scoped file content-hash cache.
//!
//! Change detection asks the `files` table two questions: which paths does
//! the published generation consider indexed, and with which content hash.
//! Both answers depend on the generation view — a generation owns rows only
//! for its changed files and inherits the rest — so the view resolution and
//! the queries live here next to the schema instead of being re-derived in
//! the application layer.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use cce_types::StorageError;
use rusqlite::{Connection, params};

use crate::SqliteClient;
use crate::helpers::execute_query;
use crate::repo::{
    FileRepository, GenerationOverrideRepository, ProjectIndexManifestRepository, ProjectRepository,
};

/// Depth bound of the inheritance chain consulted for stored-state lookups,
/// mirroring the GC protection window of the zero-copy generation model.
const GENERATION_VIEW_DEPTH: usize = 2;

/// Visible generations of a project's published file-hash rows.
#[derive(Debug, Clone)]
pub struct GenerationView {
    /// The published generation's own epoch.
    pub own_epoch: i64,
    /// Ancestor epochs of the inheritance chain, nearest first.
    pub ancestors: Vec<i64>,
    /// Paths whose inherited rows are hidden by an own-generation override.
    pub excluded_files: Vec<String>,
}

impl GenerationView {
    /// Resolve the visible generations of `project_id`.
    ///
    /// A project without an active manifest was never published: its legacy
    /// `active_epoch` meta key then records how far indexing got, and a
    /// missing row legitimately means "nothing indexed yet" (epoch 0, no
    /// ancestors). Real DB failures propagate instead of degrading into an
    /// empty view.
    pub fn resolve(conn: &Connection, project_id: i64) -> Result<Self, StorageError> {
        let Some(manifest) = ProjectIndexManifestRepository::get_active(conn, project_id)? else {
            let own_epoch =
                ProjectRepository::meta_get_int_optional(conn, project_id, "active_epoch")?
                    .unwrap_or(0);
            return Ok(Self {
                own_epoch,
                ancestors: Vec::new(),
                excluded_files: Vec::new(),
            });
        };

        let own_epoch = manifest.data_epoch;
        let mut ancestors = Vec::new();
        let mut current = manifest.parent_data_epoch;
        while ancestors.len() < GENERATION_VIEW_DEPTH
            && let Some(epoch) = current
            && epoch > 0
        {
            ancestors.push(epoch);
            current =
                ProjectIndexManifestRepository::parent_data_epoch_of(conn, project_id, epoch)?;
        }
        let excluded_files =
            GenerationOverrideRepository::list_for_generation(conn, project_id, own_epoch)?
                .into_iter()
                .map(|entry| entry.file_path)
                .collect();
        Ok(Self {
            own_epoch,
            ancestors,
            excluded_files,
        })
    }

    /// Whether `path` is registered as replaced/deleted in `own_epoch`.
    fn is_excluded(&self, path: &str) -> bool {
        self.excluded_files.iter().any(|excluded| excluded == path)
    }
}

/// Content-hash cache of one project, scoped to the active publication.
///
/// Every read resolves the generation view once and applies the same
/// visibility rule: own-generation rows win, overrides hide inherited rows
/// but never rows the published generation owns itself.
pub struct FileHashCache {
    client: Arc<SqliteClient>,
    project_id: i64,
}

impl FileHashCache {
    /// Create a cache view over `project_id`.
    pub fn new(client: Arc<SqliteClient>, project_id: i64) -> Self {
        Self { client, project_id }
    }

    /// The project whose hashes this cache exposes.
    pub fn project_id(&self) -> i64 {
        self.project_id
    }

    /// Resolve the generation view every read below is built on.
    pub fn view(&self) -> Result<GenerationView, StorageError> {
        let conn = self.client.read_connection()?;
        GenerationView::resolve(&conn, self.project_id)
    }

    /// Published data epoch; `None` only when the project was never indexed
    /// and carries no legacy `active_epoch` marker.
    pub fn active_epoch(&self) -> Result<Option<i64>, StorageError> {
        let conn = self.client.read_connection()?;
        match ProjectIndexManifestRepository::get_active(&conn, self.project_id)? {
            Some(manifest) => Ok(Some(manifest.data_epoch)),
            None => {
                ProjectRepository::meta_get_int_optional(&conn, self.project_id, "active_epoch")
            }
        }
    }

    /// Epoch baseline hashes are recorded against: the active generation's
    /// own epoch, falling back to 0 for a project that was never published.
    pub fn write_epoch(&self) -> Result<i64, StorageError> {
        Ok(self.view()?.own_epoch)
    }

    /// Load every visible `(path, content_hash)` pair in one pass.
    ///
    /// Rows without a content hash are skipped: a file whose stored hash is
    /// NULL is indistinguishable from an absent record and counts as
    /// "added", matching the single-point lookup semantics.
    pub fn stored_hashes(&self) -> Result<HashMap<PathBuf, String>, StorageError> {
        let conn = self.client.read_connection()?;
        let view = GenerationView::resolve(&conn, self.project_id)?;

        // Ancestors farthest-first so nearer generations win for paths
        // resident in more than one generation, then the own generation on
        // top: an override only ever drops an inherited entry, never a row
        // the published generation owns itself.
        let mut stored = HashMap::new();
        for epoch in view.ancestors.iter().rev() {
            load_hashes(&conn, self.project_id, *epoch, &mut stored)?;
        }
        for path in &view.excluded_files {
            stored.remove(Path::new(path));
        }
        load_hashes(&conn, self.project_id, view.own_epoch, &mut stored)?;
        Ok(stored)
    }

    /// Visible content hash of `path`.
    ///
    /// Own generation first: a hit there always wins, so this agrees with
    /// [`Self::stored_hashes`]. An own-generation override then hides the
    /// inheritance chain, because a replaced file must not surface its
    /// ancestor's content and a deleted file is invisible everywhere.
    pub fn stored_hash(&self, path: &Path) -> Result<Option<String>, StorageError> {
        let conn = self.client.read_connection()?;
        let view = GenerationView::resolve(&conn, self.project_id)?;
        let Some(path_str) = path.to_str() else {
            return Ok(None);
        };

        if let Some(hash) = FileRepository::get_content_hash_by_path_at_epoch(
            &conn,
            path_str,
            self.project_id,
            view.own_epoch,
        )? {
            return Ok(Some(hash));
        }
        if view.is_excluded(path_str) {
            return Ok(None);
        }
        for epoch in view.ancestors {
            if let Some(hash) = FileRepository::get_content_hash_by_path_at_epoch(
                &conn,
                path_str,
                self.project_id,
                epoch,
            )? {
                return Ok(Some(hash));
            }
        }
        Ok(None)
    }

    /// Paths visible from the active generation view, including rows whose
    /// content hash is NULL (they are still indexed paths).
    pub fn visible_paths(&self) -> Result<HashSet<PathBuf>, StorageError> {
        let conn = self.client.read_connection()?;
        let view = GenerationView::resolve(&conn, self.project_id)?;

        let mut visible = HashSet::new();
        for epoch in view.ancestors.iter().rev() {
            load_paths(&conn, self.project_id, *epoch, &mut visible)?;
        }
        for path in &view.excluded_files {
            visible.remove(Path::new(path));
        }
        load_paths(&conn, self.project_id, view.own_epoch, &mut visible)?;
        Ok(visible)
    }

    /// Create the project row so file rows satisfy their foreign key.
    pub fn ensure_project(&self, root_path: &str) -> Result<(), StorageError> {
        let project_id = self.project_id;
        self.client
            .with_transaction(|tx| ProjectRepository::ensure(tx, project_id, root_path))
    }

    /// Record `hashes` at `epoch`.
    pub fn record(&self, epoch: i64, hashes: &[(PathBuf, String)]) -> Result<(), StorageError> {
        let project_id = self.project_id;
        self.client.with_transaction(|tx| {
            for (path, hash) in hashes {
                FileRepository::insert_hash_for_epoch(tx, path, hash, project_id, epoch)?;
            }
            Ok(())
        })
    }

    /// Drop hash rows of `paths` at `epoch`.
    pub fn remove(&self, epoch: i64, paths: &[PathBuf]) -> Result<(), StorageError> {
        let project_id = self.project_id;
        self.client.with_transaction(|tx| {
            for path in paths {
                FileRepository::delete_by_path_at_epoch(
                    tx,
                    &path.to_string_lossy(),
                    project_id,
                    epoch,
                )?;
            }
            Ok(())
        })
    }
}

fn load_hashes(
    conn: &Connection,
    project_id: i64,
    epoch: i64,
    out: &mut HashMap<PathBuf, String>,
) -> Result<(), StorageError> {
    let rows: Vec<(String, String)> = execute_query(
        conn,
        "SELECT path, content_hash FROM files
         WHERE project_id = ?1 AND epoch = ?2 AND content_hash IS NOT NULL",
        params![project_id, epoch],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    for (path, hash) in rows {
        out.insert(PathBuf::from(path), hash);
    }
    Ok(())
}

fn load_paths(
    conn: &Connection,
    project_id: i64,
    epoch: i64,
    out: &mut HashSet<PathBuf>,
) -> Result<(), StorageError> {
    let rows: Vec<String> = execute_query(
        conn,
        "SELECT DISTINCT path FROM files WHERE project_id = ?1 AND epoch = ?2",
        params![project_id, epoch],
        |row| row.get(0),
    )?;
    for path in rows {
        out.insert(PathBuf::from(path));
    }
    Ok(())
}
