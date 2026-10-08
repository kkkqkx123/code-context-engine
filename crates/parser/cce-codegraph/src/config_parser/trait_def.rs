//! Language parser trait definition.
//!
//! Defines the `LanguageParser` trait that all build-config parsers implement,
//! and the `ParseOutcome` type.

use std::collections::HashSet;
use std::path::Path;

use cce_types::Language;

use super::error::ConfigParseError;
use super::types::UntypedDependency;

/// Result of a successful parse.
#[derive(Debug, Clone)]
pub struct ParseOutcome {
    /// Dependencies extracted from the config file.
    pub dependencies: HashSet<UntypedDependency>,
    /// Relative path of the config file (for per-file tracking).
    pub config_file: String,
}

/// Language parser trait.
///
/// Each build-system parser (Cargo, NPM, Go Modules, etc.) implements this
/// trait. The parser is called per-directory during project scanning.
///
/// # Contract
///
/// - If the config file is not found in `dir`, return `Ok(None)`.
/// - If the config file is found and parsed, return `Ok(Some(outcome))`.
/// - If IO or parse error occurs, return `Err(ConfigParseError)`.
/// - The parser MUST NOT call `BuildConfigParser::insert_packages_for_file`
///   directly — the caller (registry) handles that.
pub trait LanguageParser: Send + Sync {
    /// Try to parse build configuration from the given directory.
    ///
    /// # Parameters
    /// - `project_root`: Root directory of the project (for relativizing paths).
    /// - `dir`: Current directory being scanned.
    fn try_parse(
        &self,
        project_root: &Path,
        dir: &Path,
    ) -> Result<Option<ParseOutcome>, ConfigParseError>;

    /// Languages this parser produces dependencies for.
    ///
    /// A single parser may map to multiple languages (e.g., Cargo → Rust,
    /// package.json → JavaScript + TypeScript).
    fn supported_languages(&self) -> Vec<Language>;

    /// Config file names this parser looks for (exact names, not patterns).
    ///
    /// Used by the registry for fast filename-based dispatch.
    fn supported_config_files(&self) -> &[&str];

    /// Human-readable name for logging.
    fn name(&self) -> &str;

    /// Check if this parser supports the given filename.
    ///
    /// Default implementation checks against `supported_config_files()`.
    /// Override for parsers that need glob matching (e.g., *.csproj).
    fn supports_file(&self, filename: &str) -> bool {
        self.supported_config_files().contains(&filename)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_parser::parsers::cargo::CargoParser;

    #[test]
    fn test_cargo_parser_trait_implementation() {
        let parser = CargoParser;
        assert_eq!(parser.name(), "Cargo");
        assert_eq!(parser.supported_languages(), vec![Language::Rust]);
        assert_eq!(parser.supported_config_files(), &["Cargo.toml"]);
    }

    #[test]
    fn test_cargo_parser_missing_file() {
        let parser = CargoParser;
        let temp_dir = tempfile::tempdir().unwrap();
        let result = parser.try_parse(temp_dir.path(), temp_dir.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_supports_file_default() {
        let parser = CargoParser;
        assert!(parser.supports_file("Cargo.toml"));
        assert!(!parser.supports_file("package.json"));
    }
}
