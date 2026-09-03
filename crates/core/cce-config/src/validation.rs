//! Configuration validation
//!
//! This module provides the [`Validate`] trait for structured config validation,
//! along with dependency-validation helpers that ensure feature flags are
//! consistent across modules.

use serde::{Deserialize, Serialize};

use cce_types::error::config::ConfigValidationError;

/// Validation result type alias
pub type ValidationResult = Result<(), ConfigValidationError>;

/// Trait for structured configuration validation.
///
/// All configuration structs should implement this trait to provide
/// rich, matchable error types instead of plain `String` errors.
pub trait Validate {
    /// Validate this configuration and return structured errors.
    fn validate_structured(&self) -> ValidationResult;
}

/// Configuration validation warning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigWarning {
    /// Warning severity
    pub severity: WarningSeverity,
    /// The field that has a dependency requirement
    pub field: String,
    /// The field(s) that must be enabled for this feature to work
    pub depends_on: String,
    /// Suggested action to resolve the warning
    pub suggestion: String,
}

/// Warning severity level
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum WarningSeverity {
    /// Feature will not work as expected
    Warning,
    /// Feature is automatically enabled
    Info,
}

impl ConfigWarning {
    /// Create a new warning
    pub fn new(
        severity: WarningSeverity,
        field: impl Into<String>,
        depends_on: impl Into<String>,
        suggestion: impl Into<String>,
    ) -> Self {
        Self {
            severity,
            field: field.into(),
            depends_on: depends_on.into(),
            suggestion: suggestion.into(),
        }
    }

    /// Format warning as a log message
    pub fn to_log_message(&self) -> String {
        match self.severity {
            WarningSeverity::Warning => {
                format!(
                    "Configuration warning: '{}' requires '{}' to be enabled. {}",
                    self.field, self.depends_on, self.suggestion
                )
            }
            WarningSeverity::Info => {
                format!(
                    "Configuration info: '{}' auto-enabled because '{}' is enabled. {}",
                    self.depends_on, self.field, self.suggestion
                )
            }
        }
    }
}

/// Validate export module dependencies
///
/// Returns a list of warnings for configuration issues
pub fn validate_export_dependencies(
    export_include_summary: bool,
    export_enable_relation_enhancement: bool,
    indexer_store_summaries: bool,
    indexer_build_relations: bool,
    relation_index_enabled: bool,
) -> Vec<ConfigWarning> {
    let mut warnings = Vec::new();

    // Check export.include_summary dependency
    if export_include_summary && !indexer_store_summaries {
        warnings.push(ConfigWarning::new(
            WarningSeverity::Warning,
            "export.include_summary",
            "orchestrator.indexer.store_summaries",
            "Set orchestrator.indexer.store_summaries = true to generate file summaries.",
        ));
    }

    // Check export.enable_relation_enhancement dependency
    if export_enable_relation_enhancement {
        if !relation_index_enabled {
            warnings.push(ConfigWarning::new(
                WarningSeverity::Warning,
                "export.enable_relation_enhancement",
                "relation.index.enabled",
                "Set relation.index.enabled = true to enable relation indexing.",
            ));
        }

        if !indexer_build_relations {
            warnings.push(ConfigWarning::new(
                WarningSeverity::Warning,
                "export.enable_relation_enhancement",
                "orchestrator.indexer.build_relations",
                "Set orchestrator.indexer.build_relations = true to build relation index during indexing.",
            ));
        }
    }

    warnings
}

