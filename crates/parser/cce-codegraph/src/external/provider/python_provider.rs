use cce_types::language::Language;
use std::path::{Path, PathBuf};

use super::PackageDiscovery;
use crate::external::{ExternalLibraryRegistry, ModuleInfo};

pub struct PythonPackageProvider;

impl PythonPackageProvider {
    pub fn discover_package(
        &self,
        package_name: &str,
        project_root: &Path,
    ) -> Option<PackageDiscovery> {
        // Strategy 1: Check virtual environment (venv/.venv)
        if let Some(discovery) = discover_venv_package(package_name, project_root) {
            return Some(discovery);
        }

        // Strategy 2: Check system/site-packages
        if let Some(discovery) = discover_site_packages(package_name) {
            return Some(discovery);
        }

        // Strategy 3: Check user site-packages
        if let Some(discovery) = discover_user_site_packages(package_name) {
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
            .resolve_library(&discovery.path, Language::Python)
            .ok()
    }
}

/// Discover a Python package in a virtual environment.
fn discover_venv_package(package_name: &str, project_root: &Path) -> Option<PackageDiscovery> {
    for venv_name in &["venv", ".venv", "env", ".env"] {
        let site_packages = project_root.join(venv_name).join("lib");
        if !site_packages.is_dir() {
            continue;
        }
        if let Ok(lib_entries) = std::fs::read_dir(&site_packages) {
            for lib_entry in lib_entries.flatten() {
                let sp = lib_entry.path().join("site-packages");
                if !sp.is_dir() {
                    continue;
                }
                let pkg_path = sp.join(package_name);
                if pkg_path.is_dir() {
                    let version = extract_python_version_from_path(&pkg_path);
                    return Some(PackageDiscovery {
                        package_name: package_name.to_string(),
                        path: pkg_path,
                        version,
                    });
                }
                if let Ok(sp_entries) = std::fs::read_dir(&sp) {
                    for sp_entry in sp_entries.flatten() {
                        let dir_name = sp_entry.file_name().to_string_lossy().to_string();
                        if dir_name.starts_with(&format!("{package_name}-"))
                            && dir_name.ends_with(".dist-info")
                        {
                            let version = dir_name
                                .strip_prefix(&format!("{package_name}-"))
                                .and_then(|v| v.strip_suffix(".dist-info"))
                                .map(|v| v.to_string());
                            return Some(PackageDiscovery {
                                package_name: package_name.to_string(),
                                path: sp_entry.path(),
                                version,
                            });
                        }
                    }
                }
            }
        }
    }
    None
}

/// Discover a Python package in system site-packages.
fn discover_site_packages(package_name: &str) -> Option<PackageDiscovery> {
    let site_dirs = get_python_site_packages_dirs();
    for site_dir in site_dirs {
        let pkg_path = site_dir.join(package_name);
        if pkg_path.is_dir() {
            let version = extract_python_version_from_path(&pkg_path);
            return Some(PackageDiscovery {
                package_name: package_name.to_string(),
                path: pkg_path,
                version,
            });
        }
    }
    None
}

/// Discover a Python package in user site-packages (~/.local/lib/pythonX.Y/site-packages).
fn discover_user_site_packages(package_name: &str) -> Option<PackageDiscovery> {
    let home = dirs::home_dir()?;
    let local_lib = home.join(".local").join("lib");
    if !local_lib.is_dir() {
        return None;
    }
    if let Ok(entries) = std::fs::read_dir(&local_lib) {
        for entry in entries.flatten() {
            let dir_name = entry.file_name().to_string_lossy().to_string();
            if dir_name.starts_with("python") {
                let sp = entry.path().join("site-packages");
                if sp.is_dir() {
                    let pkg_path = sp.join(package_name);
                    if pkg_path.is_dir() {
                        let version = extract_python_version_from_path(&pkg_path);
                        return Some(PackageDiscovery {
                            package_name: package_name.to_string(),
                            path: pkg_path,
                            version,
                        });
                    }
                }
            }
        }
    }
    None
}

/// Get potential site-packages directories from the system Python.
fn get_python_site_packages_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Ok(output) = std::process::Command::new("python3")
        .args([
            "-c",
            "import site; print('\\n'.join(site.getsitepackages()))",
        ])
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let path = PathBuf::from(line.trim());
            if path.is_dir() {
                dirs.push(path);
            }
        }
    }

    if dirs.is_empty() {
        for entry in &[
            PathBuf::from("/usr/lib/python3/site-packages"),
            PathBuf::from("/usr/lib/python3.11/site-packages"),
            PathBuf::from("/usr/lib/python3.12/site-packages"),
            PathBuf::from("/usr/local/lib/python3/site-packages"),
            PathBuf::from("/usr/local/lib/python3.11/site-packages"),
            PathBuf::from("/usr/local/lib/python3.12/site-packages"),
        ] {
            if entry.is_dir() {
                dirs.push(entry.clone());
            }
        }
    }

    dirs
}

/// Extract Python version from a package path (best-effort from .dist-info).
fn extract_python_version_from_path(path: &Path) -> Option<String> {
    for meta_file in &["PKG-INFO", "METADATA"] {
        let meta_path = path.join(meta_file);
        if let Ok(content) = std::fs::read_to_string(&meta_path) {
            for line in content.lines() {
                if let Some(version) = line.strip_prefix("Version: ") {
                    return Some(version.trim().to_string());
                }
            }
        }
    }
    None
}
