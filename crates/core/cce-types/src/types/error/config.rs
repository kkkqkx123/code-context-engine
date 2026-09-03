//! Structured configuration error types
//!
//! This module provides rich, matchable error types for configuration operations.
//! The [`ConfigError`] enum covers all configuration loading/parsing/validation failures,
//! while [`ConfigValidationError`] handles field-level validation errors.

use std::path::PathBuf;
use std::sync::Arc;

use thiserror::Error;

/// Structured configuration error type
///
/// Replaces the former `ConfigError(pub String)` newtype with an enum
/// that carries contextual information for each failure mode.
#[derive(Error, Debug, Clone)]
pub enum ConfigError {
    /// Configuration file not found
    #[error("Config file not found: {path}")]
    FileNotFound { path: PathBuf },

    /// Configuration file read failure
    #[error("Failed to read config file {path}: {source}")]
    FileRead {
        path: PathBuf,
        #[source]
        source: Arc<std::io::Error>,
    },

    /// TOML parse failure
    #[error("TOML parse error in {path}: {reason}")]
    TomlParse { path: PathBuf, reason: String },

    /// Required environment variable not set
    #[error("Required environment variable not set: {var_name}")]
    MissingEnvVar { var_name: String },

    /// Environment variable has an invalid value
    #[error("Invalid value for environment variable {var_name}: {reason}")]
    InvalidEnvVar { var_name: String, reason: String },

    /// Configuration validation failed
    #[error("Configuration validation failed: {0}")]
    Validation(#[from] ConfigValidationError),

    /// Configuration already initialized (singleton conflict)
    #[error("Configuration already initialized")]
    AlreadyInitialized,

    /// Invalid project ID
    #[error("invalid project_id: {project_id} (must be positive)")]
    InvalidProjectId { project_id: i64 },

    /// Generic / unclassified configuration error
    #[error("{0}")]
    Other(String),
}

impl ConfigError {
    /// Get the error reason as a string slice.
    ///
    /// For structured variants this returns the [`Display`] representation.
    pub fn reason(&self) -> String {
        self.to_string()
    }

    /// Create an invalid project ID error.
    pub fn invalid_project_id(project_id: i64) -> Self {
        Self::InvalidProjectId { project_id }
    }

    /// Create a TOML parse error.
    pub fn tomal_parse(path: impl Into<PathBuf>, reason: impl Into<String>) -> Self {
        Self::TomlParse {
            path: path.into(),
            reason: reason.into(),
        }
    }

    /// Create a file not found error.
    pub fn file_not_found(path: impl Into<PathBuf>) -> Self {
        Self::FileNotFound { path: path.into() }
    }

    /// Create a file read error from an IO error.
    pub fn file_read(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::FileRead {
            path: path.into(),
            source: Arc::new(source),
        }
    }

    /// Create a missing environment variable error.
    pub fn missing_env_var(var_name: impl Into<String>) -> Self {
        Self::MissingEnvVar {
            var_name: var_name.into(),
        }
    }

    /// Create an invalid environment variable error.
    pub fn invalid_env_var(var_name: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::InvalidEnvVar {
            var_name: var_name.into(),
            reason: reason.into(),
        }
    }
}

impl From<std::io::Error> for ConfigError {
    fn from(e: std::io::Error) -> Self {
        Self::Other(e.to_string())
    }
}

/// Configuration validation error
///
/// Represents field-level validation failures within a configuration structure.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum ConfigValidationError {
    /// A configuration field has an invalid value
    InvalidField { field: String, reason: String },

    /// A required configuration field is missing
    MissingField { field: String },

    /// Two configuration fields are in conflict
    DependencyConflict { message: String },

    /// A field value is outside the allowed range
    OutOfRange {
        field: String,
        value: String,
        min: String,
        max: String,
    },

    /// Multiple validation errors
    Multiple { errors: Vec<ConfigValidationError> },
}

impl std::fmt::Display for ConfigValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidField { field, reason } => {
                write!(f, "Field '{}' is invalid: {}", field, reason)
            }
            Self::MissingField { field } => write!(f, "Missing required field: {}", field),
            Self::DependencyConflict { message } => {
                write!(f, "Configuration conflict: {}", message)
            }
            Self::OutOfRange {
                field,
                value,
                min,
                max,
            } => {
                write!(
                    f,
                    "Field '{}' value {} is out of valid range ({}..{})",
                    field, value, min, max
                )
            }
            Self::Multiple { errors } => {
                writeln!(f, "Multiple validation errors:")?;
                for (i, err) in errors.iter().enumerate() {
                    writeln!(f, "  {}. {}", i + 1, err)?;
                }
                Ok(())
            }
        }
    }
}

