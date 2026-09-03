use cce_types::language::Language;
use std::path::{Path, PathBuf};

use super::PackageDiscovery;
use crate::external::{ExternalLibraryRegistry, ModuleInfo};

pub struct PhpPackageProvider;

impl PhpPackageProvider {
    pub fn discover_package(
        &self,
        package_name: &str,
        project_root: &Path,
    ) -> Option<PackageDiscovery> {
        if let Some(d) = discover_composer_vendor(package_name, project_root) {
            return Some(d);
        }
        if let Some(d) = discover_composer_installed_json(package_name, project_root) {
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
            .resolve_library(&discovery.path, Language::Php)
            .ok()
    }
}

fn discover_composer_vendor(package_name: &str, project_root: &Path) -> Option<PackageDiscovery> {
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

fn discover_composer_installed_json(
    package_name: &str,
    project_root: &Path,
) -> Option<PackageDiscovery> {
    let installed = project_root
        .join("vendor")
        .join("composer")
        .join("installed.json");
    let content = std::fs::read_to_string(&installed).ok()?;
    let v: serde_json::Value = serde_json::from_str(&content).ok()?;
    let packages = v.get("packages").or_else(|| v.get("packages-dev"))?;
    let arr = packages.as_array()?;
    for pkg in arr {
        let name = pkg.get("name")?.as_str()?;
        if name == package_name {
            let path = pkg
                .get("install-path")
                .and_then(|p| p.as_str())
                .map(PathBuf::from)
                .unwrap_or_else(|| project_root.join("vendor").join(name));
            let version = pkg
                .get("version")
                .and_then(|p| p.as_str())
                .map(|s| s.to_string());
            return Some(PackageDiscovery {
                package_name: package_name.to_string(),
                path,
                version,
            });
        }
    }
    None
}
