//! Parser types for code parsing
//!
//! This module provides types used by tree-sitter query parsing.
//!
//! Note: Language, LanguageInfo, and FileType are now in
//! `cce_types::language` as they are fundamental types used across modules.

use crate::parser::ast_parser::AstNode;
use cce_types::language::Language;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use thiserror::Error;

/// Parse result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParseResult {
    /// Detected language
    pub language: Language,
    /// Complete AST node (contains the complete syntax tree structure)
    pub ast_node: AstNode,
    /// Parse errors
    pub errors: Vec<String>,
}

impl Default for ParseResult {
    fn default() -> Self {
        Self {
            language: Language::Unknown,
            ast_node: AstNode::default(),
            errors: vec![],
        }
    }
}

/// Query capture name domain
///
/// Represents the top-level domain of a query capture name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Domain {
    /// Entity domain (@entity) - for capturing code entity definitions
    #[serde(rename = "entity")]
    #[default]
    Entity,
    /// Call domain (@call) - for capturing call relationships
    #[serde(rename = "call")]
    Call,
    /// Dependency domain (@dependency) - for capturing dependencies
    #[serde(rename = "dependency")]
    Dependency,
    /// Comment domain (@comment) - for capturing comments
    #[serde(rename = "comment")]
    Comment,
    /// Control domain (@control) - for capturing control-flow structures
    #[serde(rename = "control")]
    Control,
    /// Behavior domain (@behavior) - for capturing function-body behaviors
    #[serde(rename = "behavior")]
    Behavior,
}

impl Domain {
    /// Get the string representation of the domain
    pub const fn as_str(&self) -> &'static str {
        match self {
            Domain::Entity => "entity",
            Domain::Call => "call",
            Domain::Dependency => "dependency",
            Domain::Comment => "comment",
            Domain::Control => "control",
            Domain::Behavior => "behavior",
        }
    }

    /// Get the prefix string for the domain (e.g., "entity.")
    pub const fn prefix(&self) -> &'static str {
        match self {
            Domain::Entity => "entity.",
            Domain::Call => "call.",
            Domain::Dependency => "dependency.",
            Domain::Comment => "comment.",
            Domain::Control => "control.",
            Domain::Behavior => "behavior.",
        }
    }
}

impl std::fmt::Display for Domain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for Domain {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "entity" => Ok(Domain::Entity),
            "call" => Ok(Domain::Call),
            "dependency" => Ok(Domain::Dependency),
            "comment" => Ok(Domain::Comment),
            "control" => Ok(Domain::Control),
            "behavior" => Ok(Domain::Behavior),
            _ => Err(cce_types::error::ParseDomainError::unknown(s).to_string()),
        }
    }
}

/// Parsed capture name structure
///
/// Represents a parsed Tree-sitter query capture name following the naming convention:
/// `@[domain].[category].[subtype].[role].[attribute]`
///
/// Simplified naming: 3-4 segments instead of 4-5 (removed redundant qualifiers)
///
/// # Example
///
/// ```
/// use cce_parser::tree_sitter_query::{CaptureName, Domain};
///
/// let capture = CaptureName::parse("@entity.class.name").expect("Failed to parse capture name");
/// assert_eq!(capture.domain, Domain::Entity);
/// assert_eq!(capture.category, Some("class".to_string()));
/// assert_eq!(capture.subtype, Some("name".to_string()));
/// assert_eq!(capture.role, None);
/// assert_eq!(capture.attribute, None);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CaptureName {
    /// Top-level domain (entity/call/dependency/comment/control/behavior)
    pub domain: Domain,
    /// Category (type/function/method/variable/etc.)
    pub category: Option<String>,
    /// Subtype (class/struct/definition/etc.)
    pub subtype: Option<String>,
    /// Role (name/body/params/etc.)
    pub role: Option<String>,
    /// Optional attribute (qualified/nested/template/etc.)
    pub attribute: Option<String>,
}

