//! Composer parser for PHP projects

use std::collections::HashSet;
use std::path::Path;

use cce_types::language::Language;

use super::super::error::ConfigParseError;
use super::super::trait_def::{LanguageParser, ParseOutcome};
use super::super::types::UntypedDependency;

/// Composer parser for PHP
pub struct ComposerParser;

impl LanguageParser for ComposerParser {
    fn try_parse(
        &self,
        project_root: &Path,
        dir: &Path,
    ) -> Result<Option<ParseOutcome>, ConfigParseError> {
        let composer_json = dir.join("composer.json");
        if !composer_json.exists() {
            return Ok(None);
        }

        let content =
            std::fs::read_to_string(&composer_json).map_err(|e| ConfigParseError::Io {
                path: composer_json.clone(),
                source: e,
            })?;

        let parsed: serde_json::Value =
            serde_json::from_str(&content).map_err(|e| ConfigParseError::Parse {
                path: composer_json.clone(),
                build_system: "composer".to_string(),
                reason: e.to_string(),
            })?;

        let mut dependencies = HashSet::new();

        if let Some(deps) = parsed.get("require").and_then(|v| v.as_object()) {
            for name in deps.keys() {
                if name != "php" {
                    dependencies.insert(UntypedDependency::new(name, "external"));
                }
            }
        }

        if let Some(deps) = parsed.get("require-dev").and_then(|v| v.as_object()) {
            for name in deps.keys() {
                dependencies.insert(UntypedDependency::new(name, "dev"));
            }
        }

        if dependencies.is_empty() {
            return Ok(None);
        }

        let rel = cce_types::path::relativize(project_root, &composer_json);
        Ok(Some(ParseOutcome {
            dependencies,
            config_file: rel,
        }))
    }

    fn supported_languages(&self) -> Vec<Language> {
        vec![Language::Php]
    }

    fn supported_config_files(&self) -> &[&str] {
        &["composer.json"]
    }

    fn name(&self) -> &str {
        "Composer"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_parser::detector::BuildConfigParser;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_composer_json(dir: &TempDir, content: &str) {
        let composer_json = dir.path().join("composer.json");
        let mut file = std::fs::File::create(&composer_json).expect("create failed");
        file.write_all(content.as_bytes()).expect("write failed");
    }

    #[test]
    fn test_parse_simple_composer_json() {
        let temp_dir = TempDir::new().expect("temp dir failed");
        let content = r#"{
    "name": "vendor/test-project",
    "version": "1.0.0",
    "require": {
        "php": ">=8.0",
        "monolog/monolog": "^3.0",
        "guzzlehttp/guzzle": "^7.0"
    }
}"#;
        create_test_composer_json(&temp_dir, content);

        let parser = ComposerParser;
        let outcome = parser
            .try_parse(temp_dir.path(), temp_dir.path())
            .unwrap()
            .unwrap();
        assert_eq!(outcome.dependencies.len(), 2);
        assert!(
            outcome
                .dependencies
                .iter()
                .any(|d| d.name == "monolog/monolog")
        );

        let mut build_parser = BuildConfigParser::new();
        build_parser
            .scan_project(temp_dir.path(), 0)
            .expect("parse failed");
        let packages = build_parser.packages_for_language(Language::Php);
        assert_eq!(packages.len(), 2);
        assert!(packages.contains("monolog/monolog"));
        assert!(packages.contains("guzzlehttp/guzzle"));
    }

    #[test]
    fn test_parse_composer_no_file() {
        let temp_dir = TempDir::new().expect("temp dir failed");
        let parser = ComposerParser;
        let result = parser.try_parse(temp_dir.path(), temp_dir.path()).unwrap();
        assert!(result.is_none());

        let mut build_parser = BuildConfigParser::new();
        build_parser
            .scan_project(temp_dir.path(), 0)
            .expect("parse failed");
        let packages = build_parser.packages_for_language(Language::Php);
        assert!(packages.is_empty());
    }
}
