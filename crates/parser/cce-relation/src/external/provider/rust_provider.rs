use cce_types::language::Language;
use std::path::{Path, PathBuf};

use super::PackageDiscovery;
use crate::external::{ExternalLibraryRegistry, ModuleInfo};

pub struct RustPackageProvider;

impl RustPackageProvider {
    pub fn discover_package(
        &self,
        package_name: &str,
        project_root: &Path,
    ) -> Option<PackageDiscovery> {
        // Strategy 1: Check Cargo registry cache (~/.cargo/registry/src/)
        if let Some(discovery) = discover_cargo_registry(package_name) {
            return Some(discovery);
        }

        // Strategy 2: Check target directory for compiled artifacts
        if let Some(discovery) = discover_cargo_target(package_name, project_root) {
            return Some(discovery);
        }

        // Strategy 3: Check workspace members (path dependencies)
        if let Some(discovery) = discover_cargo_path_dep(package_name, project_root) {
            return Some(discovery);
        }

        None
    }

    pub fn extract_symbols(
        &self,
        discovery: &PackageDiscovery,
        registry: &mut ExternalLibraryRegistry,
    ) -> Option<ModuleInfo> {
        registry
            .resolve_library(&discovery.path, Language::Rust)
            .ok()
    }
}

/// Discover a Rust package from the Cargo registry cache.
fn discover_cargo_registry(package_name: &str) -> Option<PackageDiscovery> {
    let home = dirs::home_dir()?;
    let registry_src = home.join(".cargo").join("registry").join("src");

    if !registry_src.is_dir() {
        return None;
    }

    let mut best_match: Option<PackageDiscovery> = None;
    if let Ok(entries) = std::fs::read_dir(&registry_src) {
        for entry in entries.flatten() {
            if !entry.path().is_dir() {
                continue;
            }
            if let Ok(pkg_entries) = std::fs::read_dir(entry.path()) {
                for pkg_entry in pkg_entries.flatten() {
                    let dir_name = pkg_entry.file_name().to_string_lossy().to_string();
                    if dir_name == package_name || dir_name.starts_with(&format!("{package_name}-"))
                    {
                        let version = dir_name
                            .strip_prefix(&format!("{package_name}-"))
                            .map(|v| v.to_string());

                        if dir_name == package_name {
                            return Some(PackageDiscovery {
                                package_name: package_name.to_string(),
                                path: pkg_entry.path(),
                                version,
                            });
                        }
                        if best_match.is_none() {
                            best_match = Some(PackageDiscovery {
                                package_name: package_name.to_string(),
                                path: pkg_entry.path(),
                                version,
                            });
                        }
                    }
                }
            }
        }
    }

    best_match
}

/// Discover a Rust package from the project's target directory.
fn discover_cargo_target(package_name: &str, project_root: &Path) -> Option<PackageDiscovery> {
    let target_dir = project_root.join("target");
    if !target_dir.is_dir() {
        return None;
    }

    for profile in &["debug", "release"] {
        let dep_dir = target_dir.join(profile).join("deps");
        if !dep_dir.is_dir() {
            continue;
        }
        if let Ok(entries) = std::fs::read_dir(&dep_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with(&format!("{package_name}-")) && name.ends_with(".d") {
                    if let Ok(content) = std::fs::read_to_string(entry.path()) {
                        if let Some(source_path) = parse_cargo_dep_file(&content, package_name) {
                            return Some(PackageDiscovery {
                                package_name: package_name.to_string(),
                                path: source_path,
                                version: None,
                            });
                        }
                    }
                }
            }
        }
    }

    None
}

/// Parse a Cargo .d dependency file to find the package source path.
fn parse_cargo_dep_file(content: &str, package_name: &str) -> Option<PathBuf> {
    for line in content.lines() {
        if let Some(targets_end) = line.find(": ") {
            let deps_part = &line[targets_end + 2..];
            for dep in deps_part.split(' ') {
                let dep = dep.trim();
                if dep.contains(package_name) && dep.contains("/src/") {
                    let path = Path::new(dep);
                    if let Some(pkg_root) = find_cargo_package_root(path) {
                        return Some(pkg_root.to_path_buf());
                    }
                }
            }
        }
    }
    None
}

/// Walk up from a source file to find the Cargo package root (contains Cargo.toml).
fn find_cargo_package_root(source_path: &Path) -> Option<&Path> {
    let mut current = source_path.parent()?;
    loop {
        if current.join("Cargo.toml").is_file() {
            return Some(current);
        }
        current = current.parent()?;
    }
}

/// Discover a Rust package as a path dependency.
fn discover_cargo_path_dep(package_name: &str, project_root: &Path) -> Option<PackageDiscovery> {
    let cargo_toml = project_root.join("Cargo.toml");
    if !cargo_toml.is_file() {
        return None;
    }

    let content = std::fs::read_to_string(&cargo_toml).ok()?;
    let manifest: toml::Value = toml::from_str(&content).ok()?;

    if let Some(deps) = manifest.get("dependencies") {
        if let Some(dep) = deps.get(package_name) {
            if let Some(path) = dep.get("path").and_then(|p| p.as_str()) {
                let dep_path = project_root.join(path);
                if dep_path.is_dir() && dep_path.join("Cargo.toml").is_file() {
                    return Some(PackageDiscovery {
                        package_name: package_name.to_string(),
                        path: dep_path,
                        version: dep
                            .get("version")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                    });
                }
            }
        }
    }

    None
}
