//! Simplified build configuration parser
//!
//! This module provides a minimal parser that only extracts package names
//! for import classification. All redundant metadata has been removed.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use cce_types::Language;

use super::error::ConfigParseError;
use super::registry::ParserRegistry;
use super::types::UntypedDependency;

/// Re-export the canonical build system metadata type from `cce_core`.
pub use cce_types::build_system::BuildSystemMetadata;

/// Simplified build config parser
///
/// Only extracts package names for import classification.
/// No version, source, or metadata is collected.
#[derive(Debug, Clone, Default)]
pub struct BuildConfigParser {
    /// Packages by language
    packages: HashMap<Language, HashSet<UntypedDependency>>,
    /// Config files discovered during `scan_project` (relative file names, e.g.
    /// `Cargo.toml`, `project/Cargo.toml`). Used to create graph nodes.
    /// Ordered list for stable synthetic node iteration.
    discovered_files: Vec<String>,
    /// Deduplication set for `discovered_files` (`O(1)` membership).
    discovered_set: HashSet<String>,
    /// Per-config-file dependencies (file name -> deps). Allows fine-grained
    /// diff without aggregating all languages.
    config_file_deps: HashMap<String, HashSet<UntypedDependency>>,
    /// Per-config-file content hash (relative file name -> hash). Used for
    /// fingerprint completeness so version/comment-only changes are detected.
    config_file_hashes: HashMap<String, String>,
    workspace_members: HashMap<String, std::path::PathBuf>,
    workspace_root: Option<std::path::PathBuf>,
}

impl BuildConfigParser {
    /// Create a new parser
    pub fn new() -> Self {
        Self::default()
    }

    /// Scan a project directory for build configurations.
    ///
    /// When `max_depth` is 0 (the default), only the root directory is
    /// examined — the original behaviour.  A positive depth causes breadth-
    /// first descent into immediate subdirectories, allowing workspace
    /// sub-crates and monorepo sub-packages to be discovered.
    pub fn scan_project(
        &mut self,
        root: impl AsRef<Path>,
        max_depth: usize,
    ) -> Result<(), ConfigParseError> {
        let root = root.as_ref().to_path_buf();
        let registry = ParserRegistry::register_builtin();
        self.scan_project_at(&root, &root, 0, max_depth, &registry)?;
        self.ensure_file_hashes(&root);
        let _ = self.scan_workspace(&root);
        Ok(())
    }

    pub fn scan_workspace(&mut self, root: &Path) -> Result<(), ConfigParseError> {
        self.workspace_root = Some(root.to_path_buf());
        self.detect_cargo_workspace(root);
        self.detect_javascript_workspace(root);
        self.detect_python_workspace(root);
        Ok(())
    }

    pub fn resolve_workspace_dep(&self, package_name: &str) -> Option<&Path> {
        self.workspace_members
            .get(package_name)
            .map(|p| p.as_path())
    }

    pub fn workspace_members(&self) -> &HashMap<String, std::path::PathBuf> {
        &self.workspace_members
    }

    pub fn is_workspace_member(&self, package_name: &str) -> bool {
        self.workspace_members.contains_key(package_name)
    }

    fn detect_cargo_workspace(&mut self, root: &Path) {
        let cargo_toml = root.join("Cargo.toml");
        let content = match std::fs::read_to_string(&cargo_toml) {
            Ok(c) => c,
            Err(_) => return,
        };
        let parsed: Result<toml::Value, _> = toml::from_str(&content);
        let parsed = match parsed {
            Ok(v) => v,
            Err(_) => return,
        };
        let members = parsed
            .get("workspace")
            .and_then(|w| w.get("members"))
            .and_then(|m| m.as_array());
        let Some(members) = members else {
            return;
        };
        for member in members {
            if let Some(pattern) = member.as_str() {
                let expanded = Self::expand_cargo_pattern(root, pattern);
                for member_path in expanded {
                    if let Some(pkg_name) = Self::read_cargo_package_name(&member_path) {
                        self.workspace_members.insert(pkg_name, member_path);
                    }
                }
            }
        }
    }