impl CaptureName {
    /// Parse a capture name string
    ///
    /// # Arguments
    ///
    /// * `capture` - The capture name string (with or without @ prefix)
    ///
    /// # Returns
    ///
    /// Returns a `CaptureName` if parsing succeeds, otherwise returns an error.
    ///
    /// # Example
    ///
    /// ```
    /// use cce_parser::tree_sitter_query::CaptureName;
    ///
    /// let capture = CaptureName::parse("@entity.class.name").expect("Failed to parse capture name");
    /// ```
    pub fn parse(capture: &str) -> Result<Self, CaptureParseError> {
        // Remove @ prefix if present
        let capture = capture.strip_prefix('@').unwrap_or(capture);

        // Split into parts
        let parts: Vec<&str> = capture.split('.').collect();

        if parts.len() < 2 {
            return Err(CaptureParseError::InvalidFormat {
                capture: capture.to_string(),
                reason: "Must have at least domain and category".to_string(),
            });
        }

        // Parse domain
        let domain = Domain::from_str(parts[0]).map_err(|e| CaptureParseError::UnknownDomain {
            domain: e.to_string(),
        })?;

        // Parse other parts
        Ok(CaptureName {
            domain,
            category: if parts.len() > 1 {
                Some(parts[1].to_string())
            } else {
                None
            },
            subtype: if parts.len() > 2 {
                Some(parts[2].to_string())
            } else {
                None
            },
            role: if parts.len() > 3 {
                Some(parts[3].to_string())
            } else {
                None
            },
            attribute: if parts.len() > 4 {
                Some(parts[4].to_string())
            } else {
                None
            },
        })
    }

    /// Convert to capture name string
    ///
    /// # Example
    ///
    /// ```
    /// use cce_parser::tree_sitter_query::CaptureName;
    ///
    /// let capture = CaptureName::parse("@entity.class.name").expect("Failed to parse capture name");
    /// assert_eq!(capture.to_capture_string(), "@entity.class.name");
    /// ```
    pub fn to_capture_string(&self) -> String {
        let mut parts = vec![self.domain.as_str().to_string()];

        if let Some(ref cat) = self.category {
            parts.push(cat.clone());
        }
        if let Some(ref sub) = self.subtype {
            parts.push(sub.clone());
        }
        if let Some(ref role) = self.role {
            parts.push(role.clone());
        }
        if let Some(ref attr) = self.attribute {
            parts.push(attr.clone());
        }

        format!("@{}", parts.join("."))
    }

    /// Check if this capture matches the given pattern
    ///
    /// # Arguments
    ///
    /// * `domain` - Required domain to match
    /// * `category` - Optional category to match (None matches any)
    /// * `subtype` - Optional subtype to match (None matches any)
    /// * `role` - Optional role to match (None matches any)
    /// * `attribute` - Optional attribute to match (None matches any)
    ///
    /// # Example
    ///
    /// ```
    /// use cce_parser::tree_sitter_query::{CaptureName, Domain};
    ///
    /// let capture = CaptureName::parse("@entity.class.name").expect("Failed to parse capture name");
    /// assert!(capture.matches(Domain::Entity, Some("class"), Some("name"), None, None));
    /// ```
    pub fn matches(
        &self,
        domain: Domain,
        category: Option<&str>,
        subtype: Option<&str>,
        role: Option<&str>,
        attribute: Option<&str>,
    ) -> bool {
        if self.domain != domain {
            return false;
        }

        if let Some(cat) = category {
            if self.category.as_deref() != Some(cat) {
                return false;
            }
        }

        if let Some(sub) = subtype {
            if self.subtype.as_deref() != Some(sub) {
                return false;
            }
        }

        if let Some(r) = role {
            if self.role.as_deref() != Some(r) {
                return false;
            }
        }

        if let Some(attr) = attribute {
            if self.attribute.as_deref() != Some(attr) {
                return false;
            }
        }

        true
    }
}

impl std::fmt::Display for CaptureName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_capture_string())
    }
}

/// Capture name parse error type
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum CaptureParseError {
    /// Invalid capture name format
    #[error("Invalid capture format '{capture}': {reason}")]
    InvalidFormat { capture: String, reason: String },
    /// Unknown domain
    #[error("Unknown domain: {domain}")]
    UnknownDomain { domain: String },
}
