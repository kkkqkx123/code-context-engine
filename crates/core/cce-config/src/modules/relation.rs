//! Relation module configuration
//!
//! This module provides unified configuration for the relation extraction system,
//! including index settings.

use serde::{Deserialize, Serialize};

use crate::validation::{Validate, ValidationResult};
use cce_types::error::config::ConfigValidationError;

/// Index configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct IndexConfig {
    /// Enable relation indexing (affects index construction).
    ///
    /// This is the feature-level switch for the relation system. It works
    /// in conjunction with:
    /// - `IndexerConfig.build_relations`: Global switch for full indexing
    /// - `HotUpdateConfig.build_relations`: Global switch for hot updates
    ///
    /// The effective flag for full indexing is: `indexer.build_relations && relation.index.enabled`
    /// The effective flag for hot updates is: `relation.index.enabled && indexer.build_relations && hot_update.build_relations`
    ///
    /// Setting this to `false` disables all relation indexing and querying,
    /// regardless of other flags.
    pub enabled: bool,

    /// Maximum number of relations retained per file (affects index construction).
    ///
    /// When the extracted relation count exceeds this budget the excess is
    /// dropped deterministically: relations that resolve to internal entities
    /// are preferred, then relations are kept in source order. `0` means
    /// unlimited (retain all extracted relations).
    pub max_relations_per_file: usize,

    /// Enable call chain resolution (query-only, does not affect index construction)
    ///
    /// This flag controls whether the query API returns transitive call chains.
    /// It does not affect how the relation index is constructed.
    pub resolve_call_chains: bool,

    /// Filter unresolved relations whose callee looks like a standard library
    /// name from the canonical graph (affects index construction).
    ///
    /// Resolution always runs first: a relation whose callee resolves to an
    /// internal entity is kept regardless of the callee's name (so a project
    /// can legitimately define `print`/`len`). With this flag enabled, only
    /// relations that remain unresolved and whose callee is recognized as a
    /// standard library name are dropped. With it disabled, those relations
    /// are preserved as external calls classified as `StandardLibrary`.
    pub filter_stdlib_calls: bool,
}

impl Default for IndexConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_relations_per_file: 10000,
            resolve_call_chains: true,
            filter_stdlib_calls: true,
        }
    }
}

impl Validate for IndexConfig {
    fn validate_structured(&self) -> ValidationResult {
        Ok(())
    }
}

impl IndexConfig {
    /// Create a new configuration with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a disabled configuration
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            max_relations_per_file: 0,
            resolve_call_chains: false,
            filter_stdlib_calls: true,
        }
    }

    /// Create a configuration optimized for small codebases
    pub fn small_codebase() -> Self {
        Self {
            enabled: true,
            max_relations_per_file: 50000,
            resolve_call_chains: true,
            filter_stdlib_calls: true,
        }
    }

    /// Create a configuration optimized for large codebases
    pub fn large_codebase() -> Self {
        Self {
            enabled: true,
            max_relations_per_file: 5000,
            resolve_call_chains: true,
            filter_stdlib_calls: true,
        }
    }
}

