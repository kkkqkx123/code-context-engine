//! Java build system parsers

pub mod gradle;
pub mod maven;

use std::path::Path;

use cce_types::Language;

use super::super::error::ConfigParseError;
use super::super::trait_def::{LanguageParser, ParseOutcome};

/// Java parser handling Maven and Gradle
pub struct JavaParser;

impl LanguageParser for JavaParser {
    fn try_parse(
        &self,
        project_root: &Path,
        dir: &Path,
    ) -> Result<Option<ParseOutcome>, ConfigParseError> {
        let mut errors = Vec::new();

        let pom_xml = dir.join("pom.xml");
        if pom_xml.exists() {
            match maven::parse_maven_file(&pom_xml) {
                Ok(deps) => {
                    if !deps.is_empty() {
                        let rel = cce_types::path::relativize(project_root, &pom_xml);
                        return Ok(Some(ParseOutcome {
                            dependencies: deps,
                            config_file: rel,
                        }));
                    } else {
                        return Ok(None);
                    }
                }
                Err(e) => {
                    errors.push(e);
                }
            }
        }

        let build_gradle = dir.join("build.gradle");
        let build_gradle_kts = dir.join("build.gradle.kts");
        let settings_gradle = dir.join("settings.gradle");
        let settings_gradle_kts = dir.join("settings.gradle.kts");

        // Both dialects can coexist (e.g. a Groovy root script with Kotlin
        // subproject scripts); their dependencies union. Settings scripts
        // only act as a fallback when no build script declares anything.
        let build_scripts: Vec<_> = [&build_gradle_kts, &build_gradle]
            .into_iter()
            .filter(|p| p.exists())
            .collect();
        let fallback_scripts: Vec<_> = [&settings_gradle_kts, &settings_gradle]
            .into_iter()
            .filter(|p| p.exists())
            .collect();

        if !build_scripts.is_empty() || !fallback_scripts.is_empty() {
            let catalog = gradle::load_version_catalog(project_root);
            let scripts = if build_scripts.is_empty() {
                fallback_scripts
            } else {
                build_scripts
            };
            let mut union = std::collections::HashSet::new();
            for script in &scripts {
                match gradle::parse_gradle_file(script, &catalog) {
                    Ok(deps) => union.extend(deps),
                    Err(e) => errors.push(e),
                }
            }
            if !union.is_empty() {
                let rel = cce_types::path::relativize(project_root, scripts[0]);
                return Ok(Some(ParseOutcome {
                    dependencies: union,
                    config_file: rel,
                }));
            } else if errors.is_empty() {
                return Ok(None);
            }
        }

        if !errors.is_empty() {
            return Err(ConfigParseError::multiple(errors));
        }

        Ok(None)
    }

    fn supported_languages(&self) -> Vec<Language> {
        vec![Language::Java]
    }

    fn supported_config_files(&self) -> &[&str] {
        &[
            "pom.xml",
            "build.gradle",
            "build.gradle.kts",
            "settings.gradle",
            "settings.gradle.kts",
        ]
    }

    fn name(&self) -> &str {
        "Maven/Gradle"
    }
}