/// Resolve export module dependencies by auto-enabling required features
///
/// Returns a list of info messages for auto-enabled features
pub fn resolve_export_dependencies(
    export_include_summary: bool,
    export_enable_relation_enhancement: bool,
    indexer_store_summaries: &mut bool,
    indexer_build_relations: &mut bool,
    relation_index_enabled: &mut bool,
) -> Vec<ConfigWarning> {
    let mut infos = Vec::new();

    // Auto-enable summary generation if export needs it
    if export_include_summary && !*indexer_store_summaries {
        *indexer_store_summaries = true;
        infos.push(ConfigWarning::new(
            WarningSeverity::Info,
            "export.include_summary",
            "orchestrator.indexer.store_summaries",
            "Summary generation has been auto-enabled.",
        ));
    }

    // Auto-enable relation indexing if export needs it
    if export_enable_relation_enhancement {
        if !*relation_index_enabled {
            *relation_index_enabled = true;
            infos.push(ConfigWarning::new(
                WarningSeverity::Info,
                "export.enable_relation_enhancement",
                "relation.index.enabled",
                "Relation indexing has been auto-enabled.",
            ));
        }

        if !*indexer_build_relations {
            *indexer_build_relations = true;
            infos.push(ConfigWarning::new(
                WarningSeverity::Info,
                "export.enable_relation_enhancement",
                "orchestrator.indexer.build_relations",
                "Relation building has been auto-enabled.",
            ));
        }
    }

    infos
}

/// Validate storage dependencies
///
/// Checks that storage flags are consistent with storage backend enabled status.
pub fn validate_storage_dependencies(
    indexer_store_vectors: bool,
    indexer_store_bm25: bool,
    qdrant_enabled: bool,
    bm25_enabled: bool,
) -> Vec<ConfigWarning> {
    let mut warnings = Vec::new();

    // Check vector storage dependency
    if indexer_store_vectors && !qdrant_enabled {
        warnings.push(ConfigWarning::new(
            WarningSeverity::Warning,
            "orchestrator.indexer.store_vectors",
            "database.qdrant.enabled",
            "Set database.qdrant.enabled = true to enable Qdrant vector storage.",
        ));
    }

    // Check BM25 storage dependency
    if indexer_store_bm25 && !bm25_enabled {
        warnings.push(ConfigWarning::new(
            WarningSeverity::Warning,
            "orchestrator.indexer.store_bm25",
            "database.bm25.enabled",
            "Set database.bm25.enabled = true to enable BM25 index storage.",
        ));
    }

    warnings
}

/// Validate relation dependencies
///
/// Checks that relation building flags are consistent with relation index status.
pub fn validate_relation_dependencies(
    indexer_build_relations: bool,
    relation_index_enabled: bool,
) -> Vec<ConfigWarning> {
    let mut warnings = Vec::new();

    // Check relation building dependency
    if indexer_build_relations && !relation_index_enabled {
        warnings.push(ConfigWarning::new(
            WarningSeverity::Warning,
            "orchestrator.indexer.build_relations",
            "relation.index.enabled",
            "Set relation.index.enabled = true to enable relation indexing.",
        ));
    }

    warnings
}

/// Validate LLM dependencies
///
/// Checks that LLM configuration is complete when LLM features are enabled.
pub fn validate_llm_dependencies(
    llm_enabled: bool,
    has_llm_provider: bool,
    has_chat_model: bool,
) -> Vec<ConfigWarning> {
    let mut warnings = Vec::new();

    // Check LLM configuration completeness
    if llm_enabled {
        if !has_llm_provider {
            warnings.push(ConfigWarning::new(
                WarningSeverity::Warning,
                "llm.enabled",
                "llm.providers",
                "Add at least one provider under [llm.providers] to use LLM features.",
            ));
        }

        if !has_chat_model {
            warnings.push(ConfigWarning::new(
                WarningSeverity::Warning,
                "llm.enabled",
                "llm.defaults.chat",
                "Set llm.defaults.chat to a model defined in [llm.chat_models] to use summary enhancement.",
            ));
        }
    }

    warnings
}

