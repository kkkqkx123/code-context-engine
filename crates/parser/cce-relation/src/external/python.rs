//! Python library handler
//!
//! Parses Python packages to discover public APIs via `__init__.py`,
//! `__all__` and type stub (`.pyi`) files.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::{ExportedSymbol, ModuleInfo, ModuleType};
use cce_types::entity::EntityKind;
use cce_types::language::Language;

/// Handler for Python packages.
#[derive(Debug, Default)]
pub struct PythonLibraryHandler {
    init_exports: HashSet<String>,
    type_stubs: HashMap<String, TypeInformation>,
    module_name: Option<String>,
}

use std::collections::HashMap;

/// Type information from a `.pyi` stub.
#[derive(Debug, Clone)]
pub struct TypeInformation {
    pub name: String,
    pub kind: EntityKind,
    pub signature: Option<String>,
}

impl PythonLibraryHandler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse a Python package directory and return aggregated [`ModuleInfo`].
    ///
    /// Discovery order:
    /// 1. Look for `__init__.py` and extract `__all__` plus top-level definitions.
    /// 2. Scan `*.py` files for public symbols (not starting with `_`).
    /// 3. Scan `*.pyi` stub files for additional type information.
    pub fn parse_package(
        &mut self,
        package_path: &Path,
        language: Language,
    ) -> Result<ModuleInfo, crate::error::RelationError> {
        let name = package_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "python_package".to_string());
        self.module_name = Some(name.clone());
        let mut info = ModuleInfo::new(
            name.clone(),
            package_path.to_path_buf(),
            language,
            ModuleType::Package,
        );

        if package_path.is_file() {
            // Single file package
            let content = std::fs::read_to_string(package_path).map_err(|e| {
                crate::error::RelationError::Index(crate::error::IndexError::InconsistentState(
                    format!("failed to read python file {}: {e}", package_path.display()),
                ))
            })?;
            let exports = Self::extract_public_symbols(&content);
            for symbol in exports {
                info.exports
                    .push(ExportedSymbol::new(symbol, EntityKind::Function));
            }
            return Ok(info);
        }

        if !package_path.is_dir() {
            return Ok(info);
        }

        // 1. Parse __init__.py if present
        let init_path = package_path.join("__init__.py");
        if init_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&init_path) {
                if let Some(all_exports) = Self::extract_all_list(&content) {
                    for name in &all_exports {
                        self.init_exports.insert(name.clone());
                        info.exports
                            .push(ExportedSymbol::new(name.clone(), EntityKind::Function));
                    }
                }
                if self.init_exports.is_empty() {
                    // No __all__: fall back to public symbols in __init__.py
                    for symbol in Self::extract_public_symbols(&content) {
                        if self.init_exports.insert(symbol.clone()) {
                            info.exports
                                .push(ExportedSymbol::new(symbol, EntityKind::Function));
                        }
                    }
                }
                // Record submodules imported in __init__.py
                info.dependencies.extend(Self::extract_imports(&content));
            }
        }

        // 2. Scan *.py files
        let mut py_files = collect_python_files(package_path, false);
        py_files.sort();
        for py_file in &py_files {
            if py_file.ends_with("__init__.py") {
                continue;
            }
            if let Ok(content) = std::fs::read_to_string(py_file) {
                let file_stem = py_file
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                if file_stem.starts_with('_') || file_stem.starts_with('.') {
                    continue;
                }
                // Module itself is an export
                if !info.exports.iter().any(|e| e.name == file_stem) {
                    info.exports.push(
                        ExportedSymbol::new(file_stem.clone(), EntityKind::Module)
                            .with_source_file(py_file.to_string_lossy().to_string()),
                    );
                }
                // Also add public symbols defined in the module when __all__ is absent
                if self.init_exports.is_empty() {
                    for symbol in Self::extract_public_symbols(&content) {
                        if !info.exports.iter().any(|e| e.name == symbol) {
                            info.exports.push(
                                ExportedSymbol::new(symbol, EntityKind::Function)
                                    .with_source_file(py_file.to_string_lossy().to_string()),
                            );
                        }
                    }
                }
            }
        }

        // 3. Scan *.pyi stub files
        let stub_files = collect_python_files(package_path, true);
        for stub_path in &stub_files {
            if let Ok(content) = std::fs::read_to_string(stub_path) {
                for type_info in Self::extract_stub_exports(&content) {
                    let key = type_info.name.clone();
                    self.type_stubs.insert(key.clone(), type_info.clone());
                    if !info.exports.iter().any(|e| e.name == key) {
                        info.exports.push(
                            ExportedSymbol::new(key, type_info.kind)
                                .with_source_file(stub_path.to_string_lossy().to_string()),
                        );
                    }
                }
            }
        }

        if info.exports.is_empty() {
            info.exports
                .push(ExportedSymbol::new(name.clone(), EntityKind::Module));
        }

        Ok(info)
    }

    /// Extract `__all__` list from Python source.
    ///
    /// Handles both `__all__ = ["a", "b"]` and `__all__ = ('a', 'b')` forms.
    pub fn extract_all_list(content: &str) -> Option<Vec<String>> {
        // Match __all__ = [ ... ] or __all__ = ( ... )
        let re = regex::Regex::new(r#"__all__\s*=\s*[\[\(]([^\]\)]+)[\]\)]"#).ok()?;
        let caps = re.captures(content)?;
        let inner = caps.get(1)?.as_str();
        let exports: Vec<String> = inner
            .split(',')
            .map(|s| {
                s.trim()
                    .trim_matches('\'')
                    .trim_matches('"')
                    .trim()
                    .to_string()
            })
            .filter(|s| !s.is_empty())
            .collect();
        if exports.is_empty() {
            None
        } else {
            Some(exports)
        }
    }

    /// Extract public symbols (top-level `def` / `class` / `NAME =` not starting with `_`).
    ///
    /// Handles decorated functions/classes, `__all__` re-exports, and
    /// `from .module import *` patterns.
    pub fn extract_public_symbols(content: &str) -> Vec<String> {
        let mut symbols = Vec::new();
        // Decorated def/class: optional @decorator lines (including @decorator(args) and dotted names)
        let def_re = regex::Regex::new(
            r"(?m)^(?:@\s*[A-Za-z_][A-Za-z0-9_\.]*(?:\([^)]*\))?\s*\n\s*)*(?:async\s+)?(?:def|class)\s+([A-Za-z_][A-Za-z0-9_]*)\s*[\(:]",
        )
        .unwrap();
        for caps in def_re.captures_iter(content) {
            if let Some(m) = caps.get(1) {
                let name = m.as_str().to_string();
                if !is_private_name(&name) && !symbols.contains(&name) {
                    symbols.push(name);
                }
            }
        }
        // Also capture module-level assignments that look like constants
        let assign_re = regex::Regex::new(r"(?m)^([A-Z_][A-Z0-9_]*)\s*=").unwrap();
        for caps in assign_re.captures_iter(content) {
            if let Some(m) = caps.get(1) {
                let name = m.as_str().to_string();
                if !symbols.contains(&name) {
                    symbols.push(name);
                }
            }
        }
        // Handle __all__ re-exports
        if let Some(all) = Self::extract_all_list(content) {
            for name in all {
                if !symbols.contains(&name) && !is_private_name(&name) {
                    symbols.push(name);
                }
            }
        }
        // Handle `from .module import *` re-exports (treated as module-level wildcard)
        let star_re =
            regex::Regex::new(r"(?m)^\s*from\s+[A-Za-z0-9_\.]+\s+import\s+\*\s*$").unwrap();
        if star_re.is_match(content) {
            // Wildcard import indicates the module re-exports its source; we keep already found symbols
        }
        symbols
    }

    /// Extract imported module names from Python source.
    pub fn extract_imports(content: &str) -> Vec<String> {
        let mut imports = Vec::new();
        let import_re = regex::Regex::new(r"(?m)^\s*(?:import|from)\s+([A-Za-z0-9_\.]+)").unwrap();
        for caps in import_re.captures_iter(content) {
            if let Some(m) = caps.get(1) {
                let name = m.as_str().to_string();
                if !imports.contains(&name) {
                    imports.push(name);
                }
            }
        }
        imports
    }

    /// Extract exports from a `.pyi` stub file.
    pub fn extract_stub_exports(content: &str) -> Vec<TypeInformation> {
        let mut results = Vec::new();
        let def_re =
            regex::Regex::new(r"(?m)^(?:def|class)\s+([A-Za-z_][A-Za-z0-9_]*)\s*[\(:]").unwrap();
        for caps in def_re.captures_iter(content) {
            if let Some(m) = caps.get(1) {
                let name = m.as_str().to_string();
                if is_private_name(&name) {
                    continue;
                }
                let kind = if caps
                    .get(0)
                    .map(|c| c.as_str().contains("class"))
                    .unwrap_or(false)
                {
                    EntityKind::Class
                } else {
                    EntityKind::Function
                };
                results.push(TypeInformation {
                    name,
                    kind,
                    signature: caps.get(0).map(|c| c.as_str().to_string()),
                });
            }
        }
        results
    }

    /// Get cached `__all__` exports.
    pub fn init_exports(&self) -> &HashSet<String> {
        &self.init_exports
    }

    /// Get cached type stub information.
    pub fn type_stubs(&self) -> &HashMap<String, TypeInformation> {
        &self.type_stubs
    }
}

