//! Builder configuration management
//!
//! Handles external packages, dependencies, and build configuration loading.
//!
//! The configuration is split into two clear layers:
//! - `ExternalPackageData`: Data extracted from build system manifests (Cargo.toml, package.json, etc.)
//! - `BuildPolicy`: User-defined policies from application configuration (TOML)
//!
//! These layers are combined in `BuilderConfig` which is consumed by the index builder.

use crate::config_parser::UntypedDependency;
use cce_types::language::Language;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use thiserror::Error;

/// Errors that can occur during builder configuration validation.
#[derive(Error, Debug)]
pub enum BuilderConfigError {
    /// An empty package list was provided for a language.
    #[error("empty package list for language {language}")]
    EmptyPackageList { language: String },

    /// An empty dependency list was provided for a language.
    #[error("empty dependency list for language {language}")]
    EmptyDependencyList { language: String },

    /// A discovered config file is missing from the dependency mapping.
    #[error("missing dependency mapping for discovered config file: {file}")]
    MissingDependencyMapping { file: String },
}

/// Lock-free fingerprint cache using atomic validity flag.
///
/// The fast path (`get`) performs a single atomic load without acquiring
/// the mutex. The mutex is only contested on cache invalidation (writes)
/// or the first computation (cold path). This reduces lock contention
/// from O(reads + writes) to O(writes) compared to a plain `Mutex<Option<String>>`.
#[derive(Debug)]
struct FingerprintCache {
    valid: AtomicBool,
    value: Mutex<Option<String>>,
}

impl FingerprintCache {
    fn new() -> Self {
        Self {
            valid: AtomicBool::new(false),
            value: Mutex::new(None),
        }
    }

    /// Try to return the cached fingerprint. Returns `None` if the cache
    /// is invalid or the mutex is poisoned.
    fn get(&self) -> Option<String> {
        if !self.valid.load(Ordering::Acquire) {
            return None;
        }
        self.value
            .lock()
            .expect("fingerprint cache lock poisoned")
            .clone()
    }

    /// Store a new fingerprint and mark the cache as valid.
    fn set(&self, fp: String) {
        if let Ok(mut guard) = self.value.lock() {
            *guard = Some(fp);
            self.valid.store(true, Ordering::Release);
        }
    }

    /// Invalidate the cache so the next `get` returns `None`.
    fn invalidate(&self) {
        self.valid.store(false, Ordering::Release);
        if let Ok(mut guard) = self.value.lock() {
            *guard = None;
        }
    }
}

/// External package data extracted from build system manifests.
///
/// This struct contains dependency information discovered by scanning
/// build configuration files (Cargo.toml, package.json, go.mod, etc.).
/// The data is used for import classification during relation resolution.
#[derive(Debug, Default, Clone)]
pub struct ExternalPackageData {
    /// External packages for import classification (language -> package names)
    pub external_packages: Option<HashMap<Language, HashSet<String>>>,
    /// Full dependency information for enhanced classification (language -> dependencies)
    pub external_dependencies: Option<HashMap<Language, Vec<UntypedDependency>>>,
    /// Discovered config files (sorted list for fingerprinting)
    pub discovered_config_files: Option<Vec<String>>,
    /// Per-config-file dependency mapping for fine-grained fingerprinting
    pub config_file_deps: Option<HashMap<String, HashSet<UntypedDependency>>>,
    /// Per-config-file content hashes for version/comment change detection
    pub config_content_hashes: Option<HashMap<String, String>>,
}

/// Build policy flags from application configuration.
///
/// These flags control how the relation index is constructed.
/// Changes to these flags trigger cache invalidation and index rebuild.
#[derive(Debug, Clone)]
pub struct BuildPolicy {
    /// Whether to filter out standard library calls from relation index
    pub filter_stdlib_calls: bool,
    /// Maximum number of resolved relations retained for one source file.
    pub max_relations_per_file: usize,
    /// Whether import and export metadata is included in the relation graph.
    pub analyze_imports: bool,
    /// Whether cross-file dependency edges are tracked.
    pub track_cross_file_deps: bool,
    /// Whether `SymbolExtract` plugins supply import/export extraction for
    /// custom languages. Default off (see `relation.plugin_symbol_extract_enabled`).
    pub symbol_extract_enabled: bool,
    /// Whether to auto-load external dependency symbols from package manager
    /// caches during the build phase. When enabled, the builder discovers
    /// installed packages and extracts their public API surface for improved
    /// relation resolution accuracy.
    pub load_external_symbols: bool,
    /// Whether `import_table` is required; when true, missing `import_table`
    /// causes an error instead of falling back to tree-sitter re-parse.
    pub require_import_table: bool,
    /// Whether to automatically detect and load external symbols.
    pub auto_detect_external_symbols: bool,
    /// Optional cache directory for external symbols.
    pub external_symbols_cache_dir: Option<std::path::PathBuf>,
}

