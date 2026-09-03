//! On-demand external symbol loader with caching.
//!
//! Provides [`ExternalSymbolLoader`] which loads external package symbols
//! on-demand during relation resolution. It checks the project symbol table
//! first, then uses language-specific providers to discover and extract
//! symbols from installed packages.

use cce_types::language::Language;
use std::collections::{HashMap, HashSet};
use std::path::Path;

use super::ExternalLibraryRegistry;
use super::provider::ProviderRegistry;
use crate::symbol_table::ProjectSymbolTable;

/// Configuration for external symbol loading.
#[derive(Debug, Clone)]
pub struct ExternalLoadConfig {
    /// Whether on-demand loading is enabled.
    pub enabled: bool,
    /// Maximum number of packages to load per language (0 = unlimited).
    pub max_packages_per_language: usize,
    /// Whether to emit tracing messages for loaded packages.
    pub verbose: bool,
}

impl Default for ExternalLoadConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_packages_per_language: 256,
            verbose: false,
        }
    }
}

/// Statistics about external symbol loading.
#[derive(Debug, Clone, Default)]
pub struct ExternalLoadStats {
    /// Number of packages discovered.
    pub discovered: usize,
    /// Number of packages successfully loaded.
    pub loaded: usize,
    /// Number of packages that were already in the symbol table.
    pub already_loaded: usize,
    /// Number of packages that failed to load.
    pub failed: usize,
    /// Per-language package counts.
    pub by_language: HashMap<Language, usize>,
}

/// On-demand loader for external package symbols.
///
/// Integrates with the [`ProjectSymbolTable`] and [`ExternalLibraryRegistry`]
/// to discover and load external symbols lazily during relation resolution.
/// Caches loaded packages to avoid redundant IO.
pub struct ExternalSymbolLoader {
    config: ExternalLoadConfig,
    registry: ExternalLibraryRegistry,
    /// Packages already loaded in this session (avoids re-loading).
    loaded_packages: HashSet<String>,
    /// Per-language package counts for enforcing limits.
    package_counts: HashMap<Language, usize>,
    /// Aggregated statistics.
    stats: ExternalLoadStats,
}

impl ExternalSymbolLoader {
    /// Create a new loader with the given configuration.
    pub fn new(config: ExternalLoadConfig) -> Self {
        Self {
            config,
            registry: ExternalLibraryRegistry::new(),
            loaded_packages: HashSet::new(),
            package_counts: HashMap::new(),
            stats: ExternalLoadStats::default(),
        }
    }

    /// Create a loader with default configuration.
    pub fn default_loader() -> Self {
        Self::new(ExternalLoadConfig::default())
    }

    /// Get a reference to the internal registry.
    pub fn registry(&self) -> &ExternalLibraryRegistry {
        &self.registry
    }

    /// Get mutable access to the internal registry.
    pub fn registry_mut(&mut self) -> &mut ExternalLibraryRegistry {
        &mut self.registry
    }

    /// Get loading statistics.
    pub fn stats(&self) -> &ExternalLoadStats {
        &self.stats
    }

    /// Check if a package has already been loaded in this session.
    pub fn is_loaded(&self, package_name: &str) -> bool {
        self.loaded_packages.contains(package_name)
    }

    /// Attempt to load external symbols for an unresolved call target.
    ///
    /// This is the main entry point for on-demand loading during relation
    /// resolution. Given a callee name that was classified as external,
    /// it:
    /// 1. Extracts the package name from the callee (first path segment)
    /// 2. Checks if the package is already loaded in the project symbol table
    /// 3. Uses the language-specific provider to discover the package path
    /// 4. Extracts symbols and registers them in the project symbol table
    ///
    /// Returns `true` if symbols were loaded (or were already present).
    pub fn try_load_for_call(
        &mut self,
        callee_name: &str,
        language: Language,
        project_root: &Path,
        symbol_table: &ProjectSymbolTable,
    ) -> bool {
        if !self.config.enabled {
            return false;
        }

        let package_name = extract_package_name(callee_name, language);
        if package_name.is_empty() {
            return false;
        }

        // Already loaded in symbol table?
        if symbol_table.get_external_dep(&package_name).is_some() {
            if !self.loaded_packages.contains(&package_name) {
                self.loaded_packages.insert(package_name);
                self.stats.already_loaded += 1;
            }
            return true;
        }

        // Already loaded in this session but not in symbol table?
        // This shouldn't happen, but handle it gracefully.
        if self.loaded_packages.contains(&package_name) {
            return true;
        }

        // Check per-language limits
        let count = self.package_counts.get(&language).copied().unwrap_or(0);
        if self.config.max_packages_per_language > 0
            && count >= self.config.max_packages_per_language
        {
            return false;
        }

        // Try to discover and load the package
        self.discover_and_load(&package_name, language, project_root, symbol_table)
    }

