//! C/C++ build system parsers

pub mod cmake;

use std::path::Path;

use cce_types::language::Language;

use super::super::error::ConfigParseError;
use super::super::trait_def::{LanguageParser, ParseOutcome};

/// CMake parser for C/C++
pub struct CMakeParser;

impl LanguageParser for CMakeParser {
    fn try_parse(
        &self,
        project_root: &Path,
        dir: &Path,
    ) -> Result<Option<ParseOutcome>, ConfigParseError> {
        let cmake = dir.join("CMakeLists.txt");
        if !cmake.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(&cmake).map_err(|e| ConfigParseError::Io {
            path: cmake.clone(),
            source: e,
        })?;

        let dependencies = cmake::parse_cmake_content(&content);
        if dependencies.is_empty() {
            return Ok(None);
        }
        let rel = cce_types::path::relativize(project_root, &cmake);
        Ok(Some(ParseOutcome {
            dependencies,
            config_file: rel,
        }))
    }

    fn supported_languages(&self) -> Vec<Language> {
        vec![Language::C, Language::Cpp]
    }

    fn supported_config_files(&self) -> &[&str] {
        &["CMakeLists.txt"]
    }

    fn name(&self) -> &str {
        "CMake"
    }
}