/// Builder configuration combining external package data and build policy.
///
/// This struct is the bridge between application configuration and
/// build system data. It is consumed by the index builder to construct
/// the relation graph.
#[derive(Debug)]
pub struct BuilderConfig {
    /// External package data from build system manifests
    pub package_data: ExternalPackageData,
    /// Build policy flags from application configuration
    pub policy: BuildPolicy,
    /// Symbol resolution tuning knobs.
    pub symbol_resolution: cce_config::SymbolResolutionConfig,
    /// Cached fingerprint computed after the last configuration load.
    /// Uses atomic validity flag for lock-free reads on the hot path.
    cached_fingerprint: FingerprintCache,
}

impl Clone for BuilderConfig {
    fn clone(&self) -> Self {
        Self {
            package_data: self.package_data.clone(),
            policy: self.policy.clone(),
            symbol_resolution: self.symbol_resolution.clone(),
            cached_fingerprint: FingerprintCache::new(),
        }
    }
}

impl Default for BuilderConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl BuilderConfig {
    /// Create a new builder config with default settings
    pub fn new() -> Self {
        Self {
            package_data: ExternalPackageData::default(),
            policy: BuildPolicy {
                filter_stdlib_calls: true,
                max_relations_per_file: 10_000,
                analyze_imports: true,
                track_cross_file_deps: true,
                symbol_extract_enabled: false,
                load_external_symbols: true,
                require_import_table: false,
                auto_detect_external_symbols: true,
                external_symbols_cache_dir: None,
            },
            symbol_resolution: cce_config::SymbolResolutionConfig::default(),
            cached_fingerprint: FingerprintCache::new(),
        }
    }

    /// Create a builder config from shared relation parameters.
    ///
    /// This method uses `RelationBuilderParams` as the single source of
    /// truth for construction-affecting configuration, eliminating
    /// duplication between the full-index and hot-update paths.
    pub fn from_params(params: &cce_config::RelationBuilderParams) -> Self {
        Self {
            package_data: ExternalPackageData::default(),
            policy: BuildPolicy {
                filter_stdlib_calls: params.filter_stdlib_calls,
                max_relations_per_file: params.max_relations_per_file,
                analyze_imports: params.analyze_imports,
                track_cross_file_deps: params.track_cross_file_deps,
                symbol_extract_enabled: params.symbol_extract_enabled,
                load_external_symbols: true,
                require_import_table: false,
                auto_detect_external_symbols: true,
                external_symbols_cache_dir: None,
            },
            symbol_resolution: cce_config::SymbolResolutionConfig::default(),
            cached_fingerprint: FingerprintCache::new(),
        }
    }

    /// Validate the builder configuration.
    ///
    /// This method checks for invalid or inconsistent configuration states.
    /// Returns `Ok(())` if the configuration is valid, or an error describing
    /// the validation failure.
    pub fn validate(&self) -> Result<(), BuilderConfigError> {
        // Validate policy invariants
        if self.policy.max_relations_per_file == 0 {
            // 0 means unlimited, which is valid
        }

        // Validate package data consistency
        if let Some(ref packages) = self.package_data.external_packages {
            for (language, names) in packages {
                if names.is_empty() {
                    return Err(BuilderConfigError::EmptyPackageList {
                        language: format!("{:?}", language),
                    });
                }
            }
        }

        if let Some(ref deps) = self.package_data.external_dependencies {
            for (language, dependencies) in deps {
                if dependencies.is_empty() {
                    return Err(BuilderConfigError::EmptyDependencyList {
                        language: format!("{:?}", language),
                    });
                }
            }
        }

        // Validate discovered config files consistency
        if let Some(ref files) = self.package_data.discovered_config_files {
            if let Some(ref file_deps) = self.package_data.config_file_deps {
                for file in files {
                    if !file_deps.contains_key(file) {
                        return Err(BuilderConfigError::MissingDependencyMapping {
                            file: file.clone(),
                        });
                    }
                }
            }
        }

        Ok(())
    }