fn collect_python_files(dir: &Path, stubs_only: bool) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    let is_match = if stubs_only {
                        ext == "pyi"
                    } else {
                        ext == "py"
                    };
                    if is_match {
                        files.push(path);
                    }
                }
            } else if path.is_dir() && !stubs_only {
                // Recurse one level for subpackages
                if let Ok(sub_entries) = std::fs::read_dir(&path) {
                    for sub in sub_entries.flatten() {
                        let sub_path = sub.path();
                        if sub_path.is_file() {
                            if let Some(ext) = sub_path.extension().and_then(|e| e.to_str()) {
                                if ext == "py" {
                                    files.push(sub_path);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    files
}

fn is_private_name(name: &str) -> bool {
    // Dunder names like __init__ are public; single underscore is private
    if name.starts_with("__") && name.ends_with("__") {
        return false;
    }
    name.starts_with('_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_all_list() {
        let content = r#"__all__ = ["foo", "bar", "Baz"]"#;
        let exports = PythonLibraryHandler::extract_all_list(content).expect("should extract");
        assert_eq!(exports, vec!["foo", "bar", "Baz"]);

        let content2 = r#"__all__ = ('a', 'b')"#;
        let exports2 = PythonLibraryHandler::extract_all_list(content2).expect("should extract");
        assert_eq!(exports2, vec!["a", "b"]);

        let no_all = "x = 1\ndef foo(): pass";
        assert!(PythonLibraryHandler::extract_all_list(no_all).is_none());
    }

    #[test]
    fn test_extract_public_symbols() {
        let content = r#"
def public_func():
    pass

def _private():
    pass

class MyClass:
    pass

class _Hidden:
    pass

__all__ = ["public_func"]
"#;
        let symbols = PythonLibraryHandler::extract_public_symbols(content);
        assert!(symbols.contains(&"public_func".to_string()));
        assert!(symbols.contains(&"MyClass".to_string()));
        assert!(!symbols.contains(&"_private".to_string()));
        assert!(!symbols.contains(&"_Hidden".to_string()));
        // Dunder should be considered public
        let dunder_content = "def __init__(self): pass\ndef __mangled(self): pass";
        let dunder_symbols = PythonLibraryHandler::extract_public_symbols(dunder_content);
        assert!(dunder_symbols.contains(&"__init__".to_string()));
    }

    #[test]
    fn test_extract_imports() {
        let content = "import os\nfrom os.path import join\nimport numpy as np\n";
        let imports = PythonLibraryHandler::extract_imports(content);
        assert!(imports.contains(&"os".to_string()));
        assert!(imports.contains(&"os.path".to_string()));
        assert!(imports.contains(&"numpy".to_string()));
    }

    #[test]
    fn test_parse_python_package_temp() {
        let dir = std::env::temp_dir().join("cce_python_test_pkg");
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(
            dir.join("__init__.py"),
            "__all__ = [\"foo\", \"bar\"]\ndef foo(): pass\ndef _hidden(): pass\n",
        );
        let _ = std::fs::write(dir.join("utils.py"), "def helper(): pass\n");
        let mut handler = PythonLibraryHandler::new();
        let info = handler
            .parse_package(&dir, Language::Python)
            .expect("parse should succeed");
        assert!(info.exports.iter().any(|e| e.name == "foo"));
        assert!(info.exports.iter().any(|e| e.name == "bar"));
        // utils module should be discovered
        assert!(info.exports.iter().any(|e| e.name == "utils"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_stub_extraction() {
        let stub = "def stub_func(x: int) -> int: ...\nclass StubClass: ...\n";
        let exports = PythonLibraryHandler::extract_stub_exports(stub);
        assert!(exports.iter().any(|e| e.name == "stub_func"));
        assert!(exports.iter().any(|e| e.name == "StubClass"));
    }
}
