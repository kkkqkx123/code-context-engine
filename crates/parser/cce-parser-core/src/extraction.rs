//! Extraction context and module-path utilities for symbol extraction.
//!
//! Provides [`ExtractionContext`] (file/project context for resolving
//! imports) and [`determine_module_path`] (converts file paths to
//! language-specific module paths).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use cce_types::language::Language;
use cce_types::path::normalize_project_path;

/// Context information for symbol extraction.
///
/// Provides necessary context for resolving imports and exports,
/// including file location, project structure, and configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionContext {
    /// Current file path (absolute).
    pub file_path: PathBuf,
    /// Project root directory.
    pub project_root: PathBuf,
    /// Current module/package path (language-specific).
    pub current_module: Option<String>,
    /// Package declaration (for Java, Go, etc.).
    pub package_declaration: Option<String>,
    /// Project configuration.
    pub config: ExtractionConfig,
}

impl Default for ExtractionContext {
    fn default() -> Self {
        Self {
            file_path: PathBuf::new(),
            project_root: PathBuf::new(),
            current_module: None,
            package_declaration: None,
            config: ExtractionConfig::default(),
        }
    }
}

impl ExtractionContext {
    /// Create extraction context for a file.
    pub fn from_file(file_path: PathBuf, project_root: PathBuf, language: Language) -> Self {
        let current_module = determine_module_path(&file_path, &project_root, language);
        let package_declaration = None;
        let config = ExtractionConfig::default();

        Self {
            file_path,
            project_root,
            current_module,
            package_declaration,
            config,
        }
    }

    /// Resolve relative import to absolute path.
    ///
    /// Java, Kotlin, Scala and C# have no relative import syntax: every
    /// import is an absolute package or namespace path, so they
    /// deliberately return none. Dart, C, C++, PHP, Ruby, Lua and Bash
    /// use file-relative paths (`./`, `../`, quoted headers, `require_relative`,
    /// `source`) and share the file-directory resolver. The JavaScript
    /// family shares one module-path resolver.
    pub fn resolve_relative_import(
        &self,
        relative_path: &str,
        language: Language,
    ) -> Option<String> {
        match language {
            Language::Python => self.resolve_python_relative(relative_path),
            Language::JavaScript | Language::TypeScript | Language::Jsx | Language::Tsx => {
                self.resolve_js_relative(relative_path)
            }
            Language::Rust => self.resolve_rust_relative(relative_path),
            Language::Go => self.resolve_go_relative(relative_path),
            Language::Dart => self.resolve_dart_relative(relative_path),
            Language::C | Language::Cpp => self.resolve_c_relative(relative_path),
            Language::Php | Language::Ruby | Language::Lua | Language::Bash => {
                self.resolve_file_relative(relative_path)
            }
            // Java, Kotlin, Scala and C# imports are always absolute
            // package or namespace paths; there is no relative syntax to resolve.
            Language::Java | Language::Kotlin | Language::Scala | Language::CSharp => None,
            _ => None,
        }
    }