/// Validate that providers sharing the same upstream base URL declare
/// consistent `rate_limit` values.
///
/// The rate limiter is shared per base URL and converges to the minimum
/// non-zero rate among all referencing providers; providers declaring a
/// looser (or zero/unlimited) rate than their same-upstream siblings are
/// silently throttled down to the effective rate. This produces a warning
/// for every group whose declared rates disagree.
pub fn validate_provider_rate_limit_conflicts(
    providers: &std::collections::HashMap<String, crate::modules::ProviderConfig>,
) -> Vec<ConfigWarning> {
    use std::collections::BTreeMap;

    // base_url -> sorted (provider_id, rate_limit) pairs of enabled providers
    let mut by_base_url: BTreeMap<&str, Vec<(&str, u32)>> = BTreeMap::new();
    for (id, provider) in providers {
        if !provider.enabled {
            continue;
        }
        by_base_url
            .entry(provider.base_url.as_str())
            .or_default()
            .push((id.as_str(), provider.rate_limit));
    }

    let mut warnings = Vec::new();
    for (base_url, mut entries) in by_base_url {
        entries.sort_by_key(|(_, rate)| *rate);
        // Distinct non-zero rates among the group
        let nonzero: Vec<u32> = entries
            .iter()
            .map(|(_, rate)| *rate)
            .filter(|rate| *rate > 0)
            .collect();
        let distinct: std::collections::BTreeSet<u32> = nonzero.iter().copied().collect();
        if distinct.len() <= 1 {
            continue;
        }
        let effective = distinct
            .first()
            .copied()
            .expect("at least one non-zero rate");
        let declared: Vec<String> = entries
            .iter()
            .map(|(id, rate)| format!("{id}={rate}"))
            .collect();
        warnings.push(ConfigWarning::new(
            WarningSeverity::Warning,
            "llm.providers",
            "llm.providers.*.rate_limit",
            format!(
                "Providers sharing base_url '{base_url}' declare inconsistent rate_limit values \
                 ({}); the shared limiter converges to the minimum non-zero rate ({effective} requests/min). \
                 Set the same rate_limit on all providers of this upstream to make the effective rate explicit.",
                declared.join(", ")
            ),
        ));
    }

    warnings
}

/// Input parameters for dependency validation
///
/// Groups all configuration flags needed by `validate_all_dependencies`
/// to avoid excessive function arguments.
pub struct DependencyParams {
    // Export dependencies
    pub export_include_summary: bool,
    pub export_enable_relation_enhancement: bool,
    // Indexer settings
    pub indexer_store_summaries: bool,
    pub indexer_build_relations: bool,
    pub indexer_store_vectors: bool,
    pub indexer_store_bm25: bool,
    // Storage settings
    pub qdrant_enabled: bool,
    pub bm25_enabled: bool,
    // Relation settings
    pub relation_index_enabled: bool,
    // LLM settings
    pub llm_enabled: bool,
    pub has_llm_provider: bool,
    pub has_chat_model: bool,
}

/// Validate all configuration dependencies
///
/// This is the main validation function that checks all cross-module dependencies.
pub fn validate_all_dependencies(params: &DependencyParams) -> Vec<ConfigWarning> {
    let mut warnings = Vec::new();

    // Export dependencies
    warnings.extend(validate_export_dependencies(
        params.export_include_summary,
        params.export_enable_relation_enhancement,
        params.indexer_store_summaries,
        params.indexer_build_relations,
        params.relation_index_enabled,
    ));

    // Storage dependencies
    warnings.extend(validate_storage_dependencies(
        params.indexer_store_vectors,
        params.indexer_store_bm25,
        params.qdrant_enabled,
        params.bm25_enabled,
    ));

    // Relation dependencies
    warnings.extend(validate_relation_dependencies(
        params.indexer_build_relations,
        params.relation_index_enabled,
    ));

    // LLM dependencies
    warnings.extend(validate_llm_dependencies(
        params.llm_enabled,
        params.has_llm_provider,
        params.has_chat_model,
    ));

    warnings
}

/// Resolve storage dependencies by auto-enabling required features
///
/// Returns a list of info messages for auto-enabled features
pub fn resolve_storage_dependencies(
    indexer_store_vectors: bool,
    indexer_store_bm25: bool,
    qdrant_enabled: &mut bool,
    bm25_enabled: &mut bool,
) -> Vec<ConfigWarning> {
    let mut infos = Vec::new();

    // Auto-enable Qdrant if vector storage is requested
    if indexer_store_vectors && !*qdrant_enabled {
        *qdrant_enabled = true;
        infos.push(ConfigWarning::new(
            WarningSeverity::Info,
            "orchestrator.indexer.store_vectors",
            "database.qdrant.enabled",
            "Qdrant storage has been auto-enabled.",
        ));
    }

    // Auto-enable BM25 if BM25 storage is requested
    if indexer_store_bm25 && !*bm25_enabled {
        *bm25_enabled = true;
        infos.push(ConfigWarning::new(
            WarningSeverity::Info,
            "orchestrator.indexer.store_bm25",
            "database.bm25.enabled",
            "BM25 index has been auto-enabled.",
        ));
    }

    infos
}

