//! Makefile parser for C/C++ projects (simplified)
//!
//! Extracts dependency information used for import classification:
//! - `-l<name>` linker flags → external libraries
//! - `pkg-config --libs <pkgs>` entries → external packages
//! - `include` / `-include` / `sinclude` directives → local makefile fragments
//! - Rule targets / prerequisites that reference other targets in the same
//!   file → internal target dependencies

use std::collections::HashSet;
use std::path::Path;

use cce_types::language::Language;

use super::super::error::ConfigParseError;
use super::super::trait_def::{LanguageParser, ParseOutcome};
use super::super::types::UntypedDependency;

/// Candidate makefile names in GNU make's resolution order.
const MAKEFILE_NAMES: &[&str] = &["GNUmakefile", "makefile", "Makefile"];

/// Make parser for C/C++
pub struct MakeParser;

impl LanguageParser for MakeParser {
    fn try_parse(
        &self,
        project_root: &Path,
        dir: &Path,
    ) -> Result<Option<ParseOutcome>, ConfigParseError> {
        let Some(makefile) = MAKEFILE_NAMES
            .iter()
            .map(|name| dir.join(name))
            .find(|path| path.is_file())
        else {
            return Ok(None);
        };

        let content = std::fs::read_to_string(&makefile).map_err(|e| ConfigParseError::Io {
            path: makefile.clone(),
            source: e,
        })?;

        let dependencies = parse_makefile_content(&content);
        if dependencies.is_empty() {
            return Ok(None);
        }
        let rel = cce_types::path::relativize(project_root, &makefile);
        Ok(Some(ParseOutcome {
            dependencies,
            config_file: rel,
        }))
    }

    fn supported_languages(&self) -> Vec<Language> {
        vec![Language::C, Language::Cpp]
    }

    fn supported_config_files(&self) -> &[&str] {
        &["GNUmakefile", "makefile", "Makefile"]
    }

    fn name(&self) -> &str {
        "Make"
    }
}

/// Parse makefile content into a dependency set.
fn parse_makefile_content(content: &str) -> HashSet<UntypedDependency> {
    let mut dependencies = HashSet::new();
    let mut rule_targets: HashSet<String> = HashSet::new();
    let mut rule_prereqs: Vec<String> = Vec::new();

    for line in join_continuations(content.lines()).lines() {
        if line.starts_with('\t') {
            continue;
        }
        let line = strip_comment(line).trim().to_string();
        if line.is_empty() {
            continue;
        }

        let mut tokens = line.split_whitespace();
        match tokens.next() {
            Some("include") | Some("-include") | Some("sinclude") => {
                for token in tokens {
                    let name = plain_name(token);
                    if is_dependency_like(&name) {
                        dependencies.insert(UntypedDependency::new(name, "local"));
                    }
                }
                continue;
            }
            _ => {}
        }

        if let Some((targets, prereqs)) = split_rule(&line) {
            for target in targets {
                if !target.contains('%') && is_dependency_like(&target) {
                    rule_targets.insert(target);
                }
            }
            for prereq in prereqs {
                if is_dependency_like(&prereq) {
                    rule_prereqs.push(prereq);
                }
            }
        }

        collect_link_libraries(&line, &mut dependencies);
    }

    for prereq in &rule_prereqs {
        if rule_targets.contains(prereq) {
            dependencies.insert(UntypedDependency::new(prereq.clone(), "local"));
        }
    }

    dependencies
}

fn join_continuations<'a>(lines: impl Iterator<Item = &'a str>) -> String {
    let mut joined = String::new();
    let mut pending = String::new();
    for line in lines {
        let trimmed_end = line.trim_end();
        if trimmed_end.ends_with('\\') {
            pending.push_str(trimmed_end.trim_end_matches('\\'));
            pending.push(' ');
        } else {
            pending.push_str(line);
            joined.push_str(&pending);
            joined.push('\n');
            pending.clear();
        }
    }
    joined.push_str(&pending);
    joined
}

fn strip_comment(line: &str) -> &str {
    match line.find('#') {
        Some(idx) => &line[..idx],
        None => line,
    }
}

fn split_rule(line: &str) -> Option<(Vec<String>, Vec<String>)> {
    let colon = line.find(':')?;
    let (targets_part, rest) = line.split_at(colon);
    let after_colon = rest.trim_start_matches(':');
    if after_colon.starts_with(['=', '?', '+']) {
        return None;
    }
    let prereqs: Vec<String> = after_colon
        .split_whitespace()
        .filter(|t| !t.starts_with('$'))
        .map(expand_basename)
        .collect();
    let targets: Vec<String> = targets_part
        .split_whitespace()
        .filter(|t| !t.starts_with('$'))
        .map(expand_basename)
        .collect();
    if targets.is_empty() {
        return None;
    }
    Some((targets, prereqs))
}

