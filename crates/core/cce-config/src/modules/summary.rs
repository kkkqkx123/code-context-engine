//! Summary generation configuration
//!
//! This module provides configuration for file summary generation.

use serde::{Deserialize, Serialize};

/// Summary generation strategy
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SummaryGenerationStrategy {
    /// Automatically select strategy based on file characteristics
    #[default]
    Auto,
    /// Use rule-based generation
    RuleBased,
    /// Use model-enhanced generation
    ModelEnhanced,
    /// Generate minimal summaries only
    Minimal,
}

/// Summary generation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SummaryConfig {
    /// Summary generation strategy
    pub strategy: SummaryGenerationStrategy,
    /// Maximum summary length in tokens
    pub max_summary_length: usize,
    /// Maximum number of entities to extract
    pub max_entities: usize,
    /// Maximum number of imports to include
    pub max_imports: usize,
    /// Maximum number of concurrent model summary requests
    pub max_concurrent: usize,
    /// Maximum number of retries for LLM requests on transient errors
    pub max_retries: usize,
    /// Timeout for LLM requests in seconds (0 means no timeout)
    pub request_timeout_secs: u64,
    /// Whether to enable graceful degradation when LLM fails
    pub enable_graceful_degradation: bool,
}

impl SummaryConfig {
    /// Create a rule-based summary configuration
    pub fn rule_based() -> Self {
        Self {
            strategy: SummaryGenerationStrategy::RuleBased,
            ..Self::default()
        }
    }

    /// Create a model-enhanced summary configuration
    pub fn model_enhanced() -> Self {
        Self {
            strategy: SummaryGenerationStrategy::ModelEnhanced,
            ..Self::default()
        }
    }
}

impl Default for SummaryConfig {
    fn default() -> Self {
        Self {
            strategy: SummaryGenerationStrategy::Auto,
            max_summary_length: 2000,
            max_entities: 10,
            max_imports: 10,
            max_concurrent: 5,
            max_retries: 3,
            request_timeout_secs: 30,
            enable_graceful_degradation: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = SummaryConfig::default();
        assert!(matches!(config.strategy, SummaryGenerationStrategy::Auto));
        assert_eq!(config.max_summary_length, 2000);
        assert_eq!(config.max_entities, 10);
        assert_eq!(config.max_imports, 10);
        assert_eq!(config.max_concurrent, 5);
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.request_timeout_secs, 30);
        assert!(config.enable_graceful_degradation);
    }

    #[test]
    fn test_rule_based() {
        let config = SummaryConfig::rule_based();
        assert!(matches!(
            config.strategy,
            SummaryGenerationStrategy::RuleBased
        ));
    }

    #[test]
    fn test_model_enhanced() {
        let config = SummaryConfig::model_enhanced();
        assert!(matches!(
            config.strategy,
            SummaryGenerationStrategy::ModelEnhanced
        ));
    }

    #[test]
    fn test_strategy_deserializes_snake_case_names() {
        let rule_based: SummaryConfig = toml::from_str("strategy = \"rule_based\"")
            .expect("rule_based strategy should deserialize");
        let model_enhanced: SummaryConfig = toml::from_str("strategy = \"model_enhanced\"")
            .expect("model_enhanced strategy should deserialize");

        assert_eq!(rule_based.strategy, SummaryGenerationStrategy::RuleBased);
        assert_eq!(
            model_enhanced.strategy,
            SummaryGenerationStrategy::ModelEnhanced
        );
    }
}