/// Resolve relation dependencies by auto-enabling required features
///
/// Returns a list of info messages for auto-enabled features
pub fn resolve_relation_dependencies(
    indexer_build_relations: bool,
    relation_index_enabled: &mut bool,
) -> Vec<ConfigWarning> {
    let mut infos = Vec::new();

    // Auto-enable relation index if relation building is requested
    if indexer_build_relations && !*relation_index_enabled {
        *relation_index_enabled = true;
        infos.push(ConfigWarning::new(
            WarningSeverity::Info,
            "orchestrator.indexer.build_relations",
            "relation.index.enabled",
            "Relation indexing has been auto-enabled.",
        ));
    }

    infos
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_no_warnings() {
        let warnings = validate_export_dependencies(
            true, // export.include_summary
            true, // export.enable_relation_enhancement
            true, // indexer.store_summaries
            true, // indexer.build_relations
            true, // relation.index.enabled
        );
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_validate_summary_dependency() {
        let warnings = validate_export_dependencies(
            true,  // export.include_summary
            false, // export.enable_relation_enhancement
            false, // indexer.store_summaries
            true,  // indexer.build_relations
            true,  // relation.index.enabled
        );
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].field, "export.include_summary");
    }

    #[test]
    fn test_validate_relation_dependencies() {
        let warnings = validate_export_dependencies(
            false, // export.include_summary
            true,  // export.enable_relation_enhancement
            true,  // indexer.store_summaries
            false, // indexer.build_relations
            false, // relation.index.enabled
        );
        assert_eq!(warnings.len(), 2);
    }

    #[test]
    fn test_resolve_dependencies() {
        let mut store_summaries = false;
        let mut build_relations = false;
        let mut index_enabled = false;

        let infos = resolve_export_dependencies(
            true, // export.include_summary
            true, // export.enable_relation_enhancement
            &mut store_summaries,
            &mut build_relations,
            &mut index_enabled,
        );

        assert_eq!(infos.len(), 3);
        assert!(store_summaries);
        assert!(build_relations);
        assert!(index_enabled);
    }

    #[test]
    fn test_warning_message_format() {
        let warning = ConfigWarning::new(
            WarningSeverity::Warning,
            "export.include_summary",
            "orchestrator.indexer.store_summaries",
            "Set store_summaries = true",
        );
        let msg = warning.to_log_message();
        assert!(msg.contains("export.include_summary"));
        assert!(msg.contains("orchestrator.indexer.store_summaries"));
    }

    #[test]
    fn test_provider_rate_limit_conflicts_no_warning_when_consistent() {
        use std::collections::HashMap;

        let mut providers = HashMap::new();
        providers.insert(
            "openai-a".to_string(),
            crate::modules::ProviderConfig {
                id: "openai-a".to_string(),
                name: "A".to_string(),
                base_url: "https://api.openai.com/v1".to_string(),
                rate_limit: 60,
                ..Default::default()
            },
        );
        providers.insert(
            "openai-b".to_string(),
            crate::modules::ProviderConfig {
                id: "openai-b".to_string(),
                name: "B".to_string(),
                base_url: "https://api.openai.com/v1".to_string(),
                rate_limit: 60,
                ..Default::default()
            },
        );

        assert!(validate_provider_rate_limit_conflicts(&providers).is_empty());
    }

    #[test]
    fn test_provider_rate_limit_conflicts_warns_on_mismatch() {
        use std::collections::HashMap;

        let mut providers = HashMap::new();
        providers.insert(
            "openai-fast".to_string(),
            crate::modules::ProviderConfig {
                id: "openai-fast".to_string(),
                name: "Fast".to_string(),
                base_url: "https://api.openai.com/v1".to_string(),
                rate_limit: 120,
                ..Default::default()
            },
        );
        providers.insert(
            "openai-slow".to_string(),
            crate::modules::ProviderConfig {
                id: "openai-slow".to_string(),
                name: "Slow".to_string(),
                base_url: "https://api.openai.com/v1".to_string(),
                rate_limit: 30,
                ..Default::default()
            },
        );

        let warnings = validate_provider_rate_limit_conflicts(&providers);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].field, "llm.providers");
        let msg = warnings[0].to_log_message();
        assert!(msg.contains("https://api.openai.com/v1"));
        assert!(
            msg.contains("30"),
            "effective rate should be the minimum: {msg}"
        );
    }

    #[test]
    fn test_provider_rate_limit_conflicts_zero_is_exempt() {
        use std::collections::HashMap;

        let mut providers = HashMap::new();
        providers.insert(
            "limited".to_string(),
            crate::modules::ProviderConfig {
                id: "limited".to_string(),
                name: "Limited".to_string(),
                base_url: "https://api.example.com".to_string(),
                rate_limit: 60,
                ..Default::default()
            },
        );
        // Unlimited (0) does not participate in the min computation and thus
        // does not trigger the conflict warning on its own.
        providers.insert(
            "unlimited".to_string(),
            crate::modules::ProviderConfig {
                id: "unlimited".to_string(),
                name: "Unlimited".to_string(),
                base_url: "https://api.example.com".to_string(),
                rate_limit: 0,
                ..Default::default()
            },
        );

        assert!(validate_provider_rate_limit_conflicts(&providers).is_empty());
    }

    #[test]
    fn test_validate_trait_basic() {
        use crate::modules::EmbedderConfig;

        let config = EmbedderConfig::default();
        // Default config has empty model, should fail
        assert!(config.validate_structured().is_err());
    }

    #[test]
    fn test_validate_trait_collects_multiple_errors() {
        use crate::modules::ChunkingConfig;

        let config = ChunkingConfig {
            max_tokens: 0,
            overlap_tokens: 0,
            max_bm25_words: 0,
            overlap_bm25_words: 0,
            max_overlap_ratio: 2.0,
            min_chunk_tokens: 0,
            min_chunk_bm25_words: 0,
            cross_group_merge_threshold: 0,
            respect_boundaries: true,
        };
        let err = config.validate_structured().unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("max_tokens"));
        assert!(msg.contains("overlap_tokens"));
        assert!(msg.contains("max_bm25_words"));
        assert!(msg.contains("max_overlap_ratio"));
    }

    #[test]
    fn test_validate_trait_valid_config() {
        use crate::modules::storage::HnswConfig;

        let config = HnswConfig::medium();
        assert!(config.validate_structured().is_ok());
    }

    #[test]
    fn test_validate_trait_provider_config() {
        use crate::modules::ProviderConfig;

        let config = ProviderConfig {
            id: "test-provider".to_string(),
            enabled: true,
            base_url: "https://api.example.com".to_string(),
            ..Default::default()
        };
        assert!(config.validate_structured().is_ok());

        // Disabled providers skip validation
        let disabled = ProviderConfig {
            enabled: false,
            ..Default::default()
        };
        assert!(disabled.validate_structured().is_ok());
    }

    #[test]
    fn test_validate_trait_nested_aggregation() {
        use crate::modules::AstToNlConfig;

        let mut config = AstToNlConfig::default();
        config.bm25.max_keywords = 0;
        let err = config.validate_structured().unwrap_err();
        assert!(err.to_string().contains("bm25.max_keywords"));
    }

    #[test]
    fn test_validate_trait_app_config_multiple_errors() {
        use crate::global::AppConfig;

        let mut config = AppConfig::default();
        config.server.port = 0;
        config.server.host = String::new();
        let err = config.validate_structured().unwrap_err();
        assert!(err.to_string().contains("port"));
        assert!(err.to_string().contains("host"));
    }
}
