//! Grouper configuration
//!
//! Configuration for PreprocessingPipeline and pattern detection.

use serde::{Deserialize, Serialize};

use crate::modules::pattern_detection::GetterSetterDetectionConfig;
use crate::validation::{Validate, ValidationResult};
use cce_types::error::config::ConfigValidationError;

// Re-use shared default value functions
use super::defaults::default_true;

/// Configuration for PreprocessingPipeline
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NestProcessorConfig {
    /// Small class threshold (in lines)
    pub small_class_threshold: usize,

    /// Simple call merge threshold
    pub simple_call_merge_threshold: usize,

    /// Enable class-method association optimization
    pub enable_class_method_association: bool,

    /// Enable simple call merging
    pub enable_call_merging: bool,

    /// Enable getter/setter merging
    pub enable_getter_setter_merging: bool,

    /// Getter/setter complexity threshold (in lines)
    pub getter_setter_complexity_threshold: usize,

    /// Enable test entity grouping
    pub enable_test_entity_grouping: bool,

    /// Include assertions in test groups
    pub include_assertions_in_groups: bool,

    /// Maximum test suite nesting depth
    pub max_test_suite_nesting: usize,

    /// Enable nested entity group extraction
    #[serde(default)]
    pub enable_nested_entity_grouping: bool,

    /// Maximum nesting depth to extract (1-3 recommended)
    #[serde(default = "default_max_nesting_depth")]
    pub max_nesting_depth: usize,

    /// Minimum size for nested entity to be extracted (in lines)
    #[serde(default = "default_min_nested_size")]
    pub min_nested_size: usize,

    /// Enable Rust impl block association (associate methods in impl blocks with their struct)
    #[serde(default = "default_true")]
    pub enable_impl_association: bool,

    /// Whether to enable plugin-based pattern detection
    #[serde(default = "default_true")]
    pub enable_plugin_pattern_detection: bool,

    /// Enable function member grouping (macros, closures, statements within functions)
    #[serde(default = "default_true")]
    pub enable_function_member_grouping: bool,

    /// Enable merging of small adjacent standalone fragments
    #[serde(default = "default_true")]
    pub enable_small_fragment_merging: bool,

    /// Minimum tokens for small fragment merging
    #[serde(default = "default_small_fragment_min_tokens")]
    pub small_fragment_min_tokens: usize,

    /// Minimum BM25 words for small fragment merging
    #[serde(default = "default_small_fragment_min_words")]
    pub small_fragment_min_words: usize,

    /// Maximum span lines for small fragment merging
    #[serde(default = "default_small_fragment_max_span_lines")]
    pub small_fragment_max_span_lines: usize,

    /// Enable group hierarchy resolution (parent-child relationships between groups)
    #[serde(default = "default_true")]
    pub enable_group_hierarchy: bool,

    /// Proximity threshold (in bytes) for near-neighbor small fragment merging.
    /// Non-adjacent standalone groups within this threshold will be merged.
    #[serde(default = "default_near_merge_proximity")]
    pub near_merge_proximity_bytes: usize,

    /// Getter/setter configuration for the getter/setter detector
    #[serde(default)]
    pub getter_setter: GetterSetterDetectionConfig,
}

fn default_max_nesting_depth() -> usize {
    2
}

fn default_min_nested_size() -> usize {
    5
}

fn default_small_fragment_min_tokens() -> usize {
    128
}

fn default_small_fragment_min_words() -> usize {
    80
}

fn default_small_fragment_max_span_lines() -> usize {
    50
}

fn default_near_merge_proximity() -> usize {
    300
}

