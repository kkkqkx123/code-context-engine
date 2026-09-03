//! Flit parser for Python projects

use std::collections::HashSet;
use std::path::Path;

use cce_types::language::Language;

use super::super::super::detector::BuildConfigParser;
use super::super::super::error::ConfigParseError;
use super::super::super::types::UntypedDependency;
use super::common::parse_python_requirement;

impl BuildConfigParser {
    /// Parse pyproject.toml file for Flit projects
    pub(crate) fn try_parse_flit(
        &mut self,
        project_root: &Path,
        config_path: &Path,
        content: &str,
    ) -> Result<(), ConfigParseError> {
        let parsed: toml::Value = toml::from_str(content).map_err(|e| ConfigParseError::Parse {
            path: config_path.to_path_buf(),
            build_system: "flit".to_string(),
            reason: e.to_string(),
        })?;

        let mut dependencies = HashSet::new();

        // Parse [tool.flit.metadata]
        if let Some(tool) = parsed.get("tool") {
            if let Some(flit) = tool.get("flit") {
                if let Some(metadata) = flit.get("metadata").and_then(|v| v.as_table()) {
                    // Parse requires
                    if let Some(requires) = metadata.get("requires").and_then(|v| v.as_array()) {
                        for req_value in requires {
                            if let Some(req_str) = req_value.as_str() {
                                if let Some(name) = parse_python_requirement(req_str) {
                                    dependencies.insert(UntypedDependency::new(name, "external"));
                                }
                            }
                        }
                    }

                    // Parse requires-extra
                    if let Some(requires_extra) =
                        metadata.get("requires-extra").and_then(|v| v.as_table())
                    {
                        for (_extra_name, extra_reqs) in requires_extra {
                            if let Some(req_array) = extra_reqs.as_array() {
                                for req_value in req_array {
                                    if let Some(req_str) = req_value.as_str() {
                                        if let Some(name) = parse_python_requirement(req_str) {
                                            dependencies
                                                .insert(UntypedDependency::new(name, "external"));
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Parse requires-dev
                    if let Some(requires_dev) =
                        metadata.get("requires-dev").and_then(|v| v.as_array())
                    {
                        for req_value in requires_dev {
                            if let Some(req_str) = req_value.as_str() {
                                if let Some(name) = parse_python_requirement(req_str) {
                                    dependencies.insert(UntypedDependency::new(name, "dev"));
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
    fn test_parse_flit_pyproject() {
        let temp_dir = TempDir::new().expect("temp dir failed");
        let pyproject = temp_dir.path().join("pyproject.toml");
        let content = r#"[build-system]
requires = ["flit_core >=3.2,<4"]
build-backend = "flit_core.buildapi"

[tool.flit.metadata]
module = "test_module"
author = "Test Author"
requires = [
    "requests>=2.31.0",
    "numpy>=1.24.0",
]

[tool.flit.metadata.requires-dev]
test = ["pytest>=7.0.0"]
"#;
        let mut file = std::fs::File::create(&pyproject).expect("create failed");
        file.write_all(content.as_bytes()).expect("write failed");

        let mut parser = BuildConfigParser::new();
        let content = std::fs::read_to_string(&pyproject).expect("read failed");
        parser
            .try_parse_flit(temp_dir.path(), &pyproject, &content)
            .expect("parse failed");

        let packages = parser.packages_for_language(Language::Python);
        assert!(!packages.is_empty());
        assert!(packages.contains("requests"));
        assert!(packages.contains("numpy"));
    }
}