/// Relation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RelationConfig {
    /// Maximum call chain depth for queries (query-only, does not affect index construction)
    ///
    /// This field only affects how queries traverse the relation graph.
    /// It does not influence the index construction process.
    pub max_call_depth: usize,

    /// Enable import/export analysis (affects index construction)
    pub analyze_imports: bool,

    /// Enable cross-file dependency tracking (affects index construction)
    pub track_cross_file_deps: bool,

    /// Maximum depth for hot-update dependency propagation (0 = unlimited) (affects index construction).
    ///
    /// The propagation scope determines which dependent files are reparsed
    /// when a file changes. A finite depth bounds the hot-update cost but
    /// leaves deeper transitive callers with stale edges; the effective depth
    /// is exposed through `IndexCapabilities` so query-side consumers can
    /// detect when call chains may lack cross-file edges.
    pub max_propagation_depth: usize,

    /// Maximum ratio of the symbol-fingerprint scope size to the project
    /// file count before an incremental candidate is conservatively rejected
    /// in favor of a full-scope rebuild (0.0 disables the check) (affects index construction).
    ///
    /// The fingerprint scope is the dependency closure around replaced files;
    /// with an unlimited propagation depth it can cover the whole project,
    /// making the per-update fingerprint traversal O(project). The ratio
    /// bounds that cost at the price of rejecting pathological candidates.
    pub max_fingerprint_scope_ratio: f64,

    /// Whether `RelationExtract` plugin symbols/relations enter the relation
    /// index (affects index construction). Default off to keep the relation graph closed by default.
    pub plugin_symbols_enabled: bool,

    /// Whether `SymbolExtract` plugins supply import/export extraction for
    /// custom languages (`Language::Custom(_)`) (affects index construction). Default off; independent of
    /// `plugin_symbols_enabled`.
    pub plugin_symbol_extract_enabled: bool,

    /// Maximum directory depth for build-manifest discovery (0 = root only) (affects index construction).
    ///
    /// When set to a value > 0, `scan_project` will descend into immediate
    /// subdirectories up to the given depth, looking for additional
    /// `Cargo.toml`, `package.json`, `go.mod`, etc. manifests in workspace
    /// sub-crates and monorepo sub-packages.  The traversal skips well-known
    /// large directories (`target/`, `node_modules/`, `.git/`, etc.).
    pub manifest_scan_depth: usize,

    /// Index configuration
    pub index: IndexConfig,
}

impl Default for RelationConfig {
    fn default() -> Self {
        Self {
            max_call_depth: 10,
            analyze_imports: true,
            track_cross_file_deps: true,
            max_propagation_depth: 3,
            max_fingerprint_scope_ratio: 0.5,
            plugin_symbols_enabled: false,
            plugin_symbol_extract_enabled: false,
            manifest_scan_depth: 0,
            index: IndexConfig::default(),
        }
    }
}

impl Validate for RelationConfig {
    fn validate_structured(&self) -> ValidationResult {
        let mut errors = Vec::new();

        if let Err(e) = self.index.validate_structured() {
            errors.push(e);
        }
        if self.max_call_depth == 0 && self.index.enabled {
            errors.push(ConfigValidationError::invalid_field(
                "max_call_depth",
                "must be greater than 0 when index is enabled",
            ));
        }
        if !(0.0..=1.0).contains(&self.max_fingerprint_scope_ratio) {
            errors.push(ConfigValidationError::out_of_range(
                "max_fingerprint_scope_ratio",
                self.max_fingerprint_scope_ratio.to_string(),
                "0.0",
                "1.0",
            ));
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(ConfigValidationError::multiple(errors))
        }
    }
}

impl RelationConfig {
    /// Create a configuration optimized for small codebases
    pub fn small_codebase() -> Self {
        Self {
            max_call_depth: 20,
            analyze_imports: true,
            track_cross_file_deps: true,
            max_propagation_depth: 3,
            max_fingerprint_scope_ratio: 0.5,
            plugin_symbols_enabled: false,
            plugin_symbol_extract_enabled: false,
            manifest_scan_depth: 0,
            index: IndexConfig::small_codebase(),
        }
    }

    /// Create a configuration optimized for large codebases
    pub fn large_codebase() -> Self {
        Self {
            max_call_depth: 5,
            analyze_imports: true,
            track_cross_file_deps: true,
            max_propagation_depth: 3,
            max_fingerprint_scope_ratio: 0.5,
            plugin_symbols_enabled: false,
            plugin_symbol_extract_enabled: false,
            manifest_scan_depth: 0,
            index: IndexConfig::large_codebase(),
        }
    }

    /// Create a disabled configuration
    pub fn disabled() -> Self {
        Self {
            max_call_depth: 0,
            analyze_imports: false,
            track_cross_file_deps: false,
            max_propagation_depth: 0,
            max_fingerprint_scope_ratio: 0.5,
            plugin_symbols_enabled: false,
            plugin_symbol_extract_enabled: false,
            manifest_scan_depth: 0,
            index: IndexConfig::disabled(),
        }
    }

