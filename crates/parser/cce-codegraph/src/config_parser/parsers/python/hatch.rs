//! Hatch parser for Python projects

use std::collections::HashSet;
use std::path::Path;

use cce_types::language::Language;

use super::super::super::detector::BuildConfigParser;
use super::super::super::error::ConfigParseError;
use super::super::super::types::UntypedDependency;
use super::common::parse_python_requirement;

impl BuildConfigParser {
    /// Parse pyproject.toml file for Hatch projects
    pub(crate) fn try_parse_hatch(
        &mut self,
        project_root: &Path,
        config_path: &Path,
        content: &str,
    ) -> Result<(), ConfigParseError> {
        let parsed: toml::Value = toml::from_str(content).map_err(|e| ConfigParseError::Parse {
            path: config_path.to_path_buf(),
            build_system: "hatch".to_string(),
            reason: e.to_string(),
        })?;

        let mut dependencies = HashSet::new();

        // Parse [project] section (PEP 621)
        if let Some(project) = parsed.get("project") {
            // Parse dependencies
            if let Some(deps) = project.get("dependencies").and_then(|v| v.as_array()) {
                for req_value in deps {
                    if let Some(req_str) = req_value.as_str() {
                        if let Some(name) = parse_python_requirement(req_str) {
                            dependencies.insert(UntypedDependency::new(name, "external"));
                        }
                    }
                }
            }

            // Parse optional-dependencies
            if let Some(optional_deps) = project
                .get("optional-dependencies")
                .and_then(|v| v.as_table())
            {
                for (_extra_name, extra_reqs) in optional_deps {
                    if let Some(req_array) = extra_reqs.as_array() {
                        for req_value in req_array {
                            if let Some(req_str) = req_value.as_str() {
                                if let Some(name) = parse_python_requirement(req_str) {
                                    dependencies.insert(UntypedDependency::new(name, "external"));
                                }
                            }
                        }
                    }
                }
            }
        }

        // Parse [tool.hatch] section
        if let Some(tool) = parsed.get("tool") {
            if let Some(hatch) = tool.get("hatch") {
                // Parse build dependencies
                if let Some(build) = hatch.get("build").and_then(|v| v.as_table()) {
                    if let Some(deps) = build.get("dependencies").and_then(|v| v.as_array()) {
                        for req_value in deps {
                            if let Some(req_str) = req_value.as_str() {
                                if let Some(name) = parse_python_requirement(req_str) {
                                    dependencies.insert(UntypedDependency::new(name, "dev"));
                                }
                            }
                        }
                    }
                }

                // Parse environment dependencies
                if let Some(envs) = hatch.get("envs").and_then(|v| v.as_table()) {
                    for (env_name, env_config) in envs {
                        let is_dev = env_name == "default"
                            || env_name.contains("dev")
                            || env_name.contains("test");

                        if let Some(deps) =
                            env_config.get("dependencies").and_then(|v| v.as_array())
                        {
                            for req_value in deps {
                                if let Some(req_str) = req_value.as_str() {
                                    if let Some(name) = parse_python_requirement(req_str) {
                                        if is_dev {
                                            dependencies
                                                .insert(UntypedDependency::new(name, "dev"));
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

    #[test]
    fn test_parse_hatch_pyproject() {
        let temp_dir = TempDir::new().expect("temp dir failed");
        let pyproject = temp_dir.path().join("pyproject.toml");
        let content = r#"[build-system]
requires = ["hatchling"]
build-backend = "hatchling.build"

[project]
name = "hatch-test-project"
version = "1.0.0"
dependencies = [
    "requests>=2.31.0",
    "numpy>=1.24.0",
]

[project.optional-dependencies]
dev = ["pytest>=7.0.0", "black>=23.0.0"]
test = ["pytest-cov>=4.0.0"]
"#;
        let mut file = std::fs::File::create(&pyproject).expect("create failed");
        file.write_all(content.as_bytes()).expect("write failed");

        let mut parser = BuildConfigParser::new();
        let content = std::fs::read_to_string(&pyproject).expect("read failed");
        parser
            .try_parse_hatch(temp_dir.path(), &pyproject, &content)
            .expect("parse failed");

        let packages = parser.packages_for_language(Language::Python);
        assert!(!packages.is_empty());
        assert!(packages.contains("requests"));
        assert!(packages.contains("numpy"));
    }
}
