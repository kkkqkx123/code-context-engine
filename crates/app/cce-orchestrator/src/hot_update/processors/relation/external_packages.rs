//! External package discovery for the relation processor.
//!
//! Collects the package names declared by each language's manifest so
//! unresolved imports can be attributed to external dependencies instead of
//! being reported as missing local symbols.

use std::collections::{HashMap, HashSet};

use cce_codegraph::BuildConfigParser;
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
