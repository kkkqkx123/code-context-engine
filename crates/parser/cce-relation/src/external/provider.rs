//! External symbol provider trait and path discovery.
//!
//! Provides a trait-based system for discovering and extracting symbols from
//! external packages installed on the system. Each language has a concrete
//! provider implementation that knows how to find package directories and
//! extract exported symbols.

use cce_types::language::Language;
use std::path::{Path, PathBuf};

use super::ExternalLibraryRegistry;
use super::ModuleInfo;

mod csharp_provider;
mod dart_provider;
mod go_provider;
mod java_provider;
mod kotlin_provider;
mod node_provider;
mod php_provider;
mod python_provider;
mod ruby_provider;
mod rust_provider;
mod scala_provider;

pub use csharp_provider::CSharpPackageProvider;
pub use dart_provider::DartPackageProvider;
pub use go_provider::GoPackageProvider;
pub use java_provider::JavaPackageProvider;
pub use kotlin_provider::KotlinPackageProvider;
pub use node_provider::NodePackageProvider;
pub use php_provider::PhpPackageProvider;
pub use python_provider::PythonPackageProvider;
pub use ruby_provider::RubyPackageProvider;
pub use rust_provider::RustPackageProvider;
pub use scala_provider::ScalaPackageProvider;

/// Result of discovering a package's installation path.
#[derive(Debug, Clone)]
pub struct PackageDiscovery {
    /// Package name as declared in the build manifest.
    pub package_name: String,
    /// Resolved filesystem path to the package root.
    pub path: PathBuf,
    /// Package version (if determinable from path or metadata).
    pub version: Option<String>,
}

/// Concrete enum for language-specific symbol providers.
///
/// Uses static dispatch via enum matching, following the project convention
/// of avoiding `dyn` indirection.
pub enum ExternalSymbolProviderEnum {
    Rust(RustPackageProvider),
    Python(PythonPackageProvider),
    Node(NodePackageProvider),
    Go(GoPackageProvider),
    Java(JavaPackageProvider),
    Kotlin(KotlinPackageProvider),
    CSharp(CSharpPackageProvider),
    Php(PhpPackageProvider),
    Ruby(RubyPackageProvider),
    Dart(DartPackageProvider),
    Scala(ScalaPackageProvider),
}

impl ExternalSymbolProviderEnum {
    /// Discover the installation path for a package.
    pub fn discover_package(
        &self,
        package_name: &str,
        project_root: &Path,
    ) -> Option<PackageDiscovery> {
        match self {
            Self::Rust(p) => p.discover_package(package_name, project_root),
            Self::Python(p) => p.discover_package(package_name, project_root),
            Self::Node(p) => p.discover_package(package_name, project_root),
            Self::Go(p) => p.discover_package(package_name, project_root),
            Self::Java(p) => p.discover_package(package_name, project_root),
            Self::Kotlin(p) => p.discover_package(package_name, project_root),
            Self::CSharp(p) => p.discover_package(package_name, project_root),
            Self::Php(p) => p.discover_package(package_name, project_root),
            Self::Ruby(p) => p.discover_package(package_name, project_root),
            Self::Dart(p) => p.discover_package(package_name, project_root),
            Self::Scala(p) => p.discover_package(package_name, project_root),
        }
    }

    /// Extract symbols from a discovered package.
    pub fn extract_symbols(
        &self,
        discovery: &PackageDiscovery,
        registry: &mut ExternalLibraryRegistry,
    ) -> Option<ModuleInfo> {
        match self {
            Self::Rust(p) => p.extract_symbols(discovery, registry),
            Self::Python(p) => p.extract_symbols(discovery, registry),
            Self::Node(p) => p.extract_symbols(discovery, registry),
            Self::Go(p) => p.extract_symbols(discovery, registry),
            Self::Java(p) => p.extract_symbols(discovery, registry),
            Self::Kotlin(p) => p.extract_symbols(discovery, registry),
            Self::CSharp(p) => p.extract_symbols(discovery, registry),
            Self::Php(p) => p.extract_symbols(discovery, registry),
            Self::Ruby(p) => p.extract_symbols(discovery, registry),
            Self::Dart(p) => p.extract_symbols(discovery, registry),
            Self::Scala(p) => p.extract_symbols(discovery, registry),
        }
    }
}

