//! Fuzzy file-path search over project files.
//!
//! Answers filename queries in large codebases without touching any index:
//! the project directory is walked directly, candidate paths are matched
//! against the query with a subsequence scorer, and the best hits are
//! returned. Two output tiers share one matcher: bare paths, or paths with
//! file metadata (size and line count only).
//!
//! The directory listing is cached per project root (bounded staleness) so
//! repeated queries skip the full walk; the metadata tier always stats the
//! live filesystem, so vanished files are still skipped. Content-type
//! exclusion applies through the shared path filter whenever the caller
//! supplies it. Branches without live project files rank indexed file rows
//! with the same matcher and ordering instead, returning path-only hits
//! without live metadata.

use std::collections::HashMap;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

use super::expansion::ExpansionPathFilter;

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

/// Options for file search, reusing the query-side path filter.
///
/// The path filter (directory prefix, content-type exclusion, category
/// include/exclude, glob include/exclude) is the same [`ExpansionPathFilter`] the expansion and
/// relation paths enforce, so a file hidden from semantic recall is hidden
/// from file search too. `respect_ignore` mirrors the indexer default:
/// `.gitignore`/`.indexignore` rules at every directory level prune the walk,
/// deeper levels overriding shallower ones. Callers thread
/// per-request filters through the file-search API surface; the walk-level
/// listing cache is unaffected because path filters apply after the walk.
#[derive(Clone, Debug)]
pub struct FileSearchOptions {
    /// Directory-prefix, content-type, category and glob filter shared with query paths.
    pub path_filter: ExpansionPathFilter,
    /// Observe ignore files during the walk (default on).
    pub respect_ignore: bool,
    /// Serve the directory listing from the per-root cache (default on).
    pub use_cache: bool,
}

impl Default for FileSearchOptions {
    fn default() -> Self {
        Self {
            path_filter: ExpansionPathFilter::default(),
            respect_ignore: true,
            use_cache: true,
        }
    }
}

/// How long a cached directory listing stays valid.
const FILE_LIST_TTL: Duration = Duration::from_secs(30);

/// One cached directory listing: raw walked paths plus the freshness
/// evidence (walk time and root mtime) used to validate it.
#[derive(Debug)]
struct CacheEntry {
    walked_at: Instant,
    root_mtime: Option<SystemTime>,
    paths: Vec<String>,
}

/// Per-process directory listing cache keyed by canonical project root plus
/// the ignore flag.
///
/// A full [`walkdir`] traversal per query is the cost this removes: entries
/// are revalidated by TTL plus root-directory mtime, so direct-child
/// add/remove usually invalidates immediately while deeper changes fall back
/// to the TTL bound. Staleness is self-healing (deleted files are skipped by
/// the live stat pass) and never affects ranking, only membership. The key
/// includes `respect_ignore` because an ignore-pruned listing must never
/// serve a caller that opted out; the remaining path filters apply per call
/// after the cache.
fn file_list_cache() -> &'static Mutex<HashMap<(PathBuf, bool), CacheEntry>> {
    static CACHE: OnceLock<Mutex<HashMap<(PathBuf, bool), CacheEntry>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Drop every cached directory listing.
///
/// Available so tests observe a fresh walk on demand and future indexer
/// hooks can invalidate a root after reindexing.
pub fn clear_file_list_cache() {
    if let Ok(mut cache) = file_list_cache().lock() {
        cache.clear();
    }
}

