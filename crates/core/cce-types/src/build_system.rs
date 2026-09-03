//! Build system metadata - single source of truth for config file ↔ language mapping.
//!
//! This module is the canonical definition of which build/config files belong to
//! which languages. All consumers (`LanguageInfo::detect_from_path`,
//! `BuildConfigParser`, hot-update watcher, `FileCategory`, document router) must
//! delegate here instead of keeping private copies.

use crate::types::Language;

/// Metadata for a single build system.
#[derive(Debug, Clone)]
pub struct BuildSystemMetadata {
    /// Human readable name (e.g. "Cargo").
    pub name: String,
    /// Exact config file names (e.g. `["Cargo.toml"]`). Suffix patterns like
    /// `"*.csproj"` are stored literally; matching is suffix-based.
    pub config_files: Vec<String>,
    /// Languages whose source files are affected by this config.
    pub languages: Vec<Language>,
    /// File extensions (without dot) of those languages.
    pub file_extensions: Vec<String>,
}

/// Directories that never contain build manifests and are skipped during
/// recursive manifest discovery. Must stay in sync with the scanner's default
/// excluded directories for the same reason (avoid descending into huge trees).
pub const MANIFEST_SCAN_EXCLUDED_DIRS: &[&str] = &[
    "target",
    "node_modules",
    ".git",
    ".svn",
    ".hg",
    "vendor",
    "dist",
    "build",
    "out",
    "__pycache__",
    ".venv",
    "venv",
    ".mypy_cache",
];

/// Canonical list of all supported build systems.
///
/// This is the **single source of truth**. To add a new language/build system,
/// update only this function.
pub fn get_supported_build_systems() -> Vec<BuildSystemMetadata> {
    use Language::*;

    vec![
        BuildSystemMetadata {
            name: "Cargo".to_string(),
            config_files: vec!["Cargo.toml".to_string()],
            languages: vec![Rust],
            file_extensions: Rust.extensions().iter().map(|s| s.to_string()).collect(),
        },
        BuildSystemMetadata {
            name: "NPM".to_string(),
            config_files: vec!["package.json".to_string(), "package-lock.json".to_string()],
            languages: vec![JavaScript, TypeScript],
            file_extensions: [
                JavaScript.extensions(),
                TypeScript.extensions(),
                Jsx.extensions(),
                Tsx.extensions(),
            ]
            .concat()
            .iter()
            .map(|s| s.to_string())
            .collect(),
        },
        BuildSystemMetadata {
            name: "PNPM".to_string(),
            config_files: vec!["pnpm-lock.yaml".to_string()],
            languages: vec![JavaScript, TypeScript],
            file_extensions: [
                JavaScript.extensions(),
                TypeScript.extensions(),
                Jsx.extensions(),
                Tsx.extensions(),
            ]
            .concat()
            .iter()
            .map(|s| s.to_string())
            .collect(),
        },
        BuildSystemMetadata {
            name: "Yarn".to_string(),
            config_files: vec!["yarn.lock".to_string()],
            languages: vec![JavaScript, TypeScript],
            file_extensions: [
                JavaScript.extensions(),
                TypeScript.extensions(),
                Jsx.extensions(),
                Tsx.extensions(),
            ]
            .concat()
            .iter()
            .map(|s| s.to_string())
            .collect(),
        },
        BuildSystemMetadata {
            name: "PyPI".to_string(),
            config_files: vec![
                "requirements.txt".to_string(),
                "pyproject.toml".to_string(),
                "Pipfile".to_string(),
                "setup.cfg".to_string(),
                "environment.yml".to_string(),
            ],
            languages: vec![Python],
            file_extensions: Python.extensions().iter().map(|s| s.to_string()).collect(),
        },
        BuildSystemMetadata {
            name: "Go Modules".to_string(),
            config_files: vec!["go.mod".to_string(), "go.sum".to_string()],
            languages: vec![Go],
            file_extensions: Go.extensions().iter().map(|s| s.to_string()).collect(),
        },
        BuildSystemMetadata {
            name: "Maven/Gradle".to_string(),
            config_files: vec![
                "pom.xml".to_string(),
                "build.gradle".to_string(),
                "build.gradle.kts".to_string(),
                "settings.gradle".to_string(),
                "settings.gradle.kts".to_string(),
            ],
            languages: vec![Java, Kotlin],
            file_extensions: [Java.extensions(), Kotlin.extensions()]
                .concat()
                .iter()
                .map(|s| s.to_string())
                .collect(),
        },
        BuildSystemMetadata {
            name: "CMake".to_string(),
            config_files: vec!["CMakeLists.txt".to_string()],
            languages: vec![C, Cpp],
            file_extensions: [C.extensions(), Cpp.extensions()]
                .concat()
                .iter()
                .map(|s| s.to_string())
                .collect(),
        },
        BuildSystemMetadata {
            name: "Composer".to_string(),
            config_files: vec!["composer.json".to_string()],
            languages: vec![Php],
            file_extensions: Php.extensions().iter().map(|s| s.to_string()).collect(),
        },
        BuildSystemMetadata {
            name: ".NET".to_string(),
            config_files: vec![
                "*.csproj".to_string(),
                "*.fsproj".to_string(),
                "*.vbproj".to_string(),
            ],
            languages: vec![],
            file_extensions: [CSharp.extensions()]
                .concat()
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
                .into_iter()
                .chain(vec!["fs".to_string(), "vb".to_string()])
                .collect(),
        },
        BuildSystemMetadata {
            name: "Bundler".to_string(),
            config_files: vec!["Gemfile".to_string(), "Gemfile.lock".to_string()],
            languages: vec![Ruby],
            file_extensions: Ruby.extensions().iter().map(|s| s.to_string()).collect(),
        },
        BuildSystemMetadata {
            name: "Make".to_string(),
            config_files: vec![
                "Makefile".to_string(),
                "makefile".to_string(),
                "GNUmakefile".to_string(),
                "gnumakefile".to_string(),
            ],
            languages: vec![C, Cpp],
            file_extensions: [C.extensions(), Cpp.extensions()]
                .concat()
                .iter()
                .map(|s| s.to_string())
                .collect(),
        },
        BuildSystemMetadata {
            name: "Docker".to_string(),
            config_files: vec!["Dockerfile".to_string(), "dockerfile".to_string()],
            languages: vec![],
            file_extensions: vec!["dockerfile".to_string()],
        },
    ]
}

