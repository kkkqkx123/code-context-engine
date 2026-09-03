//! Relation index update processor
//!
//! This module handles updates to the relation index during hot updates.
//! It also persists relation data to SQLite for fast cold start recovery.
//!
//! # Phase 3: Dependency Propagation
//!
//! This processor implements dependency propagation for hot updates:
//! 1. When a file changes, find all files that depend on it
//! 2. Collect all affected files (changed + dependents)
//! 3. Process files in topological order (dependencies first)

use std::collections::{HashMap, HashSet};

use cce_relation::BuildConfigParser;
use cce_types::Language;

/// Lightweight data structure for external packages
#[derive(Debug, Clone)]
pub struct ExternalPackageData {
    packages: HashMap<Language, HashSet<String>>,
}

impl Default for ExternalPackageData {
    fn default() -> Self {
        Self::new()
    }
}

impl ExternalPackageData {
    pub fn new() -> Self {
        Self {
            packages: HashMap::new(),
        }
    }

    pub fn add_package(&mut self, lang: Language, package: String) {
        self.packages.entry(lang).or_default().insert(package);
    }

    pub fn get_packages(&self, lang: &Language) -> Option<&HashSet<String>> {
        self.packages.get(lang)
    }
}

/// Extension trait for extracting external packages from BuildConfigParser
pub(crate) trait BuildConfigParserExt {
    fn extract_external_packages(&self) -> ExternalPackageData;
}

impl BuildConfigParserExt for BuildConfigParser {
    /// Extract external packages into lightweight structure
    fn extract_external_packages(&self) -> ExternalPackageData {
        let mut result = ExternalPackageData::new();

        // Dynamically load packages for all languages with dependencies
        for language in self.languages_with_dependencies() {
            let packages = self.packages_for_language(language);
            if !packages.is_empty() {
                result.packages.insert(language, packages);
            }
        }

        result
    }
}
