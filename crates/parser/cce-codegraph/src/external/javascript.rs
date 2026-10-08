//! JavaScript / TypeScript module handler
//!
//! Parses npm packages and ES modules to discover exported symbols via
//! `package.json` and entry file analysis.

use std::path::{Path, PathBuf};

use super::{ExportedSymbol, ModuleInfo, ModuleType};
use cce_types::entity::EntityKind;
use cce_types::language::Language;

/// Handler for JavaScript / TypeScript packages.
#[derive(Debug, Default)]
pub struct JavaScriptModuleHandler {
    package_info: HashMap<String, PackageInfo>,
    entry_exports: HashMap<String, Vec<String>>,
}

use std::collections::HashMap;

/// Information from `package.json`.
#[derive(Debug, Clone)]
pub struct PackageInfo {
    pub name: String,
    pub version: Option<String>,
    pub main: String,
    pub module: Option<String>,
    pub types: Option<String>,
    pub exports: Vec<String>,
}

impl JavaScriptModuleHandler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse an npm package directory.
    pub fn parse_package(
        &mut self,
        package_path: &Path,
        language: Language,
    ) -> Result<ModuleInfo, crate::error::RelationError> {
        let name = package_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "js_package".to_string());
        let mut info = ModuleInfo::new(
            name.clone(),
            package_path.to_path_buf(),
            language,
            ModuleType::Package,
        );

        // Single file case (e.g. direct import of a .js file)
        if package_path.is_file() {
            if let Ok(content) = std::fs::read_to_string(package_path) {
                for exported in Self::extract_exports_from_source(&content) {
                    info.exports
                        .push(ExportedSymbol::new(exported, EntityKind::Function));
                }
                if info.exports.is_empty() {
                    info.exports
                        .push(ExportedSymbol::new(name.clone(), EntityKind::Module));
                }
            }
            return Ok(info);
        }

        if !package_path.is_dir() {
            return Ok(info);
        }

        // 1. Parse package.json if present
        let package_json_path = package_path.join("package.json");
        let pkg_info = if package_json_path.exists() {
            match Self::parse_package_json(&package_json_path) {
                Ok(pkg) => {
                    self.package_info
                        .insert(package_path.to_string_lossy().to_string(), pkg.clone());
                    Some(pkg)
                }
                Err(_) => None,
            }
        } else {
            None
        };

        if let Some(pkg) = &pkg_info {
            // Use package.json name as canonical module name if available
            info.name = pkg.name.clone();
            for exported in &pkg.exports {
                info.exports
                    .push(ExportedSymbol::new(exported.clone(), EntityKind::Function));
            }
            info.dependencies
                .extend(Self::extract_dependencies_from_package_json(package_path));
        }

        // 2. Parse entry files
        let entry_candidates = Self::entry_candidates(package_path, pkg_info.as_ref());
        for entry in entry_candidates {
            if entry.exists() {
                if let Ok(content) = std::fs::read_to_string(&entry) {
                    let exports = Self::extract_exports_from_source(&content);
                    self.entry_exports
                        .insert(entry.to_string_lossy().to_string(), exports.clone());
                    for exported in exports {
                        if !info.exports.iter().any(|e| e.name == exported) {
                            info.exports
                                .push(ExportedSymbol::new(exported, EntityKind::Function));
                        }
                    }
                    // Collect re-exported dependencies for module_dependencies depth
                    for dep in Self::extract_import_sources(&content) {
                        if !info.dependencies.contains(&dep) {
                            info.dependencies.push(dep);
                        }
                    }
                }
            }
        }

        // 3. Fallback: scan top-level js/ts files for exports when no entry found
        if info.exports.is_empty() {
            for js_file in collect_js_files(package_path) {
                if let Ok(content) = std::fs::read_to_string(&js_file) {
                    for exported in Self::extract_exports_from_source(&content) {
                        if !info.exports.iter().any(|e| e.name == exported) {
                            info.exports
                                .push(ExportedSymbol::new(exported, EntityKind::Function));
                        }
                    }
                }
            }
        }

        if info.exports.is_empty() {
            info.exports
                .push(ExportedSymbol::new(info.name.clone(), EntityKind::Module));
        }

        Ok(info)
    }

    /// Parse `package.json` and return package information.
    pub fn parse_package_json(path: &Path) -> Result<PackageInfo, crate::error::RelationError> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            crate::error::RelationError::Index(crate::error::IndexError::InconsistentState(
                format!("failed to read package.json {}: {e}", path.display()),
            ))
        })?;
        let value: serde_json::Value = serde_json::from_str(&content).map_err(|e| {
            crate::error::RelationError::Index(crate::error::IndexError::InconsistentState(
                format!("invalid package.json {}: {e}", path.display()),
            ))
        })?;

        let name = value
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let version = value
            .get("version")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let main = value
            .get("main")
            .and_then(|v| v.as_str())
            .unwrap_or("index.js")
            .to_string();
        let module = value
            .get("module")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let types = value
            .get("types")
            .or_else(|| value.get("typings"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let exports = Self::extract_exports_from_package_json(&value);

        Ok(PackageInfo {
            name,
            version,
            main,
            module,
            types,
            exports,
        })
    }

    /// Extract export names from source content.
    ///
    /// Handles:
    /// - `export function foo()`
    /// - `export const bar =`
    /// - `export class Baz`
    /// - `export { a, b as c }`
    /// - `export default ...`
    /// - `module.exports = { ... }`
    pub fn extract_exports_from_source(content: &str) -> Vec<String> {
        let mut exports = Vec::new();

        // export function/class/const/let/var/interface/type name
        let named_re = regex::Regex::new(
            r"(?m)^\s*export\s+(?:default\s+)?(?:async\s+)?(?:function|class|const|let|var|interface|type|enum)\s+([A-Za-z_][A-Za-z0-9_]*)\b",
        )
        .unwrap();
        for caps in named_re.captures_iter(content) {
            if let Some(m) = caps.get(1) {
                let name = m.as_str().to_string();
                if !exports.contains(&name) {
                    exports.push(name);
                }
            }
        }

        // export { a, b, c as d }
        let reexport_re = regex::Regex::new(r"export\s*\{([^}]+)\}").unwrap();
        for caps in reexport_re.captures_iter(content) {
            if let Some(m) = caps.get(1) {
                for part in m.as_str().split(',') {
                    let trimmed = part.trim();
                    // Handle `b as c` -> export name is `c`
                    let exported = if let Some(idx) = trimmed.find(" as ") {
                        trimmed[idx + 4..].trim()
                    } else {
                        trimmed
                    };
                    let cleaned = exported.trim().to_string();
                    if !cleaned.is_empty() && !exports.contains(&cleaned) {
                        exports.push(cleaned);
                    }
                }
            }
        }

        // export default -> treat as "default"
        if content.contains("export default") && !exports.contains(&"default".to_string()) {
            exports.push("default".to_string());
        }

        // module.exports = { a, b } or exports.a = ...
        let cjs_re = regex::Regex::new(r"module\.exports\s*=\s*\{([^}]+)\}").unwrap();
        for caps in cjs_re.captures_iter(content) {
            if let Some(m) = caps.get(1) {
                for part in m.as_str().split(',') {
                    let trimmed = part.trim();
                    // Handle `a: value` or `a`
                    let name = if let Some(colon) = trimmed.find(':') {
                        trimmed[..colon].trim()
                    } else {
                        trimmed
                    };
                    let cleaned = name.trim().to_string();
                    if !cleaned.is_empty() && !exports.contains(&cleaned) {
                        exports.push(cleaned);
                    }
                }
            }
        }

        let exports_assign_re =
            regex::Regex::new(r"exports\.([A-Za-z_][A-Za-z0-9_]*)\s*=").unwrap();
        for caps in exports_assign_re.captures_iter(content) {
            if let Some(m) = caps.get(1) {
                let name = m.as_str().to_string();
                if !exports.contains(&name) {
                    exports.push(name);
                }
            }
        }

        exports
    }

    /// Extract import sources from JS/TS content.
    pub fn extract_import_sources(content: &str) -> Vec<String> {
        let mut sources = Vec::new();
        let import_re =
            regex::Regex::new(r#"(?:import\s+(?:[^'"]+\s+from\s+)?["']([^'"]+)["']|require\s*\(\s*["']([^'"]+)["']\s*\))"#)
                .unwrap();
        for caps in import_re.captures_iter(content) {
            let source = caps
                .get(1)
                .or_else(|| caps.get(2))
                .map(|m| m.as_str().to_string());
            if let Some(s) = source {
                if !sources.contains(&s) {
                    sources.push(s);
                }
            }
        }
        sources
    }

    fn extract_exports_from_package_json(value: &serde_json::Value) -> Vec<String> {
        let mut exports = Vec::new();
        if let Some(obj) = value.get("exports").and_then(|v| v.as_object()) {
            for key in obj.keys() {
                let cleaned = key
                    .trim_start_matches('.')
                    .trim_start_matches('/')
                    .to_string();
                if !cleaned.is_empty() && cleaned != "." {
                    // keys like "./utils" -> export "utils"
                    let name = cleaned.trim_start_matches("./").to_string();
                    if !exports.contains(&name) {
                        exports.push(name);
                    }
                }
            }
        }
        exports
    }

    fn extract_dependencies_from_package_json(package_path: &Path) -> Vec<String> {
        let package_json_path = package_path.join("package.json");
        if let Ok(content) = std::fs::read_to_string(&package_json_path) {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) {
                let mut deps = Vec::new();
                for field in ["dependencies", "peerDependencies", "devDependencies"] {
                    if let Some(obj) = value.get(field).and_then(|v| v.as_object()) {
                        for key in obj.keys() {
                            if !deps.contains(key) {
                                deps.push(key.clone());
                            }
                        }
                    }
                }
                return deps;
            }
        }
        Vec::new()
    }

    fn entry_candidates(package_path: &Path, pkg_info: Option<&PackageInfo>) -> Vec<PathBuf> {
        let mut candidates = Vec::new();
        if let Some(pkg) = pkg_info {
            candidates.push(package_path.join(&pkg.main));
            if let Some(module) = &pkg.module {
                candidates.push(package_path.join(module));
            }
            if let Some(types) = &pkg.types {
                candidates.push(package_path.join(types));
            }
        }
        // Fallback entries
        for fallback in [
            "index.js",
            "index.ts",
            "lib/index.js",
            "src/index.js",
            "src/index.ts",
            "dist/index.js",
        ] {
            let p = package_path.join(fallback);
            if !candidates.contains(&p) {
                candidates.push(p);
            }
        }
        candidates
    }

    /// Get cached package info.
    pub fn package_info(&self) -> &HashMap<String, PackageInfo> {
        &self.package_info
    }

    /// Get cached entry exports.
    pub fn entry_exports(&self) -> &HashMap<String, Vec<String>> {
        &self.entry_exports
    }
}

