//! Directory walker and file system scanner
//!
//! This module provides file system scanning functionality, including:
//! - Directory traversal
//! - Symlink cycle detection
//! - Streaming scan with batch callbacks for memory efficiency
//!
//! # Architecture
//!
//! The scanner uses composition to delegate specific responsibilities:
//! - `PatternMatcher`: Handles include/exclude/gitignore pattern matching
//! - `FileProcessor`: Handles file content reading, hashing, and language detection
//! - `PathTracker`: Handles symlink cycle detection
//!
//! # Streaming vs Buffered Scanning
//!
//! Two scanning modes are provided:
//!
//! 1. **Buffered** (`scan`): Returns all entries at once. Simple but uses more memory.
//! 2. **Streaming** (`scan_streaming`): Calls callback for each batch. Lower memory footprint.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use tracing::{debug, warn};

use crate::error::Result;
use crate::file_processor::FileProcessor;
use crate::models::FileEntry;
use crate::path_tracker::PathTracker;
use crate::pattern_matcher::{PatternLoadOptions, PatternMatcher};
use cce_config::ScannerConfig;
use cce_metrics::ScannerMetrics;
use cce_types::error::common;

/// Scan options for file system scanning
#[derive(Debug, Clone, Default)]
pub struct ScanOptions {
    /// Root path to scan
    pub root_path: String,
    /// Include patterns (glob patterns)
    pub include_patterns: Vec<String>,
    /// Exclude patterns (glob patterns)
    pub exclude_patterns: Vec<String>,
    /// Whether to follow symbolic links
    pub follow_symlinks: bool,
    /// Whether to respect .gitignore files
    pub respect_gitignore: bool,
    /// Additional .gitignore-style patterns
    pub gitignore_patterns: Vec<String>,
    /// Path to .gitignore file (if not in root)
    pub gitignore_path: Option<PathBuf>,
    /// Maximum file size to read content (in bytes)
    pub max_content_size: Option<u64>,
    /// Maximum file size to process in bytes, files larger than this will be skipped
    pub max_file_size: Option<u64>,
}

impl ScanOptions {
    /// Convert to pattern load options
    fn to_pattern_load_options(&self) -> PatternLoadOptions {
        PatternLoadOptions {
            include_patterns: self.include_patterns.clone(),
            exclude_patterns: self.exclude_patterns.clone(),
            respect_gitignore: self.respect_gitignore,
            gitignore_patterns: self.gitignore_patterns.clone(),
            gitignore_path: self.gitignore_path.clone(),
        }
    }
}

impl From<ScannerConfig> for ScanOptions {
    fn from(config: ScannerConfig) -> Self {
        Self {
            root_path: String::new(), // Must be set by caller
            include_patterns: config.include_patterns,
            exclude_patterns: config.exclude_patterns,
            follow_symlinks: config.follow_symlinks,
            respect_gitignore: config.respect_gitignore,
            gitignore_patterns: config.gitignore_patterns,
            gitignore_path: None,
            max_content_size: Some(config.default_max_content_size),
            max_file_size: config.max_file_size,
        }
    }
}

/// File system scanner implementation
///
/// Uses composition to delegate pattern matching and file processing
/// to specialized components.
pub struct FSScanner {
    /// File processor for reading and analyzing files
    file_processor: FileProcessor,
    /// Optional scanner metrics collector
    scanner_metrics: Option<Arc<ScannerMetrics>>,
    /// Plugin registry for the `FileFilter` capability.
    plugin_registry: Option<Arc<cce_plugin::PluginRegistry>>,
}

impl Default for FSScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl FSScanner {
    /// Create a new file system scanner
    pub fn new() -> Self {
        Self {
            file_processor: FileProcessor::default(),
            scanner_metrics: None,
            plugin_registry: None,
        }
    }

    /// Attach scanner metrics collector
    pub fn with_scanner_metrics(mut self, metrics: Arc<ScannerMetrics>) -> Self {
        self.scanner_metrics = Some(metrics);
        self
    }

    /// Attach a plugin registry for the `FileFilter` capability.
    pub fn with_plugin_registry(mut self, registry: Arc<cce_plugin::PluginRegistry>) -> Self {
        self.plugin_registry = Some(registry);
        self
    }

