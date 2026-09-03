//! Common utilities for Python build system parsers

use std::collections::HashSet;

use super::super::super::types::UntypedDependency;
use regex::Regex;

/// Parse a Python requirement specification
///
/// Supports formats:
/// - package==1.0.0
/// - package>=1.0.0,<2.0.0
/// - package[extra]==1.0.0
/// - package; python_version >= "3.8"
/// - -e git+https://github.com/user/repo.git#egg=package
/// - file:///path/to/package
/// - package @ https://example.com/package.tar.gz
pub fn parse_python_requirement(line: &str) -> Option<String> {
    let line = line.trim();

    // Skip empty lines and comments
    if line.is_empty() || line.starts_with('#') {
        return None;
    }

    // Parse editable installs (must be before skipping options)
    if line.starts_with("-e ") || line.starts_with("--editable ") {
        let path = line.split_whitespace().nth(1).unwrap_or("").trim();
        if !path.is_empty() {
            return extract_package_name_from_url_or_path(path);
        }
        return None;
    }

    // Skip other options like -r, -c, --index-url, etc.
    if line.starts_with('-') || line.starts_with("--") {
        return None;
    }

    // Parse URL requirements
    if line.contains("://") || line.starts_with("git+") {
        return extract_package_name_from_url_or_path(line);
    }

    // Parse regular requirement with extras and markers
    parse_regular_requirement_name(line)
}

/// Parse a regular requirement with extras and markers, returning only the package name
fn parse_regular_requirement_name(line: &str) -> Option<String> {
    // Split on semicolon to separate markers
    let requirement_part = line.split(';').next()?.trim();

    // Parse extras (e.g., package[extra1,extra2])
    let name = parse_package_name(requirement_part);

    if name.is_empty() { None } else { Some(name) }
}

/// Parse package name from requirement string
fn parse_package_name(requirement: &str) -> String {
    let re = Regex::new(r"^([a-zA-Z0-9_-]+)(?:\[[^\]]+\])?(.*)$").ok();

    if let Some(regex) = re {
        if let Some(caps) = regex.captures(requirement) {
            if let Some(name) = caps.get(1) {
                return name.as_str().to_string();
            }
        }
    }

    // Fallback: take first word before any version specifier
    requirement
        .split(&['=', '>', '<', '~', '!', ' '][..])
        .next()
        .unwrap_or("")
        .to_string()
}

/// Extract package name from URL or path
fn extract_package_name_from_url_or_path(url: &str) -> Option<String> {
    // Common patterns for extracting package names from URLs
    let patterns = [
        // egg=package
        r"egg=([a-zA-Z0-9_-]+)",
        // #package
        r"#([a-zA-Z0-9_-]+)$",
    ];

    for pattern in &patterns {
        if let Ok(re) = Regex::new(pattern) {
            if let Some(caps) = re.captures(url) {
                if let Some(name) = caps.get(1) {
                    return Some(name.as_str().to_string());
                }
            }
        }
    }

    // For git URLs like git+https://github.com/user/repo.git, extract repo name
    // Remove git+ prefix if present
    let cleaned_url = url.strip_prefix("git+").unwrap_or(url);

    // Extract from the last path segment
    cleaned_url
        .split('/')
        .next_back()
        .map(|s| s.trim_end_matches(".git"))
        .and_then(|s| s.split('@').next()) // Remove @ref if present
        .map(|s| s.to_string())
}

/// Parse a requirements.txt file content
pub fn parse_requirements_txt(content: &str) -> HashSet<UntypedDependency> {
    let mut dependencies = HashSet::new();

    for line in content.lines() {
        let line = line.trim();

        // Skip comments and empty lines
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Handle -r requirements.txt includes
        if line.starts_with("-r ") {
            // Note: We would need to read the referenced file
            // For now, just skip
            continue;
        }

        // Handle constraints files
        if line.starts_with("-c ") {
            continue;
        }

        if let Some(name) = parse_python_requirement(line) {
            dependencies.insert(UntypedDependency::new(name, "external"));
        }
    }

    dependencies
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_python_requirement_simple() {
        let name = parse_python_requirement("requests").expect("parse failed");
        assert_eq!(name, "requests");
    }

    #[test]
    fn test_parse_python_requirement_with_version() {
        let name = parse_python_requirement("requests==2.31.0").expect("parse failed");
        assert_eq!(name, "requests");
    }

    #[test]
    fn test_parse_python_requirement_with_extras() {
        let name = parse_python_requirement("requests[security]>=2.31.0").expect("parse failed");
        assert_eq!(name, "requests");
    }

    #[test]
    fn test_parse_python_requirement_with_markers() {
        let name = parse_python_requirement("requests>=2.31.0; python_version >= '3.8'")
            .expect("parse failed");
        assert_eq!(name, "requests");
    }

    #[test]
    fn test_parse_python_requirement_editable() {
        let name = parse_python_requirement("-e ./local-package").expect("parse failed");
        assert_eq!(name, "local-package");
    }

    #[test]
    fn test_parse_python_requirement_url() {
        let name =
            parse_python_requirement("git+https://github.com/user/repo.git").expect("parse failed");
        assert_eq!(name, "repo");
    }

    #[test]
    fn test_parse_python_requirement_empty() {
        assert!(parse_python_requirement("").is_none());
    }

    #[test]
    fn test_parse_python_requirement_comment() {
        assert!(parse_python_requirement("# This is a comment").is_none());
    }

    #[test]
    fn test_parse_python_requirement_option() {
        assert!(parse_python_requirement("-r requirements.txt").is_none());
        assert!(parse_python_requirement("--index-url https://pypi.org/simple").is_none());
    }

    #[test]
    fn test_parse_requirements_txt() {
        let content = r#"requests>=2.31.0
numpy>=1.24.0
# This is a comment
flask>=2.0.0
"#;
        let deps = parse_requirements_txt(content);
        assert_eq!(deps.len(), 3);

        let dep_names: Vec<&str> = deps.iter().map(|d| d.name.as_str()).collect();
        assert!(dep_names.contains(&"requests"));
        assert!(dep_names.contains(&"numpy"));
        assert!(dep_names.contains(&"flask"));
    }
}
