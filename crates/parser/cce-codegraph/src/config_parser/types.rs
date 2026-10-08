//! Simplified type definitions for build configuration parsing
//!
//! This module provides minimal types for dependency extraction.
//! Only package names and types are kept for import classification.
//!
//! Uses type-state pattern for compile-time type safety.

use serde::{Deserialize, Serialize};
use std::hash::Hash;
use std::marker::PhantomData;

/// Package type marker traits for type-state pattern
pub trait PackageKind:
    Clone + std::fmt::Debug + Send + Sync + 'static + Eq + PartialEq + Hash
{
    /// Package type name
    const NAME: &'static str;
}

/// External dependency marker
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct External;

/// Development dependency marker
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Dev;

/// Local path dependency marker
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Local;

impl PackageKind for External {
    const NAME: &'static str = "external";
}

impl PackageKind for Dev {
    const NAME: &'static str = "dev";
}

impl PackageKind for Local {
    const NAME: &'static str = "local";
}

/// Type-safe dependency structure
///
/// Uses type-state pattern to ensure compile-time type safety.
/// The generic parameter `K` determines the package type at compile time.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Dependency<K: PackageKind> {
    /// Package name (e.g., "serde", "tokio", "numpy")
    pub name: String,
    /// Phantom data for the package kind
    #[serde(skip)]
    _kind: PhantomData<K>,
}

impl<K: PackageKind> Dependency<K> {
    /// Create a new dependency
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            _kind: PhantomData,
        }
    }

    /// Get the package name
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Type aliases for convenience
pub type ExternalDependency = Dependency<External>;
pub type DevDependency = Dependency<Dev>;
pub type LocalDependency = Dependency<Local>;

/// Convenience constructors
impl ExternalDependency {
    /// Create a new external dependency
    pub fn external(name: impl Into<String>) -> Self {
        Self::new(name)
    }
}

impl DevDependency {
    /// Create a new development dependency
    pub fn dev(name: impl Into<String>) -> Self {
        Self::new(name)
    }
}

impl LocalDependency {
    /// Create a new local dependency
    pub fn local(name: impl Into<String>) -> Self {
        Self::new(name)
    }
}

/// Display implementations
impl<K: PackageKind> std::fmt::Display for Dependency<K> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.name, K::NAME)
    }
}

/// Untyped dependency for serialization/deserialization
///
/// This is used when the package type is not known at compile time,
/// such as when reading from configuration files.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UntypedDependency {
    /// Package name
    pub name: String,
    /// Package type name
    pub package_type: String,
}

impl UntypedDependency {
    /// Create a new untyped dependency
    pub fn new(name: impl Into<String>, package_type: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            package_type: package_type.into(),
        }
    }

    /// Convert to typed dependency
    ///
    /// Returns `None` if the package type is not recognized.
    pub fn into_typed(self) -> Option<Box<dyn std::any::Any>> {
        match self.package_type.as_str() {
            "external" => Some(Box::new(ExternalDependency::new(self.name))),
            "dev" => Some(Box::new(DevDependency::new(self.name))),
            "local" => Some(Box::new(LocalDependency::new(self.name))),
            _ => None,
        }
    }

    /// Get the package type as a string
    pub fn package_type_str(&self) -> &str {
        &self.package_type
    }

    /// Check if this is an external dependency
    pub fn is_external(&self) -> bool {
        self.package_type == "external"
    }

    /// Check if this is a development dependency
    pub fn is_dev(&self) -> bool {
        self.package_type == "dev"
    }

    /// Check if this is a local dependency
    pub fn is_local(&self) -> bool {
        self.package_type == "local"
    }

    /// Create an external dependency
    pub fn external(name: impl Into<String>) -> Self {
        Self::new(name, "external")
    }

    /// Create a dev dependency
    pub fn dev(name: impl Into<String>) -> Self {
        Self::new(name, "dev")
    }

    /// Create a local dependency
    pub fn local(name: impl Into<String>) -> Self {
        Self::new(name, "local")
    }
}

impl std::fmt::Display for UntypedDependency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.name, self.package_type)
    }
}

impl From<&ExternalDependency> for UntypedDependency {
    fn from(dep: &ExternalDependency) -> Self {
        Self::new(&dep.name, External::NAME)
    }
}