impl Default for NestProcessorConfig {
    fn default() -> Self {
        Self {
            small_class_threshold: 20,
            simple_call_merge_threshold: 3,
            enable_class_method_association: true,
            enable_call_merging: true,
            enable_getter_setter_merging: true,
            getter_setter_complexity_threshold: 3,
            enable_test_entity_grouping: true,
            include_assertions_in_groups: false,
            max_test_suite_nesting: 3,
            enable_nested_entity_grouping: false,
            max_nesting_depth: 2,
            min_nested_size: 5,
            enable_impl_association: true,
            enable_plugin_pattern_detection: true,
            enable_function_member_grouping: true,
            enable_small_fragment_merging: true,
            small_fragment_min_tokens: 128,
            small_fragment_min_words: 80,
            small_fragment_max_span_lines: 50,
            enable_group_hierarchy: true,
            near_merge_proximity_bytes: 300,
            getter_setter: GetterSetterDetectionConfig::default(),
        }
    }
}

impl Validate for NestProcessorConfig {
    fn validate_structured(&self) -> ValidationResult {
        let mut errors = Vec::new();

        if self.small_class_threshold == 0 {
            errors.push(ConfigValidationError::invalid_field(
                "small_class_threshold",
                "must be greater than 0",
            ));
        }
        if self.simple_call_merge_threshold == 0 {
            errors.push(ConfigValidationError::invalid_field(
                "simple_call_merge_threshold",
                "must be greater than 0",
            ));
        }
        if self.getter_setter_complexity_threshold == 0 {
            errors.push(ConfigValidationError::invalid_field(
                "getter_setter_complexity_threshold",
                "must be greater than 0",
            ));
        }
        if self.max_test_suite_nesting == 0 {
            errors.push(ConfigValidationError::invalid_field(
                "max_test_suite_nesting",
                "must be greater than 0",
            ));
        }
        if self.max_nesting_depth == 0 {
            errors.push(ConfigValidationError::invalid_field(
                "max_nesting_depth",
                "must be greater than 0",
            ));
        }
        if self.min_nested_size == 0 {
            errors.push(ConfigValidationError::invalid_field(
                "min_nested_size",
                "must be greater than 0",
            ));
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(ConfigValidationError::multiple(errors))
        }
    }
}

impl NestProcessorConfig {
    /// Create a new configuration with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a configuration optimized for small codebases
    pub fn small_codebase() -> Self {
        Self {
            small_class_threshold: 50,
            simple_call_merge_threshold: 5,
            getter_setter_complexity_threshold: 5,
            max_test_suite_nesting: 5,
            ..Default::default()
        }
    }

    /// Create a configuration optimized for large codebases
    pub fn large_codebase() -> Self {
        Self {
            small_class_threshold: 20,
            simple_call_merge_threshold: 3,
            getter_setter_complexity_threshold: 2,
            max_test_suite_nesting: 2,
            ..Default::default()
        }
    }

    /// Disable all optimizations
    pub fn disabled() -> Self {
        Self {
            enable_class_method_association: false,
            enable_call_merging: false,
            enable_getter_setter_merging: false,
            enable_test_entity_grouping: false,
            ..Default::default()
        }
    }

    /// Create a configuration with only basic optimizations
    pub fn basic() -> Self {
        Self {
            enable_class_method_association: true,
            enable_call_merging: false,
            enable_getter_setter_merging: false,
            enable_test_entity_grouping: true,
            ..Default::default()
        }
    }

    /// Create a configuration with all optimization enabled
    pub fn pattern_optimized() -> Self {
        Self {
            enable_class_method_association: true,
            enable_getter_setter_merging: true,
            enable_test_entity_grouping: true,
            include_assertions_in_groups: true,
            ..Default::default()
        }
    }

    /// Create a configuration optimized for test file processing
    pub fn test_optimized() -> Self {
        Self {
            small_class_threshold: 50,
            enable_class_method_association: true,
            enable_test_entity_grouping: true,
            include_assertions_in_groups: true,
            max_test_suite_nesting: 5,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = NestProcessorConfig::default();
        assert_eq!(config.small_class_threshold, 20);
        assert!(config.enable_class_method_association);
        assert!(config.enable_test_entity_grouping);
    }
}