    fn expand_cargo_pattern(root: &Path, pattern: &str) -> Vec<std::path::PathBuf> {
        let mut results = Vec::new();
        if pattern.contains('*') {
            if let Some((prefix, _suffix)) = pattern.split_once('*') {
                let base = root.join(prefix.trim_end_matches('/'));
                if let Ok(entries) = std::fs::read_dir(&base) {
                    for entry in entries.filter_map(|e| e.ok()) {
                        let path = entry.path();
                        if path.is_dir() && path.join("Cargo.toml").exists() {
                            results.push(path);
                        }
                    }
                }
            }
        } else {
            let candidate = root.join(pattern);
            if candidate.join("Cargo.toml").exists() {
                results.push(candidate);
            } else if candidate.exists() && candidate.is_dir() {
                if let Ok(entries) = std::fs::read_dir(&candidate) {
                    for entry in entries.filter_map(|e| e.ok()) {
                        let path = entry.path();
                        if path.is_dir() && path.join("Cargo.toml").exists() {
                            results.push(path);
                        }
                    }
                }
            }
        }
        results
    }

    fn read_cargo_package_name(member_path: &Path) -> Option<String> {
        let cargo_toml = member_path.join("Cargo.toml");
        let content = std::fs::read_to_string(cargo_toml).ok()?;
        let parsed: toml::Value = toml::from_str(&content).ok()?;
        parsed
            .get("package")
            .and_then(|p| p.get("name"))
            .and_then(|n| n.as_str())
            .map(|s| s.to_string())
    }

