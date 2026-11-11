//! Gradle parser for Java/Kotlin/Groovy projects
//!
//! Dependency extraction runs on tree-sitter: the Groovy DSL
//! (`build.gradle`) parses with the vendored Groovy grammar, the Kotlin DSL
//! (`build.gradle.kts`) with the Kotlin grammar. Version-catalog aliases
//! (`libs.*`) resolve through `gradle/libs.versions.toml`, mirroring the
//! reference implementation. Only Maven coordinates are returned; plugin
//! declarations, local project and file dependencies are skipped.

use std::collections::HashSet;
use std::path::Path;

use super::super::super::error::ConfigParseError;
use super::super::super::types::UntypedDependency;
use cce_parser::gradle::{VersionCatalog, extract_gradle_dependencies};

/// Parse a Gradle build script and return dependencies
pub(crate) fn parse_gradle_file(
    build_gradle: &Path,
    catalog: &VersionCatalog,
) -> Result<HashSet<UntypedDependency>, ConfigParseError> {
    let content = std::fs::read_to_string(build_gradle).map_err(|e| ConfigParseError::Io {
        path: build_gradle.to_path_buf(),
        source: e,
    })?;
    Ok(parse_gradle_content(
        &content,
        is_kotlin_script(build_gradle),
        catalog,
    ))
}

pub(crate) fn is_kotlin_script(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("kts"))
}

pub(crate) fn parse_gradle_content(
    content: &str,
    is_kotlin: bool,
    catalog: &VersionCatalog,
) -> HashSet<UntypedDependency> {
    // The dependency name is the artifact id, matching the Maven parser
    // (`UntypedDependency` carries no version field; the coordinate group
    // only scopes the lookup).
    extract_gradle_dependencies(content, is_kotlin, catalog)
        .into_iter()
        .filter_map(|dep| artifact_name(&dep.name))
        .map(|name| UntypedDependency::new(name, "external"))
        .collect()
}

fn artifact_name(coordinate: &str) -> Option<String> {
    let artifact = coordinate.split(':').nth(1)?.trim();
    if artifact.is_empty() {
        return None;
    }
    Some(artifact.to_string())
}

/// Load the version catalog (`gradle/libs.versions.toml`, with a
/// `libs.versions.toml` fallback) relative to the project root.
///
/// Missing or unparsable catalogs yield an empty table; alias references
/// then resolve to nothing instead of failing the whole parse.
pub(crate) fn load_version_catalog(project_root: &Path) -> VersionCatalog {
    for candidate in ["gradle/libs.versions.toml", "libs.versions.toml"] {
        let path = project_root.join(candidate);
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(value) = toml::from_str::<toml::Value>(&content) else {
            return VersionCatalog::empty();
        };
        return parse_version_catalog(&value);
    }
    VersionCatalog::empty()
}

fn parse_version_catalog(value: &toml::Value) -> VersionCatalog {
    let mut catalog = VersionCatalog::empty();
    let versions = value
        .get("versions")
        .and_then(|v| v.as_table())
        .cloned()
        .unwrap_or_default();
    let Some(libraries) = value.get("libraries").and_then(|v| v.as_table()) else {
        return catalog;
    };
    for (alias, entry) in libraries {
        if let Some(text) = entry.as_str() {
            if let Some((module, version)) = split_coordinate(text) {
                catalog.insert(alias, &module, &version);
            }
            continue;
        }
        let Some(table) = entry.as_table() else {
            continue;
        };
        let module = table
            .get("module")
            .and_then(|v| v.as_str())
            .and_then(split_coordinate)
            .map(|(module, _)| module);
        let Some(module) = module else {
            continue;
        };
        let version = table
            .get("version")
            .map(|v| catalog_version(v, &versions))
            .unwrap_or_default();
        catalog.insert(alias, &module, &version);
    }
    catalog
}

