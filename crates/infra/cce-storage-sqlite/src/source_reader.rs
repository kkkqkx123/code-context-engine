//! Lazy source-file reading for query-time snippet reconstruction.
//!
//! Chunk records intentionally no longer persist raw source code; consumers
//! resolve the project root once per request and read the needed line range
//! straight from disk. Failure to read is a degraded result (empty text), not
//! an error: files may legitimately have disappeared since indexing.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

/// Resolve a project root directory from SQLite.
///
/// Query contexts only know `project_id`; chunk file paths are stored relative
/// to the project root, so the root must be recovered from the registry.
/// Returns `None` when the project row or its root path is unavailable.
pub fn resolve_project_root(conn: &Connection, project_id: i64) -> Option<PathBuf> {
    conn.query_row(
        "SELECT root_path FROM projects WHERE id = ?1",
        [project_id],
        |row| row.get::<_, String>(0),
    )
    .map(PathBuf::from)
    .ok()
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_inclusive_line_range_relative_to_root() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let file = tmp.path().join("src/lib.rs");
        std::fs::create_dir_all(file.parent().expect("parent")).expect("create dir");
        std::fs::write(&file, "l0\nl1\nl2\nl3\n").expect("write file");

        let text = read_source_lines(Some(tmp.path()), "src/lib.rs", 1, 2);
        assert_eq!(text, "l1\nl2");
    }

    #[test]
    fn clamps_end_line_to_file_length() {
        let tmp = tempfile::tempdir().expect("temp dir");
        std::fs::write(tmp.path().join("a.txt"), "x\ny\n").expect("write file");

        let text = read_source_lines(Some(tmp.path()), "a.txt", 1, u32::MAX);
        assert_eq!(text, "y");
    }

    #[test]
    fn missing_file_degrades_to_empty() {
        let text = read_source_lines(None, "does/not/exist.rs", 0, 10);
        assert!(text.is_empty());
    }

    #[test]
    fn absolute_paths_ignore_root() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let file = tmp.path().join("abs.rs");
        std::fs::write(&file, "only\n").expect("write file");

        let text = read_source_lines(None, file.to_str().expect("utf8"), 0, 0);
        assert_eq!(text, "only");
    }
}
