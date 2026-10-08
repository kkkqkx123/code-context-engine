//! Pipenv parser for Python projects

use std::collections::HashSet;
use std::path::Path;

use cce_types::language::Language;

use super::super::super::detector::BuildConfigParser;
use super::super::super::error::ConfigParseError;
use super::super::super::types::UntypedDependency;

impl BuildConfigParser {
    /// Parse Pipfile for Python pipenv projects
    pub(crate) fn try_parse_pipenv(
        &mut self,
        project_root: &Path,
        path: &Path,
    ) -> Result<(), ConfigParseError> {
        let content = std::fs::read_to_string(path).map_err(|e| ConfigParseError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;

        let mut dependencies = HashSet::new();
        let mut in_packages_section = false;
        let mut in_dev_packages_section = false;

        for line in content.lines() {
            let line = line.trim();

            // Skip comments and empty lines
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Check for section headers
            if line.starts_with('[') && line.ends_with(']') {
                let section = &line[1..line.len() - 1];
                in_packages_section = section == "packages";
                in_dev_packages_section = section == "dev-packages";
                continue;
            }

            // Parse package lines
            if in_packages_section || in_dev_packages_section {
                if let Some(eq_pos) = line.find('=') {
                    let name = line[..eq_pos]
                        .trim()
                        .trim_matches('"')
                        .trim_matches('\'')
                        .to_string();

                    if in_dev_packages_section {
                        dependencies.insert(UntypedDependency::new(name, "dev"));
                    } else {
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
    fn test_parse_pipfile() {
        let temp_dir = TempDir::new().expect("temp dir failed");
        let pipfile_path = temp_dir.path().join("Pipfile");
        let content = r#"[[source]]
url = "https://pypi.org/simple"
verify_ssl = true
name = "pypi"

[packages]
requests = ">=2.31.0"
numpy = ">=1.24.0"

[dev-packages]
pytest = ">=7.0.0"
black = ">=23.0.0"

[requires]
python_version = "3.11"
"#;
        let mut file = std::fs::File::create(&pipfile_path).expect("create failed");
        file.write_all(content.as_bytes()).expect("write failed");

        let mut parser = BuildConfigParser::new();
        parser
            .try_parse_pipenv(temp_dir.path(), &pipfile_path)
            .expect("parse failed");

        let packages = parser.packages_for_language(Language::Python);
        assert_eq!(packages.len(), 4);
        assert!(packages.contains("requests"));
        assert!(packages.contains("numpy"));
        assert!(packages.contains("pytest"));
        assert!(packages.contains("black"));
    }
}
