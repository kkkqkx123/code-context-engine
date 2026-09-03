//! go.mod parser for Go projects

use std::collections::HashSet;
use std::path::Path;

use cce_types::language::Language;

use super::super::error::ConfigParseError;
use super::super::trait_def::{LanguageParser, ParseOutcome};
use super::super::types::UntypedDependency;

/// Go module parser
pub struct GoParser;

impl LanguageParser for GoParser {
    fn try_parse(
        &self,
        project_root: &Path,
        dir: &Path,
    ) -> Result<Option<ParseOutcome>, ConfigParseError> {
        let go_mod = dir.join("go.mod");
        if !go_mod.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(&go_mod).map_err(|e| ConfigParseError::Io {
            path: go_mod.clone(),
            source: e,
        })?;

        let mut dependencies = HashSet::new();
        let mut module_name = None;
        let mut in_require_block = false;
        let mut in_replace_block = false;

        for line in content.lines() {
            let trimmed = line.trim();

            if trimmed.is_empty() || trimmed.starts_with("//") {
                continue;
            }

            if trimmed.starts_with("module ") {
                module_name = trimmed
                    .strip_prefix("module ")
                    .map(|s| s.split_whitespace().next().unwrap_or(s).to_string());
                continue;
            }

            if trimmed.starts_with("require (") {
                in_require_block = true;
                continue;
            }

            if trimmed.starts_with("replace (") {
                in_replace_block = true;
                continue;
            }

            if trimmed == ")" {
                in_require_block = false;
                in_replace_block = false;
                continue;
            }

            if in_require_block || trimmed.starts_with("require ") {
                let dep_line = if in_require_block {
                    trimmed
                } else {
                    trimmed.strip_prefix("require ").unwrap_or(trimmed)
                };

                if let Some(dep_name) = parse_go_module_line(dep_line) {
                    if module_name
                        .as_ref()
                        .map(|m| m == &dep_name)
                        .unwrap_or(false)
                    {
                        continue;
                    }

                    dependencies.insert(UntypedDependency::new(dep_name, "external"));
                }
            }

            if in_replace_block {
                if let Some(local_dep) = parse_go_replace_line(trimmed) {
                    if module_name
                        .as_ref()
                        .map(|m| m == &local_dep)
                        .unwrap_or(false)
                    {
                        continue;
                    }

                    dependencies.insert(UntypedDependency::new(local_dep, "local"));
                }
            }
        }

        if dependencies.is_empty() {
            return Ok(None);
        }

        let rel = cce_types::path::relativize(project_root, &go_mod);
        Ok(Some(ParseOutcome {
            dependencies,
            config_file: rel,
        }))
    }

    fn supported_languages(&self) -> Vec<Language> {
        vec![Language::Go]
    }

    fn supported_config_files(&self) -> &[&str] {
        &["go.mod"]
    }

    fn name(&self) -> &str {
        "Go Modules"
    }
}

/// Parse a go.mod module line: "module version" or "module version // indirect"
fn parse_go_module_line(line: &str) -> Option<String> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if !parts.is_empty() {
        Some(parts[0].to_string())
    } else {
        None
    }
}

/// Parse a go.mod replace line for local path replacements.
///
/// Format: `old => ./local/path` or `old => /absolute/path`
/// Returns the module name if the replacement target is a local path.
fn parse_go_replace_line(line: &str) -> Option<String> {
    let parts: Vec<&str> = line.split("=>").collect();
    if parts.len() != 2 {
        return None;
    }

    let old_module = parts[0].split_whitespace().next()?;
    let new_target = parts[1].trim();

    // Only record local path replacements (relative or absolute paths)
    if new_target.starts_with('.') || new_target.starts_with('/') {
        Some(old_module.to_string())
    } else {
        // Module-to-module replacement — not a local dependency
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_parser::detector::BuildConfigParser;

    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_go_mod(dir: &TempDir, content: &str) {
        let go_mod = dir.path().join("go.mod");
        let mut file = std::fs::File::create(&go_mod).expect("create failed");
        file.write_all(content.as_bytes()).expect("write failed");
    }

    #[test]
    fn test_parse_simple_go_mod() {
        let temp_dir = TempDir::new().expect("temp dir failed");
        let content = r#"module github.com/example/myproject

go 1.21

require (
    github.com/gin-gonic/gin v1.9.1
    github.com/stretchr/testify v1.8.4
)
"#;
        create_test_go_mod(&temp_dir, content);

        let parser = GoParser;
        let result = parser.try_parse(temp_dir.path(), temp_dir.path()).unwrap();
        assert!(result.is_some());
        let outcome = result.unwrap();
        assert_eq!(outcome.dependencies.len(), 2);
        assert!(
            outcome
                .dependencies
                .iter()
                .any(|d| d.name == "github.com/gin-gonic/gin")
        );

        let mut build_parser = BuildConfigParser::new();
        build_parser
            .scan_project(temp_dir.path(), 0)
            .expect("parse failed");
        let packages = build_parser.packages_for_language(Language::Go);
        assert_eq!(packages.len(), 2);
        assert!(packages.contains("github.com/gin-gonic/gin"));
        assert!(packages.contains("github.com/stretchr/testify"));
    }

    #[test]
    fn test_parse_go_mod_no_file() {
        let temp_dir = TempDir::new().expect("temp dir failed");
        let parser = GoParser;
        let result = parser.try_parse(temp_dir.path(), temp_dir.path()).unwrap();
        assert!(result.is_none());

        let mut build_parser = BuildConfigParser::new();
        build_parser
            .scan_project(temp_dir.path(), 0)
            .expect("parse failed");
        let packages = build_parser.packages_for_language(Language::Go);
        assert!(packages.is_empty());
    }

    #[test]
    fn test_parse_go_mod_with_replace_block() {
        let temp_dir = TempDir::new().expect("temp dir failed");
        let content = r#"module github.com/example/myproject

go 1.21

require (
    github.com/gin-gonic/gin v1.9.1
    github.com/stretchr/testify v1.8.4
)

replace (
    github.com/gin-gonic/gin => ./local/gin
    github.com/stretchr/testify => /absolute/path/to/testify
    golang.org/x/text => golang.org/x/text v0.14.0
)
"#;
        create_test_go_mod(&temp_dir, content);

        let parser = GoParser;
        let result = parser.try_parse(temp_dir.path(), temp_dir.path()).unwrap();
        assert!(result.is_some());
        let outcome = result.unwrap();

        // 2 from require + 2 local replaces (module-to-module replace is skipped)
        assert_eq!(outcome.dependencies.len(), 4);

        // Check local path dependencies exist
        let local_deps: Vec<_> = outcome
            .dependencies
            .iter()
            .filter(|d| d.package_type == "local")
            .collect();
        assert_eq!(local_deps.len(), 2);
        assert!(
            local_deps
                .iter()
                .any(|d| d.name == "github.com/gin-gonic/gin")
        );
        assert!(
            local_deps
                .iter()
                .any(|d| d.name == "github.com/stretchr/testify")
        );
    }
}