/// All exact config file names (flattened from `get_supported_build_systems`).
///
/// Suffix patterns (`*.csproj`) are **not** included; use suffix matching for them.
pub fn all_build_config_file_names() -> Vec<String> {
    let mut names = Vec::new();
    for system in get_supported_build_systems() {
        for file in system.config_files {
            // Skip suffix patterns
            if file.starts_with("*.") {
                continue;
            }
            if !names.contains(&file) {
                names.push(file);
            }
        }
    }
    names
}

/// Check a file name against the canonical build config rule set (case-sensitive).
pub fn is_build_config_name(name: &str) -> bool {
    for system in get_supported_build_systems() {
        for pattern in &system.config_files {
            if pattern.starts_with("*.") {
                let suffix = &pattern[1..]; // ".csproj"
                if name.ends_with(suffix) {
                    return true;
                }
            } else if pattern == name {
                return true;
            }
        }
    }
    false
}

/// Case-insensitive variant for classifiers operating on lowercased strings.
pub fn is_build_config_name_lower(lower_name: &str) -> bool {
    for system in get_supported_build_systems() {
        for pattern in &system.config_files {
            if pattern.starts_with("*.") {
                let suffix = pattern[1..].to_ascii_lowercase();
                if lower_name.ends_with(&suffix) {
                    return true;
                }
            } else if pattern.to_ascii_lowercase() == lower_name {
                return true;
            }
        }
    }
    false
}

/// File extensions affected by a config file change.
///
/// Matches both exact names (`Cargo.toml`) and suffix patterns
/// (`*.csproj` → `Foo.csproj`), otherwise returns empty.
pub fn get_affected_extensions(config_filename: &str) -> Vec<String> {
    for meta in get_supported_build_systems() {
        for pattern in &meta.config_files {
            if pattern.starts_with("*.") {
                let suffix = &pattern[1..];
                if config_filename.ends_with(suffix) {
                    return meta.file_extensions.clone();
                }
            } else if pattern == config_filename {
                return meta.file_extensions.clone();
            }
        }
    }
    Vec::new()
}

