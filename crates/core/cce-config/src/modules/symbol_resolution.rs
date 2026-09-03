//! Symbol resolution configuration
//!
//! Centralizes all tuning knobs for the symbol resolution pipeline that were
//! previously hard-coded as constants scattered across `cce-relation` and
//! `cce-parser-core`.

use serde::{Deserialize, Serialize};

use crate::validation::{Validate, ValidationResult};
use cce_types::error::config::ConfigValidationError;

fn default_max_reexport_chain_depth() -> usize {
    5
}

fn default_max_scope_chain_depth() -> usize {
    100
}

fn default_resolution_cache_size() -> usize {
    4096
}

fn default_max_wildcard_expansion_size() -> usize {
    1000
}

fn default_disable_wildcard_expansion() -> bool {
    false
}

/// Configuration for the symbol resolution pipeline.
///
/// All fields have deterministic defaults matching the previous hard-coded
/// constants so existing installations keep identical behaviour without
/// configuration changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SymbolResolutionConfig {
    /// Maximum number of hops a re-export chain may follow.
    ///
    /// Chains longer than this are treated as unresolvable, bounding work
    /// and breaking cycles. Previous constant `REEXPORT_MAX_CHAIN_DEPTH = 5`.
    #[serde(default = "default_max_reexport_chain_depth")]
    pub max_reexport_chain_depth: usize,

    /// Maximum depth of the scope chain walked during local resolution.
    ///
    /// Previous hard limit `100` in `LocalSymbolTable::build_scope_chain`
    /// and `RelationResolver::build_scope_chain_from_map`.
    #[serde(default = "default_max_scope_chain_depth")]
    pub max_scope_chain_depth: usize,

    /// Maximum number of entries in the positive and negative resolution caches.
    ///
    /// Previous constant `RESOLUTION_CACHE_CAPACITY = 4096`.
    #[serde(default = "default_resolution_cache_size")]
    pub resolution_cache_size: usize,

    /// Maximum number of symbols a single wildcard import may expand to.
    ///
    /// Prevents pathological `import *` from materialising tens of thousands
    /// of candidates. `0` is treated as unlimited but validated against.
    #[serde(default = "default_max_wildcard_expansion_size")]
    pub max_wildcard_expansion_size: usize,

    /// When true, wildcard import expansion is disabled entirely.
    #[serde(default = "default_disable_wildcard_expansion")]
    pub disable_wildcard_expansion: bool,
}

impl Default for SymbolResolutionConfig {
    fn default() -> Self {
        Self {
            max_reexport_chain_depth: default_max_reexport_chain_depth(),
            max_scope_chain_depth: default_max_scope_chain_depth(),
            resolution_cache_size: default_resolution_cache_size(),
            max_wildcard_expansion_size: default_max_wildcard_expansion_size(),
            disable_wildcard_expansion: default_disable_wildcard_expansion(),
        }
    }
}

impl Validate for SymbolResolutionConfig {
    fn validate_structured(&self) -> ValidationResult {
        let mut errors = Vec::new();
        if self.max_reexport_chain_depth == 0 {
            errors.push(ConfigValidationError::invalid_field(
                "symbol_resolution.max_reexport_chain_depth",
                "must be greater than 0",
            ));
        }
        if self.max_scope_chain_depth == 0 {
            errors.push(ConfigValidationError::invalid_field(
                "symbol_resolution.max_scope_chain_depth",
                "must be greater than 0",
            ));
        }
        if self.resolution_cache_size == 0 {
            errors.push(ConfigValidationError::invalid_field(
                "symbol_resolution.resolution_cache_size",
                "must be greater than 0",
            ));
        }
        if !self.disable_wildcard_expansion && self.max_wildcard_expansion_size == 0 {
            errors.push(ConfigValidationError::invalid_field(
                "symbol_resolution.max_wildcard_expansion_size",
                "must be greater than 0 when wildcard expansion is enabled",
            ));
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(ConfigValidationError::multiple(errors))
        }
    }
}

impl SymbolResolutionConfig {
    /// Create with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a config tuned for small codebases.
    pub fn small_codebase() -> Self {
        Self {
            max_reexport_chain_depth: 8,
            max_scope_chain_depth: 150,
            resolution_cache_size: 8192,
            max_wildcard_expansion_size: 2000,
            disable_wildcard_expansion: false,
        }
    }

    /// Create a config tuned for large codebases.
    pub fn large_codebase() -> Self {
        Self {
            max_reexport_chain_depth: 3,
            max_scope_chain_depth: 50,
            resolution_cache_size: 2048,
            max_wildcard_expansion_size: 500,
            disable_wildcard_expansion: false,
        }
    }

    /// Create a disabled-wildcard variant.
    pub fn without_wildcard() -> Self {
        Self {
            disable_wildcard_expansion: true,
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_legacy_constants() {
        let cfg = SymbolResolutionConfig::default();
        assert_eq!(cfg.max_reexport_chain_depth, 5);
        assert_eq!(cfg.max_scope_chain_depth, 100);
        assert_eq!(cfg.resolution_cache_size, 4096);
        assert_eq!(cfg.max_wildcard_expansion_size, 1000);
        assert!(!cfg.disable_wildcard_expansion);
        cfg.validate_structured().expect("defaults must validate");
    }

    #[test]
    fn validate_rejects_zero_depth() {
        let mut cfg = SymbolResolutionConfig {
            max_reexport_chain_depth: 0,
            ..Default::default()
        };
        assert!(cfg.validate_structured().is_err());
        cfg = SymbolResolutionConfig {
            max_scope_chain_depth: 0,
            ..Default::default()
        };
        assert!(cfg.validate_structured().is_err());
        cfg = SymbolResolutionConfig {
            resolution_cache_size: 0,
            ..Default::default()
        };
        assert!(cfg.validate_structured().is_err());
    }

    #[test]
    fn validate_rejects_zero_wildcard_when_enabled() {
        let mut cfg = SymbolResolutionConfig {
            max_wildcard_expansion_size: 0,
            ..Default::default()
        };
        assert!(cfg.validate_structured().is_err());
        cfg.disable_wildcard_expansion = true;
        cfg.validate_structured()
            .expect("zero size ok when disabled");
    }

    #[test]
    fn serde_roundtrip() {
        let cfg = SymbolResolutionConfig::default();
        let s = toml::to_string_pretty(&cfg).expect("serialize");
        let de: SymbolResolutionConfig = toml::from_str(&s).expect("deserialize");
        assert_eq!(de.max_reexport_chain_depth, cfg.max_reexport_chain_depth);
        assert_eq!(de.resolution_cache_size, cfg.resolution_cache_size);
    }
}
