//! Parser registry — central dispatch for language parsers.
//!
//! Replaces the hardcoded `try_parse_*` calls in `scan_project_at`
//! with a dynamic dispatch table indexed by config filename.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use super::detector::BuildConfigParser;
use super::error::ConfigParseError;
use super::strategy::ErrorStrategy;
use super::trait_def::LanguageParser;

/// Registry of language parsers, indexed by config filename.
pub struct ParserRegistry {
    /// config_file_name → parser (first match wins per filename)
    parsers: HashMap<String, Arc<dyn LanguageParser>>,
    /// All registered parsers for fallback matching (supports_file)
    all_parsers: Vec<Arc<dyn LanguageParser>>,
}

impl ParserRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            parsers: HashMap::new(),
            all_parsers: Vec::new(),
        }
    }

    /// Register a parser for its supported config files.
    pub fn register(&mut self, parser: Arc<dyn LanguageParser>) {
        for &config_file in parser.supported_config_files() {
            self.parsers.insert(config_file.to_string(), parser.clone());
        }
        // Always add to all_parsers for supports_file fallback
        // Use pointer equality to avoid duplicates
        let already = self.all_parsers.iter().any(|p| Arc::ptr_eq(p, &parser));
        if !already {
            self.all_parsers.push(parser);
        }
    }

    /// Register all built-in parsers.
    pub fn register_builtin() -> Self {
        let mut registry = Self::new();

        registry.register(Arc::new(super::parsers::cargo::CargoParser));
        registry.register(Arc::new(super::parsers::go::GoParser));
        registry.register(Arc::new(super::parsers::javascript::NpmParser));
        registry.register(Arc::new(super::parsers::python::PythonParser));
        registry.register(Arc::new(super::parsers::java::JavaParser));
        registry.register(Arc::new(super::parsers::cpp::CMakeParser));
        registry.register(Arc::new(super::parsers::php::ComposerParser));
        registry.register(Arc::new(super::parsers::dotnet::DotNetParser));
        registry.register(Arc::new(super::parsers::ruby::BundlerParser));
        registry.register(Arc::new(super::parsers::makefile::MakeParser));
        registry.register(Arc::new(super::parsers::dockerfile::DockerParser));

        registry
    }

    /// Get the parser for a config filename, if registered.
    pub fn get_parser(&self, config_filename: &str) -> Option<&Arc<dyn LanguageParser>> {
        if let Some(p) = self.parsers.get(config_filename) {
            return Some(p);
        }
        self.all_parsers
            .iter()
            .find(|parser| parser.supports_file(config_filename))
    }

    /// Scan a single directory using registered parsers with default error strategy.
    ///
    /// This is a convenience method that uses `ErrorStrategy::Default`.
    /// For custom error handling, use [`scan_directory_with_strategy`].
    pub fn scan_directory(
        &self,
        project_root: &Path,
        dir: &Path,
        entries: &[std::fs::DirEntry],
        parser: &mut BuildConfigParser,
    ) -> Result<(), ConfigParseError> {
        self.scan_directory_with_strategy(
            project_root,
            dir,
            entries,
            parser,
            ErrorStrategy::Default,
        )
    }

    /// Scan a single directory using registered parsers with custom error strategy.
    ///
    /// For each file in `entries`, checks if its name matches a registered
    /// parser, and if so, invokes the parser. Results are inserted into
    /// the `BuildConfigParser` via `insert_packages_for_file`.
    ///
    /// The `strategy` parameter controls how errors are handled:
    /// - `FailOnIoError`: propagate all errors (default)
    /// - `SkipOnMissingFile`: skip when config file not found
    /// - `SkipOnParseError`: skip on parse errors
    /// - `SkipAll`: skip all errors
    pub fn scan_directory_with_strategy(
        &self,
        project_root: &Path,
        dir: &Path,
        entries: &[std::fs::DirEntry],
        parser: &mut BuildConfigParser,
        strategy: ErrorStrategy,
    ) -> Result<(), ConfigParseError> {
        // Deduplicate parsers per directory to avoid calling same parser multiple times
        // when multiple files match same parser (e.g., *.csproj, multiple makefile names)
        let mut invoked: HashSet<*const ()> = HashSet::new();
        for entry in entries {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = entry.file_name().to_str().map(|s| s.to_string()) else {
                continue;
            };

            // Find parser for this file
            let lang_parser = match self.get_parser(&name) {
                Some(p) => p,
                None => continue,
            };

            // Deduplicate: only invoke each parser once per directory
            let ptr = Arc::as_ptr(lang_parser) as *const ();
            if !invoked.insert(ptr) {
                continue;
            }

            tracing::debug!(
                parser = lang_parser.name(),
                dir = %dir.display(),
                file = %name,
                "dispatching language parser"
            );
            match lang_parser.try_parse(project_root, dir) {
                Ok(Some(outcome)) => {
                    let rel = cce_types::path::relativize(project_root, &path);
                    let outcome_file = if outcome.config_file.is_empty() {
                        rel.clone()
                    } else {
                        outcome.config_file.clone()
                    };
                    for lang in lang_parser.supported_languages() {
                        parser.insert_packages_for_file(
                            &outcome_file,
                            lang,
                            outcome.dependencies.clone(),
                        );
                    }
                }
                Ok(None) => { /* config file not found, skip */ }
                Err(e) => {
                    strategy.handle_error(e)?;
                }
            }
        }
        Ok(())
    }
}

