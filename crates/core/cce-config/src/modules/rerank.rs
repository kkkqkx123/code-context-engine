//! Rerank module configuration
//!
//! This module defines the configuration for the reranking service.

use serde::{Deserialize, Serialize};

use super::search::ScoreFusionStrategy;

/// Rerank execution order for the `plugin` + LLM rerankers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RerankOrder {
    /// Only plugin rerankers run.
    #[serde(rename = "plugin_only")]
    PluginOnly,
    /// Only the LLM reranker runs.
    #[serde(rename = "llm_only")]
    LlmOnly,
    /// Plugin reranker runs first, then the LLM reranker on its output.
    #[serde(rename = "plugin_then_llm")]
    #[default]
    PluginThenLlm,
}

/// Rerank service configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RerankConfig {
    /// Whether reranking is enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Execution order of plugin vs. LLM rerankers
    #[serde(default)]
    pub order: RerankOrder,

    /// Model name for reranking
    #[serde(default = "default_model")]
    pub model: String,

    /// Maximum number of candidates to rerank
    #[serde(default = "default_max_candidates")]
    pub max_candidates: usize,

    /// Temperature parameter for LLM-based reranking
    #[serde(default = "default_temperature")]
    pub temperature: f32,

    /// Whether to return reasoning for reranking decisions
    #[serde(default)]
    pub return_reasoning: bool,

    /// Score fusion strategy
    #[serde(default = "default_score_fusion")]
    pub score_fusion_strategy: ScoreFusionStrategy,

    /// Timeout in milliseconds
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,

    /// Minimum number of candidates (fallback)
    #[serde(default = "default_min_candidates")]
    pub min_candidates: usize,

    /// Score drop threshold for dynamic candidate selection
    #[serde(default = "default_score_drop_threshold")]
    pub score_drop_threshold: f32,

    /// Minimum score threshold for reranking
    #[serde(default = "default_min_score", alias = "min_score_for_rerank")]
    pub min_score: f32,

    /// Score threshold to start detecting drop-off
    #[serde(default = "default_drop_detection_start")]
    pub drop_detection_start: f32,
}

fn default_enabled() -> bool {
    false
}

fn default_model() -> String {
    "gpt-4o-mini".to_string()
}

fn default_max_candidates() -> usize {
    50
}

fn default_temperature() -> f32 {
    0.0
}

fn default_score_fusion() -> ScoreFusionStrategy {
    ScoreFusionStrategy::LinearWeighted { alpha: 0.7 }
}

fn default_timeout_ms() -> u64 {
    5000
}

fn default_min_candidates() -> usize {
    3
}

fn default_score_drop_threshold() -> f32 {
    0.05
}

fn default_min_score() -> f32 {
    0.3
}

fn default_drop_detection_start() -> f32 {
    0.6
}

impl Default for RerankConfig {
    fn default() -> Self {
        // Use serde default functions as single source of truth,
        // avoiding hardcoded duplicates that can drift.
        Self {
            enabled: default_enabled(),
            order: RerankOrder::PluginThenLlm,
            model: default_model(),
            max_candidates: default_max_candidates(),
            temperature: default_temperature(),
            return_reasoning: false,
            score_fusion_strategy: default_score_fusion(),
            timeout_ms: default_timeout_ms(),
            min_candidates: default_min_candidates(),
            score_drop_threshold: default_score_drop_threshold(),
            min_score: default_min_score(),
            drop_detection_start: default_drop_detection_start(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = RerankConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.model, "gpt-4o-mini");
        assert_eq!(config.max_candidates, 50);
        assert_eq!(config.temperature, 0.0);
        assert!(!config.return_reasoning);
        assert_eq!(
            config.score_fusion_strategy,
            ScoreFusionStrategy::LinearWeighted { alpha: 0.7 }
        );
        assert_eq!(config.timeout_ms, 5000);
        assert_eq!(config.min_candidates, 3);
        assert!((config.score_drop_threshold - 0.05).abs() < f32::EPSILON);
        assert!((config.min_score - 0.3).abs() < f32::EPSILON);
        assert!((config.drop_detection_start - 0.6).abs() < f32::EPSILON);
    }

    #[test]
    fn test_toml_deserialization() {
        let toml_str = r#"
            enabled = true
            model = "gpt-4"
            max_candidates = 30
            temperature = 0.1
            return_reasoning = true
            score_fusion_strategy = "multiplicative"
            timeout_ms = 10000
        "#;

        let config: RerankConfig = toml::from_str(toml_str).expect("Failed to parse TOML");
        assert!(config.enabled);
        assert_eq!(config.model, "gpt-4");
        assert_eq!(config.max_candidates, 30);
        assert_eq!(config.temperature, 0.1);
        assert!(config.return_reasoning);
        assert_eq!(
            config.score_fusion_strategy,
            ScoreFusionStrategy::Multiplicative
        );
        assert_eq!(config.timeout_ms, 10000);
    }
}
