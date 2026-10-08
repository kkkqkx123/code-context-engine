//! Build configuration parser for detecting project dependencies
//!
//! This module provides functionality to parse various build system configurations:
//! - Cargo.toml (Rust)
//! - package.json (JavaScript/TypeScript)
//! - requirements.txt / pyproject.toml / Pipfile / setup.cfg / environment.yml (Python)
//! - go.mod (Go)
//! - pom.xml / build.gradle / build.gradle.kts / settings.gradle / settings.gradle.kts (Java)
//! - CMakeLists.txt (C/C++)
//! - composer.json (PHP)
//! - *.csproj (C#/.NET)
//! - Gemfile (Ruby)
//!
//! The parser extracts dependency information to help classify imports.
//!
//! # Simplified Design
//!
//! This module has been simplified to only extract package names for import classification.
//! All redundant metadata (version, source, features, etc.) has been removed.

mod detector;
mod error;
pub(crate) mod parsers;
pub(crate) mod registry;
pub(crate) mod strategy;
pub(crate) mod trait_def;
mod types;

// Re-export public types
pub use detector::BuildConfigParser;
pub use error::ConfigParseError;
pub use strategy::ErrorStrategy;
pub use types::{
    DependencyCollection, Dev, DevDependency, External, ExternalDependency, Local, LocalDependency,
    PackageKind, UntypedDependency,
};
