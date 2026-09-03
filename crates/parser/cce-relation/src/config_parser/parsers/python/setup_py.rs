//! setup.py parser for Python setuptools projects

use std::collections::HashSet;
use std::path::Path;

use cce_types::language::Language;

use super::super::super::detector::BuildConfigParser;
use super::super::super::error::ConfigParseError;
use super::super::super::types::UntypedDependency;
use super::common::parse_python_requirement;

impl BuildConfigParser {
    /// Parse setup.py file for setuptools projects
    pub(crate) fn try_parse_setup_py(
        &mut self,
        project_root: &Path,
        path: &Path,
    ) -> Result<(), ConfigParseError> {
        let content = std::fs::read_to_string(path).map_err(|e| ConfigParseError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;

        let mut dependencies = HashSet::new();

        // Simple regex-based parsing for setup.py
        // (?s) enables DOTALL mode so . matches newlines
        let install_requires_re =
            regex::Regex::new(r#"(?s)install_requires\s*=\s*\[([^\]]+)\]"#).unwrap();

        // Extract install_requires
        if let Some(caps) = install_requires_re.captures(&content) {
            let requires_text = &caps[1];
            let requirements = parse_python_list(requires_text);

            // Parse each requirement
            for req in requirements {
                if let Some(name) = parse_python_requirement(&req) {
                    dependencies.insert(UntypedDependency::new(name, "external"));
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

/// Parse a Python list literal from setup.py
fn parse_python_list(list_text: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut quote_char = '\0';
    let mut escape = false;

    for ch in list_text.chars() {
        if escape {
            current.push(ch);
            escape = false;
            continue;
        }

        match ch {
            '\\' => {
                escape = true;
                current.push(ch);
            }
            '\'' | '"' => {
                if !in_quotes {
                    in_quotes = true;
                    quote_char = ch;
                    current.push(ch);
                } else if ch == quote_char {
                    in_quotes = false;
                    current.push(ch);
                } else {
                    current.push(ch);
                }
            }
            ',' if !in_quotes => {
                let item = current.trim().to_string();
                // Remove surrounding quotes
                let item = item.trim_matches('"').trim_matches('\'').to_string();
                if !item.is_empty() {
                    result.push(item);
                }
                current.clear();
            }
            _ => {
                current.push(ch);
            }
        }
    }

    // Add the last item
    let item = current.trim().to_string();
    // Remove surrounding quotes
    let item = item.trim_matches('"').trim_matches('\'').to_string();
    if !item.is_empty() {
        result.push(item);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_parser::detector::BuildConfigParser;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_parse_setup_py() {
        let temp_dir = TempDir::new().expect("temp dir failed");
        let setup_py = temp_dir.path().join("setup.py");
        let content = r#"from setuptools import setup, find_packages

setup(
    name="test-setup-project",
    version="1.0.0",
    install_requires=[
        "requests>=2.31.0",
        "numpy>=1.24.0",
    ],
)"#;
        let mut file = std::fs::File::create(&setup_py).expect("create failed");
        file.write_all(content.as_bytes()).expect("write failed");

        let mut parser = BuildConfigParser::new();
        parser
            .try_parse_setup_py(temp_dir.path(), &setup_py)
            .expect("parse failed");

        let packages = parser.packages_for_language(Language::Python);
        assert_eq!(packages.len(), 2);
        assert!(packages.contains("requests"));
        assert!(packages.contains("numpy"));
    }

    #[test]
    fn test_parse_python_list() {
        let list_text = r#""requests>=2.31.0",
        "numpy>=1.24.0",
        "flask>=2.0.0""#;
        let result = parse_python_list(list_text);
        assert_eq!(result.len(), 3);
        assert!(result.iter().any(|s| s.contains("requests")));
        assert!(result.iter().any(|s| s.contains("numpy")));
        assert!(result.iter().any(|s| s.contains("flask")));
    }
}
