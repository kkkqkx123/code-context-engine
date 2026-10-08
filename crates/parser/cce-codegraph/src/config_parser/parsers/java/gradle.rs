//! Gradle parser for Java projects

use std::collections::HashSet;
use std::path::Path;

use super::super::super::error::ConfigParseError;
use super::super::super::types::UntypedDependency;
use regex::Regex;

/// Parse build.gradle file and return dependencies
pub(crate) fn parse_gradle_file(
    build_gradle: &Path,
) -> Result<HashSet<UntypedDependency>, ConfigParseError> {
    let content = std::fs::read_to_string(build_gradle).map_err(|e| ConfigParseError::Io {
        path: build_gradle.to_path_buf(),
        source: e,
    })?;
    Ok(parse_gradle_content(&content))
}

pub(crate) fn parse_gradle_content(content: &str) -> HashSet<UntypedDependency> {
    let mut dependencies = HashSet::new();

    let re = Regex::new(r#"(?:implementation|api|compileOnly|runtimeOnly|testImplementation|testCompileOnly|testRuntimeOnly)\s*['"]([^'"]+)['"]"#).unwrap();

    for caps in re.captures_iter(content) {
        let dep_str = &caps[1];
        let parts: Vec<&str> = dep_str.split(':').collect();
        if parts.len() >= 2 {
            dependencies.insert(UntypedDependency::new(parts[1], "external"));
        } else {
            dependencies.insert(UntypedDependency::new(dep_str, "external"));
        }
    }

    dependencies
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

        let deps = parse_gradle_file(&gradle).expect("parse failed");
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
}