/// Whether a filename is a recognized build config (delegates to `is_build_config_name`).
pub fn is_build_config(filename: &str) -> bool {
    is_build_config_name(filename)
}

/// Canonicalize a package or import name for cross-language comparison.
///
/// Python follows the `packaging` canonicalization (`[-_.]+` collapsed to `-`,
/// case-insensitive). JavaScript/TypeScript package names are case-insensitive
/// (lowercased) but preserve scope and hyphen/underscore distinction. Other
/// languages fall back to lowercasing.
pub fn canonicalize_package_name(name: &str, language: Language) -> String {
    match language {
        Language::Python => canonicalize_python_name(name),
        Language::JavaScript | Language::TypeScript | Language::Jsx | Language::Tsx => {
            name.to_ascii_lowercase()
        }
        _ => name.to_ascii_lowercase(),
    }
}

fn canonicalize_python_name(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    let mut result = String::with_capacity(lower.len());
    let mut last_was_dash = false;
    for ch in lower.chars() {
        if ch == '-' || ch == '_' || ch == '.' {
            if !last_was_dash {
                result.push('-');
                last_was_dash = true;
            }
        } else {
            result.push(ch);
            last_was_dash = false;
        }
    }
    result
}

/// Extract the base package identifier from an import source string, in a
/// language-aware way. For scoped npm packages (`@scope/name`), the first two
/// path segments are kept; otherwise the first segment before `/`, `::`, or
/// `.` is returned.
fn extract_import_package_base(source: &str, language: Language) -> String {
    if matches!(
        language,
        Language::JavaScript | Language::TypeScript | Language::Jsx | Language::Tsx
    ) && source.starts_with('@')
    {
        let mut parts = source.split('/');
        let scope = parts.next().unwrap_or("");
        if let Some(name) = parts.next() {
            if !name.is_empty() {
                return format!("{scope}/{name}");
            }
        }
        return scope.to_string();
    }
    let idx_double_colon = source.find("::");
    let idx_slash = source.find('/');
    let idx_dot = source.find('.');
    let mut idx: Option<usize> = None;
    for pos in [idx_double_colon, idx_slash, idx_dot].into_iter().flatten() {
        idx = Some(idx.map_or(pos, |cur: usize| cur.min(pos)));
    }
    if let Some(pos) = idx {
        source[..pos].to_string()
    } else {
        source.to_string()
    }
}

/// Language-aware import-to-package matching.
///
/// Both sides are canonicalized according to `language` rules before
/// comparison, and qualified imports (`pkg/sub`, `pkg.module`, `pkg::mod`)
/// match the base package.
pub fn imports_match_package(source: &str, pkg: &str, language: Language) -> bool {
    let canon_pkg = canonicalize_package_name(pkg, language);
    let base = extract_import_package_base(source, language);
    let canon_base = canonicalize_package_name(&base, language);
    canon_base == canon_pkg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_build_config_matches_expected() {
        assert!(is_build_config_name("Cargo.toml"));
        assert!(is_build_config_name("package.json"));
        assert!(is_build_config_name("Foo.csproj"));
        assert!(is_build_config_name("Makefile"));
        assert!(is_build_config_name("Dockerfile"));
        assert!(!is_build_config_name("main.rs"));
        assert!(!is_build_config_name("cargo.toml")); // case-sensitive
    }

    #[test]
    fn is_build_config_lower_matches() {
        assert!(is_build_config_name_lower("cargo.toml"));
        assert!(is_build_config_name_lower("foo.csproj"));
        assert!(!is_build_config_name_lower("main.rs"));
    }

    #[test]
    fn affected_extensions_delegates() {
        assert_eq!(get_affected_extensions("Cargo.toml"), vec!["rs"]);
        assert!(get_affected_extensions("package.json").contains(&"ts".to_string()));
        assert!(get_affected_extensions("unknown.conf").is_empty());
    }
}
