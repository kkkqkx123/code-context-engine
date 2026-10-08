//! setuptools parser for Python projects

use std::collections::HashSet;
use std::path::Path;

use cce_types::language::Language;

use super::super::super::detector::BuildConfigParser;
use super::super::super::error::ConfigParseError;
use super::super::super::types::UntypedDependency;
use super::common::parse_python_requirement;

impl BuildConfigParser {
    /// Parse setup.cfg file for Python setuptools projects
    pub(crate) fn try_parse_setuptools(
        &mut self,
        project_root: &Path,
        path: &Path,
    ) -> Result<(), ConfigParseError> {
        let content = std::fs::read_to_string(path).map_err(|e| ConfigParseError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;

        let mut dependencies = HashSet::new();
        let mut in_options_section = false;
        let mut install_requires_value = String::new();
        let mut in_install_requires = false;

        for line in content.lines() {
            let trimmed = line.trim();

            // Skip comments
            if trimmed.starts_with('#') || trimmed.starts_with(';') {
                continue;
            }

            // Check for section headers
            if trimmed.starts_with('[') {
                in_options_section = trimmed == "[options]";
                in_install_requires = false;
                continue;
            }

            // Parse install_requires
            if in_options_section && trimmed.starts_with("install_requires") {
                if let Some(eq_pos) = trimmed.find('=') {
                    let value = trimmed[eq_pos + 1..].trim();
                    install_requires_value.push_str(value);
                    // Check if value continues on next lines
                    in_install_requires = value.is_empty() || value.ends_with('\\');
                }
            } else if in_install_requires {
                // Continue multi-line value
                if trimmed.is_empty() || trimmed.starts_with('[') {
                    // End of value
                    in_install_requires = false;
                } else if trimmed.ends_with('\\') {
                    install_requires_value.push_str(trimmed.trim_end_matches('\\'));
                } else {
                    install_requires_value.push(' ');
                    install_requires_value.push_str(trimmed);
                }
            }
        }

        // Parse the accumulated requirements
        if !install_requires_value.is_empty() {
            // First try comma-separated, then whitespace-separated
            let reqs: Vec<&str> = if install_requires_value.contains(',') {
                install_requires_value.split(',').collect()
            } else {
                // Split by whitespace for multi-line format without commas
                install_requires_value.split_whitespace().collect()
            };

            for req in reqs {
                let req = req.trim().trim_matches('"').trim_matches('\'');
                if !req.is_empty() {
                    if let Some(name) = parse_python_requirement(req) {
                        dependencies.insert(UntypedDependency::new(name, "external"));
                    }
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
    fn test_parse_setup_cfg() {
        let temp_dir = TempDir::new().expect("temp dir failed");
        let setup_cfg = temp_dir.path().join("setup.cfg");
        let content = r#"[metadata]
name = test-project
version = 1.0.0

[options]
install_requires =
    requests>=2.31.0
    numpy>=1.24.0
    flask>=2.0.0
"#;
        let mut file = std::fs::File::create(&setup_cfg).expect("create failed");
        file.write_all(content.as_bytes()).expect("write failed");

        let mut parser = BuildConfigParser::new();
        parser
            .try_parse_setuptools(temp_dir.path(), &setup_cfg)
            .expect("parse failed");

        let packages = parser.packages_for_language(Language::Python);
        assert_eq!(packages.len(), 3);
        assert!(packages.contains("requests"));
        assert!(packages.contains("numpy"));
        assert!(packages.contains("flask"));
    }
}