impl Default for ParserRegistry {
    fn default() -> Self {
        Self::register_builtin()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::Language;

    #[test]
    fn test_registry_register_builtin() {
        let registry = ParserRegistry::register_builtin();
        assert!(registry.get_parser("Cargo.toml").is_some());
        assert!(registry.get_parser("go.mod").is_some());
        assert!(registry.get_parser("package.json").is_some());
        assert!(registry.get_parser("requirements.txt").is_some());
        assert!(registry.get_parser("pom.xml").is_some());
        assert!(registry.get_parser("CMakeLists.txt").is_some());
        assert!(registry.get_parser("composer.json").is_some());
        assert!(registry.get_parser("Gemfile").is_some());
        assert!(registry.get_parser("Dockerfile").is_some());
        assert!(registry.get_parser("Makefile").is_some());
        assert!(registry.get_parser("unknown.xyz").is_none());
    }

    #[test]
    fn test_registry_scan_cargo_project() {
        let temp_dir = tempfile::tempdir().unwrap();
        std::fs::write(
            temp_dir.path().join("Cargo.toml"),
            r#"
[package]
name = "test"
version = "0.1.0"

[dependencies]
serde = "1.0"
tokio = { version = "1.0", features = ["full"] }
"#,
        )
        .unwrap();

        let registry = ParserRegistry::register_builtin();
        let mut parser = BuildConfigParser::new();
        registry
            .scan_directory(
                temp_dir.path(),
                temp_dir.path(),
                &std::fs::read_dir(temp_dir.path())
                    .unwrap()
                    .filter_map(|e| e.ok())
                    .collect::<Vec<_>>(),
                &mut parser,
            )
            .unwrap();

        let packages = parser.packages_for_language(Language::Rust);
        assert!(packages.contains("serde"));
        assert!(packages.contains("tokio"));
    }

    #[test]
    fn test_registry_dotnet_glob() {
        let temp_dir = tempfile::tempdir().unwrap();
        // Create a csproj file - should be matched via supports_file
        let registry = ParserRegistry::register_builtin();
        assert!(registry.get_parser("TestProject.csproj").is_some());
        assert_eq!(
            registry.get_parser("TestProject.csproj").unwrap().name(),
            ".NET"
        );
        // Verify scan picks it up
        std::fs::write(
            temp_dir.path().join("TestProject.csproj"),
            r#"<Project Sdk="Microsoft.NET.Sdk">
  <ItemGroup>
    <PackageReference Include="Newtonsoft.Json" Version="13.0.3" />
  </ItemGroup>
</Project>"#,
        )
        .unwrap();
        let mut parser = BuildConfigParser::new();
        registry
            .scan_directory(
                temp_dir.path(),
                temp_dir.path(),
                &std::fs::read_dir(temp_dir.path())
                    .unwrap()
                    .filter_map(|e| e.ok())
                    .collect::<Vec<_>>(),
                &mut parser,
            )
            .unwrap();
        let packages = parser.packages_for_language(Language::CSharp);
        assert!(packages.contains("Newtonsoft.Json"));
    }
}