fn collect_js_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if matches!(ext, "js" | "ts" | "jsx" | "tsx" | "mjs" | "cjs") {
                        files.push(path);
                    }
                }
            }
        }
    }
    files.sort();
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_named_exports() {
        let content = r#"
            export function foo() {}
            export const bar = 42;
            export class Baz {}
            export interface MyInterface {}
        "#;
        let exports = JavaScriptModuleHandler::extract_exports_from_source(content);
        assert!(exports.contains(&"foo".to_string()));
        assert!(exports.contains(&"bar".to_string()));
        assert!(exports.contains(&"Baz".to_string()));
        assert!(exports.contains(&"MyInterface".to_string()));
    }

    #[test]
    fn test_extract_reexports() {
        let content = r#"export { a, b as c, d }"#;
        let exports = JavaScriptModuleHandler::extract_exports_from_source(content);
        assert!(exports.contains(&"a".to_string()));
        assert!(exports.contains(&"c".to_string()));
        assert!(exports.contains(&"d".to_string()));
    }

    #[test]
    fn test_extract_default_export() {
        let content = "export default function myFunc() {}";
        let exports = JavaScriptModuleHandler::extract_exports_from_source(content);
        assert!(exports.contains(&"myFunc".to_string()));
        assert!(exports.contains(&"default".to_string()));
    }

    #[test]
    fn test_extract_cjs_exports() {
        let content = r#"
            module.exports = { foo, bar: baz }
            exports.qux = 42;
        "#;
        let exports = JavaScriptModuleHandler::extract_exports_from_source(content);
        assert!(exports.contains(&"foo".to_string()));
        assert!(exports.contains(&"bar".to_string()));
        assert!(exports.contains(&"qux".to_string()));
    }

    #[test]
    fn test_extract_import_sources() {
        let content = r#"
            import React from 'react';
            import { useState } from "react";
            const fs = require('fs');
            import utils from './utils';
        "#;
        let sources = JavaScriptModuleHandler::extract_import_sources(content);
        assert!(sources.contains(&"react".to_string()));
        assert!(sources.contains(&"fs".to_string()));
        assert!(sources.contains(&"./utils".to_string()));
    }

    #[test]
    fn test_parse_js_package_temp() {
        let dir = std::env::temp_dir().join("cce_js_test_pkg");
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(
            dir.join("package.json"),
            r#"{"name": "test-pkg", "version": "1.0.0", "main": "index.js"}"#,
        );
        let _ = std::fs::write(
            dir.join("index.js"),
            "export function hello() {}\nexport const world = 1;\n",
        );
        let mut handler = JavaScriptModuleHandler::new();
        let info = handler
            .parse_package(&dir, Language::JavaScript)
            .expect("parse should succeed");
        assert_eq!(info.name, "test-pkg");
        assert!(info.exports.iter().any(|e| e.name == "hello"));
        assert!(info.exports.iter().any(|e| e.name == "world"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_parse_package_json() {
        let dir = std::env::temp_dir().join("cce_js_pkg_json_test");
        let _ = std::fs::create_dir_all(&dir);
        let pkg_path = dir.join("package.json");
        let _ = std::fs::write(
            &pkg_path,
            r#"{"name": "my-lib", "version": "2.0.0", "main": "lib/index.js", "module": "esm/index.js"}"#,
        );
        let pkg =
            JavaScriptModuleHandler::parse_package_json(&pkg_path).expect("parse should succeed");
        assert_eq!(pkg.name, "my-lib");
        assert_eq!(pkg.version, Some("2.0.0".to_string()));
        assert_eq!(pkg.main, "lib/index.js");
        assert_eq!(pkg.module, Some("esm/index.js".to_string()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