    /// Extract construction-affecting parameters into a transfer object.
    ///
    /// This method creates a `RelationBuilderParams` that captures all
    /// configuration fields that affect index construction. Query-only
    /// fields (`max_call_depth`, `resolve_call_chains`) are intentionally
    /// excluded.
    pub fn to_builder_params(&self) -> RelationBuilderParams {
        RelationBuilderParams {
            filter_stdlib_calls: self.index.filter_stdlib_calls,
            max_relations_per_file: self.index.max_relations_per_file,
            analyze_imports: self.analyze_imports,
            track_cross_file_deps: self.track_cross_file_deps,
            symbol_extract_enabled: self.plugin_symbol_extract_enabled,
            plugin_symbols_enabled: self.plugin_symbols_enabled,
            max_propagation_depth: self.max_propagation_depth,
            max_fingerprint_scope_ratio: self.max_fingerprint_scope_ratio,
            manifest_scan_depth: self.manifest_scan_depth,
        }
    }
}

/// Transfer object for construction-affecting relation configuration.
///
/// This struct captures all configuration fields that affect how the
/// relation index is constructed. It is used as a shared intermediate
/// representation between:
/// - `RelationConfig` (user-facing TOML config)
/// - `BuilderConfig` (full-index construction)
/// - `RelationUpdateProcessor` (hot-update construction)
///
/// Query-only fields (`max_call_depth`, `resolve_call_chains`) are
/// intentionally excluded from this struct.
#[derive(Debug, Clone)]
pub struct RelationBuilderParams {
    /// Whether to filter out standard library calls from relation index
    pub filter_stdlib_calls: bool,
    /// Maximum number of relations retained per file
    pub max_relations_per_file: usize,
    /// Whether import/export metadata is included in the relation graph
    pub analyze_imports: bool,
    /// Whether cross-file dependency edges are tracked
    pub track_cross_file_deps: bool,
    /// Whether `SymbolExtract` plugins supply import extraction
    pub symbol_extract_enabled: bool,
    /// Whether `RelationExtract` plugin symbols/relations are replayed
    pub plugin_symbols_enabled: bool,
    /// Maximum depth for hot-update dependency propagation (0 = unlimited)
    pub max_propagation_depth: usize,
    /// Maximum ratio of fingerprint scope size to project file count
    pub max_fingerprint_scope_ratio: f64,
    /// Maximum directory depth for build-manifest discovery
    pub manifest_scan_depth: usize,
}

impl Default for RelationBuilderParams {
    fn default() -> Self {
        RelationConfig::default().to_builder_params()
    }
}

impl From<RelationConfig> for RelationBuilderParams {
    fn from(config: RelationConfig) -> Self {
        config.to_builder_params()
    }
}

impl RelationBuilderParams {
    /// Create a new params with default values (matches `RelationConfig::default()`)
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a disabled configuration
    pub fn disabled() -> Self {
        Self {
            filter_stdlib_calls: true,
            max_relations_per_file: 0,
            analyze_imports: false,
            track_cross_file_deps: false,
            symbol_extract_enabled: false,
            plugin_symbols_enabled: false,
            max_propagation_depth: 0,
            max_fingerprint_scope_ratio: 0.5,
            manifest_scan_depth: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_bound_propagation_and_fingerprint_cost() {
        let config = RelationConfig::default();
        assert_eq!(config.max_propagation_depth, 3);
        assert_eq!(config.max_fingerprint_scope_ratio, 0.5);
        config
            .validate_structured()
            .expect("defaults must validate");
    }

    #[test]
    fn validate_rejects_out_of_range_scope_ratio() {
        let mut config = RelationConfig {
            max_fingerprint_scope_ratio: 1.5,
            ..RelationConfig::default()
        };
        assert!(config.validate_structured().is_err());
        config.max_fingerprint_scope_ratio = -0.1;
        assert!(config.validate_structured().is_err());
        config.max_fingerprint_scope_ratio = 0.0;
        config
            .validate_structured()
            .expect("0.0 disables the check");
        config.max_fingerprint_scope_ratio = 1.0;
        config
            .validate_structured()
            .expect("1.0 is the upper bound");
    }

    #[test]
    fn disabled_config_keeps_zero_propagation() {
        assert_eq!(RelationConfig::disabled().max_propagation_depth, 0);
    }
}