    /// Classify an import as internal or external.
    ///
    /// Relative specifiers are always internal: Python leading `.`,
    /// Rust `crate::`/`self::`/`super::`, JS family and Dart leading
    /// `./`/`../`, Go leading `.`, C/C++ quoted includes (anything not
    /// `<...>` or absolute), PHP/Ruby/Lua/Bash leading `./`/`../`, and
    /// PHP file includes (`*.php` or containing `/`). Absolute package
    /// or namespace imports fall back to `package_declaration` prefix
    /// (Java family, C#, PHP namespaces, Go) and then to configured
    /// `project_packages` prefixes. Dart `dart:` URIs are SDK and never
    /// internal; `package:` URIs are internal only when they match a
    /// known project package.
    pub fn is_internal_import(&self, import_path: &str, language: Language) -> bool {
        if Self::is_relative_specifier(import_path, language) {
            return true;
        }
        if matches!(language, Language::Dart) && import_path.starts_with("dart:") {
            return false;
        }
        if let Some(ref package) = self.package_declaration {
            match language {
                Language::Java | Language::Kotlin | Language::Scala | Language::CSharp => {
                    if import_path.starts_with(package) {
                        let remaining = import_path.strip_prefix(package).unwrap_or("");
                        if remaining.is_empty() || remaining.starts_with('.') {
                            return true;
                        }
                    }
                    let package_parts: Vec<&str> = package.split('.').collect();
                    let import_parts: Vec<&str> = import_path.split('.').collect();
                    let common_prefix_len = package_parts
                        .iter()
                        .zip(import_parts.iter())
                        .take_while(|(a, b)| a == b)
                        .count();
                    if common_prefix_len >= 2 {
                        return true;
                    }
                }
                Language::Php => {
                    // PHP namespaces use `\` separators.
                    let package_prefix = package.replace('\\', ".");
                    let import_normalized = import_path.replace('\\', ".");
                    if import_normalized.starts_with(&package_prefix) {
                        return true;
                    }
                }
                Language::Go => {}
                _ => {}
            }
        }

        for project_pkg in &self.config.project_packages {
            match language {
                Language::Java | Language::Kotlin | Language::Scala | Language::CSharp => {
                    if import_path.starts_with(project_pkg) {
                        return true;
                    }
                }
                Language::Go => {
                    if import_path.starts_with(project_pkg) {
                        return true;
                    }
                }
                Language::Python => {
                    if import_path.starts_with(project_pkg) {
                        return true;
                    }
                }
                Language::JavaScript | Language::TypeScript | Language::Jsx | Language::Tsx => {
                    if import_path.starts_with(project_pkg) {
                        return true;
                    }
                }
                Language::Dart => {
                    // `package:myapp/foo.dart` is internal when `myapp`
                    // matches a known project package.
                    let without_scheme =
                        import_path.strip_prefix("package:").unwrap_or(import_path);
                    if without_scheme == project_pkg.as_str()
                        || without_scheme.starts_with(&format!("{project_pkg}/"))
                        || import_path.starts_with(project_pkg)
                    {
                        return true;
                    }
                }
                Language::Php => {
                    let import_normalized = import_path.replace('\\', "/");
                    let pkg_normalized = project_pkg.replace(['.', '\\'], "/");
                    if import_normalized.starts_with(&pkg_normalized)
                        || import_normalized.starts_with(project_pkg)
                    {
                        return true;
                    }
                }
                Language::Ruby | Language::Lua | Language::Bash => {
                    if import_path.starts_with(project_pkg) {
                        return true;
                    }
                }
                Language::Rust
                    if import_path.starts_with("crate::")
                        || import_path.starts_with("self::")
                        || import_path.starts_with("super::")
                        || import_path.starts_with(project_pkg) =>
                {
                    return true;
                }
                _ => {}
            }
        }

        false
    }

    /// Relative-specifier check shared by `is_internal_import`.
    ///
    /// Returns true when the specifier is file- or module-relative and
    /// therefore project-local by construction, before any package
    /// prefix is consulted.
    fn is_relative_specifier(import_path: &str, language: Language) -> bool {
        match language {
            Language::Python => import_path.starts_with('.'),
            Language::Rust => {
                import_path.starts_with("crate::")
                    || import_path.starts_with("self::")
                    || import_path.starts_with("super::")
            }
            Language::JavaScript | Language::TypeScript | Language::Jsx | Language::Tsx => {
                import_path.starts_with("./")
                    || import_path.starts_with("../")
                    || import_path == "."
                    || import_path == ".."
            }
            Language::Go => import_path.starts_with('.'),
            Language::C | Language::Cpp => {
                // Quoted includes are project-local; angle-bracket and
                // absolute paths are system or external.
                !(import_path.starts_with('<') || import_path.starts_with('/'))
            }
            Language::Dart => {
                import_path.starts_with("./")
                    || import_path.starts_with("../")
                    || import_path == "."
                    || import_path == ".."
            }
            Language::Php => {
                import_path.starts_with("./")
                    || import_path.starts_with("../")
                    || import_path.ends_with(".php")
                    || (import_path.contains('/') && !import_path.contains('\\'))
            }
            Language::Ruby | Language::Lua | Language::Bash => {
                import_path.starts_with("./")
                    || import_path.starts_with("../")
                    || import_path == "."
                    || import_path == ".."
            }
            // Java, Kotlin, Scala and C# use absolute package paths only.
            _ => false,
        }
    }

