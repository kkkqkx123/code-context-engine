//! Bundler parser for Ruby projects

use std::collections::HashSet;
use std::path::Path;

use cce_types::language::Language;

use super::super::error::ConfigParseError;
use super::super::trait_def::{LanguageParser, ParseOutcome};
use super::super::types::UntypedDependency;

/// Bundler parser for Ruby
pub struct BundlerParser;

impl LanguageParser for BundlerParser {
    fn try_parse(
        &self,
        project_root: &Path,
        dir: &Path,
    ) -> Result<Option<ParseOutcome>, ConfigParseError> {
        let gemfile = dir.join("Gemfile");
        if !gemfile.exists() {
            return Ok(None);
        }

        let lockfile = dir.join("Gemfile.lock");

        let (dependencies, source_path) = if lockfile.exists() {
            (parse_gemfile_lock(&lockfile)?, lockfile)
        } else {
            (parse_gemfile_fallback(&gemfile)?, gemfile)
        };

        if dependencies.is_empty() {
            return Ok(None);
        }

        let rel = cce_types::path::relativize(project_root, &source_path);
        Ok(Some(ParseOutcome {
            dependencies,
            config_file: rel,
        }))
    }

    fn supported_languages(&self) -> Vec<Language> {
        vec![Language::Ruby]
    }

    fn supported_config_files(&self) -> &[&str] {
        &["Gemfile", "Gemfile.lock"]
    }

    fn name(&self) -> &str {
        "Bundler"
    }
}

fn parse_gemfile_lock(
    lockfile_path: &Path,
) -> Result<HashSet<UntypedDependency>, ConfigParseError> {
    let content = std::fs::read_to_string(lockfile_path).map_err(|e| ConfigParseError::Io {
        path: lockfile_path.to_path_buf(),
        source: e,
    })?;

    let mut dependencies = HashSet::new();
    let mut in_specs = false;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed == "specs:" {
            in_specs = true;
            continue;
        }

        if in_specs
            && !trimmed.is_empty()
            && !trimmed.starts_with('#')
            && line.starts_with("    ")
            && trimmed.contains('(')
        {
            if let Some(name) = parse_gem_spec_line(trimmed) {
                dependencies.insert(UntypedDependency::new(name, "external"));
            }
        }
    }

    Ok(dependencies)
}

fn parse_gemfile_fallback(
    gemfile_path: &Path,
) -> Result<HashSet<UntypedDependency>, ConfigParseError> {
    let content = std::fs::read_to_string(gemfile_path).map_err(|e| ConfigParseError::Io {
        path: gemfile_path.to_path_buf(),
        source: e,
    })?;

    let mut dependencies = HashSet::new();

    for line in content.lines() {
        let line = line.trim();

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if line.starts_with("gem ") {
            if let Some(name) = parse_gem_declaration(line) {
                dependencies.insert(UntypedDependency::new(name, "external"));
            }
        }
    }

    Ok(dependencies)
}

fn parse_gem_declaration(line: &str) -> Option<String> {
    let gem_content = line[4..].trim();
    let name = if let Some(name_end) =
        gem_content.find(|c| c == ',' || c == ' ' && gem_content.contains(','))
    {
        gem_content[..name_end].trim()
    } else {
        gem_content
    };

    let name = name.trim_matches(['\'', '"']).to_string();
    if name.is_empty() { None } else { Some(name) }
}

fn parse_gem_spec_line(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if let Some(paren_start) = trimmed.find('(') {
        let name = trimmed[..paren_start].trim().to_string();
        if !name.is_empty() {
            return Some(name);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_parser::detector::BuildConfigParser;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_gemfile(dir: &TempDir, content: &str) {
        let gemfile = dir.path().join("Gemfile");
        let mut file = std::fs::File::create(&gemfile).expect("create failed");
        file.write_all(content.as_bytes()).expect("write failed");
    }

    #[test]
    fn test_parse_gemfile_fallback() {
        let temp_dir = TempDir::new().expect("temp dir failed");
        let gemfile_content = r#"source 'https://rubygems.org'

gem 'rails', '~> 7.0'
gem 'rspec', '~> 3.0', group: :test

group :development do
  gem 'pry'
  gem 'byebug'
end

gem 'nokogiri', '>= 1.0', '< 2.0'"#;
        create_test_gemfile(&temp_dir, gemfile_content);

        let parser = BundlerParser;
        let outcome = parser
            .try_parse(temp_dir.path(), temp_dir.path())
            .unwrap()
            .unwrap();
        assert!(outcome.dependencies.iter().any(|d| d.name == "rails"));

        let mut build_parser = BuildConfigParser::new();
        build_parser
            .scan_project(temp_dir.path(), 0)
            .expect("parse failed");
        let packages = build_parser.packages_for_language(Language::Ruby);
        assert!(!packages.is_empty());
        assert!(packages.contains("rails"));
        assert!(packages.contains("rspec"));
        assert!(packages.contains("pry"));
        assert!(packages.contains("byebug"));
        assert!(packages.contains("nokogiri"));
    }

    #[test]
    fn test_parse_gemfile_no_file() {
        let temp_dir = TempDir::new().expect("temp dir failed");
        let parser = BundlerParser;
        let result = parser.try_parse(temp_dir.path(), temp_dir.path()).unwrap();
        assert!(result.is_none());

        let mut build_parser = BuildConfigParser::new();
        build_parser
            .scan_project(temp_dir.path(), 0)
            .expect("parse failed");
        let packages = build_parser.packages_for_language(Language::Ruby);
        assert!(packages.is_empty());
    }

    #[test]
    fn test_bundler_parser_trait() {
        let parser = BundlerParser;
        assert_eq!(parser.name(), "Bundler");
        assert_eq!(parser.supported_languages(), vec![Language::Ruby]);
        assert!(parser.supported_config_files().contains(&"Gemfile"));
    }
}
