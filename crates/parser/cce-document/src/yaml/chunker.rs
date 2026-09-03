//! YAML chunker
//!
//! This module provides chunking functionality for YAML groups.
//! Uses GenericChunker's default two-layer chunking (embedding truncation + BM25 word-based sub-chunking).

use crate::common::{GenericChunker, MergingConfig};
use crate::yaml::types::{YamlGroup, YamlGroupType, YamlNode};
use cce_config::modules::ChunkingConfig;
use cce_types::GroupType;
use cce_types::language::Language;
use cce_utils::token_estimation::TokenEstimator;

/// YAML chunker
///
/// Uses the default two-layer chunking from GenericChunker:
/// - Embedding: single chunk per group (truncated if exceeding max_tokens)
/// - BM25: word-count-based sub-chunks sharing source_group_id
pub struct YamlChunker {
    config: ChunkingConfig,
    estimator: TokenEstimator,
    merging_config: MergingConfig,
}

crate::chunker_boilerplate!(YamlChunker);

impl GenericChunker<YamlGroup, YamlNode> for YamlChunker {
    fn config(&self) -> &ChunkingConfig {
        &self.config
    }

    fn estimator(&self) -> &TokenEstimator {
        &self.estimator
    }

    fn language(&self) -> Language {
        Language::Yaml
    }

    fn to_group_type(group: &YamlGroup) -> GroupType {
        match group.group_type {
            YamlGroupType::RootMapping => GroupType::ModuleWithContents,
            YamlGroupType::NamedMapping => GroupType::ClassWithMethods,
            YamlGroupType::SequenceElement => GroupType::RelatedFunctions,
            YamlGroupType::KeyValueGroup => GroupType::Standalone,
        }
    }

    fn to_embedding_text(text: &str, _group: &YamlGroup) -> String {
        text.to_string()
    }

    fn bm25_title_for_group(group: &YamlGroup, file_path: &str) -> Option<String> {
        if !group.path_prefix.is_empty() {
            return Some(group.path_prefix.clone());
        }
        if let Some(ref header) = group.header {
            if let Some(ref key) = header.key {
                return Some(key.clone());
            }
        }
        std::path::Path::new(file_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .map(String::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_yaml_chunker_group_types() {
        let config = ChunkingConfig::default();
        let _chunker = YamlChunker::new(config);

        // Test root mapping
        let root_group = YamlGroup::new(
            "group1".to_string(),
            YamlGroupType::RootMapping,
            String::new(),
        );
        assert_eq!(
            YamlChunker::to_group_type(&root_group),
            GroupType::ModuleWithContents
        );

        // Test named mapping
        let mapping_group = YamlGroup::new(
            "group2".to_string(),
            YamlGroupType::NamedMapping,
            "database".to_string(),
        );
        assert_eq!(
            YamlChunker::to_group_type(&mapping_group),
            GroupType::ClassWithMethods
        );

        // Test sequence element
        let seq_elem_group = YamlGroup::new(
            "group3".to_string(),
            YamlGroupType::SequenceElement,
            "items".to_string(),
        );
        assert_eq!(
            YamlChunker::to_group_type(&seq_elem_group),
            GroupType::RelatedFunctions
        );

        // Test key-value group
        let kv_group = YamlGroup::new(
            "group4".to_string(),
            YamlGroupType::KeyValueGroup,
            "config".to_string(),
        );
        assert_eq!(YamlChunker::to_group_type(&kv_group), GroupType::Standalone);
    }
}