    fn resolve_python_relative(&self, relative_path: &str) -> Option<String> {
        let current = self.current_module.as_ref()?;
        let mut parts: Vec<&str> = current.split('.').collect();
        let dot_count = relative_path.chars().take_while(|&c| c == '.').count();
        for _ in 0..dot_count {
            parts.pop();
        }
        let module_name = relative_path.trim_start_matches('.');
        if !module_name.is_empty() {
            parts.push(module_name);
        }
        Some(parts.join("."))
    }

    fn resolve_js_relative(&self, relative_path: &str) -> Option<String> {
        let current = self.current_module.as_ref()?;
        let mut parts: Vec<&str> = current.split('/').collect();
        parts.pop();
        for segment in relative_path.split('/') {
            match segment {
                "." => {}
                ".." => {
                    parts.pop();
                }
                _ => parts.push(segment),
            }
        }
        Some(parts.join("/"))
    }

    fn resolve_rust_relative(&self, relative_path: &str) -> Option<String> {
        let current = self.current_module.as_ref()?;
        let mut parts: Vec<&str> = current.split("::").collect();
        for segment in relative_path.split("::") {
            match segment {
                "super" => {
                    parts.pop();
                }
                "self" => {}
                "crate" => {
                    parts.clear();
                    parts.push("crate");
                }
                _ => parts.push(segment),
            }
        }
        Some(parts.join("::"))
    }

    fn resolve_go_relative(&self, relative_path: &str) -> Option<String> {
        self.resolve_file_relative(relative_path)
    }

    /// File-directory resolver shared by Dart, C, C++, PHP, Ruby, Lua,
    /// Bash and Go. Joins the importing file's directory with the given
    /// specifier and strips the project root, yielding a project-relative
    /// posix path. Absolute filesystem paths, angle-bracket headers,
    /// Dart `package:`/`dart:` URIs, variable interpolations and globs
    /// are absolute or undecidable and return none.
    fn resolve_file_relative(&self, relative_path: &str) -> Option<String> {
        if relative_path.is_empty()
            || relative_path.starts_with('/')
            || relative_path.starts_with('<')
            || relative_path.starts_with("dart:")
            || relative_path.starts_with("package:")
            || relative_path.contains('$')
            || relative_path.contains('*')
            || relative_path.contains('?')
            || relative_path.chars().any(|c| c.is_whitespace())
        {
            return None;
        }
        let current_dir = self.file_path.parent()?;
        let resolved = current_dir.join(relative_path);
        let normalized_abs = normalize_abs_posix(&resolved.to_string_lossy().replace('\\', "/"));
        let root = self.project_root.to_string_lossy().replace('\\', "/");
        let root = root.trim_end_matches('/');
        let relative = normalized_abs
            .strip_prefix(root)
            .and_then(|s| s.strip_prefix('/').or(Some(s)))
            .filter(|s| !s.is_empty() && !s.starts_with(".."))?;
        // Empty remainder means the import points at the project root itself.
        if relative.is_empty() {
            return None;
        }
        Some(normalize_relative_posix(relative))
    }

    /// Dart relative URIs (`./`, `../` or bare `foo/bar.dart`) resolve
    /// against the importing file; `dart:` and `package:` URIs are absolute.
    fn resolve_dart_relative(&self, relative_path: &str) -> Option<String> {
        self.resolve_file_relative(relative_path)
    }

    /// C/C++ quoted includes (`"foo.h"`, `"./foo.h"`) resolve against
    /// the including file; angle-bracket system headers are absolute.
    fn resolve_c_relative(&self, relative_path: &str) -> Option<String> {
        self.resolve_file_relative(relative_path)
    }
}

