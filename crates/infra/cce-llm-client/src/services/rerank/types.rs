//! Rerank service type re-exports

pub use cce_config::modules::search::ScoreFusionStrategy;
pub use cce_llm::rerank::{RerankRequest, RerankRuntimeConfig};
pub use cce_types::ast_to_nl::{RerankCandidate, RerankResult, RerankedCandidate};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_score_fusion_rerank_only() {
        let strategy = ScoreFusionStrategy::RerankOnly;
        let score = strategy.calculate(0.9, 0.8, 0);
        assert!((score - 0.9).abs() < f32::EPSILON);
    }

    #[test]
    fn test_score_fusion_linear_weighted() {
        let strategy = ScoreFusionStrategy::LinearWeighted { alpha: 0.7 };
        let score = strategy.calculate(0.9, 0.8, 0);
        let expected = 0.7 * 0.9 + 0.3 * 0.8;
        assert!((score - expected).abs() < f32::EPSILON);
    }

    #[test]
    fn test_score_fusion_multiplicative() {
        let strategy = ScoreFusionStrategy::Multiplicative;
        let score = strategy.calculate(0.9, 0.8, 0);
        assert!((score - 0.72).abs() < f32::EPSILON);
    }

    #[test]
    fn test_rerank_runtime_config_default() {
        let config = RerankRuntimeConfig::default();
        assert_eq!(config.max_candidates, 50);
        assert_eq!(config.temperature, 0.0);
        assert!(!config.return_reasoning);
        assert_eq!(config.timeout_ms, 5000);
    }
}
