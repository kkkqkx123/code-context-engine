//! Export configuration
//!
//! This module provides configuration types for the export functionality.

use cce_types::{ExternalCallType, RelationType};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Export configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportConfig {
    /// Project root directory
    pub project_root: PathBuf,
    /// Project ID for multi-project isolation
    pub project_id: i64,
    /// Whether to include file summaries
    pub include_summary: bool,
    /// Whether to enable relation enhancement
    pub enable_relation_enhancement: bool,
}

impl ExportConfig {
    /// Create a new export configuration
    pub fn new(project_root: PathBuf, project_id: i64) -> Self {
        Self {
            project_root,
            project_id,
            include_summary: true,
            enable_relation_enhancement: false,
        }
    }

    /// Create from module config
    ///
    /// Converts the global module configuration to runtime configuration.
    pub fn from_module_config(
        module_config: &cce_config::modules::ExportModuleConfig,
        project_root: PathBuf,
        project_id: i64,
    ) -> Self {
        Self {
            project_root,
            project_id,
            include_summary: module_config.include_summary,
            enable_relation_enhancement: module_config.enable_relation_enhancement,
        }
    }

    /// Output directory (under .cce/nl_docs/)
    pub fn output_dir(&self) -> PathBuf {
        self.project_root.join(".cce").join("nl_docs")
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

impl Default for ExportConfig {
    /// Creates a test-only default with project_id = 0.
    /// Real usage must always provide a valid project_id.
    fn default() -> Self {
        Self::new(PathBuf::from("."), 0)
    }
}

/// Relation enhancement configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationEnhancerConfig {
    /// Maximum number of related entities to include
    pub max_related_entities: usize,
    /// Whether to include cross-file relations
    pub include_cross_file: bool,
    /// Whether to include standard library calls
    pub include_stdlib: bool,
    /// If set, only include relations whose `relation_type` is in this list.
    /// Takes precedence over `exclude_relation_types`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_relation_types: Option<Vec<RelationType>>,
    /// If set, exclude relations whose `relation_type` is in this list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclude_relation_types: Option<Vec<RelationType>>,
    /// If set, only include relations whose `external_type` discriminant
    /// matches one of the listed classifications (stdlib / external / dev /
    /// local / unknown). Ignores inner data fields for matching.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_classifications: Option<Vec<ExternalCallType>>,
    /// If set, exclude relations whose `external_type` discriminant matches
    /// one of the listed classifications.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclude_classifications: Option<Vec<ExternalCallType>>,
}

impl Default for RelationEnhancerConfig {
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

impl RelationEnhancerConfig {
    /// Create a new relation enhancer configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Create from module config
    ///
    /// Converts the global module configuration to runtime configuration.
    pub fn from_module_config(
        module_config: &cce_config::modules::RelationEnhancementConfig,
    ) -> Self {
        Self {
            max_related_entities: module_config.max_related_entities,
            include_cross_file: module_config.include_cross_file,
            include_stdlib: module_config.include_stdlib,
            include_relation_types: module_config.include_relation_types.clone(),
            exclude_relation_types: module_config.exclude_relation_types.clone(),
            include_classifications: module_config.include_classifications.clone(),
            exclude_classifications: module_config.exclude_classifications.clone(),
        }
    }

    /// Set max related entities
    pub fn with_max_related(mut self, max: usize) -> Self {
        self.max_related_entities = max;
        self
    }

    /// Set include cross file flag
    pub fn with_cross_file(mut self, include: bool) -> Self {
        self.include_cross_file = include;
        self
    }

    /// Set include stdlib flag
    pub fn with_stdlib(mut self, include: bool) -> Self {
        self.include_stdlib = include;
        self
    }

    /// Set include relation types filter
    pub fn with_include_relation_types(mut self, types: Vec<RelationType>) -> Self {
        self.include_relation_types = Some(types);
        self
    }

    /// Set exclude relation types filter
    pub fn with_exclude_relation_types(mut self, types: Vec<RelationType>) -> Self {
        self.exclude_relation_types = Some(types);
        self
    }

    /// Set include classifications filter
    pub fn with_include_classifications(mut self, classifications: Vec<ExternalCallType>) -> Self {
        self.include_classifications = Some(classifications);
        self
    }

    /// Set exclude classifications filter
    pub fn with_exclude_classifications(mut self, classifications: Vec<ExternalCallType>) -> Self {
        self.exclude_classifications = Some(classifications);
        self
    }
}
