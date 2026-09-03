use cce_types::language::Language;
use std::path::{Path, PathBuf};

use super::PackageDiscovery;
use crate::external::{ExternalLibraryRegistry, ModuleInfo};

pub struct RubyPackageProvider;

impl RubyPackageProvider {
    pub fn discover_package(
        &self,
        package_name: &str,
        project_root: &Path,
    ) -> Option<PackageDiscovery> {
        if let Some(d) = discover_bundler_gems(package_name, project_root) {
            return Some(d);
        }
        if let Some(d) = discover_ruby_gems(package_name) {
            return Some(d);
        }
        None
    }

    pub fn extract_symbols(
        &self,
        discovery: &PackageDiscovery,
        registry: &mut ExternalLibraryRegistry,
    ) -> Option<ModuleInfo> {
        registry
            .resolve_library(&discovery.path, Language::Ruby)
            .ok()
    }
}

fn discover_bundler_gems(package_name: &str, project_root: &Path) -> Option<PackageDiscovery> {
    for dir in &["vendor/bundle", ".bundle", "gems"] {
        let base = project_root.join(dir);
        if !base.is_dir() {
            continue;
        }
        if let Ok(entries) = std::fs::read_dir(&base) {
            for e in entries.flatten() {
                let n = e.file_name().to_string_lossy().to_string();
                if n == package_name || n.starts_with(&format!("{package_name}-")) {
                    let version = n
                        .strip_prefix(&format!("{package_name}-"))
                        .map(|v| v.to_string());
                    return Some(PackageDiscovery {
                        package_name: package_name.to_string(),
                        path: e.path(),
                        version,
                    });
                }
            }
        }
    }
    None
}

fn discover_ruby_gems(package_name: &str) -> Option<PackageDiscovery> {
    if let Ok(output) = std::process::Command::new("gem")
        .args(["env", "gemdir"])
        .output()
    {
        let dir = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let gem_path = PathBuf::from(dir).join("gems");
        if gem_path.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&gem_path) {
                for e in entries.flatten() {
                    let n = e.file_name().to_string_lossy().to_string();
                    if n == package_name || n.starts_with(&format!("{package_name}-")) {
                        let version = n
                            .strip_prefix(&format!("{package_name}-"))
                            .map(|v| v.to_string());
                        return Some(PackageDiscovery {
                            package_name: package_name.to_string(),
                            path: e.path(),
                            version,
                        });
                    }
                }
            }
        }
    }
    let home = dirs::home_dir()?;
    for base in [
        home.join(".gem").join("ruby"),
        home.join(".rvm").join("gems"),
        home.join(".rbenv").join("versions"),
    ] {
        if base.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&base) {
                for e in entries.flatten() {
                    let n = e.file_name().to_string_lossy().to_string();
                    if n.contains(package_name) && e.path().is_dir() {
                        return Some(PackageDiscovery {
                            package_name: package_name.to_string(),
                            path: e.path(),
                            version: None,
                        });
                    }
                }
            }
        }
    }
    None
}
