use cce_types::language::Language;
use std::path::{Path, PathBuf};

use super::PackageDiscovery;
use crate::external::{ExternalLibraryRegistry, ModuleInfo};

pub struct DartPackageProvider;

impl DartPackageProvider {
    pub fn discover_package(
        &self,
        package_name: &str,
        project_root: &Path,
    ) -> Option<PackageDiscovery> {
        if let Some(d) = discover_dart_tool(package_name, project_root) {
            return Some(d);
        }
        if let Some(d) = discover_pub_cache(package_name) {
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
            .resolve_library(&discovery.path, Language::Dart)
            .ok()
    }
}

fn discover_dart_tool(package_name: &str, project_root: &Path) -> Option<PackageDiscovery> {
    let config = project_root.join(".dart_tool").join("package_config.json");
    let content = std::fs::read_to_string(&config).ok()?;
    let v: serde_json::Value = serde_json::from_str(&content).ok()?;
    let packages = v.get("packages")?.as_array()?;
    for pkg in packages {
        let name = pkg.get("name")?.as_str()?;
        if name == package_name {
            let uri = pkg.get("rootUri")?.as_str()?;
            let path = if uri.starts_with("file://") {
                PathBuf::from(uri.trim_start_matches("file://"))
            } else {
                project_root.join(uri)
            };
            let version = pkg
                .get("version")
                .and_then(|x| x.as_str())
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

fn discover_pub_cache(package_name: &str) -> Option<PackageDiscovery> {
    let home = dirs::home_dir()?;
    let pub_cache = home.join(".pub-cache").join("hosted").join("pub.dev");
    if pub_cache.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&pub_cache) {
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
