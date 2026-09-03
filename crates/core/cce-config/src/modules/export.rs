//! Export module configuration
//!
//! This module provides configuration types for the export functionality.

use cce_types::{ExternalCallType, RelationType};
use serde::{Deserialize, Serialize};

/// Export module configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ExportModuleConfig {
    /// Whether the export module is enabled
    pub enabled: bool,
    /// Whether to include file summaries in exported documents.
    ///
    /// When `true`, exported Markdown files include metadata lines for imports,
    /// exports, and summary text. Requires `indexer.store_summaries = true` in
    /// the indexer config; validation will reject the combination
    /// `include_summary = true` with `store_summaries = false`.
    ///
    /// When `false` or when summary generation is disabled, exported files
    /// omit the metadata section entirely (degraded mode: no imports/exports/summary).
    pub include_summary: bool,
    /// Whether to enable relation enhancement.
    ///
    /// When `true`, exported documents include related-entity annotations
    /// (callers, callees, dependencies) sourced from the relation index.
    /// Requires `indexer.enable_relation = true`; the relation index must
    /// be populated before enhancement takes effect.
    pub enable_relation_enhancement: bool,
    /// Relation enhancement configuration
    pub relation_enhancement: RelationEnhancementConfig,
}

impl Default for ExportModuleConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            include_summary: true,
            enable_relation_enhancement: false,
            relation_enhancement: RelationEnhancementConfig::default(),
        }
    }
}

impl ExportModuleConfig {
    /// Create a new export configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Set include_summary flag
    pub fn with_summary(mut self, include: bool) -> Self {
        self.include_summary = include;
        self
    }

    /// Set enable_relation_enhancement flag
    pub fn with_relation_enhancement(mut self, enable: bool) -> Self {
        self.enable_relation_enhancement = enable;
        self
    }
}

/// Relation enhancement configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationEnhancementConfig {
    /// Maximum number of related entities to include per entity
    pub max_related_entities: usize,
    /// Whether to include cross-file relations
    pub include_cross_file: bool,
    /// Whether to include standard library calls
    pub include_stdlib: bool,
    /// If set, only include relations whose `relation_type` is in this list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_relation_types: Option<Vec<RelationType>>,
    /// If set, exclude relations whose `relation_type` is in this list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclude_relation_types: Option<Vec<RelationType>>,
    /// If set, only include relations whose `external_type` discriminant
    /// matches one of the listed classifications.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_classifications: Option<Vec<ExternalCallType>>,
    /// If set, exclude relations whose `external_type` discriminant matches
    /// one of the listed classifications.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclude_classifications: Option<Vec<ExternalCallType>>,
}

impl Default for RelationEnhancementConfig {
    fn default() -> Self {
        Self {
            max_related_entities: 10,
            include_cross_file: true,
            include_stdlib: false,
            include_relation_types: None,
            exclude_relation_types: None,
            include_classifications: None,
            exclude_classifications: None,
        }
    }
}

impl RelationEnhancementConfig {
    /// Create a new relation enhancement configuration
    pub fn new() -> Self {
        Self::default()
    }
}