    /// Get a reference to the scanner metrics, if attached
    pub fn scanner_metrics(&self) -> Option<&Arc<ScannerMetrics>> {
        self.scanner_metrics.as_ref()
    }

    /// Reset the scanner state for reuse
    pub fn reset(&mut self) {
        // FileProcessor is stateless, nothing to reset
    }

    /// Create IO error with context
    fn io_error(
        context: &str,
        path: &Path,
        e: impl std::fmt::Display,
    ) -> crate::error::ScannerError {
        crate::error::ScannerError::Io(common::IoError(std::io::Error::other(format!(
            "{}: {} - {}",
            context,
            path.display(),
            e
        ))))
    }

    /// Validate and prepare root path for scanning
    fn prepare_root_path(root_path: &str) -> Result<PathBuf> {
        let root = Path::new(root_path);
        let abs_root = root
            .canonicalize()
            .map_err(|e| Self::io_error("failed to canonicalize root path", root, e))?;

        if !abs_root.exists() {
            return Err(crate::error::ScannerError::NotFound(
                cce_types::error::common::NotFoundError::new(format!(
                    "directory does not exist: {}",
                    abs_root.display()
                )),
            ));
        }

        if !abs_root.is_dir() {
            return Err(crate::error::ScannerError::invalid_argument(format!(
                "path is not a directory: {}",
                abs_root.display()
            )));
        }

        Ok(abs_root)
    }

    /// Scan a directory with streaming callback for each batch
    ///
    /// This method provides memory-efficient scanning by processing files
    /// in batches and calling the callback for each batch.
    pub fn scan_streaming<F>(
        &mut self,
        opts: &ScanOptions,
        batch_size: usize,
        mut callback: F,
    ) -> Result<usize>
    where
        F: FnMut(&mut Vec<FileEntry>),
    {
        let abs_root = Self::prepare_root_path(&opts.root_path)?;

        debug!(
            root_path = %abs_root.display(),
            batch_size = batch_size,
            follow_symlinks = opts.follow_symlinks,
            respect_gitignore = opts.respect_gitignore,
            max_file_size = ?opts.max_file_size,
            "Starting streaming file system scan"
        );

        let pattern_matcher =
            PatternMatcher::from_options(&opts.to_pattern_load_options(), &abs_root);
        let mut walker = DirectoryWalker::new(
            &self.file_processor,
            opts,
            &abs_root,
            &pattern_matcher,
            self.scanner_metrics.clone(),
        )
        .with_plugin_registry(self.plugin_registry.clone());

        let mut batch = Vec::with_capacity(batch_size);
        let mut total_count = 0;
        let mut batch_num = 0;

        let result = walker.walk(&mut |file_entry| {
            batch.push(file_entry);
            total_count += 1;

            if batch.len() >= batch_size {
                batch_num += 1;
                callback(&mut batch);
                batch.clear();
            }
            Ok(())
        });

        // Process final batch if not empty
        if !batch.is_empty() {
            batch_num += 1;
            debug!(
                batch_num = batch_num,
                files_in_batch = batch.len(),
                "Processing final batch"
            );
            callback(&mut batch);
        }

        let dirs_count = walker.dirs_count();

        result?;

        debug!(
            total_files = total_count,
            directories_scanned = dirs_count,
            batches_processed = batch_num,
            "Streaming scan completed successfully"
        );
        Ok(total_count)
    }

    /// Scan a directory and return all file entries
    ///
    /// This is a convenience method that internally uses streaming scan
    /// and collects all entries.
    pub fn scan(&mut self, opts: &ScanOptions) -> Result<Vec<FileEntry>> {
        self.scan_impl(opts, None)
    }

    /// Incremental scan: reuse the content hashes of `previous` (keyed by
    /// relative path) for files whose (size, mtime) fingerprint is unchanged,
    /// so unchanged files are never re-read or re-hashed.
    ///
    /// # Correctness note
    ///
    /// Reuse relies on mtime granularity. Callers that need to bound the
    /// staleness window (files modified twice within the mtime granularity
    /// with the same size) must periodically force a full scan instead.
    pub fn scan_incremental(
        &mut self,
        opts: &ScanOptions,
        previous: &HashMap<PathBuf, FileEntry>,
    ) -> Result<Vec<FileEntry>> {
        self.scan_impl(opts, Some(previous))
    }

