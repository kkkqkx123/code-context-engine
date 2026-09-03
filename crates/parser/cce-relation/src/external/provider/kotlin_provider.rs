use cce_types::language::Language;
use std::path::Path;

use super::PackageDiscovery;
use crate::external::{ExternalLibraryRegistry, ModuleInfo};

use super::java_provider::JavaPackageProvider;

pub struct KotlinPackageProvider;

impl KotlinPackageProvider {
    pub fn discover_package(
        &self,
        package_name: &str,
        project_root: &Path,
    ) -> Option<PackageDiscovery> {
        JavaPackageProvider.discover_package(package_name, project_root)
    }

    pub fn extract_symbols(
        &self,
        discovery: &PackageDiscovery,
        registry: &mut ExternalLibraryRegistry,
    ) -> Option<ModuleInfo> {
        registry
            .resolve_library(&discovery.path, Language::Kotlin)
            .ok()
    }
}
