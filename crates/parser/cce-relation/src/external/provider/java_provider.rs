use cce_types::language::Language;
use std::path::Path;

use super::PackageDiscovery;
use crate::external::{ExternalLibraryRegistry, ModuleInfo};

pub struct JavaPackageProvider;

impl JavaPackageProvider {
    pub fn discover_package(
        &self,
        package_name: &str,
        project_root: &Path,
    ) -> Option<PackageDiscovery> {
        if let Some(d) = discover_maven_local(package_name) {
            return Some(d);
        }
        if let Some(d) = discover_gradle_cache(package_name) {
            return Some(d);
        }
        let vendor = project_root.join("vendor").join(package_name);
        if vendor.is_dir() {
            return Some(PackageDiscovery {
                package_name: package_name.to_string(),
                path: vendor,
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
        registry
            .resolve_library(&discovery.path, Language::Java)
            .ok()
    }
}

fn discover_maven_local(package_name: &str) -> Option<PackageDiscovery> {
    let home = dirs::home_dir()?;
    let m2 = home.join(".m2").join("repository");
    if !m2.is_dir() {
        return None;
    }
    let group_path = package_name.replace('.', "/");
    let pkg_path = m2.join(&group_path);
    if pkg_path.is_dir() {
        let version = std::fs::read_dir(&pkg_path)
            .ok()?
            .flatten()
            .filter_map(|e| {
                let n = e.file_name().to_string_lossy().to_string();
                if e.path().is_dir() && n.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                    Some(n)
                } else {
                    None
                }
            })
            .max();
        return Some(PackageDiscovery {
            package_name: package_name.to_string(),
            path: pkg_path,
            version,
        });
    }
    // Fallback: search by artifact name
    if let Ok(entries) = std::fs::read_dir(&m2) {
        for e in entries.flatten() {
            if e.file_name().to_string_lossy().contains(package_name) {
                let p = e.path();
                if p.is_dir() {
                    return Some(PackageDiscovery {
                        package_name: package_name.to_string(),
                        path: p,
                        version: None,
                    });
                }
            }
        }
    }
    None
}

fn discover_gradle_cache(package_name: &str) -> Option<PackageDiscovery> {
    let home = dirs::home_dir()?;
    let gradle = home
        .join(".gradle")
        .join("caches")
        .join("modules-2")
        .join("files-2.1");
    if !gradle.is_dir() {
        return None;
    }
    let group_path = package_name.replace('.', "/");
    let candidates = [gradle.join(&group_path), gradle.join(package_name)];
    for cand in candidates {
        if cand.is_dir() {
            return Some(PackageDiscovery {
                package_name: package_name.to_string(),
                path: cand,
                version: None,
            });
        }
    }
    None
}