impl ConfigValidationError {
    /// Create a field invalid error
    pub fn invalid_field(field: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::InvalidField {
            field: field.into(),
            reason: reason.into(),
        }
    }

    /// Create a missing field error
    pub fn missing_field(field: impl Into<String>) -> Self {
        Self::MissingField {
            field: field.into(),
        }
    }

    /// Create a dependency conflict error
    pub fn dependency_conflict(message: impl Into<String>) -> Self {
        Self::DependencyConflict {
            message: message.into(),
        }
    }

    /// Create an out-of-range error
    pub fn out_of_range(
        field: impl Into<String>,
        value: impl Into<String>,
        min: impl Into<String>,
        max: impl Into<String>,
    ) -> Self {
        Self::OutOfRange {
            field: field.into(),
            value: value.into(),
            min: min.into(),
            max: max.into(),
        }
    }

    /// Create a multiple-errors aggregate
    pub fn multiple(errors: Vec<ConfigValidationError>) -> Self {
        Self::Multiple { errors }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_error_file_not_found() {
        let err = ConfigError::file_not_found("/etc/config.toml");
        assert!(matches!(err, ConfigError::FileNotFound { .. }));
        assert_eq!(err.to_string(), "Config file not found: /etc/config.toml");
    }

    #[test]
    fn test_config_error_file_read() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "permission denied");
        let err = ConfigError::file_read("/etc/config.toml", io_err);
        assert!(matches!(err, ConfigError::FileRead { .. }));
        assert!(err.to_string().contains("permission denied"));
    }

    #[test]
    fn test_config_error_file_read_source() {
        use std::error::Error;
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "not found");
        let err = ConfigError::file_read("/etc/config.toml", io_err);
        let source = err.source().expect("should have source");
        assert!(source.to_string().contains("not found"));
    }

    #[test]
    fn test_config_error_toml_parse() {
        let err = ConfigError::tomal_parse("/etc/config.toml", "invalid TOML");
        assert!(matches!(err, ConfigError::TomlParse { .. }));
        assert!(err.to_string().contains("invalid TOML"));
    }

    #[test]
    fn test_config_error_missing_env_var() {
        let err = ConfigError::missing_env_var("CCE_API_KEY");
        assert!(matches!(err, ConfigError::MissingEnvVar { .. }));
        assert!(err.to_string().contains("CCE_API_KEY"));
    }

    #[test]
    fn test_config_error_invalid_env_var() {
        let err = ConfigError::invalid_env_var("CCE_PORT", "not a number");
        assert!(matches!(err, ConfigError::InvalidEnvVar { .. }));
        assert!(err.to_string().contains("CCE_PORT"));
    }

    #[test]
    fn test_config_error_invalid_project_id() {
        let err = ConfigError::invalid_project_id(-1);
        assert!(matches!(
            err,
            ConfigError::InvalidProjectId { project_id: -1 }
        ));
        assert!(err.to_string().contains("-1"));
    }

    #[test]
    fn test_config_error_already_initialized() {
        let err = ConfigError::AlreadyInitialized;
        assert_eq!(err.to_string(), "Configuration already initialized");
    }

    #[test]
    fn test_config_error_clone() {
        let a = ConfigError::missing_env_var("KEY");
        let b = a.clone();
        assert_eq!(a.to_string(), b.to_string());
    }

    #[test]
    fn test_validation_error_invalid_field() {
        let err = ConfigValidationError::invalid_field("port", "must be > 0");
        assert!(err.to_string().contains("port"));
        assert!(err.to_string().contains("must be > 0"));
    }

    #[test]
    fn test_validation_error_missing_field() {
        let err = ConfigValidationError::missing_field("host");
        assert!(err.to_string().contains("host"));
    }

    #[test]
    fn test_validation_error_conflict() {
        let err = ConfigValidationError::dependency_conflict("A requires B");
        assert!(err.to_string().contains("A requires B"));
    }

    #[test]
    fn test_validation_error_out_of_range() {
        let err = ConfigValidationError::out_of_range("port", "99999", "1", "65535");
        assert!(err.to_string().contains("port"));
        assert!(err.to_string().contains("99999"));
    }

    #[test]
    fn test_validation_error_multiple() {
        let errors = vec![
            ConfigValidationError::invalid_field("port", "bad"),
            ConfigValidationError::missing_field("host"),
        ];
        let err = ConfigValidationError::multiple(errors);
        let msg = err.to_string();
        assert!(msg.contains("port"));
        assert!(msg.contains("host"));
    }

    #[test]
    fn test_config_error_from_validation_error() {
        let validation_err = ConfigValidationError::missing_field("host");
        let config_err: ConfigError = validation_err.into();
        assert!(matches!(config_err, ConfigError::Validation(_)));
    }
}
