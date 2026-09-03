//! Poetry parser for Python projects

use std::collections::HashSet;
use std::path::Path;

use cce_types::language::Language;

use super::super::super::detector::BuildConfigParser;
use super::super::super::error::ConfigParseError;
use super::super::super::types::UntypedDependency;

impl BuildConfigParser {
    /// Parse pyproject.toml file for Poetry projects
    pub(crate) fn try_parse_poetry(
        &mut self,
        project_root: &Path,
        config_path: &Path,
        content: &str,
    ) -> Result<(), ConfigParseError> {
        let parsed: toml::Value = toml::from_str(content).map_err(|e| ConfigParseError::Parse {
            path: config_path.to_path_buf(),
            build_system: "poetry".to_string(),
            reason: e.to_string(),
        })?;

        let mut dependencies = HashSet::new();

        // Parse [tool.poetry.dependencies]
        if let Some(tool) = parsed.get("tool") {
            if let Some(poetry) = tool.get("poetry") {
                // Parse main dependencies
                if let Some(deps) = poetry.get("dependencies").and_then(|v| v.as_table()) {
                    for name in deps.keys() {
                        if name != "python" {
                            dependencies.insert(UntypedDependency::new(name, "external"));
                        }
                    }
                }

                // Parse [tool.poetry.dev-dependencies]
                if let Some(deps) = poetry.get("dev-dependencies").and_then(|v| v.as_table()) {
                    for name in deps.keys() {
                        dependencies.insert(UntypedDependency::new(name, "dev"));
                    }
                }

                // Parse group dependencies (Poetry 1.2+)
                if let Some(group) = poetry.get("group") {
                    if let Some(group_table) = group.as_table() {
                        for (group_name, group_config) in group_table {
                            let is_dev = group_name == "dev";
                            if let Some(deps) =
                                group_config.get("dependencies").and_then(|v| v.as_table())
                            {
                                for name in deps.keys() {
                                    if is_dev {
                                        dependencies.insert(UntypedDependency::new(name, "dev"));
                                    } else {
                                        dependencies
                                            .insert(UntypedDependency::new(name, "external"));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if !dependencies.is_empty() {
            let rel = cce_types::path::relativize(project_root, config_path);
            self.insert_packages_for_file(&rel, Language::Python, dependencies);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_parser::detector::BuildConfigParser;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_pyproject_toml(dir: &TempDir, content: &str) {
        let path = dir.path().join("pyproject.toml");
        let mut file = std::fs::File::create(&path).expect("create failed");
        file.write_all(content.as_bytes()).expect("write failed");
    }

    #[test]
    fn test_parse_poetry_pyproject() {
        let temp_dir = TempDir::new().expect("temp dir failed");
        let content = r#"[tool.poetry]
name = "poetry-test-project"
version = "1.0.0"
description = "A test project"

[tool.poetry.dependencies]
python = "^3.11"
requests = "^2.31.0"
numpy = ">=1.24.0,<2.0.0"

[tool.poetry.dev-dependencies]
pytest = "^7.0.0"
black = "^23.0.0"
"#;
        create_test_pyproject_toml(&temp_dir, content);

        let mut parser = BuildConfigParser::new();
        let content =
            std::fs::read_to_string(temp_dir.path().join("pyproject.toml")).expect("read failed");
        parser
            .try_parse_poetry(
                temp_dir.path(),
                &temp_dir.path().join("pyproject.toml"),
                &content,
            )
            .expect("parse failed");

        let packages = parser.packages_for_language(Language::Python);
        // Should have 4 dependencies (python version constraint is skipped): requests, numpy, pytest, black
        assert_eq!(packages.len(), 4);
        assert!(packages.contains("requests"));
        assert!(packages.contains("numpy"));
        assert!(packages.contains("pytest"));
        assert!(packages.contains("black"));
    }
}
