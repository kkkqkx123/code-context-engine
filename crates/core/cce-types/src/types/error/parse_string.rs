//! Parse error types for string-to-enum conversions

use thiserror::Error;

/// Error type for parsing RelationLevel from string
#[derive(Error, Debug, PartialEq)]
pub enum ParseRelationLevelError {
    #[error("Unknown RelationLevel: {0}")]
    Unknown(String),
}

/// Error type for parsing RelationType from string
#[derive(Error, Debug, PartialEq)]
pub enum ParseRelationTypeError {
    #[error("Unknown RelationType: {0}")]
    Unknown(String),
}

/// Error type for parsing Domain from string
#[derive(Error, Debug, PartialEq)]
pub enum ParseDomainError {
    #[error("Unknown domain: {0}")]
    Unknown(String),
}

/// Error type for parsing GroupRole from string
#[derive(Error, Debug, PartialEq)]
pub enum ParseGroupRoleError {
    #[error("Unknown GroupRole: {0}")]
    Unknown(String),
}

impl ParseRelationLevelError {
    /// Create a new error for unknown relation level
    pub fn unknown(value: impl Into<String>) -> Self {
        Self::Unknown(value.into())
    }
}

impl ParseRelationTypeError {
    /// Create a new error for unknown relation type
    pub fn unknown(value: impl Into<String>) -> Self {
        Self::Unknown(value.into())
    }
}

impl ParseDomainError {
    /// Create a new error for unknown domain
    pub fn unknown(value: impl Into<String>) -> Self {
        Self::Unknown(value.into())
    }
}

impl ParseGroupRoleError {
    /// Create a new error for unknown group role
    pub fn unknown(value: impl Into<String>) -> Self {
        Self::Unknown(value.into())
    }
}
