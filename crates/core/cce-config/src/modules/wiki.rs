//! Wiki context export configuration
//!
//! Controls how the Wiki Context Package (WCP) export renders external
//! dependency symbols, whether entity entries carry source snippets, and
//! whether the package includes deterministic Mermaid graph skeletons.

use serde::{Deserialize, Serialize};

use crate::validation::{Validate, ValidationResult};

/// Presentation mode for external dependency symbols in the WCP export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExternalDepsMode {
    /// One entry per external symbol name (default).
    #[default]
    Symbol,
    /// One entry per package, aggregating the symbols it provides.
    Package,
}

impl ExternalDepsMode {
    /// Parse a wire label, case-insensitively.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "symbol" => Some(Self::Symbol),
            "package" => Some(Self::Package),
            _ => None,
        }
    }

    /// Stable wire label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Symbol => "symbol",
            Self::Package => "package",
        }
    }
}

/// Wiki context export configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WikiExportConfig {
    /// How external dependency symbols are aggregated in the export.
    ///
    /// The default is per-symbol; per-package stays available behind this
    /// switch until end-to-end review picks the long-term default.
    pub external_deps_mode: ExternalDepsMode,

    /// Whether entity entries carry short source snippets.
    ///
    /// Snippets reduce downstream `read_file` round-trips at the cost of a
    /// larger package. Kept off until real runs validate the trade-off.
    pub include_snippets: bool,

    /// Whether the package carries deterministic Mermaid graph skeletons.
    ///
    /// Enabled by default so callers get graph skeletons without an explicit
    /// request override; disable here to shrink every export package.
    pub include_mermaid: bool,
}

impl Default for WikiExportConfig {
    fn default() -> Self {
        Self {
            external_deps_mode: ExternalDepsMode::default(),
            include_snippets: false,
            include_mermaid: true,
        }
    }
}

impl Validate for WikiExportConfig {
    fn validate_structured(&self) -> ValidationResult {
        Ok(())
    }
}
