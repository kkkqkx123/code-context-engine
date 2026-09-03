//! Cargo.toml parser for Rust projects (simplified)

use std::collections::HashSet;
use std::path::Path;

use cce_types::Language;

use super::super::error::ConfigParseError;
use super::super::trait_def::{LanguageParser, ParseOutcome};
use super::super::types::UntypedDependency;

/// Cargo.toml parser
pub struct CargoParser;

impl LanguageParser for CargoParser {
    fn try_parse(
        &self,
        project_root: &Path,
        dir: &Path,
    ) -> Result<Option<ParseOutcome>, ConfigParseError> {
        let cargo_toml = dir.join("Cargo.toml");
        if !cargo_toml.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(&cargo_toml).map_err(|e| ConfigParseError::Io {
            path: cargo_toml.clone(),
            source: e,
        })?;

        let parsed: toml::Value = toml::from_str(&content)
            .map_err(|e| ConfigParseError::parse(cargo_toml.clone(), "Cargo", e.to_string()))?;
        let mut deps = HashSet::new();

        if let Some(deps_table) = parsed.get("dependencies").and_then(|v| v.as_table()) {
            for (name, val) in deps_table.iter() {
                let dep_type = if is_local_cargo_dep(val) {
                    "local"
                } else {
                    "external"
                };
                deps.insert(UntypedDependency::new(name, dep_type));
            }
        }

        if let Some(deps_table) = parsed.get("dev-dependencies").and_then(|v| v.as_table()) {
            for (name, val) in deps_table.iter() {
                let dep_type = if is_local_cargo_dep(val) {
                    "local"
                } else {
                    "dev"
                };
                // dev dependencies that are local paths should still be considered local for workspace
                if dep_type == "local" {
                    deps.insert(UntypedDependency::new(name, "local"));
                } else {
                    deps.insert(UntypedDependency::new(name, "dev"));
                }
            }
        }

        if deps.is_empty() {
            return Ok(None);
        }

        let rel = cce_types::path::relativize(project_root, &cargo_toml);
        Ok(Some(ParseOutcome {
            dependencies: deps,
            config_file: rel,
        }))
    }

    fn supported_languages(&self) -> Vec<Language> {
        vec![Language::Rust]
    }

    fn supported_config_files(&self) -> &[&str] {
        &["Cargo.toml"]
    }

    fn name(&self) -> &str {
        "Cargo"
    }
}

fn is_local_cargo_dep(val: &toml::Value) -> bool {
    if let Some(table) = val.as_table() {
        if table.contains_key("path") {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_parser::detector::BuildConfigParser;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_cargo_toml(dir: &TempDir, content: &str) {
        let cargo_toml = dir.path().join("Cargo.toml");
        let mut file = std::fs::File::create(&cargo_toml).unwrap();
        file.write_all(content.as_bytes()).unwrap();
    }

    #[test]
    fn test_parse_simple_cargo_toml() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"
[package]
name = "test-project"
version = "1.0.0"
edition = "2021"

[dependencies]
serde = "1.0"
tokio = { version = "1.0", features = ["full"] }
"#;
        create_test_cargo_toml(&temp_dir, content);

        let parser = CargoParser;
        let result = parser.try_parse(temp_dir.path(), temp_dir.path()).unwrap();
        assert!(result.is_some());
        let outcome = result.unwrap();
        assert_eq!(outcome.dependencies.len(), 2);
        assert!(outcome.dependencies.iter().any(|d| d.name == "serde"));
        assert!(outcome.dependencies.iter().any(|d| d.name == "tokio"));

        // Also test via registry integration
        let mut build_parser = BuildConfigParser::new();
        build_parser
            .scan_project(temp_dir.path(), 0)
            .expect("scan failed");
        let packages = build_parser.packages_for_language(cce_types::language::Language::Rust);
        assert_eq!(packages.len(), 2);
        assert!(packages.contains("serde"));
        assert!(packages.contains("tokio"));
    }

    #[test]
    fn test_parse_cargo_with_dev_dependencies() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"
[package]
name = "test-project"
version = "1.0.0"

[dependencies]
serde = "1.0"

[dev-dependencies]
tokio-test = "0.4"
"#;
        create_test_cargo_toml(&temp_dir, content);

        let parser = CargoParser;
        let result = parser
            .try_parse(temp_dir.path(), temp_dir.path())
            .unwrap()
            .unwrap();
        assert_eq!(result.dependencies.len(), 2);
        let dev_deps: Vec<_> = result.dependencies.iter().filter(|d| d.is_dev()).collect();
        assert_eq!(dev_deps.len(), 1);
        assert_eq!(dev_deps[0].name, "tokio-test");
    }

    #[test]
    fn test_parse_cargo_no_file() {
        let temp_dir = TempDir::new().unwrap();
        let parser = CargoParser;
        let result = parser.try_parse(temp_dir.path(), temp_dir.path()).unwrap();
        assert!(result.is_none());

        let mut build_parser = BuildConfigParser::new();
        build_parser
            .scan_project(temp_dir.path(), 0)
            .expect("scan failed");
        let packages = build_parser.packages_for_language(cce_types::language::Language::Rust);
        assert!(packages.is_empty());
    }
}
