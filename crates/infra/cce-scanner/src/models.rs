//! Scanner models for file system scanning

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use cce_types::language::LanguageInfo;

/// File entry representing a scanned file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    /// Absolute file path
    pub path: PathBuf,
    /// Relative path from scan root
    pub relative_path: PathBuf,
    /// File size in bytes
    pub size: u64,
    /// Last modification time
    pub modified: DateTime<Utc>,
    /// Content hash (for change detection)
    pub content_hash: Option<String>,
    /// Language and file type information
    pub language_info: Option<LanguageInfo>,
}

impl FileEntry {
    /// Create a new file entry
    ///
    /// `relative_path` is normalized to the canonical project-relative form
    /// (forward slashes, no redundant segments) so storage keys derived from
    /// it are stable regardless of how the caller spelled the path.
    pub fn new(path: PathBuf, relative_path: PathBuf, size: u64, modified: DateTime<Utc>) -> Self {
        Self {
            path,
            relative_path: PathBuf::from(cce_types::path::normalize_project_path(
                &relative_path.to_string_lossy(),
            )),
            size,
            modified,
            content_hash: None,
            language_info: None,
        }
    }

    /// Set content hash
    pub fn with_hash(mut self, hash: String) -> Self {
        self.content_hash = Some(hash);
        self
    }

    /// Set language information
    pub fn with_language_info(mut self, language_info: LanguageInfo) -> Self {
        self.language_info = Some(language_info);
        self
    }

    /// Check if this is a text file based on language info
    /// Returns false if language_info is None (indicates binary file)
    ///
    /// Every `FileType` variant describes a text file; binary files are
    /// marked by the scanner leaving `language_info` unset.
    pub fn is_text(&self) -> bool {
        self.language_info.is_some()
    }

    /// Canonical project-relative identity shared by local and remote supply.
    ///
    /// Storage keys, cache keys and progress display derive from this form;
    /// the absolute path remains a local load hint only.
    pub fn identity_key(&self) -> String {
        self.relative_path.to_string_lossy().to_string()
    }

    /// Local filesystem path used to load content on demand.
    ///
    /// Remote payloads carry no local path; they populate content directly.
    pub fn local_load_path(&self) -> &std::path::Path {
        &self.path
    }

    /// Build a manifest entry from a content payload plus caller-supplied stat.
    ///
    /// The payload already normalizes the relative path and carries the
    /// expected content hash; `local_path` is recorded only as a load hint
    /// for the local on-demand path and falls back to the relative form for
    /// remote payloads that have no local file.
    pub fn from_payload(
        payload: &FileContentPayload,
        size: u64,
        modified: DateTime<Utc>,
        language_info: Option<LanguageInfo>,
        local_path: Option<std::path::PathBuf>,
    ) -> Self {
        Self {
            path: local_path.unwrap_or_else(|| payload.relative_path.clone()),
            relative_path: payload.relative_path.clone(),
            size,
            modified,
            content_hash: payload.expected_hash.clone(),
            language_info,
        }
    }
}

/// Content payload exchanged between file supply and file consumption.
///
/// Local supply uses the on-demand form (no bytes; the consumer loads from
/// the local path). Remote supply uses the ready form (bytes travel with the
/// payload and no filesystem access happens downstream). Both forms verify
/// against the same full-content hash domain before decoding.
#[derive(Debug, Clone)]
pub struct FileContentPayload {
    /// Canonical project-relative path (forward slashes, no redundant parts).
    pub relative_path: PathBuf,
    /// Expected full-content hash; `None` skips verification.
    pub expected_hash: Option<String>,
    /// File bytes when already supplied; `None` means load locally on demand.
    pub bytes: Option<Vec<u8>>,
    /// Known size, when the supplier already stated it.
    pub size: Option<u64>,
    /// Known modification time, when the supplier already stated it.
    pub modified: Option<DateTime<Utc>>,
}

impl FileContentPayload {
    /// Payload whose bytes travel with it (remote/gateway form).
    pub fn ready(
        relative_path: impl Into<PathBuf>,
        bytes: Vec<u8>,
        expected_hash: Option<String>,
    ) -> Self {
        Self {
            relative_path: cce_types::supply::normalize_relative(relative_path.into()),
            expected_hash,
            bytes: Some(bytes),
            size: None,
            modified: None,
        }
    }

    /// Payload that must be loaded from the local path on demand.
    pub fn load_local(relative_path: impl Into<PathBuf>, expected_hash: Option<String>) -> Self {
        Self {
            relative_path: cce_types::supply::normalize_relative(relative_path.into()),
            expected_hash,
            bytes: None,
            size: None,
            modified: None,
        }
    }

    /// Whether the bytes are already supplied (no filesystem read needed).
    pub fn is_ready(&self) -> bool {
        self.bytes.is_some()
    }

    /// Canonical identity shared with [`FileEntry::identity_key`].
    pub fn identity_key(&self) -> String {
        self.relative_path.to_string_lossy().to_string()
    }
}

/// Single file inside a filesystem-independent file-set view.
pub use cce_types::supply::FileViewEntry;

/// Filesystem-independent file collection consumed by relation parsing.
///
/// The local directory scan is one way to build this view; a remote gateway
/// builds the same view from pushed bytes. Consumers only see relative paths
/// and bytes, never a local root directory.
pub use cce_types::supply::FileSetView;