fn collect_link_libraries(line: &str, dependencies: &mut HashSet<UntypedDependency>) {
    for token in line.split_whitespace() {
        if let Some(name) = token.strip_prefix("-l") {
            let name = name.trim_end_matches([')', ';']);
            if is_dependency_like(name) {
                dependencies.insert(UntypedDependency::new(name, "external"));
            }
        }
    }

    if let Some(idx) = line.find("pkg-config") {
        for token in line[idx..].split_whitespace() {
            if token == "pkg-config" || token.starts_with('-') || token.contains('$') {
                continue;
            }
            let name = token.trim_end_matches([')', ';', '"']);
            if is_dependency_like(name) {
                dependencies.insert(UntypedDependency::new(name, "external"));
            }
        }
    }
}

fn expand_basename(token: &str) -> String {
    Path::new(token)
        .file_name()
        .and_then(|n| n.to_str())
        .map(str::to_string)
        .unwrap_or_else(|| token.to_string())
}

fn plain_name(token: &str) -> String {
    if token.contains('$') {
        return String::new();
    }
    expand_basename(token.trim_matches(['"', '\'']))
}

fn is_dependency_like(name: &str) -> bool {
    !name.is_empty() && name.chars().any(|c| c.is_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_parser::detector::BuildConfigParser;
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn write_file(dir: &TempDir, name: &str, content: &str) -> PathBuf {
        let path = dir.path().join(name);
        let mut file = std::fs::File::create(&path).expect("create failed");
        file.write_all(content.as_bytes()).expect("write failed");
        path
    }

    #[test]
    fn test_parse_link_flags_and_pkg_config() {
        let content = r#"
LDFLAGS = -lpthread -lssl # comment
LIBS := $(shell pkg-config --libs openssl sqlite3)
all: app
"#;
        let deps = parse_makefile_content(content);
        assert!(deps.contains(&UntypedDependency::new("pthread", "external")));
        assert!(deps.contains(&UntypedDependency::new("ssl", "external")));
        assert!(deps.contains(&UntypedDependency::new("openssl", "external")));
        assert!(deps.contains(&UntypedDependency::new("sqlite3", "external")));
    }

    #[test]
    fn test_parse_include_directives() {
        let content = r#"
include common.mk
-include src/local.mk rules.mk
sinclude generated.inc
"#;
        let deps = parse_makefile_content(content);
        assert!(deps.contains(&UntypedDependency::new("common.mk", "local")));
        assert!(deps.contains(&UntypedDependency::new("local.mk", "local")));
        assert!(deps.contains(&UntypedDependency::new("rules.mk", "local")));
        assert!(deps.contains(&UntypedDependency::new("generated.inc", "local")));
    }

    #[test]
    fn test_parse_internal_target_dependencies() {
        let content = r#"
app: main.o util.o
main.o: main.c
util.o: util.c
"#;
        let deps = parse_makefile_content(content);
        assert!(deps.contains(&UntypedDependency::new("main.o", "local")));
        assert!(deps.contains(&UntypedDependency::new("util.o", "local")));
        assert!(!deps.contains(&UntypedDependency::new("main.c", "local")));
    }

    #[test]
    fn test_variable_assignments_are_not_rules() {
        let content = r#"
CC := gcc
CFLAGS ?= -O2
SRCS += main.c
"#;
        let deps = parse_makefile_content(content);
        assert!(deps.is_empty());
    }

    #[test]
    fn test_make_parser_missing_file() {
        let temp_dir = TempDir::new().expect("temp dir failed");
        let parser = MakeParser;
        let result = parser.try_parse(temp_dir.path(), temp_dir.path()).unwrap();
        assert!(result.is_none());

        let mut build_parser = BuildConfigParser::new();
        build_parser
            .scan_project(temp_dir.path(), 0)
            .expect("missing makefile must not error");
        assert!(build_parser.packages_for_language(Language::C).is_empty());
    }

    #[test]
    fn test_make_parser_trait() {
        let parser = MakeParser;
        assert_eq!(parser.name(), "Make");
        assert_eq!(
            parser.supported_languages(),
            vec![Language::C, Language::Cpp]
        );
        assert!(parser.supported_config_files().contains(&"Makefile"));
    }

    #[test]
    fn test_try_parse_makefile_populates_c_and_cpp() {
        let temp_dir = TempDir::new().expect("temp dir failed");
        write_file(
            &temp_dir,
            "GNUmakefile",
            "libs: -lzlib\n\tgcc -o libs libs.c\n",
        );

        let parser = MakeParser;
        let outcome = parser
            .try_parse(temp_dir.path(), temp_dir.path())
            .unwrap()
            .unwrap();
        assert!(
            outcome
                .dependencies
                .contains(&UntypedDependency::new("zlib", "external"))
        );

        let mut build_parser = BuildConfigParser::new();
        build_parser
            .scan_project(temp_dir.path(), 0)
            .expect("parse failed");
        for lang in [Language::C, Language::Cpp] {
            let packages = build_parser.packages_for_language(lang);
            assert!(
                packages.contains("zlib"),
                "{lang:?} should know zlib from -lzlib"
            );
        }
    }
}