    fn scan_impl(
        &mut self,
        opts: &ScanOptions,
        previous_entries: Option<&HashMap<PathBuf, FileEntry>>,
    ) -> Result<Vec<FileEntry>> {
        let abs_root = Self::prepare_root_path(&opts.root_path)?;

        debug!(
            root_path = %abs_root.display(),
            follow_symlinks = opts.follow_symlinks,
            respect_gitignore = opts.respect_gitignore,
            reuse_previous = previous_entries.is_some(),
            "Starting buffered file system scan"
        );

        let pattern_matcher =
            PatternMatcher::from_options(&opts.to_pattern_load_options(), &abs_root);
        let mut walker = DirectoryWalker::new(
            &self.file_processor,
            opts,
            &abs_root,
            &pattern_matcher,
            self.scanner_metrics.clone(),
        )
        .with_plugin_registry(self.plugin_registry.clone())
        .with_previous_entries(previous_entries);

        let mut entries = Vec::new();
        let result = walker.walk(&mut |file_entry| {
            entries.push(file_entry);
            Ok(())
        });

        let dirs_count = walker.dirs_count();

        result?;

        debug!(
            directory = %abs_root.display(),
            files_found = entries.len(),
            directories_scanned = dirs_count,
            "Buffered scan completed successfully"
        );

        Ok(entries)
    }
}

/// Internal directory walker
///
/// Handles the recursive directory traversal with symlink cycle detection.
struct DirectoryWalker<'a> {
    file_processor: &'a FileProcessor,
    opts: &'a ScanOptions,
    root_path: &'a Path,
    pattern_matcher: &'a PatternMatcher,
    path_tracker: PathTracker,
    dirs_count: usize,
    scanner_metrics: Option<Arc<ScannerMetrics>>,
    /// Plugin registry for the `FileFilter` capability.
    plugin_registry: Option<Arc<cce_plugin::PluginRegistry>>,
    /// Per-directory `FileFilter` decisions (`true` = force-include,
    /// `false` = force-exclude). Only non-`Neutral` directory decisions are
    /// cached and reused for the whole subtree, avoiding a plugin call for
    /// every file under a decided directory.
    filter_cache: HashMap<PathBuf, bool>,
    /// Entries of the previous scan keyed by relative path. Files whose
    /// (size, mtime) fingerprint is unchanged reuse the previous content hash
    /// instead of re-reading and re-hashing the file (incremental scan).
    previous_entries: Option<&'a HashMap<PathBuf, FileEntry>>,
}

impl<'a> DirectoryWalker<'a> {
    fn new(
        file_processor: &'a FileProcessor,
        opts: &'a ScanOptions,
        root_path: &'a Path,
        pattern_matcher: &'a PatternMatcher,
        scanner_metrics: Option<Arc<ScannerMetrics>>,
    ) -> Self {
        Self {
            file_processor,
            opts,
            root_path,
            pattern_matcher,
            path_tracker: PathTracker::new(),
            dirs_count: 0,
            scanner_metrics,
            plugin_registry: None,
            filter_cache: HashMap::new(),
            previous_entries: None,
        }
    }

    /// Attach a plugin registry for the `FileFilter` capability.
    fn with_plugin_registry(
        mut self,
        plugin_registry: Option<Arc<cce_plugin::PluginRegistry>>,
    ) -> Self {
        self.plugin_registry = plugin_registry;
        self
    }

    /// Attach the previous scan's entries (keyed by relative path) so files
    /// with an unchanged (size, mtime) fingerprint skip re-hashing.
    fn with_previous_entries(
        mut self,
        previous_entries: Option<&'a HashMap<PathBuf, FileEntry>>,
    ) -> Self {
        self.previous_entries = previous_entries;
        self
    }

