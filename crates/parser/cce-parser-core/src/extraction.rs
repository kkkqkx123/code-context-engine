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
    pub fn resolve_relative_import(
        &self,
        relative_path: &str,
        language: Language,
    ) -> Option<String> {
        match language {
            Language::Python => self.resolve_python_relative(relative_path),
            Language::JavaScript | Language::TypeScript => self.resolve_js_relative(relative_path),
            Language::Rust => self.resolve_rust_relative(relative_path),
            Language::Go => self.resolve_go_relative(relative_path),
            _ => None,
        }
    }

    /// Classify an import as internal or external.
    pub fn is_internal_import(&self, import_path: &str, language: Language) -> bool {
        if let Some(ref package) = self.package_declaration {
            match language {
                Language::Java | Language::Kotlin | Language::Scala => {
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
                Language::Go => {}
                _ => {}
            }
        }

        for project_pkg in &self.config.project_packages {
            match language {
                Language::Java | Language::Kotlin | Language::Scala => {
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
                Language::Rust
                    if import_path.starts_with("crate::")
                        || import_path.starts_with(project_pkg) =>
                {
                    return true;
                }
                _ => {}
            }
        }

        false
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
        let current_dir = self.file_path.parent()?;
        let resolved = current_dir.join(relative_path);
        let relative = resolved.strip_prefix(&self.project_root).ok()?;
        Some(relative.to_string_lossy().replace('\\', "/"))
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
        Language::JavaScript
        | Language::TypeScript
        | Language::Jsx
        | Language::Tsx
        | Language::Ruby
        | Language::Php
        | Language::Dart
        | Language::Bash
        | Language::Lua
        | Language::C
        | Language::Cpp
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
}
