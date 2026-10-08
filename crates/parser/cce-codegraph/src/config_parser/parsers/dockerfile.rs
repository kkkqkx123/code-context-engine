//! Dockerfile parser (simplified)
//!
//! Extracts dependency information for build-stage relations:
//! - `FROM <image> [AS <stage>]` → external base image, stage alias recorded
//! - `COPY --from=<source>` → local edge when `source` is a known stage,
//!   otherwise an external image reference
//!
//! The parsed dependencies are stored under [`Language::Unknown`]: Docker has
//! no source language, so the entries participate in config reloads and stay
//! available for future tooling without polluting per-language import
//! classification.

use std::collections::HashSet;
use std::path::Path;

use cce_types::language::Language;

use super::super::error::ConfigParseError;
use super::super::trait_def::{LanguageParser, ParseOutcome};
use super::super::types::UntypedDependency;

/// Candidate Dockerfile names (both common casings).
const DOCKERFILE_NAMES: &[&str] = &["Dockerfile", "dockerfile"];

/// Dockerfile parser
pub struct DockerParser;

impl LanguageParser for DockerParser {
    fn try_parse(
        &self,
        project_root: &Path,
        dir: &Path,
    ) -> Result<Option<ParseOutcome>, ConfigParseError> {
        let Some(dockerfile) = DOCKERFILE_NAMES
            .iter()
            .map(|name| dir.join(name))
            .find(|path| path.is_file())
        else {
            return Ok(None);
        };

        let content = std::fs::read_to_string(&dockerfile).map_err(|e| ConfigParseError::Io {
            path: dockerfile.clone(),
            source: e,
        })?;

        let dependencies = parse_dockerfile_content(&content);
        if dependencies.is_empty() {
            return Ok(None);
        }
        let rel = cce_types::path::relativize(project_root, &dockerfile);
        Ok(Some(ParseOutcome {
            dependencies,
            config_file: rel,
        }))
    }

    fn supported_languages(&self) -> Vec<Language> {
        vec![Language::Unknown]
    }

    fn supported_config_files(&self) -> &[&str] {
        &["Dockerfile", "dockerfile"]
    }

    fn name(&self) -> &str {
        "Docker"
    }
}

/// Parse Dockerfile content into a dependency set.
fn parse_dockerfile_content(content: &str) -> HashSet<UntypedDependency> {
    let mut dependencies = HashSet::new();
    let mut stages: HashSet<String> = HashSet::new();

    for line in content.lines() {
        let line = strip_comment(line);
        if instruction_arg(line, "FROM").is_some()
            && let Some(stage) = stage_alias(line)
        {
            stages.insert(stage);
        }
    }

    for line in content.lines() {
        let line = strip_comment(line);

        if let Some(image) = instruction_arg(line, "FROM") {
            if image.eq_ignore_ascii_case("scratch") {
                continue;
            }
            let name = normalize_image(&image);
            if is_dependency_like(&name) {
                dependencies.insert(UntypedDependency::new(name, "external"));
            }
            continue;
        }

        if let Some(from) = copy_from_source(line) {
            if stages.contains(&from) {
                dependencies.insert(UntypedDependency::new(from, "local"));
            } else {
                let name = normalize_image(&from);
                if is_dependency_like(&name) {
                    dependencies.insert(UntypedDependency::new(name, "external"));
                }
            }
        }
    }

    dependencies
}

fn strip_comment(line: &str) -> &str {
    match line.find('#') {
        Some(idx) => &line[..idx],
        None => line,
    }
}

fn instruction_arg(line: &str, keyword: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let rest = trimmed.strip_prefix(keyword)?;
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let arg = rest
        .split_whitespace()
        .find(|token| !token.starts_with("--"))?;
    Some(arg.to_string())
}

fn stage_alias(line: &str) -> Option<String> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    let pos = tokens.iter().position(|t| t.eq_ignore_ascii_case("AS"))?;
    tokens.get(pos + 1).map(|s| s.to_string())
}

fn copy_from_source(line: &str) -> Option<String> {
    let flag = line.split_whitespace().find(|t| t.starts_with("--from="))?;
    Some(flag["--from=".len()..].to_string())
}

fn normalize_image(image: &str) -> String {
    let without_digest = image.split('@').next().unwrap_or(image);
    match without_digest.rfind(':') {
        Some(idx) if !without_digest[idx + 1..].contains('/') => without_digest[..idx].to_string(),
        _ => without_digest.to_string(),
    }
}

fn is_dependency_like(name: &str) -> bool {
    !name.is_empty() && name.chars().any(|c| c.is_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_parser::detector::BuildConfigParser;
    use std::io::Write;
    use tempfile::TempDir;

    fn write_dockerfile(dir: &TempDir, content: &str) {
        let path = dir.path().join("Dockerfile");
        let mut file = std::fs::File::create(&path).expect("create failed");
        file.write_all(content.as_bytes()).expect("write failed");
    }

    #[test]
    fn test_parse_from_images() {
        let content = r#"
FROM ubuntu:22.04 AS base
FROM rust:1.86@sha256:abc123 AS builder
# FROM commented-out:latest
"#;
        let deps = parse_dockerfile_content(content);
        assert!(deps.contains(&UntypedDependency::new("ubuntu", "external")));
        assert!(deps.contains(&UntypedDependency::new("rust", "external")));
        assert!(!deps.contains(&UntypedDependency::new("commented-out", "external")));
    }

    #[test]
    fn test_parse_copy_from_stage_and_image() {
        let content = r#"
FROM alpine:3.19 AS build
FROM scratch
COPY --from=build /app/bin /usr/local/bin
COPY --from=busybox:1.36 /bin/busybox /bin/sh
"#;
        let deps = parse_dockerfile_content(content);
        assert!(deps.contains(&UntypedDependency::new("build", "local")));
        assert!(deps.contains(&UntypedDependency::new("busybox", "external")));
    }

    #[test]
    fn test_registry_port_is_preserved() {
        assert_eq!(
            normalize_image("registry:5000/team/app:v2"),
            "registry:5000/team/app"
        );
        assert_eq!(normalize_image("ubuntu:22.04"), "ubuntu");
        assert_eq!(normalize_image("ubuntu"), "ubuntu");
    }

    #[test]
    fn test_docker_parser_trait() {
        let parser = DockerParser;
        assert_eq!(parser.name(), "Docker");
        assert_eq!(parser.supported_languages(), vec![Language::Unknown]);
        assert_eq!(
            parser.supported_config_files(),
            &["Dockerfile", "dockerfile"]
        );
        let temp_dir = TempDir::new().unwrap();
        let result = parser.try_parse(temp_dir.path(), temp_dir.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_try_parse_dockerfile_populates_unknown_bucket() {
        let temp_dir = TempDir::new().expect("temp dir failed");
        write_dockerfile(&temp_dir, "FROM debian:12\n");

        let parser = DockerParser;
        let outcome = parser
            .try_parse(temp_dir.path(), temp_dir.path())
            .unwrap()
            .unwrap();
        assert!(outcome.dependencies.iter().any(|d| d.name == "debian"));

        let mut build_parser = BuildConfigParser::new();
        build_parser
            .scan_project(temp_dir.path(), 0)
            .expect("parse failed");
        let packages = build_parser.packages_for_language(Language::Unknown);
        assert!(packages.contains("debian"));
    }
}