/// Resolve a `[versions]` entry: a bare string, a `{ ref = "..." }`
/// indirection, or an inline table carrying `version`/`require`/`strictly`/
/// `prefer`.
fn catalog_version(entry: &toml::Value, versions: &toml::map::Map<String, toml::Value>) -> String {
    if let Some(text) = entry.as_str() {
        let text = text.trim();
        if let Some(referenced) = versions.get(text) {
            return catalog_version(referenced, versions);
        }
        return text.to_string();
    }
    let Some(table) = entry.as_table() else {
        return String::new();
    };
    for key in ["version", "require", "strictly", "prefer"] {
        if let Some(text) = table.get(key).and_then(|v| v.as_str())
            && !text.trim().is_empty()
        {
            return text.trim().to_string();
        }
    }
    if let Some(name) = table.get("ref").and_then(|v| v.as_str())
        && let Some(referenced) = versions.get(name.trim())
    {
        return catalog_version(referenced, versions);
    }
    String::new()
}

fn split_coordinate(text: &str) -> Option<(String, String)> {
    let mut parts = text.split(':');
    let group = parts.next()?.trim();
    let artifact = parts.next()?.trim();
    if group.is_empty() || artifact.is_empty() {
        return None;
    }
    let version = parts.next().unwrap_or("").trim().to_string();
    Some((format!("{group}:{artifact}"), version))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_parser::detector::BuildConfigParser;
    use crate::config_parser::parsers::java::JavaParser;
    use crate::config_parser::trait_def::LanguageParser;
    use cce_types::Language;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_parse_gradle() {
        let temp_dir = TempDir::new().expect("temp dir failed");
        let gradle = temp_dir.path().join("build.gradle");
        let content = r#"
plugins {
    id 'java'
}

dependencies {
    implementation 'org.springframework:spring-core:6.0.0'
    testImplementation 'junit:junit:4.13.2'
}
"#;
        let mut file = std::fs::File::create(&gradle).expect("create failed");
        file.write_all(content.as_bytes()).expect("write failed");

        let catalog = VersionCatalog::empty();
        let deps = parse_gradle_file(&gradle, &catalog).expect("parse failed");
        assert!(deps.iter().any(|d| d.name == "spring-core"));

        let outcome = JavaParser
            .try_parse(temp_dir.path(), temp_dir.path())
            .unwrap()
            .unwrap();
        assert!(outcome.dependencies.iter().any(|d| d.name == "spring-core"));

        let mut build_parser = BuildConfigParser::new();
        build_parser
            .scan_project(temp_dir.path(), 0)
            .expect("scan failed");
        let packages = build_parser.packages_for_language(Language::Java);
        assert!(packages.contains("spring-core"));
        assert!(packages.contains("junit"));
    }

    #[test]
    fn test_parse_gradle_kts_paren_form() {
        let temp_dir = TempDir::new().expect("temp dir failed");
        let gradle = temp_dir.path().join("build.gradle.kts");
        let content = r#"
plugins {
    kotlin("jvm") version "1.9.0"
}

dependencies {
    implementation("org.jetbrains.kotlin:kotlin-stdlib:1.9.0")
    testImplementation("junit:junit:4.13.2")
}
"#;
        let mut file = std::fs::File::create(&gradle).expect("create failed");
        file.write_all(content.as_bytes()).expect("write failed");

        let catalog = VersionCatalog::empty();
        let deps = parse_gradle_file(&gradle, &catalog).expect("parse failed");
        assert!(deps.iter().any(|d| d.name == "kotlin-stdlib"));
        assert!(deps.iter().any(|d| d.name == "junit"));
    }

    #[test]
    fn test_parse_gradle_version_catalog() {
        let temp_dir = TempDir::new().expect("temp dir failed");
        let gradle_dir = temp_dir.path().join("gradle");
        std::fs::create_dir(&gradle_dir).expect("create dir failed");
        std::fs::write(
            gradle_dir.join("libs.versions.toml"),
            "[versions]\nspring = \"6.0.0\"\n[libraries]\nspring-core = { module = \"org.springframework:spring-core\", version.ref = \"spring\" }\n",
        )
        .expect("write catalog failed");
        let catalog = load_version_catalog(temp_dir.path());

        let deps = parse_gradle_content(
            "dependencies {\n    implementation libs.spring.core\n}\n",
            false,
            &catalog,
        );
        assert!(deps.iter().any(|d| d.name == "spring-core"));
    }
}
