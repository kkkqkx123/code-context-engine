//! Filesystem source snippet helpers.
//!
//! Pure file reads shared by query enrichment and symbol tools: chunk file
//! paths are stored relative to the project root, so callers resolve the
//! root through the project registry and read raw lines without touching
//! any database.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Read lines `[start_line, end_line]` (inclusive, zero-based) from a source
/// file, joining them with newlines.
///
/// `file_path` is used as-is when absolute; relative paths are resolved
/// against `project_root` first so resolution never depends on the process
/// working directory. An end line beyond the file simply clamps to the last
/// line, which lets callers request "to EOF" with a large sentinel. Returns an
/// empty string when the file cannot be opened or decoded as UTF-8.
pub fn read_source_lines(
    project_root: Option<&Path>,
    file_path: &str,
    start_line: u32,
    end_line: u32,
) -> String {
    if end_line < start_line {
        return String::new();
    }
    let path_buf = PathBuf::from(file_path);
    let candidate = if path_buf.is_absolute() {
        path_buf
    } else {
        match project_root {
            Some(root) => root.join(&path_buf),
            None => path_buf,
        }
    };

    let content = match std::fs::read_to_string(&candidate) {
        Ok(content) => content,
        Err(error) => {
            tracing::debug!(
                path = %candidate.display(),
                error = %error,
                "Lazy source read failed; snippet degrades to empty"
            );
            return String::new();
        }
    };

    let start = start_line as usize;
    let end = (end_line as usize + 1).min(content.lines().count());
    content
        .lines()
        .skip(start)
        .take(end.saturating_sub(start))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Per-request cache of source file contents.
///
/// Hybrid recall often yields multiple hits in the same file. Reading and
/// splitting the file once per request bounds enrichment I/O by distinct
/// files instead of by hit count.
pub struct SourceFileCache {
    entries: HashMap<PathBuf, String>,
}

impl SourceFileCache {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    fn resolve_candidate(project_root: Option<&Path>, file_path: &str) -> PathBuf {
        let path_buf = PathBuf::from(file_path);
        if path_buf.is_absolute() {
            path_buf
        } else {
            match project_root {
                Some(root) => root.join(&path_buf),
                None => path_buf,
            }
        }
    }

    fn file_content(&mut self, candidate: &Path) -> Option<&str> {
        if !self.entries.contains_key(candidate) {
            match std::fs::read_to_string(candidate) {
                Ok(content) => {
                    self.entries.insert(candidate.to_path_buf(), content);
                }
                Err(error) => {
                    tracing::debug!(
                        path = %candidate.display(),
                        error = %error,
                        "Lazy source read failed; snippet degrades to empty"
                    );
                    return None;
                }
            }
        }
        self.entries.get(candidate).map(String::as_str)
    }
}

impl Default for SourceFileCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Cached variant of [`read_source_lines`].
///
/// The file is read at most once per cache lifetime; subsequent hits in the
/// same file slice the cached content. Returns `None` when the file cannot be
/// read, so callers can distinguish an unreadable file from an empty range;
/// an empty range yields `Some("")`.
pub fn read_source_lines_cached(
    cache: &mut SourceFileCache,
    project_root: Option<&Path>,
    file_path: &str,
    start_line: u32,
    end_line: u32,
) -> Option<String> {
    let candidate = SourceFileCache::resolve_candidate(project_root, file_path);
    let content = cache.file_content(&candidate)?;
    if end_line < start_line {
        return Some(String::new());
    }
    let start = start_line as usize;
    let end = (end_line as usize + 1).min(content.lines().count());
    Some(
        content
            .lines()
            .skip(start)
            .take(end.saturating_sub(start))
            .collect::<Vec<_>>()
            .join("\n"),
    )
}
