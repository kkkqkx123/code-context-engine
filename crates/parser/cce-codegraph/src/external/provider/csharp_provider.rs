use cce_types::language::Language;
use std::path::Path;

use super::PackageDiscovery;
use crate::external::{ExternalLibraryRegistry, ModuleInfo};

pub struct CSharpPackageProvider;

impl CSharpPackageProvider {
    pub fn discover_package(
        &self,
        package_name: &str,
        project_root: &Path,
    ) -> Option<PackageDiscovery> {
        if let Some(d) = discover_nuget_cache(package_name) {
            return Some(d);
        }
        let packages = project_root.join("packages").join(package_name);
        if packages.is_dir() {
            return Some(PackageDiscovery {
                package_name: package_name.to_string(),
                path: packages,
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
            .resolve_library(&discovery.path, Language::CSharp)
            .ok()
    }
}

fn discover_nuget_cache(package_name: &str) -> Option<PackageDiscovery> {
    let home = dirs::home_dir()?;
    let nuget = home.join(".nuget").join("packages");
    if !nuget.is_dir() {
        return None;
    }
    let lower = package_name.to_ascii_lowercase();
    if let Ok(entries) = std::fs::read_dir(&nuget) {
        for e in entries.flatten() {
            let n = e.file_name().to_string_lossy().to_ascii_lowercase();
            if n == lower {
                let version = std::fs::read_dir(e.path())
                    .ok()?
                    .flatten()
                    .filter_map(|v| {
                        let s = v.file_name().to_string_lossy().to_string();
                        if v.path().is_dir() { Some(s) } else { None }
                    })
                    .max();
                return Some(PackageDiscovery {
                    package_name: package_name.to_string(),
                    path: e.path(),
                    version,
                });
            }
        }
    }
    None
}
