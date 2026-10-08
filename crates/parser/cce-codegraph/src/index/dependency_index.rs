//! Dependency index for efficient lookup
//!
//! Provides a prefix-based index structure for fast dependency matching.
//! This is an internal optimization structure used by RelationResolver.

use crate::config_parser::{DevDependency, ExternalDependency, LocalDependency, UntypedDependency};
use cce_types::language::Language;
use std::collections::HashMap;

/// Dependency index for efficient prefix-based lookup
///
/// Uses a simple prefix matching strategy optimized for package name lookups.
/// For most package ecosystems, package names are matched by prefix (e.g., "numpy.core"
/// matches dependency "numpy").
#[derive(Debug, Clone, Default)]
pub struct DependencyIndex {
    /// Dependencies indexed by language (untyped for flexibility)
    by_language: HashMap<Language, Vec<UntypedDependency>>,
    /// Exact match index: language -> (package_name -> index)
    exact_index: HashMap<Language, HashMap<String, usize>>,
}

impl DependencyIndex {
    /// Create a new empty index
    pub fn new() -> Self {
        Self::default()
    }

    /// Build index from untyped dependencies
    pub fn build(dependencies: &HashMap<Language, Vec<UntypedDependency>>) -> Self {
        let mut index = Self::new();

        for (language, deps) in dependencies {
            let mut lang_exact_index = HashMap::new();

            // Store dependencies
            index.by_language.insert(*language, deps.clone());

            // Build exact match index
            for (idx, dep) in deps.iter().enumerate() {
                lang_exact_index.insert(dep.name.clone(), idx);
            }

            index.exact_index.insert(*language, lang_exact_index);
        }

        index
    }

    /// Build index from typed dependencies
    pub fn build_from_typed(
        external: &HashMap<Language, Vec<ExternalDependency>>,
        dev: &HashMap<Language, Vec<DevDependency>>,
        local: &HashMap<Language, Vec<LocalDependency>>,
    ) -> Self {
        let mut dependencies: HashMap<Language, Vec<UntypedDependency>> = HashMap::new();

        // Collect all dependencies
        for (language, deps) in external {
            for dep in deps {
                dependencies
                    .entry(*language)
                    .or_default()
                    .push(UntypedDependency::from(dep));
            }
        }

        for (language, deps) in dev {
            for dep in deps {
                dependencies
                    .entry(*language)
                    .or_default()
                    .push(UntypedDependency::from(dep));
            }
        }

        for (language, deps) in local {
            for dep in deps {
                dependencies
                    .entry(*language)
                    .or_default()
                    .push(UntypedDependency::from(dep));
            }
        }

        Self::build(&dependencies)
    }

    /// Find dependency by name
    ///
    /// Returns the dependency if a match is found. Matching strategy:
    /// 1. Try exact match first (O(1))
    /// 2. Try first-segment match: `numpy.core` -> dependency `numpy`
    ///    (`fmt::Something` -> `fmt`). Never a bare prefix match, so
    ///    dependency `serde` cannot be hit by callee `serde_derive::X`.
    pub fn find_dependency(&self, language: Language, name: &str) -> Option<&UntypedDependency> {
        let lang_deps = self.by_language.get(&language)?;
        let lang_exact = self.exact_index.get(&language)?;

        // Try exact match first
        if let Some(&idx) = lang_exact.get(name) {
            return lang_deps.get(idx);
        }

        // Try first-segment match
        let first_segment = name.split([':', '.']).next().unwrap_or(name);
        lang_deps.iter().find(|dep| dep.name == first_segment)
    }

    /// Get all dependencies for a language
    pub fn get_dependencies(&self, language: Language) -> Option<&[UntypedDependency]> {
        self.by_language.get(&language).map(|deps| deps.as_slice())
    }

    /// Get dependency count for a language
    pub fn dependency_count(&self, language: Language) -> usize {
        self.by_language
            .get(&language)
            .map(|deps| deps.len())
            .unwrap_or(0)
    }

    /// Get total dependency count across all languages
    pub fn total_dependency_count(&self) -> usize {
        self.by_language.values().map(|deps| deps.len()).sum()
    }

    /// Get statistics about the index
    pub fn stats(&self) -> IndexStats {
        let mut stats = IndexStats::default();

        for (language, deps) in &self.by_language {
            let count = deps.len();
            stats.languages.push((*language, count));
            stats.total += count;

            // Count by package type
            for dep in deps {
                match dep.package_type.as_str() {
                    "external" => stats.external_count += 1,
                    "dev" => stats.dev_count += 1,
                    "local" => stats.local_count += 1,
                    _ => {}
                }
            }
        }

        stats
    }
}

/// Statistics about the dependency index
#[derive(Debug, Clone, Default)]
pub struct IndexStats {
    /// Total number of dependencies
    pub total: usize,
    /// Dependencies per language
    pub languages: Vec<(Language, usize)>,
    /// External dependencies
    pub external_count: usize,
    /// Development dependencies
    pub dev_count: usize,
    /// Local path dependencies
    pub local_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_index() {
        let index = DependencyIndex::new();
        assert_eq!(index.total_dependency_count(), 0);
    }

    #[test]
    fn test_exact_match() {
        let mut deps = HashMap::new();
        deps.insert(
            Language::Rust,
            vec![UntypedDependency::new("serde", "external")],
        );

        let index = DependencyIndex::build(&deps);

        let found = index.find_dependency(Language::Rust, "serde");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "serde");
    }

    #[test]
    fn test_prefix_match() {
        let mut deps = HashMap::new();
        deps.insert(
            Language::Python,
            vec![UntypedDependency::new("numpy", "external")],
        );

        let index = DependencyIndex::build(&deps);

        let found = index.find_dependency(Language::Python, "numpy.core");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "numpy");
    }

    #[test]
    fn test_no_match() {
        let mut deps = HashMap::new();
        deps.insert(
            Language::JavaScript,
            vec![UntypedDependency::new("react", "external")],
        );

        let index = DependencyIndex::build(&deps);

        let found = index.find_dependency(Language::JavaScript, "vue");
        assert!(found.is_none());
    }

    #[test]
    fn test_stats() {
        let mut deps = HashMap::new();
        deps.insert(
            Language::Rust,
            vec![
                UntypedDependency::new("serde", "external"),
                UntypedDependency::new("tokio", "dev"),
            ],
        );

        let index = DependencyIndex::build(&deps);
        let stats = index.stats();

        assert_eq!(stats.total, 2);
        assert_eq!(stats.external_count, 1);
        assert_eq!(stats.dev_count, 1);
    }

    #[test]
    fn test_build_from_typed() {
        let mut external = HashMap::new();
        external.insert(Language::Rust, vec![ExternalDependency::new("serde")]);

        let mut dev = HashMap::new();
        dev.insert(Language::Rust, vec![DevDependency::new("tokio")]);

        let mut local = HashMap::new();
        local.insert(Language::Rust, vec![LocalDependency::new("./my-crate")]);

        let index = DependencyIndex::build_from_typed(&external, &dev, &local);
        let stats = index.stats();

        assert_eq!(stats.total, 3);
        assert_eq!(stats.external_count, 1);
        assert_eq!(stats.dev_count, 1);
        assert_eq!(stats.local_count, 1);
    }
}
