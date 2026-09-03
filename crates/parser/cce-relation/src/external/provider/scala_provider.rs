use cce_types::language::Language;
use std::path::Path;

use super::PackageDiscovery;
use crate::external::{ExternalLibraryRegistry, ModuleInfo};

pub struct ScalaPackageProvider;

impl ScalaPackageProvider {
    pub fn discover_package(
        &self,
        package_name: &str,
        _project_root: &Path,
    ) -> Option<PackageDiscovery> {
        if let Some(d) = discover_ivy_cache(package_name) {
            return Some(d);
        }
        if let Some(d) = discover_coursier_cache(package_name) {
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
            .resolve_library(&discovery.path, Language::Scala)
            .ok()
    }
}

fn discover_ivy_cache(package_name: &str) -> Option<PackageDiscovery> {
    let home = dirs::home_dir()?;
    let ivy = home.join(".ivy2").join("cache");
    if !ivy.is_dir() {
        return None;
    }
    let pkg_path = ivy.join(package_name.replace('.', "/"));
    if pkg_path.is_dir() {
        return Some(PackageDiscovery {
            package_name: package_name.to_string(),
            path: pkg_path,
            version: None,
        });
    }
    None
}

fn discover_coursier_cache(package_name: &str) -> Option<PackageDiscovery> {
    let home = dirs::home_dir()?;
    let coursier = home
        .join(".cache")
        .join("coursier")
        .join("v1")
        .join("https")
        .join("repo1.maven.org")
        .join("maven2");
    if !coursier.is_dir() {
        return None;
    }
    let group_path = package_name.replace('.', "/");
    let cand = coursier.join(&group_path);
    if cand.is_dir() {
        return Some(PackageDiscovery {
            package_name: package_name.to_string(),
            path: cand,
            version: None,
        });
    }
    None
}