/// Static dispatch registry mapping languages to their symbol providers.
pub struct ProviderRegistry;

impl ProviderRegistry {
    /// Get the provider for a given language, if one exists.
    pub fn provider_for(language: Language) -> Option<ExternalSymbolProviderEnum> {
        match language {
            Language::Rust => Some(ExternalSymbolProviderEnum::Rust(RustPackageProvider)),
            Language::Python => Some(ExternalSymbolProviderEnum::Python(PythonPackageProvider)),
            Language::JavaScript | Language::TypeScript | Language::Jsx | Language::Tsx => {
                Some(ExternalSymbolProviderEnum::Node(NodePackageProvider))
            }
            Language::Go => Some(ExternalSymbolProviderEnum::Go(GoPackageProvider)),
            Language::Java => Some(ExternalSymbolProviderEnum::Java(JavaPackageProvider)),
            Language::Kotlin => Some(ExternalSymbolProviderEnum::Kotlin(KotlinPackageProvider)),
            Language::CSharp => Some(ExternalSymbolProviderEnum::CSharp(CSharpPackageProvider)),
            Language::Php => Some(ExternalSymbolProviderEnum::Php(PhpPackageProvider)),
            Language::Ruby => Some(ExternalSymbolProviderEnum::Ruby(RubyPackageProvider)),
            Language::Dart => Some(ExternalSymbolProviderEnum::Dart(DartPackageProvider)),
            Language::Scala => Some(ExternalSymbolProviderEnum::Scala(ScalaPackageProvider)),
            _ => None,
        }
    }

    /// Check if a language has a symbol provider available.
    pub fn has_provider(language: Language) -> bool {
        matches!(
            language,
            Language::Rust
                | Language::Python
                | Language::JavaScript
                | Language::TypeScript
                | Language::Jsx
                | Language::Tsx
                | Language::Go
                | Language::Java
                | Language::Kotlin
                | Language::CSharp
                | Language::Php
                | Language::Ruby
                | Language::Dart
                | Language::Scala
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_registry_has_rust() {
        assert!(ProviderRegistry::has_provider(Language::Rust));
    }

    #[test]
    fn test_provider_registry_has_python() {
        assert!(ProviderRegistry::has_provider(Language::Python));
    }

    #[test]
    fn test_provider_registry_has_javascript() {
        assert!(ProviderRegistry::has_provider(Language::JavaScript));
    }

    #[test]
    fn test_provider_registry_has_go() {
        assert!(ProviderRegistry::has_provider(Language::Go));
    }

    #[test]
    fn test_provider_registry_has_java() {
        assert!(ProviderRegistry::has_provider(Language::Java));
        assert!(ProviderRegistry::has_provider(Language::Kotlin));
        assert!(ProviderRegistry::has_provider(Language::CSharp));
        assert!(ProviderRegistry::has_provider(Language::Php));
        assert!(ProviderRegistry::has_provider(Language::Ruby));
        assert!(ProviderRegistry::has_provider(Language::Dart));
        assert!(ProviderRegistry::has_provider(Language::Scala));
    }

    #[test]
    fn test_provider_registry_no_unknown() {
        assert!(!ProviderRegistry::has_provider(Language::Unknown));
    }

    #[test]
    fn test_rust_provider_discover_cargo_path_dep() {
        let tmp = std::env::temp_dir().join("cce_test_path_dep");
        let _ = std::fs::create_dir_all(&tmp);

        // Create a fake Cargo.toml with a path dependency that includes a version
        let cargo_toml = r#"
[dependencies]
mylib = { path = "libs/mylib", version = "0.1.0" }
"#;
        std::fs::write(tmp.join("Cargo.toml"), cargo_toml).unwrap();

        // Create the dependency directory
        let dep_dir = tmp.join("libs").join("mylib");
        std::fs::create_dir_all(&dep_dir).unwrap();
        std::fs::write(
            dep_dir.join("Cargo.toml"),
            "[package]\nname = \"mylib\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();

        let provider = RustPackageProvider;
        let discovery = provider.discover_package("mylib", &tmp);
        assert!(discovery.is_some());
        let d = discovery.unwrap();
        assert_eq!(d.package_name, "mylib");
        assert_eq!(d.version, Some("0.1.0".to_string()));

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