    fn invalidate_cache(&self) {
        self.cached_fingerprint.invalidate();
    }

    fn cache_fingerprint(&self, fp: String) {
        self.cached_fingerprint.set(fp);
    }

    /// Set external packages for import classification
    ///
    /// This allows the builder to classify imports into standard library,
    /// external library, and internal module categories.
    pub fn set_external_packages(&mut self, language: Language, packages: HashSet<String>) {
        self.package_data
            .external_packages
            .get_or_insert_with(HashMap::new)
            .insert(language, packages);
        self.invalidate_cache();
    }

    /// Set full dependency information for enhanced classification
    ///
    /// This provides detailed dependency metadata (version, dev flag, local path, etc.)
    /// for more accurate import classification.
    pub fn set_external_dependencies(
        &mut self,
        language: Language,
        dependencies: Vec<UntypedDependency>,
    ) {
        // Also update the simple package names for backward compatibility
        let package_names: HashSet<String> = dependencies.iter().map(|d| d.name.clone()).collect();
        // set_external_packages invalidates, so avoid double invalidation by inserting directly
        self.package_data
            .external_packages
            .get_or_insert_with(HashMap::new)
            .insert(language, package_names);

        // Store full dependency information
        self.package_data
            .external_dependencies
            .get_or_insert_with(HashMap::new)
            .insert(language, dependencies);
        self.invalidate_cache();
    }

    /// Automatically load external packages from a BuildConfigParser
    ///
    /// This method scans all parsed build configurations and extracts external
    /// package names for each supported language.
    pub fn auto_load_external_packages(
        &mut self,
        config_parser: &crate::config_parser::BuildConfigParser,
    ) {
        let mut changed = false;
        // Dynamically load packages for all languages with dependencies
        for language in config_parser.languages_with_dependencies() {
            let packages = config_parser.packages_for_language(language);
            if !packages.is_empty() {
                let package_count = packages.len();
                // Insert directly to avoid per-language invalidation
                self.package_data
                    .external_packages
                    .get_or_insert_with(HashMap::new)
                    .insert(language, packages);
                changed = true;
                tracing::debug!(
                    "Loaded {} external packages for language {:?}",
                    package_count,
                    language
                );
            }
        }
        if changed {
            self.invalidate_cache();
            let fp = self.compute_fingerprint();
            self.cache_fingerprint(fp);
        }
    }

    /// Automatically load full dependency information from a BuildConfigParser
    ///
    /// This method extracts complete dependency metadata (version, dev flag, local path, etc.)
    /// for enhanced import classification. It also captures discovered config file
    /// list, per-file dependency mapping, and content hashes for fingerprint
    /// completeness so version/comment-only changes are detected.
    pub fn auto_load_dependencies(
        &mut self,
        config_parser: &crate::config_parser::BuildConfigParser,
    ) {
        // Direct insertion avoids per-language cache invalidation; cache is
        // refreshed once after all data is in place.
        // Dynamically load dependencies for all languages with dependencies
        for language in config_parser.languages_with_dependencies() {
            let deps = config_parser.dependencies_for_language(language);
            let dependencies: Vec<UntypedDependency> = deps.into_iter().collect();

            if !dependencies.is_empty() {
                let dep_count = dependencies.len();
                let package_names: HashSet<String> =
                    dependencies.iter().map(|d| d.name.clone()).collect();
                self.package_data
                    .external_packages
                    .get_or_insert_with(HashMap::new)
                    .insert(language, package_names);
                self.package_data
                    .external_dependencies
                    .get_or_insert_with(HashMap::new)
                    .insert(language, dependencies);
                tracing::debug!(
                    "Loaded {} dependencies with full metadata for language {:?}",
                    dep_count,
                    language
                );
            }
        }
        let discovered = config_parser.discovered_config_files().to_vec();
        if !discovered.is_empty() {
            self.package_data.discovered_config_files = Some(discovered);
        } else {
            self.package_data.discovered_config_files = None;
        }
        let file_deps = config_parser.config_file_dependencies().clone();
        if !file_deps.is_empty() {
            self.package_data.config_file_deps = Some(file_deps);
        } else {
            self.package_data.config_file_deps = None;
        }
        let hashes = config_parser.config_file_hashes().clone();
        if !hashes.is_empty() {
            self.package_data.config_content_hashes = Some(hashes);
        } else {
            self.package_data.config_content_hashes = None;
        }
        let fp = self.compute_fingerprint();
        self.cache_fingerprint(fp);
    }