    fn detect_javascript_workspace(&mut self, root: &Path) {
        let pkg_json = root.join("package.json");
        let content = match std::fs::read_to_string(&pkg_json) {
            Ok(c) => c,
            Err(_) => return,
        };
        let parsed: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => return,
        };
        let workspaces = parsed.get("workspaces").and_then(|w| {
            if let Some(arr) = w.as_array() {
                Some(
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect::<Vec<_>>(),
                )
            } else if let Some(obj) = w.as_object() {
                obj.get("packages").and_then(|p| p.as_array()).map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect::<Vec<_>>()
                })
            } else {
                None
            }
        });
        let Some(workspaces) = workspaces else {
            return;
        };
        for pattern in workspaces {
            let expanded = Self::expand_js_pattern(root, &pattern);
            for member_path in expanded {
                if let Some(pkg_name) = Self::read_package_json_name(&member_path) {
                    self.workspace_members.insert(pkg_name, member_path);
                }
            }
        }
    }

    fn expand_js_pattern(root: &Path, pattern: &str) -> Vec<std::path::PathBuf> {
        if pattern.contains('*') {
            if let Some((prefix, suffix)) = pattern.split_once('*') {
                let base = root.join(prefix.trim_end_matches('/'));
                let mut results = Vec::new();
                if let Ok(entries) = std::fs::read_dir(&base) {
                    for entry in entries.filter_map(|e| e.ok()) {
                        let path = entry.path();
                        if path.is_dir()
                            && path.join("package.json").exists()
                            && (suffix.is_empty()
                                || path
                                    .to_string_lossy()
                                    .ends_with(suffix.trim_start_matches('/')))
                        {
                            results.push(path);
                        }
                    }
                }
                return results;
            }
        }
        let candidate = root.join(pattern);
        if candidate.join("package.json").exists() {
            vec![candidate]
        } else {
            Vec::new()
        }
    }

    fn read_package_json_name(member_path: &Path) -> Option<String> {
        let pkg_json = member_path.join("package.json");
        let content = std::fs::read_to_string(pkg_json).ok()?;
        let parsed: serde_json::Value = serde_json::from_str(&content).ok()?;
        parsed
            .get("name")
            .and_then(|n| n.as_str())
            .map(|s| s.to_string())
    }

    fn detect_python_workspace(&mut self, root: &Path) {
        let pyproject = root.join("pyproject.toml");
        let content = match std::fs::read_to_string(&pyproject) {
            Ok(c) => c,
            Err(_) => return,
        };
        let parsed: Result<toml::Value, _> = toml::from_str(&content);
        let parsed = match parsed {
            Ok(v) => v,
            Err(_) => return,
        };
        let members = parsed
            .get("tool")
            .and_then(|t| t.get("uv"))
            .and_then(|u| u.get("workspace"))
            .and_then(|w| w.get("members"))
            .and_then(|m| m.as_array());
        let Some(members) = members else {
            return;
        };
        for member in members {
            if let Some(pattern) = member.as_str() {
                let expanded = Self::expand_cargo_pattern(root, pattern);
                for member_path in expanded {
                    if let Some(pkg_name) = Self::read_pyproject_name(&member_path) {
                        self.workspace_members.insert(pkg_name, member_path);
                    } else if let Some(dir_name) = member_path.file_name().and_then(|n| n.to_str())
                    {
                        self.workspace_members
                            .insert(dir_name.to_string(), member_path);
                    }
                }
            }
        }
    }

    fn read_pyproject_name(member_path: &Path) -> Option<String> {
        let pyproject = member_path.join("pyproject.toml");
        let content = std::fs::read_to_string(pyproject).ok()?;
        let parsed: toml::Value = toml::from_str(&content).ok()?;
        parsed
            .get("project")
            .and_then(|p| p.get("name"))
            .and_then(|n| n.as_str())
            .map(|s| s.to_string())
    }

    /// Async variant of [`Self::scan_project`] that offloads filesystem
    /// traversal to the blocking thread pool.
    ///
    /// The directory walk and per-manifest parsing are pure synchronous
    /// operations that would otherwise block the async executor when
    /// `manifest_scan_depth > 0`. This wrapper moves the work into
    /// `spawn_blocking` so callers in async contexts can `await` without
    /// stalling the executor. Parsing sub-steps remain synchronous inside
    /// the blocking task.
    pub async fn scan_project_async(
        &mut self,
        root: impl AsRef<Path> + Send + 'static,
        max_depth: usize,
    ) -> Result<(), ConfigParseError> {
        let root_buf = root.as_ref().to_path_buf();
        let root_for_task = root_buf.clone();
        let mut taken = std::mem::take(self);
        let (result, returned) = tokio::task::spawn_blocking(move || {
            let res = taken.scan_project(&root_for_task, max_depth);
            (res, taken)
        })
        .await
        .map_err(|e| ConfigParseError::Io {
            path: root_buf.clone(),
            source: std::io::Error::other(e.to_string()),
        })?;
        *self = returned;
        result
    }

    /// Ensure content hashes are populated for all discovered files.
    ///
    /// Uses the already-discovered file list and reads each file once to
    /// compute its hash. This avoids requiring every parser to duplicate the
    /// hashing logic while still providing a single-pass content fingerprint.
    /// File reads are batched and executed concurrently (rayon) with the
    /// discovery list reused as the single source of paths.
    fn ensure_file_hashes(&mut self, project_root: &Path) {
        let files = self.discovered_files.clone();
        let pending: Vec<String> = files
            .into_iter()
            .filter(|rel| !self.config_file_hashes.contains_key(rel))
            .collect();
        if pending.is_empty() {
            return;
        }
        let project_root = project_root.to_path_buf();
        use rayon::prelude::*;
        let hashes: Vec<(String, String)> = pending
            .par_iter()
            .map(|rel| {
                let path = project_root.join(rel);
                let content = std::fs::read(&path).unwrap_or_default();
                let hash = cce_utils::hash::calculate_hash(&content);
                (rel.clone(), hash)
            })
            .collect();
        for (rel, hash) in hashes {
            self.config_file_hashes.insert(rel, hash);
        }
    }

    /// Directories to skip during recursive manifest discovery.
    ///
    /// Delegates to the canonical list in `cce_types::build_system`.
    const EXCLUDED_DIRS: &[&str] = cce_types::build_system::MANIFEST_SCAN_EXCLUDED_DIRS;

    /// Record a discovered config file (deduplicated).
    fn record_discovered(&mut self, file_name: &str) {
        if self.discovered_set.insert(file_name.to_string()) {
            self.discovered_files.push(file_name.to_string());
        }
    }

    /// Insert packages for a specific config file (tracks per-file mapping).
    pub(crate) fn insert_packages_for_file(
        &mut self,
        file_name: &str,
        lang: Language,
        deps: HashSet<UntypedDependency>,
    ) {
        if !deps.is_empty() {
            self.record_discovered(file_name);
            self.config_file_deps
                .entry(file_name.to_string())
                .or_default()
                .extend(deps.clone());
            self.packages.entry(lang).or_default().extend(deps);
        }
    }

    /// Recursive directory scan for build manifests.
    fn scan_project_at(
        &mut self,
        project_root: &Path,
        dir: &Path,
        current_depth: usize,
        max_depth: usize,
        registry: &ParserRegistry,
    ) -> Result<(), ConfigParseError> {
        let entries: Vec<std::fs::DirEntry> = match std::fs::read_dir(dir) {
            Ok(e) => e.filter_map(|x| x.ok()).collect(),
            Err(_) => Vec::new(),
        };

        self.discover_from_entries(project_root, &entries);
        registry.scan_directory(project_root, dir, &entries, self)?;

        if current_depth >= max_depth {
            return Ok(());
        }

        for entry in &entries {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let dir_name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n,
                None => continue,
            };
            if Self::EXCLUDED_DIRS.contains(&dir_name) {
                continue;
            }
            self.scan_project_at(project_root, &path, current_depth + 1, max_depth, registry)?;
        }

        Ok(())
    }

    /// Get package names for a language
    pub fn packages_for_language(&self, lang: Language) -> HashSet<String> {
        self.packages
            .get(&lang)
            .map(|deps| deps.iter().map(|d| d.name.clone()).collect())
            .unwrap_or_default()
    }

    /// Get all languages that have dependencies
    ///
    /// Returns a list of languages for which dependencies were discovered
    /// during project scanning. This allows dynamic loading without hardcoded
    /// language lists.
    pub fn languages_with_dependencies(&self) -> Vec<Language> {
        self.packages
            .keys()
            .filter(|lang| !self.packages[*lang].is_empty())
            .cloned()
            .collect()
    }

    /// Get all dependencies with type information for a language
    pub fn dependencies_for_language(&self, lang: Language) -> HashSet<UntypedDependency> {
        self.packages.get(&lang).cloned().unwrap_or_default()
    }

    /// Check if a package is known for a language
    pub fn is_known_package(&self, package: &str, lang: Language) -> bool {
        self.packages
            .get(&lang)
            .map(|deps| deps.iter().any(|d| d.name == package))
            .unwrap_or(false)
    }

    /// Discover config files present in `dir` (non-recursive) from a
    /// pre-collected directory entry list. Single `read_dir` per directory
    /// is shared with the caller (`scan_project_at`), eliminating the
    /// previous per-pattern `read_dir` overhead.
    fn discover_from_entries(&mut self, project_root: &Path, entries: &[std::fs::DirEntry]) {
        for entry in entries {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = entry.file_name().to_str().map(|s| s.to_string()) else {
                continue;
            };
            if cce_types::build_system::is_build_config_name(&name) {
                let rel = cce_types::path::relativize(project_root, &path);
                self.record_discovered(&rel);
            }
        }
    }

    /// All config files discovered during scanning.
    pub fn discovered_config_files(&self) -> &[String] {
        &self.discovered_files
    }

    /// Per-config-file dependency mapping (file name -> deps).
    pub fn config_file_dependencies(&self) -> &HashMap<String, HashSet<UntypedDependency>> {
        &self.config_file_deps
    }

    /// Per-config-file content hashes (file name -> hash).
    pub fn config_file_hashes(&self) -> &HashMap<String, String> {
        &self.config_file_hashes
    }

    /// Package-level diff between two parser snapshots.
    ///
    /// Returns `(added_by_language, removed_by_language)` aggregated over all
    /// discovered config files. Callers can intersect `added` with the import
    /// index to find files that actually import a changed package, instead of
    /// invalidating the whole extension closure.
    pub fn package_diff(
        &self,
        other: &Self,
    ) -> HashMap<Language, (HashSet<String>, HashSet<String>)> {
        let mut diff: HashMap<Language, (HashSet<String>, HashSet<String>)> = HashMap::new();
        let mut seen: HashSet<Language> = HashSet::new();
        for lang in cce_types::build_system::get_supported_build_systems()
            .iter()
            .flat_map(|m| m.languages.clone())
        {
            if !seen.insert(lang) {
                continue;
            }
            let old: HashSet<String> = self.packages_for_language(lang);
            let new: HashSet<String> = other.packages_for_language(lang);
            let added: HashSet<String> = new.difference(&old).cloned().collect();
            let removed: HashSet<String> = old.difference(&new).cloned().collect();
            if !added.is_empty() || !removed.is_empty() {
                diff.insert(lang, (added, removed));
            }
        }
        diff
    }

    /// Build a synthetic `ParsedFile` from already-read content.
    ///
    /// Internal helper so batch loaders can reuse the same construction after
    /// a single batched file-content collection.
    fn synthetic_parsed_file_from_content(
        &self,
        rel_path: &str,
        content: String,
    ) -> cce_types::ParsedFile {
        use cce_types::{
            Entity, EntityId, EntityKind, ImportTable, ParsedFile, RawRelationData, RelationLevel,
            RelationType, Span,
        };
        let mut parsed = ParsedFile::new(
            cce_types::Language::Unknown,
            rel_path.to_string(),
            content.clone(),
        );
        parsed.import_table = Some(ImportTable {
            file_id: rel_path.to_string(),
            ..Default::default()
        });
        let entity = Entity {
            id: EntityId(0),
            kind: EntityKind::Module,
            name: rel_path.to_string(),
            signature: rel_path.to_string(),
            parameters: Vec::new(),
            return_type: None,
            span: Span::default(),
            depth: 0,
            parent: None,
            children: Vec::new(),
            doc_comment: None,
            modifiers: Vec::new(),
            attributes: HashMap::new(),
            metadata: HashMap::new(),
            is_stdlib: false,
            subtype: None,
            stdlib_category: None,
        };
        parsed.add_entity(entity);
        if let Some(deps) = self.config_file_deps.get(rel_path) {
            let mut emitted: HashSet<String> = HashSet::new();
            for dep in deps {
                if emitted.insert(dep.name.clone()) {
                    parsed.add_relation(RawRelationData {
                        src: EntityId(0),
                        level: RelationLevel::Entity,
                        dst_name: dep.name.clone(),
                        relation_type: RelationType::DirectCall,
                        span: Span::default(),
                        stdlib_category: None,
                    });
                }
            }
        }
        parsed.file_hash = Some(cce_utils::hash::calculate_hash(content.as_bytes()));
        parsed
    }

    /// Build a synthetic `ParsedFile` for a single config file.
    ///
    /// Always creates the module entity for `rel_path` and attaches
    /// per-file dependency edges if present. This is the single source of
    /// truth for synthetic config nodes; both bulk and single-file callers
    /// delegate here to keep construction consistent.
    pub fn synthetic_parsed_file_for(&self, root: &Path, rel_path: &str) -> cce_types::ParsedFile {
        let path = root.join(rel_path);
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        self.synthetic_parsed_file_from_content(rel_path, content)
    }

    /// Build synthetic `ParsedFile` placeholders for discovered config files.
    ///
    /// Each config file becomes a file-level entity (`Module` kind) so the
    /// relation graph can store `config_file -> declared_dependency` edges and
    /// `source_file -> config_file` governance edges. The synthetic entity uses
    /// `EntityId(0)` locally and will be remapped by `IndexBuilder`.
    /// File contents are collected in a single batch and read concurrently
    /// (rayon, controlled by the global thread pool) reusing the discovery
    /// list without any additional filesystem traversal.
    pub fn synthetic_config_parsed_files(&self, root: &Path) -> Vec<cce_types::ParsedFile> {
        if self.discovered_files.is_empty() {
            return Vec::new();
        }
        let root_buf = root.to_path_buf();
        use rayon::prelude::*;
        let contents: Vec<(String, String)> = self
            .discovered_files
            .par_iter()
            .map(|rel| {
                let path = root_buf.join(rel);
                let content = std::fs::read_to_string(&path).unwrap_or_default();
                (rel.clone(), content)
            })
            .collect();
        contents
            .into_iter()
            .map(|(rel, content)| self.synthetic_parsed_file_from_content(&rel, content))
            .collect()
    }

    // ========== Build System Metadata ==========

    /// Get metadata for all supported build systems
    ///
    /// Delegates to the canonical table in `cce_types::build_system`.
    pub fn get_supported_build_systems() -> Vec<BuildSystemMetadata> {
        cce_types::build_system::get_supported_build_systems()
    }

    /// Get file extensions affected by a config file change
    ///
    /// Delegates to `cce_types::build_system::get_affected_extensions`.
    pub fn get_affected_extensions(config_filename: &str) -> Vec<String> {
        cce_types::build_system::get_affected_extensions(config_filename)
    }

    /// Check if a file is a recognized build configuration
    ///
    /// Delegates to the canonical rule set in `cce_types::build_system`.
    pub fn is_build_config(filename: &str) -> bool {
        cce_types::build_system::is_build_config(filename)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_affected_extensions_cargo() {
        let exts = BuildConfigParser::get_affected_extensions("Cargo.toml");
        assert_eq!(exts, vec!["rs"]);
    }

    #[test]
    fn test_get_affected_extensions_npm() {
        let exts = BuildConfigParser::get_affected_extensions("package.json");
        assert!(exts.contains(&"js".to_string()));
        assert!(exts.contains(&"ts".to_string()));
        assert!(exts.contains(&"tsx".to_string()));
    }

    #[test]
    fn test_get_affected_extensions_python() {
        let exts = BuildConfigParser::get_affected_extensions("requirements.txt");
        assert!(exts.contains(&"py".to_string()));

        let exts = BuildConfigParser::get_affected_extensions("pyproject.toml");
        assert!(exts.contains(&"py".to_string()));
    }

    #[test]
    fn test_get_affected_extensions_go() {
        let exts = BuildConfigParser::get_affected_extensions("go.mod");
        assert_eq!(exts, vec!["go"]);
    }

    #[test]
    fn test_get_affected_extensions_java() {
        let exts = BuildConfigParser::get_affected_extensions("pom.xml");
        assert!(exts.contains(&"java".to_string()));

        let exts = BuildConfigParser::get_affected_extensions("build.gradle");
        assert!(exts.contains(&"java".to_string()));
        assert!(exts.contains(&"kt".to_string()));

        let exts = BuildConfigParser::get_affected_extensions("build.gradle.kts");
        assert!(exts.contains(&"java".to_string()));
        assert!(exts.contains(&"kt".to_string()));

        let exts = BuildConfigParser::get_affected_extensions("settings.gradle");
        assert!(exts.contains(&"java".to_string()));
        assert!(exts.contains(&"kt".to_string()));

        let exts = BuildConfigParser::get_affected_extensions("settings.gradle.kts");
        assert!(exts.contains(&"java".to_string()));
        assert!(exts.contains(&"kt".to_string()));
    }

    #[test]
    fn test_get_affected_extensions_unknown() {
        let exts = BuildConfigParser::get_affected_extensions("unknown.conf");
        assert!(exts.is_empty());
    }

    #[test]
    fn test_is_build_config_recognized() {
        assert!(BuildConfigParser::is_build_config("Cargo.toml"));
        assert!(BuildConfigParser::is_build_config("package.json"));
        assert!(BuildConfigParser::is_build_config("requirements.txt"));
        assert!(BuildConfigParser::is_build_config("go.mod"));
        assert!(BuildConfigParser::is_build_config("pom.xml"));
        assert!(BuildConfigParser::is_build_config("build.gradle.kts"));
        assert!(BuildConfigParser::is_build_config("settings.gradle"));
        assert!(BuildConfigParser::is_build_config("settings.gradle.kts"));
    }

    #[test]
    fn test_is_build_config_unrecognized() {
        assert!(!BuildConfigParser::is_build_config("README.md"));
        assert!(!BuildConfigParser::is_build_config("config.yaml"));
        // Makefile/Dockerfile are build configs now (see test_is_build_config_make_docker)
        assert!(!BuildConfigParser::is_build_config("Justfile"));
    }

    #[test]
    fn test_is_build_config_make_and_docker() {
        assert!(BuildConfigParser::is_build_config("Makefile"));
        assert!(BuildConfigParser::is_build_config("GNUmakefile"));
        assert!(BuildConfigParser::is_build_config("Dockerfile"));
        assert!(BuildConfigParser::is_build_config("makefile"));
    }

    #[test]
    fn test_get_affected_extensions_make() {
        let exts = BuildConfigParser::get_affected_extensions("Makefile");
        assert!(!exts.is_empty());
        assert!(exts.contains(&"c".to_string()));
        assert!(exts.contains(&"cpp".to_string()));
    }

    #[test]
    fn test_get_affected_extensions_docker() {
        let exts = BuildConfigParser::get_affected_extensions("Dockerfile");
        // Docker has no source-language closure; only the config reload fires.
        assert_eq!(exts, vec!["dockerfile".to_string()]);
    }

    #[test]
    fn test_get_supported_build_systems_count() {
        let systems = BuildConfigParser::get_supported_build_systems();
        // Should have at least the major build systems
        assert!(systems.len() >= 8);

        // Verify each system has required fields
        for system in &systems {
            assert!(!system.name.is_empty());
            assert!(!system.config_files.is_empty());
            assert!(!system.file_extensions.is_empty());
        }
    }

    #[test]
    fn test_build_system_metadata_consistency() {
        let systems = BuildConfigParser::get_supported_build_systems();

        // Verify Cargo metadata
        let cargo = systems
            .iter()
            .find(|s| s.name == "Cargo")
            .expect("Cargo should exist");
        assert_eq!(cargo.config_files, vec!["Cargo.toml"]);
        assert_eq!(cargo.languages, vec![Language::Rust]);
        assert_eq!(
            cargo.file_extensions,
            Language::Rust
                .extensions()
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        );

        // Verify NPM metadata includes all JS/TS extensions
        let npm = systems
            .iter()
            .find(|s| s.name == "NPM")
            .expect("NPM should exist");
        assert!(npm.file_extensions.contains(&"js".to_string()));
        assert!(npm.file_extensions.contains(&"ts".to_string()));
        assert!(npm.file_extensions.contains(&"jsx".to_string()));
        assert!(npm.file_extensions.contains(&"tsx".to_string()));

        // Verify Python metadata
        let pypi = systems
            .iter()
            .find(|s| s.name == "PyPI")
            .expect("PyPI should exist");
        assert_eq!(pypi.languages, vec![Language::Python]);
        assert_eq!(
            pypi.file_extensions,
            Language::Python
                .extensions()
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_language_extensions_consistency() {
        // Verify that BuildConfigParser uses Language::extensions() consistently
        let systems = BuildConfigParser::get_supported_build_systems();

        for system in &systems {
            for lang in &system.languages {
                let lang_exts: Vec<String> =
                    lang.extensions().iter().map(|s| s.to_string()).collect();

                // All language extensions should be present in the build system's file_extensions
                for ext in &lang_exts {
                    assert!(
                        system.file_extensions.contains(ext),
                        "Build system '{}' should include extension '{}' for language {:?}",
                        system.name,
                        ext,
                        lang
                    );
                }
            }
        }
    }

    #[test]
    fn test_no_hardcoded_extensions_in_build_systems() {
        // This test ensures we're using Language::extensions() instead of hard-coded strings
        let systems = BuildConfigParser::get_supported_build_systems();

        // Check that Rust only has 'rs' extension (from Language::Rust.extensions())
        let cargo = systems
            .iter()
            .find(|s| s.name == "Cargo")
            .expect("Cargo should exist");
        assert_eq!(cargo.file_extensions.len(), 1);
        assert_eq!(cargo.file_extensions[0], "rs");

        // Check that Go only has 'go' extension
        let go_modules = systems
            .iter()
            .find(|s| s.name == "Go Modules")
            .expect("Go Modules should exist");
        assert_eq!(go_modules.file_extensions.len(), 1);
        assert_eq!(go_modules.file_extensions[0], "go");

        // Check that PHP only has 'php' extension
        let composer = systems
            .iter()
            .find(|s| s.name == "Composer")
            .expect("Composer should exist");
        assert_eq!(composer.file_extensions.len(), 1);
        assert_eq!(composer.file_extensions[0], "php");

        // Check that Ruby only has 'rb' extension
        let bundler = systems
            .iter()
            .find(|s| s.name == "Bundler")
            .expect("Bundler should exist");
        assert_eq!(bundler.file_extensions.len(), 1);
        assert_eq!(bundler.file_extensions[0], "rb");
    }
}