/// Determine module path from file path.
///
/// Converts a file path to a language-specific module path.
///
/// # Examples
///
/// - Rust: `"src/utils/helper.rs"` -> `"utils::helper"`
/// - Python: `"myapp/utils/helper.py"` -> `"myapp.utils.helper"`
/// - Java: `"com/example/Utils.java"` -> `"com.example.Utils"`
/// - Go: `"pkg/utils/helper.go"` -> `"pkg/utils/helper"`
/// - JS/TS: `"src/utils/helper.ts"` -> `"src/utils/helper"`
/// - C/C++: `"src/util.h"` -> `"src/util.h"` (the translation unit is the
///   file, so the extension is kept and `util.h` / `util.c` stay distinct)
pub fn determine_module_path(
    file_path: &Path,
    project_root: &Path,
    language: Language,
) -> Option<String> {
    let relative = match file_path.strip_prefix(project_root) {
        Ok(rel) => rel.to_string_lossy().replace('\\', "/"),
        Err(_) => {
            let normalized = normalize_project_path(&file_path.to_string_lossy());
            if !file_path.is_absolute() && !normalized.starts_with("../") {
                normalized
            } else {
                return None;
            }
        }
    };

    match language {
        Language::Python => {
            let path_str = relative.as_str();
            let module = path_str
                .trim_end_matches("/__init__.py")
                .trim_end_matches("\\__init__.py")
                .trim_end_matches(".py")
                .replace(['/', '\\'], ".");
            Some(module)
        }
        Language::Java => {
            let path_str = relative.as_str();
            let module = path_str.trim_end_matches(".java").replace(['/', '\\'], ".");
            Some(module)
        }
        Language::Rust => {
            let path_str = relative.as_str();
            // Note: This assumes `src/` is the crate root, which is the de facto
            // standard for Rust projects. For non-standard layouts (e.g., crates
            // with `path` attributes in Cargo.toml), the `src/` prefix may not
            // apply. A future improvement could parse Cargo.toml to detect the
            // actual crate root path.
            let module = path_str
                .trim_start_matches("src/")
                .trim_start_matches("src\\")
                .trim_end_matches(".rs")
                .replace(['/', '\\'], "::");
            Some(module)
        }
        Language::Go => {
            let path_str = relative.as_str();
            // Note: This uses the filesystem path as the module path. For proper
            // Go module support, the `module` directive from go.mod should be used
            // as the prefix. A future improvement could parse go.mod to derive
            // the correct module path.
            let module = path_str.trim_end_matches(".go").trim_end_matches("/");
            Some(module.to_string())
        }
        Language::Kotlin | Language::Scala | Language::CSharp => {
            let path_str = relative.as_str();
            let module = strip_file_extension(path_str).replace(['/', '\\'], ".");
            Some(module)
        }
        Language::C | Language::Cpp => {
            // C/C++ have no module system: the translation unit is the file.
            // A header and its implementation share a stem (`foo.h` / `foo.c`),
            // so the extension is kept to keep the two module paths distinct.
            Some(relative)
        }
        Language::JavaScript
        | Language::TypeScript
        | Language::Jsx
        | Language::Tsx
        | Language::Ruby
        | Language::Php
        | Language::Dart
        | Language::Bash
        | Language::Lua
        | Language::Vue
        | Language::Svelte => {
            let path_str = relative.as_str();
            let module = strip_file_extension(path_str).replace(['/', '\\'], "/");
            Some(module)
        }
        _ => {
            let path_str = relative.as_str();
            let module = strip_file_extension(path_str).replace(['/', '\\'], "/");
            Some(module)
        }
    }
}

fn strip_file_extension(path_str: &str) -> String {
    match path_str.rsplit_once('.') {
        Some((stem, _ext)) if !stem.is_empty() => stem.to_string(),
        _ => path_str.to_string(),
    }
}

/// Collapse `.` and `..` segments in a project-relative posix path.
fn normalize_relative_posix(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(segment),
        }
    }
    parts.join("/")
}

/// Collapse `.` and `..` in an absolute posix path without touching the filesystem.
fn normalize_abs_posix(path: &str) -> String {
    let absolute = path.starts_with('/');
    let mut parts: Vec<&str> = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(segment),
        }
    }
    if absolute {
        format!("/{}", parts.join("/"))
    } else {
        parts.join("/")
    }
}