impl From<&DevDependency> for UntypedDependency {
    fn from(dep: &DevDependency) -> Self {
        Self::new(&dep.name, Dev::NAME)
    }
}

impl From<&LocalDependency> for UntypedDependency {
    fn from(dep: &LocalDependency) -> Self {
        Self::new(&dep.name, Local::NAME)
    }
}

impl TryFrom<UntypedDependency> for ExternalDependency {
    type Error = ();

    fn try_from(dep: UntypedDependency) -> Result<Self, Self::Error> {
        if dep.package_type == External::NAME {
            Ok(Self::new(dep.name))
        } else {
            Err(())
        }
    }
}

impl TryFrom<UntypedDependency> for DevDependency {
    type Error = ();

    fn try_from(dep: UntypedDependency) -> Result<Self, Self::Error> {
        if dep.package_type == Dev::NAME {
            Ok(Self::new(dep.name))
        } else {
            Err(())
        }
    }
}

impl TryFrom<UntypedDependency> for LocalDependency {
    type Error = ();

    fn try_from(dep: UntypedDependency) -> Result<Self, Self::Error> {
        if dep.package_type == Local::NAME {
            Ok(Self::new(dep.name))
        } else {
            Err(())
        }
    }
}

/// A collection of dependencies of different types
#[derive(Debug, Clone, Default)]
pub struct DependencyCollection {
    /// External dependencies
    pub external: Vec<ExternalDependency>,
    /// Development dependencies
    pub dev: Vec<DevDependency>,
    /// Local dependencies
    pub local: Vec<LocalDependency>,
}

impl DependencyCollection {
    /// Create a new empty collection
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an external dependency
    pub fn add_external(&mut self, dep: ExternalDependency) {
        self.external.push(dep);
    }

    /// Add a development dependency
    pub fn add_dev(&mut self, dep: DevDependency) {
        self.dev.push(dep);
    }

    /// Add a local dependency
    pub fn add_local(&mut self, dep: LocalDependency) {
        self.local.push(dep);
    }

    /// Get total number of dependencies
    pub fn len(&self) -> usize {
        self.external.len() + self.dev.len() + self.local.len()
    }

    /// Check if the collection is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get all dependency names
    pub fn all_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = Vec::with_capacity(self.len());
        names.extend(self.external.iter().map(|d| d.name()));
        names.extend(self.dev.iter().map(|d| d.name()));
        names.extend(self.local.iter().map(|d| d.name()));
        names
    }
}

impl std::fmt::Display for DependencyCollection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "DependencyCollection(external={}, dev={}, local={})",
            self.external.len(),
            self.dev.len(),
            self.local.len()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_external_dependency() {
        let dep = ExternalDependency::new("serde");
        assert_eq!(dep.name(), "serde");
        assert_eq!(dep.to_string(), "serde (external)");
    }

    #[test]
    fn test_dev_dependency() {
        let dep = DevDependency::new("tokio");
        assert_eq!(dep.name(), "tokio");
        assert_eq!(dep.to_string(), "tokio (dev)");
    }

    #[test]
    fn test_local_dependency() {
        let dep = LocalDependency::new("./my-crate");
        assert_eq!(dep.name(), "./my-crate");
        assert_eq!(dep.name(), "./my-crate");
        assert_eq!(dep.to_string(), "./my-crate (local)");
    }

    #[test]
    fn test_untyped_dependency_conversion() {
        let untyped = UntypedDependency::new("serde", "external");
        let typed: ExternalDependency = untyped.try_into().unwrap();
        assert_eq!(typed.name(), "serde");
    }

    #[test]
    fn test_untyped_dependency_conversion_error() {
        let untyped = UntypedDependency::new("serde", "dev");
        let result: Result<ExternalDependency, _> = untyped.try_into();
        assert!(result.is_err());
    }

    #[test]
    fn test_dependency_collection() {
        let mut collection = DependencyCollection::new();
        collection.add_external(ExternalDependency::new("serde"));
        collection.add_dev(DevDependency::new("tokio"));
        collection.add_local(LocalDependency::new("./my-crate"));

        assert_eq!(collection.len(), 3);
        assert!(!collection.is_empty());
        assert_eq!(collection.all_names(), vec!["serde", "tokio", "./my-crate"]);
    }
}
