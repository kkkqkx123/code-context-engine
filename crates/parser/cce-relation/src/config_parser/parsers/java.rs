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

        let is_gradle_project = build_gradle.exists()
            || build_gradle_kts.exists()
            || settings_gradle.exists()
            || settings_gradle_kts.exists();

        if is_gradle_project {
            let config_path = if build_gradle_kts.exists() {
                build_gradle_kts
            } else if build_gradle.exists() {
                build_gradle
            } else if settings_gradle_kts.exists() {
                settings_gradle_kts
            } else {
                settings_gradle
            };

            match gradle::parse_gradle_file(&config_path) {
                Ok(deps) => {
                    if !deps.is_empty() {
                        let rel = cce_types::path::relativize(project_root, &config_path);
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
