//! Fuzzy file-path search over project files.
//!
//! Answers filename queries in large codebases without touching any index:
//! the project directory is walked directly, candidate paths are matched
//! against the query with a subsequence scorer, and the best hits are
//! returned. Two output tiers share one matcher: bare paths, or paths with
//! file metadata (size and line count only).
//!
//! The walk always observes the latest filesystem state and never applies
//! generation filtering: files have no cross-backend consistency requirement,
//! so returning the newest data is correct. The remote relation branch holds
//! no project files, so file search there reports unavailability instead of
//! silently returning indexed (potentially stale) rows.

use std::io::BufRead;
use std::path::Path;

/// A single file hit with the metadata tier attached.
///
/// Only size and line count are reported; hashes and timestamps stay out so
/// the metadata tier never forces content reads beyond a newline scan.
/// Metadata is absent for index-based hits on branches without live files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileHit {
    /// Project-relative path with `/` separators.
    pub path: String,
    /// File size in bytes, present only for live-filesystem hits.
    pub size_bytes: Option<u64>,
    /// Number of newline-terminated lines, present only for live hits.
    pub line_count: Option<u64>,
}

/// Search file paths matching `query`, returning at most `limit` hits.
///
/// Paths are project-relative, ordered by match quality with a lexical
/// tiebreak so repeated queries over an unchanged tree are stable.
pub fn search_file_paths(
    root: &Path,
    query: &str,
    limit: usize,
) -> crate::query::error::Result<Vec<String>> {
    let ranked = rank_paths(root, query, limit)?;
    Ok(ranked.into_iter().map(|(path, _)| path).collect())
}

/// Search files matching `query`, attaching size and line count.
///
/// Files that vanish or become unreadable between the walk and the stat pass
/// are skipped; the returned list holds at most `limit` hits.
pub fn search_files(
    root: &Path,
    query: &str,
    limit: usize,
) -> crate::query::error::Result<Vec<FileHit>> {
    let ranked = rank_paths(root, query, limit)?;
    let mut hits = Vec::with_capacity(ranked.len());
    for (path, _) in ranked {
        let full = root.join(&path);
        let size_bytes = match std::fs::symlink_metadata(&full) {
            Ok(meta) if meta.is_file() => meta.len(),
            _ => continue,
        };
        let line_count = count_lines(&full);
        hits.push(FileHit {
            path,
            size_bytes: Some(size_bytes),
            line_count: Some(line_count),
        });
    }
    Ok(hits)
}

/// Rank an index-provided path list with the same matcher and ordering as
/// the live walk, for branches without project files.
///
/// Shares validation, the full-path-vs-filename rule and the
/// score-descending plus lexical tiebreak with [`search_file_paths`].
/// Hits carry no size or line metadata on this path.
pub fn rank_file_paths(
    paths: &[String],
    query: &str,
    limit: usize,
) -> crate::query::error::Result<Vec<FileHit>> {
    let pattern = checked_query(query, limit)?;
    let match_full_path = pattern.contains('/');
    let mut ranked: Vec<(String, i64)> = Vec::new();
    for path in paths {
        let target = if match_full_path {
            path.clone()
        } else {
            file_name_of(path).to_string()
        };
        if let Some(score) = fuzzy_score(&pattern, &target) {
            ranked.push((path.clone(), score));
        }
    }
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    ranked.truncate(limit);
    Ok(ranked
        .into_iter()
        .map(|(path, _)| FileHit {
            path,
            size_bytes: None,
            line_count: None,
        })
        .collect())
}

/// Validate inputs shared by both output tiers.
fn checked_query(query: &str, limit: usize) -> crate::query::error::Result<String> {
    if limit == 0 {
        return Err(crate::query::error::QueryError::invalid(
            "file search limit must be greater than 0",
        ));
    }
    let pattern = query.trim().to_string();
    if pattern.is_empty() {
        return Err(crate::query::error::QueryError::invalid(
            "file search query must not be empty",
        ));
    }
    Ok(pattern)
}

/// Walk `root` and rank every visible file against the query.
fn rank_paths(
    root: &Path,
    query: &str,
    limit: usize,
) -> crate::query::error::Result<Vec<(String, i64)>> {
    let pattern = checked_query(query, limit)?;
    let match_full_path = pattern.contains('/');
    let mut ranked: Vec<(String, i64)> = Vec::new();
    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| entry.depth() == 0 || !is_skipped_dir(entry))
    {
        let entry = entry.map_err(|e| {
            crate::query::error::QueryError::storage(&format!("Failed to walk project files: {e}"))
        })?;
        if !entry.file_type().is_file() {
            continue;
        }
        let relative = entry.path().strip_prefix(root).unwrap_or(entry.path());
        let path = relative
            .components()
            .map(|component| component.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        let target = if match_full_path {
            path.clone()
        } else {
            file_name_of(&path).to_string()
        };
        if let Some(score) = fuzzy_score(&pattern, &target) {
            ranked.push((path, score));
        }
    }
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    ranked.truncate(limit);
    Ok(ranked)
}

