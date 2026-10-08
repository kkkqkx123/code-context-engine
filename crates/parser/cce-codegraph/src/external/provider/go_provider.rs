use cce_types::language::Language;
use std::path::{Path, PathBuf};

use super::PackageDiscovery;
use crate::external::{ExternalLibraryRegistry, ModuleInfo};

pub struct GoPackageProvider;

impl GoPackageProvider {
    pub fn discover_package(
        &self,
        package_name: &str,
        project_root: &Path,
    ) -> Option<PackageDiscovery> {
        if let Some(discovery) = discover_go_module_cache(package_name) {
            return Some(discovery);
        }

        let vendor_path = project_root.join("vendor").join(package_name);
        if vendor_path.is_dir() {
            return Some(PackageDiscovery {
                package_name: package_name.to_string(),
                path: vendor_path,
                version: None,
            });
        }

        None
    }

    pub fn extract_symbols(
        &self,
        discovery: &PackageDiscovery,
        registry: &mut ExternalLibraryRegistry,
    ) -> Option<ModuleInfo> {
        if let Some(info) = self.extract_go_symbols(discovery) {
            return Some(info);
        }
        registry.resolve_library(&discovery.path, Language::Go).ok()
    }

    fn extract_go_symbols(&self, discovery: &PackageDiscovery) -> Option<ModuleInfo> {
        use crate::external::ExportedSymbol;
        use cce_types::entity::EntityKind;

        let path = &discovery.path;
        let mut info = crate::external::ModuleInfo::new(
            discovery.package_name.clone(),
            path.clone(),
            Language::Go,
            crate::external::ModuleType::Package,
        );
        let go_files = if path.is_file() {
            vec![path.clone()]
        } else {
            collect_go_files(path)
        };
        for file in go_files {
            let content = std::fs::read_to_string(&file).ok()?;
            if is_go_build_ignored(&content) {
                continue;
            }
            for sym in extract_go_symbols_from_content(&content) {
                let kind = match sym.kind.as_str() {
                    "func" => EntityKind::Function,
                    "type" => EntityKind::Class,
                    "var" => EntityKind::Variable,
                    "const" => EntityKind::Constant,
                    _ => EntityKind::Function,
                };
                if !info.exports.iter().any(|e| e.name == sym.name) {
                    info.exports.push(
                        ExportedSymbol::new(sym.name, kind)
                            .with_source_file(file.to_string_lossy().to_string()),
                    );
                }
            }
            // Also treat the file stem as a module export if not already
            if let Some(stem) = file.file_stem().and_then(|s| s.to_str()) {
                if !stem.starts_with('_') && !info.exports.iter().any(|e| e.name == stem) {
                    info.exports.push(
                        ExportedSymbol::new(stem.to_string(), EntityKind::Module)
                            .with_source_file(file.to_string_lossy().to_string()),
                    );
                }
            }
        }
        if info.exports.is_empty() {
            None
        } else {
            Some(info)
        }
    }
}

struct GoSymbol {
    name: String,
    kind: String,
}

fn collect_go_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_file() && p.extension().and_then(|s| s.to_str()) == Some("go") {
                files.push(p);
            } else if p.is_dir() {
                files.extend(collect_go_files(&p));
            }
        }
    }
    files
}

fn is_go_build_ignored(content: &str) -> bool {
    for line in content.lines().take(5) {
        let t = line.trim();
        if t.starts_with("//go:build") && t.contains("ignore") {
            return true;
        }
    }
    false
}

fn extract_go_symbols_from_content(content: &str) -> Vec<GoSymbol> {
    let mut syms = Vec::new();
    // func with optional receiver: func (r *Type) Name(
    let func_re =
        regex::Regex::new(r"(?m)^\s*func\s+(?:\([^)]+\)\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*\(")
            .unwrap();
    for caps in func_re.captures_iter(content) {
        if let Some(m) = caps.get(1) {
            syms.push(GoSymbol {
                name: m.as_str().to_string(),
                kind: "func".to_string(),
            });
        }
    }
    let type_re = regex::Regex::new(r"(?m)^\s*type\s+([A-Za-z_][A-Za-z0-9_]*)\s+").unwrap();
    for caps in type_re.captures_iter(content) {
        if let Some(m) = caps.get(1) {
            syms.push(GoSymbol {
                name: m.as_str().to_string(),
                kind: "type".to_string(),
            });
        }
    }
    let var_re = regex::Regex::new(r"(?m)^\s*var\s+([A-Za-z_][A-Za-z0-9_]*)\b").unwrap();
    for caps in var_re.captures_iter(content) {
        if let Some(m) = caps.get(1) {
            syms.push(GoSymbol {
                name: m.as_str().to_string(),
                kind: "var".to_string(),
            });
        }
    }
    let const_re = regex::Regex::new(r"(?m)^\s*const\s+([A-Za-z_][A-Za-z0-9_]*)\b").unwrap();
    for caps in const_re.captures_iter(content) {
        if let Some(m) = caps.get(1) {
            syms.push(GoSymbol {
                name: m.as_str().to_string(),
                kind: "const".to_string(),
            });
        }
    }
    // Short var declaration and const block handling could be added
    syms
}

/// Discover a Go package from the module cache.
fn discover_go_module_cache(package_name: &str) -> Option<PackageDiscovery> {
    let gopath = std::env::var("GOPATH")
        .ok()
        .map(PathBuf::from)
        .or_else(|| {
            let home = dirs::home_dir()?;
            Some(home.join("go").join("pkg").join("mod"))
        })?;

    let mod_dir = gopath.join("pkg").join("mod");
    if !mod_dir.is_dir() {
        return None;
    }

    if let Ok(entries) = std::fs::read_dir(&mod_dir) {
        for entry in entries.flatten() {
            let dir_name = entry.file_name().to_string_lossy().to_string();
            if dir_name == package_name || dir_name.starts_with(&format!("{package_name}@")) {
                let version = dir_name
                    .strip_prefix(&format!("{package_name}@"))
                    .map(|v| v.to_string());
                return Some(PackageDiscovery {
                    package_name: package_name.to_string(),
                    path: entry.path(),
                    version,
                });
            }
        }
    }

    None
}
