//! JavaScript/TypeScript build system parsers

mod common;

use std::path::Path;

use cce_types::language::Language;

use super::super::error::ConfigParseError;
use super::super::trait_def::{LanguageParser, ParseOutcome};

use self::common::parse_package_json_deps;

/// NPM package.json parser for JavaScript/TypeScript
pub struct NpmParser;

impl LanguageParser for NpmParser {
    fn try_parse(
        &self,
        project_root: &Path,
        dir: &Path,
    ) -> Result<Option<ParseOutcome>, ConfigParseError> {
        let package_json = dir.join("package.json");
        if !package_json.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(&package_json).map_err(|e| ConfigParseError::Io {
            path: package_json.clone(),
            source: e,
        })?;

        let dependencies =
            parse_package_json_deps(&content).map_err(|e| ConfigParseError::Parse {
                path: package_json.clone(),
                build_system: "javascript".to_string(),
                reason: e,
            })?;

        if dependencies.is_empty() {
            return Ok(None);
        }

        let rel = cce_types::path::relativize(project_root, &package_json);
        Ok(Some(ParseOutcome {
            dependencies,
            config_file: rel,
        }))
    }

    fn supported_languages(&self) -> Vec<Language> {
        vec![Language::JavaScript, Language::TypeScript]
    }

    fn supported_config_files(&self) -> &[&str] {
        &["package.json"]
    }

    fn name(&self) -> &str {
        "NPM"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_parser::detector::BuildConfigParser;
    use crate::config_parser::types::UntypedDependency;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_files(dir: &TempDir, package_json: &str, lock_files: &[(&str, &str)]) {
        let file_path = dir.path().join("package.json");
        let mut file = std::fs::File::create(&file_path).expect("create failed");
        file.write_all(package_json.as_bytes())
            .expect("write failed");

        for (lock_name, lock_content) in lock_files {
            let lock_path = dir.path().join(lock_name);
            let mut lock_file = std::fs::File::create(&lock_path).expect("create failed");
            lock_file
                .write_all(lock_content.as_bytes())
                .expect("write failed");
        }
    }

    #[test]
    fn test_parse_javascript_project() {
        let temp_dir = TempDir::new().expect("temp dir failed");
        let package_json = r#"{
    "name": "test-project",
    "version": "1.0.0",
    "dependencies": {
        "express": "^4.18.0",
        "lodash": "^4.17.21"
    }
}"#;
        create_test_files(&temp_dir, package_json, &[]);

        let parser = NpmParser;
        let result = parser.try_parse(temp_dir.path(), temp_dir.path()).unwrap();
        assert!(result.is_some());
        let outcome = result.unwrap();
        assert!(outcome.dependencies.iter().any(|d| d.name == "express"));

        let mut build_parser = BuildConfigParser::new();
        build_parser
            .scan_project(temp_dir.path(), 0)
            .expect("parse failed");
        let packages = build_parser.packages_for_language(Language::JavaScript);
        assert!(packages.contains("express"));
        assert!(packages.contains("lodash"));
        assert_eq!(packages.len(), 2);
        // Also available for TypeScript
        let ts_packages = build_parser.packages_for_language(Language::TypeScript);
        assert!(ts_packages.contains("express"));
    }

    #[test]
    fn test_parse_javascript_with_dev_deps() {
        let temp_dir = TempDir::new().expect("temp dir failed");
        let package_json = r#"{
    "name": "test-project",
    "dependencies": {
        "express": "^4.18.0"
    },
    "devDependencies": {
        "jest": "^29.0.0"
    }
}"#;
        create_test_files(&temp_dir, package_json, &[]);

        let parser = NpmParser;
        let outcome = parser
            .try_parse(temp_dir.path(), temp_dir.path())
            .unwrap()
            .unwrap();
        let dev_deps: Vec<&UntypedDependency> = outcome
            .dependencies
            .iter()
            .filter(|d| d.package_type == "dev")
            .collect();
        assert_eq!(dev_deps.len(), 1);
        assert_eq!(dev_deps[0].name, "jest");
    }

    #[test]
    fn test_parse_javascript_no_package_json() {
        let temp_dir = TempDir::new().expect("temp dir failed");
        let parser = NpmParser;
        let result = parser.try_parse(temp_dir.path(), temp_dir.path()).unwrap();
        assert!(result.is_none());

        let mut build_parser = BuildConfigParser::new();
        build_parser
            .scan_project(temp_dir.path(), 0)
            .expect("parse failed");
        let packages = build_parser.packages_for_language(Language::JavaScript);
        assert!(packages.is_empty());
    }

    #[test]
    fn test_parse_javascript_empty_deps() {
        let temp_dir = TempDir::new().expect("temp dir failed");
        let package_json = r#"{"name": "empty-project", "version": "1.0.0"}"#;
        create_test_files(&temp_dir, package_json, &[]);

        let parser = NpmParser;
        let result = parser.try_parse(temp_dir.path(), temp_dir.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_javascript_with_lock_file() {
        let temp_dir = TempDir::new().expect("temp dir failed");
        let package_json = r#"{
    "name": "test",
    "dependencies": {
        "vue": "^3.3.0"
    }
}"#;
        create_test_files(
            &temp_dir,
            package_json,
            &[("pnpm-lock.yaml", "lockfileVersion: '6.0'")],
        );

        let parser = NpmParser;
        let outcome = parser
            .try_parse(temp_dir.path(), temp_dir.path())
            .unwrap()
            .unwrap();
        assert!(outcome.dependencies.iter().any(|d| d.name == "vue"));
    }
}
