//! CMake parser for C/C++ projects
//!
//! Extracts dependency information from the commands that carry dependency
//! semantics, distinguishing internal (project-defined) from external deps:
//!
//! | Command | Extraction |
//! |---------|-----------|
//! | `find_package()` / `find_library()` | external package/library names |
//! | `target_link_libraries()` | link items; items matching a locally defined target are internal |
//! | `add_subdirectory()` | internal sub-project directories |
//! | `include_directories()` | internal header search paths (path segments only) |

use std::collections::HashSet;

use super::super::super::types::UntypedDependency;
use regex::Regex;

/// Visibility/scope keywords in `target_link_libraries` that are never libs.
const LINK_SCOPE_KEYWORDS: &[&str] = &[
    "PUBLIC",
    "PRIVATE",
    "INTERFACE",
    "LINK_PUBLIC",
    "LINK_PRIVATE",
    "LINK_INTERFACE_LIBRARIES",
    "debug",
    "optimized",
    "general",
];

/// Parse CMake content for dependencies (standalone, no BuildConfigParser)
pub(crate) fn parse_cmake_content(content: &str) -> HashSet<UntypedDependency> {
    let body = strip_cmake_comments(content);
    let mut dependencies = HashSet::new();

    let local_targets = command_args(&body, r"(?:add_library|add_executable)")
        .iter()
        .filter_map(|args| args.first().cloned())
        .collect::<HashSet<String>>();

    for args in command_args(&body, r"find_package") {
        if let Some(name) = args.first() {
            dependencies.insert(UntypedDependency::new(strip_namespace(name), "external"));
        }
    }

    for args in command_args(&body, r"find_library") {
        if let Some(name) = args.get(1) {
            if !name.starts_with(['$', '<']) {
                dependencies.insert(UntypedDependency::new(strip_namespace(name), "external"));
            }
        }
    }

    for args in command_args(&body, r"add_subdirectory") {
        if let Some(dir) = args.first() {
            if let Some(segment) = plain_path_segment(dir) {
                dependencies.insert(UntypedDependency::new(segment, "local"));
            }
        }
    }

    for args in command_args(&body, r"include_directories") {
        for arg in &args {
            if let Some(segment) = plain_path_segment(arg) {
                dependencies.insert(UntypedDependency::new(segment, "local"));
            }
        }
    }

    for args in command_args(&body, r"target_link_libraries") {
        for item in args.iter().skip(1) {
            if LINK_SCOPE_KEYWORDS.contains(&item.as_str()) || item.starts_with('$') {
                continue;
            }
            if local_targets.contains(item) {
                dependencies.insert(UntypedDependency::new(item.clone(), "local"));
            } else {
                dependencies.insert(UntypedDependency::new(strip_namespace(item), "external"));
            }
        }
    }

    dependencies.retain(|dep| !dep.name.is_empty());
    dependencies
}

fn command_args(body: &str, command: &str) -> Vec<Vec<String>> {
    let re = Regex::new(&format!(r"(?s)\b{command}\s*\(([^)]*)\)")).expect("valid regex");
    re.captures_iter(body)
        .map(|caps| {
            caps[1]
                .split_whitespace()
                .map(|arg| arg.trim_matches('"').to_string())
                .filter(|arg| !arg.is_empty())
                .collect::<Vec<String>>()
        })
        .collect()
}

fn strip_cmake_comments(content: &str) -> String {
    content
        .lines()
        .map(|line| match line.find('#') {
            Some(idx) => &line[..idx],
            None => line,
        })
        .collect::<Vec<&str>>()
        .join("\n")
}

fn strip_namespace(item: &str) -> String {
    if item.contains('$') {
        return String::new();
    }
    item.trim_end_matches([')', ';']).to_string()
}

fn plain_path_segment(arg: &str) -> Option<String> {
    const GENERIC_SEGMENTS: &[&str] = &["include", "src", "lib"];

    if arg.contains('$') || arg.starts_with('/') || arg.contains("::") {
        return None;
    }
    let segment = std::path::Path::new(arg)
        .file_name()
        .and_then(|s| s.to_str())?;
    if GENERIC_SEGMENTS.contains(&segment) {
        return None;
    }
    Some(segment.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_parser::detector::BuildConfigParser;
    use crate::config_parser::trait_def::LanguageParser;
    use cce_types::Language;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_parse_cmake_find_package() {
        let temp_dir = TempDir::new().expect("temp dir failed");
        let cmake = temp_dir.path().join("CMakeLists.txt");
        let content = r#"
cmake_minimum_required(VERSION 3.10)
project(TestProject)

find_package(Boost REQUIRED)
find_package(OpenCV REQUIRED)
"#;
        let mut file = std::fs::File::create(&cmake).expect("create failed");
        file.write_all(content.as_bytes()).expect("write failed");

        let parser = crate::config_parser::parsers::cpp::CMakeParser;
        let outcome = parser
            .try_parse(temp_dir.path(), temp_dir.path())
            .unwrap()
            .unwrap();
        assert!(outcome.dependencies.iter().any(|d| d.name == "Boost"));
        // Also via BuildConfigParser scan
        let mut build_parser = BuildConfigParser::new();
        build_parser
            .scan_project(temp_dir.path(), 0)
            .expect("parse failed");
        for lang in [Language::C, Language::Cpp] {
            let packages = build_parser.packages_for_language(lang);
            assert!(packages.contains("Boost"), "{lang:?} should know Boost");
            assert!(packages.contains("OpenCV"), "{lang:?} should know OpenCV");
        }
    }

    #[test]
    fn test_parse_cmake_target_link_libraries_internal_vs_external() {
        let content = r#"
add_library(core STATIC core.c)
add_executable(app main.c)
target_link_libraries(app PRIVATE core ${OPENSSL_LIBRARIES})
target_link_libraries(app PUBLIC Boost::filesystem pthread)
"#;
        let deps = parse_cmake_content(content);
        assert!(deps.contains(&UntypedDependency::new("core", "local")));
        assert!(!deps.contains(&UntypedDependency::new("core", "external")));
        assert!(deps.contains(&UntypedDependency::new("Boost::filesystem", "external")));
        assert!(deps.contains(&UntypedDependency::new("pthread", "external")));
        assert!(!deps.iter().any(|d| d.name.contains("OPENSSL")));
    }

    #[test]
    fn test_parse_cmake_add_subdirectory_and_include_dirs() {
        let content = r#"
add_subdirectory(plugins)
add_subdirectory(${THIRD_PARTY_DIR} EXCLUDE_FROM_ALL)
include_directories(include third_party/fmt /usr/local/include)
find_library(MATH_LIB m)
"#;
        let deps = parse_cmake_content(content);
        assert!(deps.contains(&UntypedDependency::new("plugins", "local")));
        assert_eq!(deps.iter().filter(|d| d.package_type == "local").count(), 2);
        assert!(!deps.contains(&UntypedDependency::new("include", "local")));
        assert!(deps.contains(&UntypedDependency::new("fmt", "local")));
        assert!(!deps.iter().any(|d| d.name == "usr"));
        assert!(deps.contains(&UntypedDependency::new("m", "external")));
    }

    #[test]
    fn test_find_library_skips_variable_names() {
        let content = "find_library(MATH_LIB m PATHS /usr/lib)\n";
        let deps = parse_cmake_content(content);
        assert!(deps.contains(&UntypedDependency::new("m", "external")));
        assert!(!deps.contains(&UntypedDependency::new("MATH_LIB", "external")));
    }
}