    /// Clear all external packages and dependencies
    ///
    /// This method should be called before reloading configurations to ensure
    /// old package information is removed.
    pub fn clear(&mut self) {
        self.package_data = ExternalPackageData::default();
        self.invalidate_cache();
        tracing::debug!("Cleared all external packages and dependencies");
    }

    /// Get external packages for rollback
    pub fn get_external_packages(&self) -> Option<HashMap<Language, HashSet<String>>> {
        self.package_data.external_packages.as_ref().cloned()
    }

    /// Set all external packages at once (for rollback)
    pub fn set_all_external_packages(&mut self, packages: HashMap<Language, HashSet<String>>) {
        self.package_data.external_packages = Some(packages);
        self.invalidate_cache();
    }

    /// Set whether to filter out standard library calls
    pub fn set_filter_stdlib_calls(&mut self, filter: bool) {
        if self.policy.filter_stdlib_calls != filter {
            self.policy.filter_stdlib_calls = filter;
            self.invalidate_cache();
        }
    }

    /// Set the per-file relation cap used while constructing candidates.
    pub fn set_max_relations_per_file(&mut self, limit: usize) {
        if self.policy.max_relations_per_file != limit {
            self.policy.max_relations_per_file = limit;
            self.invalidate_cache();
        }
    }

    /// Set import/export and dependency graph construction policies.
    pub fn set_graph_options(&mut self, analyze_imports: bool, track_cross_file_deps: bool) {
        if self.policy.analyze_imports != analyze_imports
            || self.policy.track_cross_file_deps != track_cross_file_deps
        {
            self.policy.analyze_imports = analyze_imports;
            self.policy.track_cross_file_deps = track_cross_file_deps;
            self.invalidate_cache();
        }
    }

    /// Set whether `SymbolExtract` plugins provide import/export extraction
    /// for custom languages.
    pub fn set_symbol_extract_enabled(&mut self, enabled: bool) {
        if self.policy.symbol_extract_enabled != enabled {
            self.policy.symbol_extract_enabled = enabled;
            self.invalidate_cache();
        }
    }

    /// Set whether to auto-load external dependency symbols from package
    /// manager caches during the build phase.
    pub fn set_load_external_symbols(&mut self, enabled: bool) {
        if self.policy.load_external_symbols != enabled {
            self.policy.load_external_symbols = enabled;
            self.invalidate_cache();
        }
    }

    /// Set whether `import_table` is required; when true, missing
    /// `import_table` causes fallback to be skipped.
    pub fn set_require_import_table(&mut self, required: bool) {
        if self.policy.require_import_table != required {
            self.policy.require_import_table = required;
            self.invalidate_cache();
        }
    }

    /// Set whether to automatically detect external symbols.
    pub fn set_auto_detect_external_symbols(&mut self, enabled: bool) {
        if self.policy.auto_detect_external_symbols != enabled {
            self.policy.auto_detect_external_symbols = enabled;
            self.invalidate_cache();
        }
    }

    /// Set external symbols cache directory.
    pub fn set_external_symbols_cache_dir(&mut self, dir: Option<std::path::PathBuf>) {
        if self.policy.external_symbols_cache_dir != dir {
            self.policy.external_symbols_cache_dir = dir;
            self.invalidate_cache();
        }
    }

    /// Set symbol resolution configuration.
    pub fn set_symbol_resolution_config(&mut self, config: cce_config::SymbolResolutionConfig) {
        if self.symbol_resolution.max_reexport_chain_depth != config.max_reexport_chain_depth
            || self.symbol_resolution.max_scope_chain_depth != config.max_scope_chain_depth
            || self.symbol_resolution.resolution_cache_size != config.resolution_cache_size
            || self.symbol_resolution.max_wildcard_expansion_size
                != config.max_wildcard_expansion_size
            || self.symbol_resolution.disable_wildcard_expansion
                != config.disable_wildcard_expansion
        {
            self.symbol_resolution = config;
            self.invalidate_cache();
        }
    }

    /// Fingerprint only inputs that can change deterministic relation resolution.
    ///
    /// Covers filter flags plus external packages, dependencies, discovered file
    /// list, per-file dependency mapping, and per-file content hashes. The
    /// latter ensures version/comment-only changes trigger fingerprint drift
    /// even when the package set is unchanged. The value is cached after the
    /// first computation; mutations invalidate the cache.
    pub fn fingerprint(&self) -> String {
        if let Some(cached) = self.cached_fingerprint.get() {
            return cached;
        }
        let computed = self.compute_fingerprint();
        self.cache_fingerprint(computed.clone());
        computed
    }

