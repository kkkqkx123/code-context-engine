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
}
