//! Conda parser for Python projects

use std::collections::HashSet;
use std::path::Path;

use cce_types::language::Language;

use super::super::super::detector::BuildConfigParser;
use super::super::super::error::ConfigParseError;
use super::super::super::types::UntypedDependency;

impl BuildConfigParser {
    /// Parse environment.yml file for Conda projects
    pub(crate) fn try_parse_conda(
        &mut self,
        project_root: &Path,
        path: &Path,
    ) -> Result<(), ConfigParseError> {
        let content = std::fs::read_to_string(path).map_err(|e| ConfigParseError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;

        let mut dependencies = HashSet::new();
        let mut in_dependencies_section = false;

        for line in content.lines() {
            let line = line.trim();

            // Skip comments
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Check for dependencies section
            if line.starts_with("dependencies:") {
                in_dependencies_section = true;
                continue;
            }

            if in_dependencies_section {
                // Check for end of list or new section
                if line.starts_with('-') && !line.starts_with("--") {
                    let dep_line = line[1..].trim();
                    if !dep_line.is_empty() {
                        // Parse conda package spec: package=version, package>=version, etc.
                        // Extract just the package name
                        let name = if let Some(pos) = dep_line.find(['=', '>', '<', '~']) {
                            dep_line[..pos].trim().to_string()
                        } else {
                            dep_line.to_string()
                        };

                        if !name.is_empty() && name != "pip" {
                            dependencies.insert(UntypedDependency::new(name, "external"));
                        }
                    }
                } else if !line.starts_with(' ') && !line.starts_with('-') {
                    // New section started
                    in_dependencies_section = false;
                }
            }
        }

        if !dependencies.is_empty() {
            let rel = cce_types::path::relativize(project_root, path);
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
    fn test_parse_environment_yml() {
        let temp_dir = TempDir::new().expect("temp dir failed");
        let env_yml = temp_dir.path().join("environment.yml");
        let content = r#"name: test-env
channels:
  - conda-forge
  - defaults
dependencies:
  - python=3.11
  - numpy=1.24.0
  - pandas>=2.0.0
  - pip:
    - requests>=2.31.0
"#;
        let mut file = std::fs::File::create(&env_yml).expect("create failed");
        file.write_all(content.as_bytes()).expect("write failed");

        let mut parser = BuildConfigParser::new();
        parser
            .try_parse_conda(temp_dir.path(), &env_yml)
            .expect("parse failed");

        let packages = parser.packages_for_language(Language::Python);
        assert!(!packages.is_empty());
        assert!(packages.contains("numpy"));
        assert!(packages.contains("pandas"));
    }
}