    /// Compute the fingerprint without consulting the cache.
    fn compute_fingerprint(&self) -> String {
        let mut entries = vec![
            format!("filter_stdlib={}", self.policy.filter_stdlib_calls),
            format!(
                "max_relations_per_file={}",
                self.policy.max_relations_per_file
            ),
            format!("analyze_imports={}", self.policy.analyze_imports),
            format!(
                "track_cross_file_deps={}",
                self.policy.track_cross_file_deps
            ),
            format!("symbol_extract={}", self.policy.symbol_extract_enabled),
            format!(
                "load_external_symbols={}",
                self.policy.load_external_symbols
            ),
            format!("require_import_table={}", self.policy.require_import_table),
            format!(
                "auto_detect_external_symbols={}",
                self.policy.auto_detect_external_symbols
            ),
            format!(
                "external_symbols_cache_dir={:?}",
                self.policy.external_symbols_cache_dir
            ),
            format!(
                "symbol_resolution.reexport_depth={}",
                self.symbol_resolution.max_reexport_chain_depth
            ),
            format!(
                "symbol_resolution.scope_depth={}",
                self.symbol_resolution.max_scope_chain_depth
            ),
            format!(
                "symbol_resolution.cache_size={}",
                self.symbol_resolution.resolution_cache_size
            ),
            format!(
                "symbol_resolution.wildcard_expand={}",
                self.symbol_resolution.max_wildcard_expansion_size
            ),
            format!(
                "symbol_resolution.disable_wildcard={}",
                self.symbol_resolution.disable_wildcard_expansion
            ),
        ];
        if let Some(packages) = &self.package_data.external_packages {
            for (language, names) in packages {
                for name in names {
                    entries.push(format!("package:{language}:{name}"));
                }
            }
        }
        if let Some(dependencies) = &self.package_data.external_dependencies {
            for (language, values) in dependencies {
                for dependency in values {
                    entries.push(format!(
                        "dependency:{language}:{}:{}",
                        dependency.name, dependency.package_type
                    ));
                }
            }
        }
        if let Some(discovered) = &self.package_data.discovered_config_files {
            let mut sorted = discovered.clone();
            sorted.sort();
            for file in sorted {
                entries.push(format!("discovered:{file}"));
            }
        }
        if let Some(file_deps) = &self.package_data.config_file_deps {
            let mut files: Vec<&String> = file_deps.keys().collect();
            files.sort();
            for file in files {
                if let Some(deps) = file_deps.get(file) {
                    let mut dep_entries: Vec<String> = deps
                        .iter()
                        .map(|d| format!("{}:{}", d.name, d.package_type))
                        .collect();
                    dep_entries.sort();
                    for dep in dep_entries {
                        entries.push(format!("config_file_dep:{file}:{dep}"));
                    }
                    if deps.is_empty() {
                        entries.push(format!("config_file_dep:{file}:empty"));
                    }
                }
            }
        }
        if let Some(hashes) = &self.package_data.config_content_hashes {
            let mut files: Vec<&String> = hashes.keys().collect();
            files.sort();
            for file in files {
                if let Some(hash) = hashes.get(file) {
                    entries.push(format!("config_content:{file}:{hash}"));
                }
            }
        }
        entries.sort();
        let mut hasher = Sha256::new();
        for entry in entries {
            hasher.update(entry.as_bytes());
            hasher.update([0]);
        }
        format!("{:x}", hasher.finalize())
    }
}

/// Load configuration with automatic project scanning.
///
/// `manifest_scan_depth` controls how many directory levels below the root
/// are searched for additional build manifests (0 = root only).
pub fn load_auto_config(
    project_root: impl AsRef<std::path::Path>,
    enable_stdlib_filtering: bool,
    manifest_scan_depth: usize,
) -> Result<BuilderConfig, crate::config_parser::ConfigParseError> {
    let mut config = BuilderConfig::new();
    config.set_filter_stdlib_calls(enable_stdlib_filtering);

    let mut config_parser = crate::config_parser::BuildConfigParser::new();
    config_parser.scan_project(project_root.as_ref(), manifest_scan_depth)?;

    config.auto_load_dependencies(&config_parser);

    tracing::info!(
        "Successfully loaded build configurations from {:?}",
        project_root.as_ref()
    );

    Ok(config)
}