/// Search file paths matching `query`, returning at most `limit` hits.
///
/// Paths are project-relative, ordered by match quality with a lexical
/// tiebreak so repeated queries over an unchanged tree are stable.
pub fn search_file_paths(
    root: &Path,
    query: &str,
    limit: usize,
    options: &FileSearchOptions,
) -> crate::query::error::Result<Vec<String>> {
    let ranked = rank_paths(root, query, limit, options)?;
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
    options: &FileSearchOptions,
) -> crate::query::error::Result<Vec<FileHit>> {
    let ranked = rank_paths(root, query, limit, options)?;
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
    options: &FileSearchOptions,
) -> crate::query::error::Result<Vec<FileHit>> {
    let pattern = checked_query(query, limit)?;
    // Index rows already passed generation filtering; the query-side path
    // 判定 still applies so both branches hide the same files. Ignore rules
    // are a live-filesystem concept and do not apply here.
    let candidates: Vec<String> = paths
        .iter()
        .filter(|path| options.path_filter.allows(path))
        .cloned()
        .collect();
    let ranked = rank_candidates(candidates, &pattern, limit);
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

/// Rank pre-filtered candidate paths against the query pattern.
fn rank_candidates(candidates: Vec<String>, pattern: &str, limit: usize) -> Vec<(String, i64)> {
    let match_full_path = pattern.contains('/');
    let mut ranked: Vec<(String, i64)> = Vec::new();
    for path in candidates {
        let target = if match_full_path {
            path.clone()
        } else {
            file_name_of(&path).to_string()
        };
        if let Some(score) = fuzzy_score(pattern, &target) {
            ranked.push((path, score));
        }
    }
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    ranked.truncate(limit);
    ranked
}

/// Walk `root` and rank every visible file against the query.
fn rank_paths(
    root: &Path,
    query: &str,
    limit: usize,
    options: &FileSearchOptions,
) -> crate::query::error::Result<Vec<(String, i64)>> {
    let pattern = checked_query(query, limit)?;
    let candidates = cached_or_walked_files(root, options)?;
    let candidates: Vec<String> = candidates
        .into_iter()
        .filter(|path| options.path_filter.allows(path))
        .collect();
    Ok(rank_candidates(candidates, &pattern, limit))
}

/// Serve the walked file list from the per-root cache when fresh.
fn cached_or_walked_files(
    root: &Path,
    options: &FileSearchOptions,
) -> crate::query::error::Result<Vec<String>> {
    if !options.use_cache {
        return walk_project_files(root, load_ignore_stack(root, options));
    }
    let key = (
        root.canonicalize().unwrap_or_else(|_| root.to_path_buf()),
        options.respect_ignore,
    );
    let root_mtime = std::fs::metadata(root).and_then(|m| m.modified()).ok();
    if let Ok(cache) = file_list_cache().lock()
        && let Some(entry) = cache.get(&key)
        && entry.walked_at.elapsed() <= FILE_LIST_TTL
        && mtime_matches(entry.root_mtime, root_mtime)
    {
        return Ok(entry.paths.clone());
    }
    let paths = walk_project_files(root, load_ignore_stack(root, options))?;
    if let Ok(mut cache) = file_list_cache().lock() {
        cache.insert(
            key,
            CacheEntry {
                walked_at: Instant::now(),
                root_mtime,
                paths: paths.clone(),
            },
        );
    }
    Ok(paths)
}

/// Root mtimes agree when both sides are known and equal; an unknown mtime
/// never invalidates on its own, leaving the TTL bound in charge.
fn mtime_matches(cached: Option<SystemTime>, current: Option<SystemTime>) -> bool {
    match (cached, current) {
        (Some(cached), Some(current)) => cached == current,
        _ => true,
    }
}

/// Stack ignore rules under `root`: the merged root `.gitignore` plus
/// `.indexignore` as level zero, deeper per-directory files discovered during
/// the walk with deeper rules overriding shallower ones. Missing files simply
/// contribute nothing. Nested edits surface after the listing cache expires,
/// like deep tree changes.
fn load_ignore_stack(root: &Path, options: &FileSearchOptions) -> Option<cce_scanner::IgnoreStack> {
    if !options.respect_ignore {
        return None;
    }
    let mut merged: Option<cce_scanner::IgnoreMatcher> = None;
    for name in [".gitignore", ".indexignore"] {
        match cce_scanner::IgnoreMatcher::from_file(root.join(name)) {
            Ok(matcher) => {
                if let Some(existing) = merged.as_mut() {
                    existing.merge(matcher);
                } else {
                    merged = Some(matcher);
                }
            }
            Err(_) => continue,
        }
    }
    let root_matcher = merged.unwrap_or_else(|| cce_scanner::IgnoreMatcher::new(Vec::new(), root));
    Some(cce_scanner::IgnoreStack::new(
        root.to_path_buf(),
        root_matcher,
        vec![".gitignore".to_string(), ".indexignore".to_string()],
    ))
}

/// Walk `root` and collect project-relative file paths.
///
/// Hidden directories are always pruned; when an ignore stack is present
/// ignored directories are pruned too (so `target/`-scale subtrees are
/// never descended into) and ignored files are skipped. Nested ignore files
/// apply with deeper rules overriding shallower ones.
fn walk_project_files(
    root: &Path,
    ignore: Option<cce_scanner::IgnoreStack>,
) -> crate::query::error::Result<Vec<String>> {
    let mut paths = Vec::new();
    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            if entry.depth() == 0 {
                return true;
            }
            if is_skipped_dir(entry) {
                return false;
            }
            if let Some(stack) = ignore.as_ref()
                && stack.is_ignored(entry.path())
            {
                return false;
            }
            true
        })
    {
        let entry = entry.map_err(|e| {
            crate::query::error::QueryError::storage(&format!("Failed to walk project files: {e}"))
        })?;
        if !entry.file_type().is_file() {
            continue;
        }
        let relative = entry.path().strip_prefix(root).unwrap_or(entry.path());
        paths.push(
            relative
                .components()
                .map(|component| component.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/"),
        );
    }
    Ok(paths)
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

    fn search_options() -> FileSearchOptions {
        FileSearchOptions::default()
    }

    #[test]
    fn search_paths_ranks_and_limits() {
        let dir = tempfile::tempdir().expect("tempdir");
        for name in ["main.rs", "domain.rs", "lib.rs"] {
            std::fs::write(dir.path().join(name), "fn x() {}\n").expect("write file");
        }
        let hits = search_file_paths(dir.path(), "main", 10, &search_options()).expect("search");
        assert_eq!(hits, vec!["main.rs".to_string(), "domain.rs".to_string()]);
        let limited = search_file_paths(dir.path(), "rs", 2, &search_options()).expect("search");
        assert_eq!(limited.len(), 2);
    }

    #[test]
    fn search_files_attaches_size_and_lines() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("a.rs"), "one\ntwo\nthree\n").expect("write file");
        let hits = search_files(dir.path(), "a.rs", 10, &search_options()).expect("search");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, "a.rs");
        assert!(hits[0].size_bytes.is_some_and(|size| size > 0));
        assert_eq!(hits[0].line_count, Some(3));

        let ranked = rank_file_paths(
            &["a.rs".to_string(), "b.rs".to_string()],
            "a",
            10,
            &search_options(),
        )
        .expect("rank");
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].path, "a.rs");
        assert_eq!(ranked[0].size_bytes, None);
        assert_eq!(ranked[0].line_count, None);
    }

    #[test]
    fn empty_query_is_rejected() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(search_file_paths(dir.path(), "   ", 10, &search_options()).is_err());
        assert!(search_files(dir.path(), "", 10, &search_options()).is_err());
    }

    #[test]
    fn zero_limit_is_rejected() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(search_file_paths(dir.path(), "a", 0, &search_options()).is_err());
    }

    #[test]
    fn ignore_rules_prune_walk() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join(".gitignore"), "target/\n*.log\n").expect("ignore file");
        std::fs::create_dir_all(dir.path().join("target")).expect("target dir");
        std::fs::write(dir.path().join("target/build.rs"), "fn x() {}\n").expect("write file");
        std::fs::write(dir.path().join("debug.log"), "noise\n").expect("write file");
        std::fs::write(dir.path().join("main.rs"), "fn x() {}\n").expect("write file");
        let hits = search_file_paths(dir.path(), "a", 10, &search_options()).expect("search");
        assert_eq!(hits, vec!["main.rs".to_string()]);
    }

    #[test]
    fn nested_ignore_child_reinclude_overrides_parent() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join(".gitignore"), "*.log\n").expect("ignore file");
        std::fs::create_dir_all(dir.path().join("sub")).expect("sub dir");
        std::fs::write(dir.path().join("sub/.gitignore"), "!keep.log\n").expect("ignore file");
        std::fs::write(dir.path().join("sub/keep.log"), "keep\n").expect("write file");
        std::fs::write(dir.path().join("sub/drop.log"), "drop\n").expect("write file");
        std::fs::write(dir.path().join("root.log"), "drop\n").expect("write file");
        let hits = search_file_paths(dir.path(), "log", 10, &search_options()).expect("search");
        assert_eq!(hits, vec!["sub/keep.log".to_string()]);
    }

    #[test]
    fn nested_ignore_prunes_ignored_subtree() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join(".gitignore"), "build/\n").expect("ignore file");
        std::fs::create_dir_all(dir.path().join("build")).expect("build dir");
        std::fs::write(dir.path().join("build/output.rs"), "fn x() {}\n").expect("write file");
        std::fs::write(dir.path().join("src_main.rs"), "fn x() {}\n").expect("write file");
        let hits = search_file_paths(dir.path(), "rs", 10, &search_options()).expect("search");
        assert_eq!(hits, vec!["src_main.rs".to_string()]);
    }

    #[test]
    fn ignore_rules_can_be_disabled() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join(".gitignore"), "target/\n").expect("ignore file");
        std::fs::create_dir_all(dir.path().join("target")).expect("target dir");
        std::fs::write(dir.path().join("target/build.rs"), "fn x() {}\n").expect("write file");
        let options = FileSearchOptions {
            respect_ignore: false,
            ..search_options()
        };
        let hits = search_file_paths(dir.path(), "build", 10, &options).expect("search");
        assert_eq!(hits, vec!["target/build.rs".to_string()]);
    }

    #[test]
    fn path_filter_applies_to_file_search() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("src")).expect("src dir");
        std::fs::write(dir.path().join("src/main.rs"), "fn x() {}\n").expect("write file");
        std::fs::write(dir.path().join("other.rs"), "fn x() {}\n").expect("write file");
        let mut options = search_options();
        options.path_filter.directory_prefix = Some("src".to_string());
        let hits = search_file_paths(dir.path(), "rs", 10, &options).expect("search");
        assert_eq!(hits, vec!["src/main.rs".to_string()]);
    }

    #[test]
    fn combined_path_filter_applies_to_file_search() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("src")).expect("src dir");
        std::fs::write(dir.path().join("src/main.rs"), "fn x() {}\n").expect("write file");
        std::fs::write(dir.path().join("src/skip.rs"), "fn x() {}\n").expect("write file");
        std::fs::write(dir.path().join("src/gen.generated.rs"), "fn x() {}\n").expect("write file");
        let mut options = search_options();
        options.path_filter.directory_prefix = Some("src".to_string());
        options.path_filter.exclude_patterns = vec!["src/skip.rs".to_string()];
        options.path_filter.exclude_content_types =
            vec![crate::query::types::ExcludableContentType::Generated];
        let hits = search_file_paths(dir.path(), "rs", 10, &options).expect("search");
        assert_eq!(hits, vec!["src/main.rs".to_string()]);
    }

    #[test]
    fn listing_cache_serves_stale_until_cleared() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("sub")).expect("sub dir");
        std::fs::write(dir.path().join("a.rs"), "fn x() {}\n").expect("write file");
        let first = search_file_paths(dir.path(), "rs", 10, &search_options()).expect("search");
        assert_eq!(first, vec!["a.rs".to_string()]);
        // Adding a file inside an existing subdirectory leaves the root
        // mtime alone, so the TTL-fresh cache keeps serving the old list.
        std::fs::write(dir.path().join("sub/b.rs"), "fn x() {}\n").expect("write file");
        let stale = search_file_paths(dir.path(), "rs", 10, &search_options()).expect("search");
        assert_eq!(stale, vec!["a.rs".to_string()]);
        clear_file_list_cache();
        let fresh = search_file_paths(dir.path(), "rs", 10, &search_options()).expect("search");
        assert_eq!(fresh, vec!["a.rs".to_string(), "sub/b.rs".to_string()]);
    }

    #[test]
    fn cache_can_be_bypassed_per_call() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("a.rs"), "fn x() {}\n").expect("write file");
        let options = FileSearchOptions {
            use_cache: false,
            ..search_options()
        };
        let hits = search_file_paths(dir.path(), "a", 10, &options).expect("search");
        assert_eq!(hits, vec!["a.rs".to_string()]);
    }
}