/// Hidden directories (`.git` and friends) never participate in file search.
fn is_skipped_dir(entry: &walkdir::DirEntry) -> bool {
    entry.file_type().is_dir()
        && entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with('.'))
}

/// File name portion of a `/`-joined relative path.
fn file_name_of(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Case-insensitive subsequence score, if every pattern character appears in
/// order. Contiguous runs and early/word-boundary starts score higher; gaps
/// between matches penalize. Higher is better.
fn fuzzy_score(pattern: &str, target: &str) -> Option<i64> {
    let pattern: Vec<char> = pattern.to_lowercase().chars().collect();
    let target: Vec<char> = target.to_lowercase().chars().collect();
    if pattern.is_empty() || pattern.len() > target.len() {
        return None;
    }
    let mut positions: Vec<usize> = Vec::with_capacity(pattern.len());
    let mut cursor = 0;
    for needle in &pattern {
        let found = target[cursor..].iter().position(|c| c == needle)?;
        positions.push(cursor + found);
        cursor += found + 1;
    }
    let mut gaps: i64 = 0;
    let mut longest_run: i64 = 1;
    let mut run: i64 = 1;
    for window in positions.windows(2) {
        if window[1] == window[0] + 1 {
            run += 1;
            longest_run = longest_run.max(run);
        } else {
            gaps += (window[1] - window[0] - 1) as i64;
            run = 1;
        }
    }
    let start = positions[0] as i64;
    let boundary_bonus = if start == 0 {
        50
    } else if matches!(
        target.get((start - 1) as usize),
        Some('/' | '-' | '_' | '.')
    ) {
        30
    } else {
        0
    };
    let length_penalty = target.len() as i64 - pattern.len() as i64;
    Some(pattern.len() as i64 * 100 + longest_run * 20 - gaps * 5 - length_penalty + boundary_bonus)
}

/// Count newline-terminated lines without validating encoding.
///
/// Unreadable files report zero lines; callers skip such files outright, so
/// this helper never fails the search.
fn count_lines(path: &Path) -> u64 {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(_) => return 0,
    };
    let mut reader = std::io::BufReader::new(file);
    let mut count = 0u64;
    let mut buffer = Vec::with_capacity(8 * 1024);
    loop {
        buffer.clear();
        match reader.read_until(b'\n', &mut buffer) {
            Ok(0) => break,
            Ok(_) => count += 1,
            Err(_) => break,
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_filename_outranks_subsequence() {
        assert!(
            fuzzy_score("main.rs", "main.rs").unwrap()
                > fuzzy_score("main.rs", "domain.rs").unwrap()
        );
    }

    #[test]
    fn non_subsequence_has_no_score() {
        assert_eq!(fuzzy_score("xyz", "main.rs"), None);
    }

    #[test]
    fn contiguous_match_beats_gapped_match() {
        assert!(fuzzy_score("abc", "abc.rs").unwrap() > fuzzy_score("abc", "a_b_c.rs").unwrap());
    }

    #[test]
    fn search_paths_ranks_and_limits() {
        let dir = tempfile::tempdir().expect("tempdir");
        for name in ["main.rs", "domain.rs", "lib.rs"] {
            std::fs::write(dir.path().join(name), "fn x() {}\n").expect("write file");
        }
        let hits = search_file_paths(dir.path(), "main", 10).expect("search");
        assert_eq!(hits, vec!["main.rs".to_string(), "domain.rs".to_string()]);
        let limited = search_file_paths(dir.path(), "rs", 2).expect("search");
        assert_eq!(limited.len(), 2);
    }

    #[test]
    fn search_files_attaches_size_and_lines() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("a.rs"), "one\ntwo\nthree\n").expect("write file");
        let hits = search_files(dir.path(), "a.rs", 10).expect("search");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, "a.rs");
        assert!(hits[0].size_bytes.is_some_and(|size| size > 0));
        assert_eq!(hits[0].line_count, Some(3));

        let ranked =
            rank_file_paths(&["a.rs".to_string(), "b.rs".to_string()], "a", 10).expect("rank");
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].path, "a.rs");
        assert_eq!(ranked[0].size_bytes, None);
        assert_eq!(ranked[0].line_count, None);
    }

    #[test]
    fn empty_query_is_rejected() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(search_file_paths(dir.path(), "   ", 10).is_err());
        assert!(search_files(dir.path(), "", 10).is_err());
    }

    #[test]
    fn zero_limit_is_rejected() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(search_file_paths(dir.path(), "a", 0).is_err());
    }
}
