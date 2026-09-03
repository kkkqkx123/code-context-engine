use cce_types::language::Language;
use std::path::Path;

use super::PackageDiscovery;
use crate::external::{ExternalLibraryRegistry, ModuleInfo};

pub struct NodePackageProvider;

impl NodePackageProvider {
    pub fn discover_package(
        &self,
        package_name: &str,
        project_root: &Path,
    ) -> Option<PackageDiscovery> {
        let nm_path = project_root.join("node_modules").join(package_name);
        if nm_path.is_dir() {
            let version = extract_npm_version(&nm_path);
            return Some(PackageDiscovery {
                package_name: package_name.to_string(),
                path: nm_path,
                version,
            });
        }

        let mut current = project_root.parent();
        while let Some(parent) = current {
            let nm_path = parent.join("node_modules").join(package_name);
            if nm_path.is_dir() {
                let version = extract_npm_version(&nm_path);
                return Some(PackageDiscovery {
                    package_name: package_name.to_string(),
                    path: nm_path,
                    version,
                });
            }
            current = parent.parent();
        }

        None
    }

    pub fn extract_symbols(
        &self,
        discovery: &PackageDiscovery,
        registry: &mut ExternalLibraryRegistry,
    ) -> Option<ModuleInfo> {
        registry
            .resolve_library(&discovery.path, Language::JavaScript)
            .ok()
    }
}

/// Extract version from a Node.js package's package.json.
fn extract_npm_version(pkg_path: &Path) -> Option<String> {
    let package_json = pkg_path.join("package.json");
    let content = std::fs::read_to_string(&package_json).ok()?;
    let manifest: serde_json::Value = serde_json::from_str(&content).ok()?;
    manifest
        .get("version")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}
