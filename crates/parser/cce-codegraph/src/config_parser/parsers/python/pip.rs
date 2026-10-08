//! pip/requirements.txt parser for Python projects

use std::path::Path;

use cce_types::language::Language;

use super::super::super::detector::BuildConfigParser;
use super::super::super::error::ConfigParseError;
use super::common::parse_requirements_txt;

impl BuildConfigParser {
    /// Parse requirements.txt file for Python pip projects
    pub(crate) fn try_parse_pip(
        &mut self,
        project_root: &Path,
        path: &Path,
    ) -> Result<(), ConfigParseError> {
        let content = std::fs::read_to_string(path).map_err(|e| ConfigParseError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;

        let dependencies = parse_requirements_txt(&content);

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
    fn test_parse_requirements_txt() {
        let temp_dir = TempDir::new().expect("temp dir failed");
        let requirements_path = temp_dir.path().join("requirements.txt");
        let content = r#"requests>=2.31.0
numpy>=1.24.0
flask>=2.0.0
"#;
        let mut file = std::fs::File::create(&requirements_path).expect("create failed");
        file.write_all(content.as_bytes()).expect("write failed");

        let mut parser = BuildConfigParser::new();
        parser
            .try_parse_pip(temp_dir.path(), &requirements_path)
            .expect("parse failed");

        let packages = parser.packages_for_language(Language::Python);
        assert_eq!(packages.len(), 3);
        assert!(packages.contains("requests"));
        assert!(packages.contains("numpy"));
        assert!(packages.contains("flask"));
    }
}
