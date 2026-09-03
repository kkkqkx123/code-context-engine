//! Error types for configuration parsing

use std::path::PathBuf;
use thiserror::Error;

/// Configuration parse error
#[derive(Error, Debug)]
pub enum ConfigParseError {
    /// IO error reading file
    #[error("Failed to read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Parse error
    #[error("Failed to parse {path} ({build_system}): {reason}")]
    Parse {
        path: PathBuf,
        build_system: String,
        reason: String,
    },

    /// Invalid dependency specification
    #[error("Invalid dependency '{name}' in {path} ({build_system}): {reason}")]
    InvalidDependency {
        path: PathBuf,
        build_system: String,
        name: String,
        reason: String,
    },

    /// Missing required field
    #[error("Missing required field '{field}' in {path} ({build_system})")]
    MissingField {
        path: PathBuf,
        build_system: String,
        field: String,
    },

    /// Multiple errors accumulated during scanning
    #[error("Multiple errors occurred during configuration parsing:\n{0}")]
    Multiple(String),
}

impl ConfigParseError {
    /// Create a new multiple errors variant
    pub fn multiple(errors: Vec<ConfigParseError>) -> Self {
        if errors.len() == 1 {
            errors.into_iter().next().unwrap()
        } else {
            let formatted = errors
                .iter()
                .enumerate()
                .map(|(i, e)| format!("  {}. {}", i + 1, e))
                .collect::<Vec<_>>()
                .join("\n");
            Self::Multiple(format!(
                "Multiple errors occurred during configuration parsing:\n{}",
                formatted
            ))
        }
    }

    /// Create a parse error with build system context
    pub fn parse(
        path: PathBuf,
        build_system: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self::Parse {
            path,
            build_system: build_system.into(),
            reason: reason.into(),
        }
    }

    /// Create an invalid dependency error
    pub fn invalid_dependency(
        path: PathBuf,
        build_system: impl Into<String>,
        name: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self::InvalidDependency {
            path,
            build_system: build_system.into(),
            name: name.into(),
            reason: reason.into(),
        }
    }

    /// Create a missing field error
    pub fn missing_field(
        path: PathBuf,
        build_system: impl Into<String>,
        field: impl Into<String>,
    ) -> Self {
        Self::MissingField {
            path,
            build_system: build_system.into(),
            field: field.into(),
        }
    }
}