/// Project-level configuration for extraction.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExtractionConfig {
    /// Project package prefixes (for internal module detection).
    pub project_packages: Vec<String>,
    /// Path mappings (for TypeScript, JavaScript).
    pub path_mappings: Vec<PathMapping>,
    /// Implicit imports (for C# implicit usings).
    pub implicit_imports: Vec<String>,
    /// Module aliases (for Python `__init__.py`).
    pub module_aliases: Vec<ModuleAlias>,
}

/// Path mapping configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathMapping {
    /// Pattern to match (e.g. `"@/*"`).
    pub pattern: String,
    /// Replacement paths.
    pub paths: Vec<String>,
}

/// Module alias configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleAlias {
    /// Alias name.
    pub alias: String,
    /// Target module path.
    pub target: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_python_module_path() {
        let file_path = PathBuf::from("/project/myapp/utils/helper.py");
        let project_root = PathBuf::from("/project");
        let module = determine_module_path(&file_path, &project_root, Language::Python);
        assert_eq!(module, Some("myapp.utils.helper".to_string()));
    }

    #[test]
    fn test_python_init_module_path() {
        let file_path = PathBuf::from("/project/myapp/utils/__init__.py");
        let project_root = PathBuf::from("/project");
        let module = determine_module_path(&file_path, &project_root, Language::Python);
        assert_eq!(module, Some("myapp.utils".to_string()));
    }

    #[test]
    fn test_java_module_path() {
        let file_path = PathBuf::from("/project/src/com/example/myapp/Utils.java");
        let project_root = PathBuf::from("/project/src");
        let module = determine_module_path(&file_path, &project_root, Language::Java);
        assert_eq!(module, Some("com.example.myapp.Utils".to_string()));
    }

    #[test]
    fn test_rust_module_path() {
        let file_path = PathBuf::from("/project/src/utils/helper.rs");
        let project_root = PathBuf::from("/project");
        let module = determine_module_path(&file_path, &project_root, Language::Rust);
        assert_eq!(module, Some("utils::helper".to_string()));
    }

    #[test]
    fn test_rust_module_path_relative_file_absolute_root() {
        let file_path = PathBuf::from("src/utils/helper.rs");
        let project_root = PathBuf::from("/tmp/project-root");
        let module = determine_module_path(&file_path, &project_root, Language::Rust);
        assert_eq!(module, Some("utils::helper".to_string()));
    }

    #[test]
    fn test_module_path_outside_root_is_none() {
        let file_path = PathBuf::from("/elsewhere/src/utils/helper.rs");
        let project_root = PathBuf::from("/tmp/project-root");
        let module = determine_module_path(&file_path, &project_root, Language::Rust);
        assert_eq!(module, None);

        let escaping = PathBuf::from("../outside/helper.rs");
        let module = determine_module_path(&escaping, &project_root, Language::Rust);
        assert_eq!(module, None);
    }

    #[test]
    fn test_go_module_path() {
        let file_path = PathBuf::from("/project/pkg/utils/helper.go");
        let project_root = PathBuf::from("/project");
        let module = determine_module_path(&file_path, &project_root, Language::Go);
        assert_eq!(module, Some("pkg/utils/helper".to_string()));
    }

    #[test]
    fn test_c_module_path_keeps_extension() {
        let file_path = PathBuf::from("/project/libmov/test/mov-file-buffer.c");
        let project_root = PathBuf::from("/project");
        let module = determine_module_path(&file_path, &project_root, Language::C);
        assert_eq!(module, Some("libmov/test/mov-file-buffer.c".to_string()));
    }

    #[test]
    fn test_cpp_header_and_source_module_paths_are_distinct() {
        let project_root = PathBuf::from("/project");
        let header = determine_module_path(
            &PathBuf::from("/project/librtsp/rtp.h"),
            &project_root,
            Language::Cpp,
        );
        let source = determine_module_path(
            &PathBuf::from("/project/librtsp/rtp.c"),
            &project_root,
            Language::Cpp,
        );
        let impl_file = determine_module_path(
            &PathBuf::from("/project/librtsp/rtp.cpp"),
            &project_root,
            Language::Cpp,
        );

        assert_eq!(header, Some("librtsp/rtp.h".to_string()));
        assert_eq!(source, Some("librtsp/rtp.c".to_string()));
        assert_eq!(impl_file, Some("librtsp/rtp.cpp".to_string()));
    }

    #[test]
    fn test_python_relative_import() {
        let context = ExtractionContext {
            file_path: PathBuf::from("/project/myapp/utils/helper.py"),
            project_root: PathBuf::from("/project"),
            current_module: Some("myapp.utils.helper".to_string()),
            ..Default::default()
        };
        assert_eq!(
            context.resolve_relative_import("..models", Language::Python),
            Some("myapp.models".to_string())
        );
        assert_eq!(
            context.resolve_relative_import(".", Language::Python),
            Some("myapp.utils".to_string())
        );
        assert_eq!(
            context.resolve_relative_import("...top", Language::Python),
            Some("top".to_string())
        );
    }

    #[test]
    fn test_js_relative_import() {
        let context = ExtractionContext {
            file_path: PathBuf::from("/project/src/utils/helper.ts"),
            project_root: PathBuf::from("/project"),
            current_module: Some("src/utils/helper".to_string()),
            ..Default::default()
        };
        assert_eq!(
            context.resolve_relative_import("../models", Language::TypeScript),
            Some("src/models".to_string())
        );
        assert_eq!(
            context.resolve_relative_import("./module", Language::TypeScript),
            Some("src/utils/module".to_string())
        );
    }

    #[test]
    fn test_rust_relative_import() {
        let context = ExtractionContext {
            file_path: PathBuf::from("/project/src/utils/helper.rs"),
            project_root: PathBuf::from("/project"),
            current_module: Some("cce_utils::helper".to_string()),
            ..Default::default()
        };
        assert_eq!(
            context.resolve_relative_import("super::models", Language::Rust),
            Some("cce_utils::models".to_string())
        );
        assert_eq!(
            context.resolve_relative_import("self::module", Language::Rust),
            Some("cce_utils::helper::module".to_string())
        );
        assert_eq!(
            context.resolve_relative_import("crate::other", Language::Rust),
            Some("crate::other".to_string())
        );
    }

    #[test]
    fn test_file_relative_import_dart_c_php_ruby_lua_bash() {
        let context = ExtractionContext {
            file_path: PathBuf::from("/project/src/utils/helper.dart"),
            project_root: PathBuf::from("/project"),
            current_module: Some("src/utils/helper".to_string()),
            ..Default::default()
        };
        assert_eq!(
            context.resolve_relative_import("./other.dart", Language::Dart),
            Some("src/utils/other.dart".to_string())
        );
        assert_eq!(
            context.resolve_relative_import("dart:core", Language::Dart),
            None
        );
        assert_eq!(
            context.resolve_relative_import("package:myapp/other.dart", Language::Dart),
            None
        );

        let c_context = ExtractionContext {
            file_path: PathBuf::from("/project/src/main.c"),
            project_root: PathBuf::from("/project"),
            current_module: Some("src/main.c".to_string()),
            ..Default::default()
        };
        assert_eq!(
            context.resolve_relative_import("./other.dart", Language::Dart),
            Some("src/utils/other.dart".to_string())
        );
        assert_eq!(
            c_context.resolve_relative_import("utils.h", Language::C),
            Some("src/utils.h".to_string())
        );
        assert_eq!(
            c_context.resolve_relative_import("<stdio.h>", Language::C),
            None
        );
        assert_eq!(
            c_context.resolve_relative_import("../common.h", Language::Cpp),
            Some("common.h".to_string())
        );

        let php_context = ExtractionContext {
            file_path: PathBuf::from("/project/app/Controller.php"),
            project_root: PathBuf::from("/project"),
            current_module: Some("app/Controller".to_string()),
            ..Default::default()
        };
        assert_eq!(
            php_context.resolve_relative_import("./config.php", Language::Php),
            Some("app/config.php".to_string())
        );
        let ruby_context = ExtractionContext {
            file_path: PathBuf::from("/project/lib/app.rb"),
            project_root: PathBuf::from("/project"),
            current_module: Some("lib/app".to_string()),
            ..Default::default()
        };
        assert_eq!(
            ruby_context.resolve_relative_import("helpers", Language::Ruby),
            Some("lib/helpers".to_string())
        );
        let bash_context = ExtractionContext {
            file_path: PathBuf::from("/project/scripts/run.sh"),
            project_root: PathBuf::from("/project"),
            current_module: Some("scripts/run".to_string()),
            ..Default::default()
        };
        assert_eq!(
            bash_context.resolve_relative_import("./lib.sh", Language::Bash),
            Some("scripts/lib.sh".to_string())
        );
        assert_eq!(
            bash_context.resolve_relative_import("/opt/common.sh", Language::Bash),
            None
        );
        let lua_context = ExtractionContext {
            file_path: PathBuf::from("/project/lua/main.lua"),
            project_root: PathBuf::from("/project"),
            current_module: Some("lua/main".to_string()),
            ..Default::default()
        };
        assert_eq!(
            lua_context.resolve_relative_import("./helper", Language::Lua),
            Some("lua/helper".to_string())
        );
    }

    #[test]
    fn test_jvm_and_csharp_have_no_relative_syntax() {
        let context = ExtractionContext {
            file_path: PathBuf::from("/project/src/com/example/App.java"),
            project_root: PathBuf::from("/project"),
            current_module: Some("com.example.App".to_string()),
            ..Default::default()
        };
        for language in [
            Language::Java,
            Language::Kotlin,
            Language::Scala,
            Language::CSharp,
        ] {
            assert_eq!(
                context.resolve_relative_import("com.example.Other", language),
                None,
                "{language:?} imports are absolute package paths"
            );
        }
    }

    #[test]
    fn test_is_internal_import_covers_new_languages() {
        let mut context = ExtractionContext {
            file_path: PathBuf::from("/project/src/app.ts"),
            project_root: PathBuf::from("/project"),
            current_module: Some("src/app".to_string()),
            package_declaration: Some("com.example".to_string()),
            config: ExtractionConfig {
                project_packages: vec!["com.example".to_string(), "myapp".to_string()],
                ..Default::default()
            },
        };
        assert!(context.is_internal_import("./utils", Language::TypeScript));
        assert!(context.is_internal_import("../shared", Language::JavaScript));
        assert!(!context.is_internal_import("lodash", Language::TypeScript));
        assert!(context.is_internal_import("com.example.Foo", Language::Java));
        assert!(context.is_internal_import("com.example.sub.Bar", Language::Kotlin));
        assert!(context.is_internal_import("myapp.models", Language::Python));
        assert!(context.is_internal_import(".models", Language::Python));
        assert!(context.is_internal_import("crate::foo", Language::Rust));
        assert!(context.is_internal_import("self::foo", Language::Rust));
        assert!(context.is_internal_import("super::foo", Language::Rust));
        assert!(context.is_internal_import("myheader.h", Language::C));
        assert!(context.is_internal_import("./other.dart", Language::Dart));
        assert!(!context.is_internal_import("dart:core", Language::Dart));
        assert!(context.is_internal_import("./config.php", Language::Php));
        assert!(context.is_internal_import("./helper", Language::Ruby));
        assert!(context.is_internal_import("./helper", Language::Lua));
        assert!(context.is_internal_import("./lib.sh", Language::Bash));
        context.package_declaration = Some("MyApp.Services".to_string());
        assert!(context.is_internal_import("MyApp.Services.Foo", Language::CSharp));
        context.config.project_packages = vec!["package:myapp".to_string()];
        assert!(context.is_internal_import("package:myapp/foo.dart", Language::Dart));
        assert!(!context.is_internal_import("package:other/foo.dart", Language::Dart));
    }

    #[test]
    fn test_file_relative_rejects_undecidable_paths() {
        let context = ExtractionContext {
            file_path: PathBuf::from("/project/scripts/run.sh"),
            project_root: PathBuf::from("/project"),
            current_module: Some("scripts/run".to_string()),
            ..Default::default()
        };
        assert_eq!(
            context.resolve_relative_import("$BASE/lib.sh", Language::Bash),
            None
        );
        assert_eq!(
            context.resolve_relative_import("./lib/*.sh", Language::Bash),
            None
        );
        assert_eq!(
            context.resolve_relative_import("../../escape.sh", Language::Bash),
            None
        );
    }
}
