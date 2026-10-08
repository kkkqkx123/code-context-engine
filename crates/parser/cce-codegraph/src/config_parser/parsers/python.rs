//! Python build system parsers

mod common;
mod conda;
mod flit;
mod hatch;
mod pip;
mod pipenv;
mod poetry;
mod setup_py;
mod setuptools;

use std::path::Path;

use cce_types::Language;

use super::super::detector::BuildConfigParser;
use super::super::error::ConfigParseError;
use super::super::trait_def::{LanguageParser, ParseOutcome};

impl BuildConfigParser {
    /// Main entry point for Python build system detection
    ///
    /// Tries all available Python build config files and merges dependencies.
    /// Reads pyproject.toml once at the start to avoid redundant I/O.
    /// Does not short-circuit on first success - all config files are tried.
    pub(crate) fn try_parse_python(
        &mut self,
        project_root: &Path,
        dir: &Path,
    ) -> Result<(), ConfigParseError> {
        let mut errors = Vec::new();
        let mut any_success = false;

        // === pyproject.toml (modern Python projects) ===
        let pyproject_toml = dir.join("pyproject.toml");
        let pyproject_content = if pyproject_toml.exists() {
            match std::fs::read_to_string(&pyproject_toml) {
                Ok(c) => Some(c),
                Err(e) => {
                    errors.push(ConfigParseError::Io {
                        path: pyproject_toml.clone(),
                        source: e,
                    });
                    None
                }
            }
        } else {
            None
        };

        // Try pyproject.toml-based parsers based on content patterns
        if let Some(ref content) = pyproject_content {
            // Try all matching backends (not just the first match)
            if content.contains("[tool.poetry]") {
                match self.try_parse_poetry(project_root, &pyproject_toml, content) {
                    Ok(()) => any_success = true,
                    Err(e) => errors.push(e),
                }
            }
            if content.contains("[tool.flit]") || content.contains("[project]") {
                match self.try_parse_flit(project_root, &pyproject_toml, content) {
                    Ok(()) => any_success = true,
                    Err(e) => errors.push(e),
                }
            }
            if content.contains("[tool.hatch]")
                || content.contains("hatchling")
                || content.contains("[project]")
            {
                match self.try_parse_hatch(project_root, &pyproject_toml, content) {
                    Ok(()) => any_success = true,
                    Err(e) => errors.push(e),
                }
            }
        }

        // === Legacy config files (all tried regardless of pyproject.toml) ===

        // setup.py
        let setup_py = dir.join("setup.py");
        if setup_py.exists() {
            match self.try_parse_setup_py(project_root, &setup_py) {
                Ok(()) => any_success = true,
                Err(e) => errors.push(e),
            }
        }

        // setup.cfg
        let setup_cfg = dir.join("setup.cfg");
        if setup_cfg.exists() {
            match self.try_parse_setuptools(project_root, &setup_cfg) {
                Ok(()) => any_success = true,
                Err(e) => errors.push(e),
            }
        }

        // Pipfile
        let pipfile = dir.join("Pipfile");
        if pipfile.exists() {
            match self.try_parse_pipenv(project_root, &pipfile) {
                Ok(()) => any_success = true,
                Err(e) => errors.push(e),
            }
        }

        // requirements.txt
        let requirements_txt = dir.join("requirements.txt");
        if requirements_txt.exists() {
            match self.try_parse_pip(project_root, &requirements_txt) {
                Ok(()) => any_success = true,
                Err(e) => errors.push(e),
            }
        }

        // environment.yml
        let environment_yml = dir.join("environment.yml");
        if environment_yml.exists() {
            match self.try_parse_conda(project_root, &environment_yml) {
                Ok(()) => any_success = true,
                Err(e) => errors.push(e),
            }
        }

        // If all parsers failed and we have errors, return them
        if !any_success && !errors.is_empty() {
            return Err(ConfigParseError::multiple(errors));
        }

        Ok(())
    }
}

/// Python parser handling multiple build files (requirements.txt, pyproject.toml, etc.)
pub struct PythonParser;

impl LanguageParser for PythonParser {
    fn try_parse(
        &self,
        project_root: &Path,
        dir: &Path,
    ) -> Result<Option<ParseOutcome>, ConfigParseError> {
        // Delegate to existing BuildConfigParser logic via temporary instance
        let mut temp = BuildConfigParser::new();
        temp.try_parse_python(project_root, dir)?;

        let deps = temp.dependencies_for_language(Language::Python);
        if deps.is_empty() {
            return Ok(None);
        }

        // Pick first config file as representative for per-file tracking
        let config_file = temp
            .config_file_dependencies()
            .keys()
            .next()
            .cloned()
            .unwrap_or_else(|| {
                // Fallback: find first existing python file
                for name in self.supported_config_files() {
                    let p = dir.join(name);
                    if p.exists() {
                        return cce_types::path::relativize(project_root, &p);
                    }
                }
                String::new()
            });

        Ok(Some(ParseOutcome {
            dependencies: deps,
            config_file,
        }))
    }

    fn supported_languages(&self) -> Vec<Language> {
        vec![Language::Python]
    }

    fn supported_config_files(&self) -> &[&str] {
        &[
            "requirements.txt",
            "pyproject.toml",
            "setup.py",
            "setup.cfg",
            "Pipfile",
            "environment.yml",
        ]
    }

    fn name(&self) -> &str {
        "PyPI"
    }
}