    /// Discover and load a specific package by name.
    fn discover_and_load(
        &mut self,
        package_name: &str,
        language: Language,
        project_root: &Path,
        symbol_table: &ProjectSymbolTable,
    ) -> bool {
        let provider = match ProviderRegistry::provider_for(language) {
            Some(p) => p,
            None => return false,
        };

        let discovery = match provider.discover_package(package_name, project_root) {
            Some(d) => d,
            None => {
                self.stats.failed += 1;
                if self.config.verbose {
                    tracing::debug!(
                        "External symbol loader: package '{}' not found for {:?}",
                        package_name,
                        language
                    );
                }
                return false;
            }
        };

        let module_info = match provider.extract_symbols(&discovery, &mut self.registry) {
            Some(info) => info,
            None => {
                self.stats.failed += 1;
                if self.config.verbose {
                    tracing::debug!(
                        "External symbol loader: failed to extract symbols from '{}' at {}",
                        package_name,
                        discovery.path.display()
                    );
                }
                return false;
            }
        };

        // Convert to ExternalSymbolTable and register
        let table = module_info.into_external_table(discovery.version.clone());
        let export_count = table.all_exports().len();
        symbol_table.add_external_dep(table);

        // Update tracking
        self.loaded_packages.insert(package_name.to_string());
        *self.package_counts.entry(language).or_insert(0) += 1;
        self.stats.loaded += 1;
        *self.stats.by_language.entry(language).or_insert(0) += 1;

        if self.config.verbose {
            tracing::info!(
                "External symbol loader: loaded {} exports from '{}' ({}) for {:?}",
                export_count,
                package_name,
                discovery.version.as_deref().unwrap_or("unknown version"),
                language
            );
        }

        true
    }

    /// Bulk-load symbols for all known external dependencies.
    ///
    /// Called during the build phase to pre-load symbols for all packages
    /// declared in build manifests. This avoids on-demand loading overhead
    /// during relation resolution.
    pub fn load_all_known(
        &mut self,
        external_dependencies: &HashMap<Language, Vec<crate::UntypedDependency>>,
        project_root: &Path,
        symbol_table: &ProjectSymbolTable,
    ) -> ExternalLoadStats {
        let mut stats = ExternalLoadStats::default();

        for (language, deps) in external_dependencies {
            let provider = match ProviderRegistry::provider_for(*language) {
                Some(p) => p,
                None => continue,
            };

            for dep in deps {
                if !dep.is_external() {
                    continue;
                }

                // Skip if already loaded
                if symbol_table.get_external_dep(&dep.name).is_some() {
                    stats.already_loaded += 1;
                    continue;
                }
                if self.loaded_packages.contains(&dep.name) {
                    stats.already_loaded += 1;
                    continue;
                }

                // Check limits
                let count = self.package_counts.get(language).copied().unwrap_or(0);
                if self.config.max_packages_per_language > 0
                    && count >= self.config.max_packages_per_language
                {
                    break;
                }

                if let Some(discovery) = provider.discover_package(&dep.name, project_root) {
                    if let Some(module_info) =
                        provider.extract_symbols(&discovery, &mut self.registry)
                    {
                        let table = module_info.into_external_table(discovery.version.clone());
                        let export_count = table.all_exports().len();
                        symbol_table.add_external_dep(table);

                        self.loaded_packages.insert(dep.name.clone());
                        *self.package_counts.entry(*language).or_insert(0) += 1;
                        stats.loaded += 1;
                        *stats.by_language.entry(*language).or_insert(0) += 1;

                        if self.config.verbose {
                            tracing::info!(
                                "External symbol loader: pre-loaded {} exports from '{}' for {:?}",
                                export_count,
                                dep.name,
                                language
                            );
                        }
                    } else {
                        stats.failed += 1;
                    }
                } else {
                    stats.failed += 1;
                }
            }
        }

        stats.discovered = stats.loaded + stats.failed;
        self.stats = stats.clone();
        stats
    }
}

/// Extract the top-level package name from a callee name.
///
/// For Rust: `serde::Serialize` -> `serde`
/// For Python: `requests.get` -> `requests`
/// For JS: `lodash.merge` -> `lodash`
/// For Go: `fmt.Println` -> `fmt`
fn extract_package_name(callee_name: &str, language: Language) -> String {
    let separator = match language {
        Language::Rust => "::",
        Language::Go => ".",
        _ => ".",
    };

    // For Rust, handle absolute paths like `::std::collections::HashMap`
    // by splitting on the separator and taking the first non-empty segment.
    let name = if language == Language::Rust && callee_name.starts_with("::") {
        callee_name
            .split(separator)
            .find(|s| !s.is_empty())
            .unwrap_or("")
    } else {
        callee_name.split(separator).next().unwrap_or(callee_name)
    };

    name.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_package_name_rust() {
        assert_eq!(
            extract_package_name("serde::Serialize", Language::Rust),
            "serde"
        );
        assert_eq!(
            extract_package_name("tokio::spawn", Language::Rust),
            "tokio"
        );
        assert_eq!(
            extract_package_name("::std::collections::HashMap", Language::Rust),
            "std"
        );
    }

    #[test]
    fn test_extract_package_name_python() {
        assert_eq!(
            extract_package_name("requests.get", Language::Python),
            "requests"
        );
        assert_eq!(extract_package_name("os.path.join", Language::Python), "os");
    }

    #[test]
    fn test_extract_package_name_javascript() {
        assert_eq!(
            extract_package_name("lodash.merge", Language::JavaScript),
            "lodash"
        );
        assert_eq!(
            extract_package_name("express.Router", Language::JavaScript),
            "express"
        );
    }

    #[test]
    fn test_extract_package_name_go() {
        assert_eq!(extract_package_name("fmt.Println", Language::Go), "fmt");
        assert_eq!(
            extract_package_name("net/http.Get", Language::Go),
            "net/http"
        );
    }

    #[test]
    fn test_loader_default_config() {
        let config = ExternalLoadConfig::default();
        assert!(config.enabled);
        assert_eq!(config.max_packages_per_language, 256);
        assert!(!config.verbose);
    }

    #[test]
    fn test_loader_is_loaded() {
        let mut loader = ExternalSymbolLoader::default_loader();
        assert!(!loader.is_loaded("serde"));
        loader.loaded_packages.insert("serde".to_string());
        assert!(loader.is_loaded("serde"));
    }
}
