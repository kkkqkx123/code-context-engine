//! Filesystem-independent file supply.
//!
//! The file-set view is the boundary between file enumeration (local walk or
//! remote gateway push) and file consumption (indexing, relation parsing).
//! Producers supply canonical project-relative paths plus raw bytes;
//! consumers never see a local root directory.

use std::path::PathBuf;

/// Single file inside a file-set view.
#[derive(Debug, Clone)]
pub struct FileViewEntry {
    /// Canonical project-relative path (forward slashes, no redundant parts).
    pub relative_path: PathBuf,
    /// Raw file bytes supplied by whoever constructed the view.
    pub bytes: Vec<u8>,
}

/// Filesystem-independent file collection.
///
/// The local directory scan is one way to build this view; a remote gateway
/// builds the same view from pushed bytes. Consumers only see relative paths
/// and bytes, never a local root directory.
#[derive(Debug, Clone, Default)]
pub struct FileSetView {
    /// Files in the view, keyed by canonical relative path.
    pub files: Vec<FileViewEntry>,
}

impl FileSetView {
    /// Build a view from already-supplied relative paths and bytes.
    pub fn from_files(files: Vec<(PathBuf, Vec<u8>)>) -> Self {
        Self {
            files: files
                .into_iter()
                .map(|(relative_path, bytes)| FileViewEntry {
                    relative_path: normalize_relative(relative_path),
                    bytes,
                })
                .collect(),
        }
    }

    /// Look up a file by its canonical relative path.
    pub fn get(&self, relative_path: &str) -> Option<&FileViewEntry> {
        let normalized = crate::path::normalize_project_path(relative_path);
        self.files
            .iter()
            .find(|entry| entry.relative_path.to_string_lossy() == normalized)
    }

    /// Whether the view contains no files.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Number of files in the view.
    pub fn len(&self) -> usize {
        self.files.len()
    }
}

/// Normalize a relative path to the canonical project form.
pub fn normalize_relative(relative_path: PathBuf) -> PathBuf {
    PathBuf::from(crate::path::normalize_project_path(
        &relative_path.to_string_lossy(),
    ))
}