    /// Root-relative view of `path`, used for pattern matching so that
    /// patterns like `tests/**` match regardless of the absolute root.
    fn pattern_path<'b>(&self, path: &'b Path) -> &'b Path {
        path.strip_prefix(self.root_path).unwrap_or(path)
    }

    /// Look up the nearest cached directory-prefix decision for `path`.
    ///
    /// The cache holds non-`Neutral` directory decisions; a file or nested
    /// directory inherits the decision of its nearest decided ancestor.
    fn cached_filter_decision(&self, path: &Path) -> Option<bool> {
        let mut current = Some(path);
        while let Some(dir) = current {
            if let Some(decision) = self.filter_cache.get(dir) {
                return Some(*decision);
            }
            current = dir.parent();
        }
        None
    }

    /// Ask `FileFilter` plugins for an inclusion/exclusion decision.
    ///
    /// Only the override tier (priority ≥ 0) is consulted here: the first
    /// non-`Neutral` decision wins; all-neutral defers to the built-in
    /// `PatternMatcher` (handled by the caller), after which the
    /// below-builtin fallback tier (negative priority) may veto via
    /// [`Self::plugin_filter_fallback_decision`]. Directory decisions are
    /// cached per directory prefix and reused for the whole subtree, so
    /// descendant files/dirs skip the plugin call entirely.
    fn plugin_filter_decision(
        &mut self,
        path: &Path,
        is_directory: bool,
        size: u64,
    ) -> Option<bool> {
        let registry = self.plugin_registry.as_ref()?;

        if let Some(decision) = self.cached_filter_decision(path) {
            return Some(decision);
        }

        let (above, _) = registry.get_override_plugins(
            cce_plugin::PluginCapability::FileFilter,
            Some(&path.to_string_lossy()),
            None,
        );
        if above.is_empty() {
            return None;
        }

        let mut decision: Option<bool> = None;
        for plugin in above {
            let plugin_id = plugin.metadata().id.clone();
            let plugin = plugin.clone();
            let path_str = path.to_string_lossy().to_string();
            let result = cce_plugin_runtime::execute_with_timeout_blocking(
                move |_| plugin.filter_file(&path_str, is_directory, size),
                5_000,
                &plugin_id,
                "filter_file",
            );
            match result {
                Ok(Some(cce_types::FileFilterDecision::Include)) => {
                    decision = Some(true);
                    break;
                }
                Ok(Some(cce_types::FileFilterDecision::Exclude)) => {
                    decision = Some(false);
                    break;
                }
                Ok(Some(cce_types::FileFilterDecision::Neutral)) | Ok(None) => {}
                Err(e) => {
                    tracing::warn!(
                        plugin = %plugin_id,
                        path = %path.display(),
                        error = %e,
                        "filter_file failed, deferring to built-in matcher"
                    );
                }
            }
        }

        if is_directory {
            if let Some(d) = decision {
                self.filter_cache.insert(path.to_path_buf(), d);
            }
        }
        decision
    }

    /// Ask below-builtin fallback `FileFilter` plugins (negative priority)
    /// after the built-in matcher already decided to include the path.
    ///
    /// The built-in matcher always produces a decision, so the fallback tier
    /// can only *veto*: an `Exclude` wins, `Include`/`Neutral` leave the
    /// built-in decision untouched. Returns `Some(false)` on a veto, `None`
    /// otherwise.
    fn plugin_filter_fallback_decision(
        &mut self,
        path: &Path,
        is_directory: bool,
        size: u64,
    ) -> Option<bool> {
        let registry = self.plugin_registry.as_ref()?;
        let (_, below) = registry.get_override_plugins(
            cce_plugin::PluginCapability::FileFilter,
            Some(&path.to_string_lossy()),
            None,
        );
        if below.is_empty() {
            return None;
        }
        for plugin in below {
            let plugin_id = plugin.metadata().id.clone();
            let plugin = plugin.clone();
            let path_str = path.to_string_lossy().to_string();
            let result = cce_plugin_runtime::execute_with_timeout_blocking(
                move |_| plugin.filter_file(&path_str, is_directory, size),
                5_000,
                &plugin_id,
                "filter_file",
            );
            match result {
                Ok(Some(cce_types::FileFilterDecision::Exclude)) => {
                    return Some(false);
                }
                Ok(Some(cce_types::FileFilterDecision::Include))
                | Ok(Some(cce_types::FileFilterDecision::Neutral))
                | Ok(None) => {}
                Err(e) => {
                    tracing::warn!(
                        plugin = %plugin_id,
                        path = %path.display(),
                        error = %e,
                        "fallback filter_file failed, keeping built-in decision"
                    );
                }
            }
        }
        None
    }

    /// Get the count of directories scanned
    fn dirs_count(&self) -> usize {
        self.dirs_count
    }

    /// Walk the directory tree and call callback for each file
    fn walk<F>(&mut self, callback: &mut F) -> Result<()>
    where
        F: FnMut(FileEntry) -> Result<()>,
    {
        self.walk_dir(self.root_path, callback)
    }

    fn walk_dir<F>(&mut self, dir: &Path, callback: &mut F) -> Result<()>
    where
        F: FnMut(FileEntry) -> Result<()>,
    {
        // Symlink cycle detection
        if self.path_tracker.is_visited(dir) {
            warn!(path = %dir.display(), "Detected symlink cycle, skipping");
            return Ok(());
        }
        self.path_tracker.mark_visited(dir.to_path_buf());
        self.dirs_count += 1;

        let entries_iter = std::fs::read_dir(dir)
            .map_err(|e| FSScanner::io_error("failed to read directory", dir, e))?;

        for entry in entries_iter {
            let entry = entry
                .map_err(|e| FSScanner::io_error("failed to access directory entry", dir, e))?;

            let path = entry.path();

            // Non-UTF-8 paths cannot be persisted losslessly as storage keys
            // (SQLite `UNIQUE(project_id, epoch, path)`, Qdrant point IDs,
            // BM25 document IDs); skip them at the scan boundary instead of
            // silently mangling them via `to_string_lossy`.
            if cce_types::path::is_non_utf8(&path) {
                warn!(
                    path = %path.to_string_lossy(),
                    "Skipping path with non-UTF-8 components (cannot be indexed losslessly)"
                );
                continue;
            }

            let file_type = entry
                .file_type()
                .map_err(|e| FSScanner::io_error("failed to get file type", &path, e))?;

            if file_type.is_dir() {
                self.handle_directory(&path, callback)?;
            } else if file_type.is_file() {
                self.handle_file(&path, callback)?;
            } else if file_type.is_symlink() {
                self.handle_symlink(&path, callback)?;
            }
        }

        Ok(())
    }

    fn handle_directory<F>(&mut self, path: &Path, callback: &mut F) -> Result<()>
    where
        F: FnMut(FileEntry) -> Result<()>,
    {
        if let Some(include) = self.plugin_filter_decision(path, true, 0) {
            if !include {
                return Ok(());
            }
        }
        if self
            .pattern_matcher
            .should_exclude_dir(self.pattern_path(path))
        {
            return Ok(());
        }
        // Below-builtin fallback tier (negative priority): may veto after the
        // built-in matcher included the directory.
        if let Some(false) = self.plugin_filter_fallback_decision(path, true, 0) {
            return Ok(());
        }
        self.walk_dir(path, callback)
    }

    fn handle_file<F>(&mut self, path: &Path, callback: &mut F) -> Result<()>
    where
        F: FnMut(FileEntry) -> Result<()>,
    {
        let start = Instant::now();

        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        if let Some(include) = self.plugin_filter_decision(path, false, size) {
            if !include {
                let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                if let Some(ref metrics) = self.scanner_metrics {
                    metrics.record_scan(elapsed, true, false);
                }
                return Ok(());
            }
            // The plugin forced inclusion. If the file is not recognizable
            // and no FormatParse plugin covers it, flag the mismatch so the
            // "Include" decision is not silently wasted.
            let info = cce_types::LanguageInfo::detect_from_path(&path.to_string_lossy());
            let has_format_plugin = self.plugin_registry.as_ref().is_some_and(|registry| {
                !registry
                    .get_plugins(
                        cce_plugin::PluginCapability::FormatParse,
                        Some(&path.to_string_lossy()),
                        None,
                    )
                    .is_empty()
            });
            if matches!(info.language, cce_types::language::Language::Unknown) && !has_format_plugin
            {
                tracing::info!(
                    path = %path.display(),
                    "File included by a plugin FileFilter decision but no parser handles it; it will not produce index content"
                );
            }
        } else if !self
            .pattern_matcher
            .should_include_file(self.pattern_path(path))
        {
            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
            if let Some(ref metrics) = self.scanner_metrics {
                metrics.record_scan(elapsed, true, false);
            }
            return Ok(());
        }

        // Below-builtin fallback tier (negative priority): may veto after the
        // built-in matcher included the file.
        if let Some(false) = self.plugin_filter_fallback_decision(path, false, size) {
            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
            if let Some(ref metrics) = self.scanner_metrics {
                metrics.record_scan(elapsed, true, false);
            }
            return Ok(());
        }

        match self.process_file(path) {
            Ok(file_entry) => {
                let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                let skipped = file_entry.content_hash.is_none();
                if let Some(ref metrics) = self.scanner_metrics {
                    metrics.record_scan(elapsed, false, skipped);
                    if file_entry.language_info.is_some() {
                        if let Some(ref metrics) = self.scanner_metrics {
                            metrics.languages_detected_total.increment();
                        }
                    }
                }
                callback(file_entry)
            }
            Err(e) => {
                warn!(
                    path = %path.display(),
                    error = %e,
                    "Failed to process file, skipping"
                );
                Ok(())
            }
        }
    }

    fn handle_symlink<F>(&mut self, path: &Path, callback: &mut F) -> Result<()>
    where
        F: FnMut(FileEntry) -> Result<()>,
    {
        if !self.opts.follow_symlinks {
            return Ok(());
        }

        if let Ok(target_path) = path.canonicalize() {
            if target_path.is_dir() {
                if self.path_tracker.is_visited(&target_path) {
                    warn!(
                        link = %path.display(),
                        target = %target_path.display(),
                        reason = "cycle_detected",
                        "Symlink target already visited, skipping to prevent cycle"
                    );
                    return Ok(());
                }
                self.walk_dir(&target_path, callback)?;
            } else if target_path.is_file() {
                let start = Instant::now();

                if !self.pattern_matcher.should_include_file(&target_path) {
                    if let Some(ref metrics) = self.scanner_metrics {
                        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                        metrics.record_scan(elapsed, true, false);
                    }
                    return Ok(());
                }

                match self.process_file(&target_path) {
                    Ok(file_entry) => {
                        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                        let skipped = file_entry.content_hash.is_none();
                        if let Some(ref metrics) = self.scanner_metrics {
                            metrics.record_scan(elapsed, false, skipped);
                            if file_entry.language_info.is_some() {
                                metrics.languages_detected_total.increment();
                            }
                        }
                        callback(file_entry)?;
                    }
                    Err(e) => {
                        warn!(
                            link = %path.display(),
                            target = %target_path.display(),
                            error = %e,
                            "Failed to process symlink target"
                        );
                    }
                }
            }
        } else {
            warn!(
                link = %path.display(),
                reason = "canonicalization_failed",
                "Failed to resolve symbolic link target"
            );
        }

        Ok(())
    }

    fn process_file(&self, path: &Path) -> Result<FileEntry> {
        let metadata = std::fs::metadata(path)
            .map_err(|e| FSScanner::io_error("failed to get file metadata", path, e))?;

        let file_size = metadata.len();

        // Incremental scan: when a previous entry exists for the same
        // relative path with an identical (size, mtime) fingerprint, the
        // content cannot have changed under any mtime granularity coarser
        // than the delta — reuse the previously computed hash instead of
        // re-reading the file. Entries that previously carried no hash are
        // never reused (they fall through to the normal path).
        if let Some(previous) = self.previous_entries {
            let relative = PathBuf::from(cce_types::path::relativize(self.root_path, path));
            if let Some(prev) = previous.get(&relative) {
                let modified_unchanged = metadata
                    .modified()
                    .map(|modified| {
                        prev.modified == chrono::DateTime::<chrono::Utc>::from(modified)
                    })
                    .unwrap_or(false);
                if prev.size == file_size && modified_unchanged && prev.content_hash.is_some() {
                    if let Some(metrics) = &self.scanner_metrics {
                        metrics.record_hash_reuse();
                    }
                    return Ok(prev.clone());
                }
            }
        }

        // Check if file exceeds maximum size limit
        if let Some(max_size) = self.opts.max_file_size {
            if file_size > max_size {
                let modified = metadata.modified().map_err(|e| {
                    warn!(
                        path = %path.display(),
                        error = %e,
                        "Failed to get modification time for oversized file"
                    );
                    e
                })?;

                return Ok(FileEntry {
                    path: path.to_path_buf(),
                    relative_path: PathBuf::from(cce_types::path::relativize(self.root_path, path)),
                    size: file_size,
                    modified: modified.into(),
                    content_hash: None,
                    language_info: None,
                });
            }
        }

        self.file_processor.process_file(path, self.root_path)
    }
}

#[cfg(test)]
mod tests;
