//! TOML chunker
//!
//! This module provides chunking functionality for TOML groups.
//! Uses GenericChunker's default two-layer chunking (embedding truncation + BM25 word-based sub-chunking).

use crate::common::{GenericChunker, MergingConfig};
use crate::toml::types::{TomlGroup, TomlGroupType, TomlNode};
use cce_config::modules::ChunkingConfig;
use cce_types::GroupType;
use cce_types::language::Language;
use cce_utils::token_estimation::TokenEstimator;

/// TOML chunker
///
/// Uses the default two-layer chunking from GenericChunker:
/// - Embedding: single chunk per group (truncated if exceeding max_tokens)
/// - BM25: word-count-based sub-chunks sharing source_group_id
pub struct TomlChunker {
    config: ChunkingConfig,
    estimator: TokenEstimator,
    merging_config: MergingConfig,
}

crate::chunker_boilerplate!(TomlChunker);

impl GenericChunker<TomlGroup, TomlNode> for TomlChunker {
    fn config(&self) -> &ChunkingConfig {
        &self.config
    }

    fn estimator(&self) -> &TokenEstimator {
        &self.estimator
    }

    fn language(&self) -> Language {
        Language::Toml
    }

    fn to_group_type(group: &TomlGroup) -> GroupType {
        match group.group_type {
            TomlGroupType::RootTable => GroupType::ModuleWithContents,
            TomlGroupType::NamedTable => GroupType::ClassWithMethods,
            TomlGroupType::ArrayTableElement => GroupType::RelatedFunctions,
            TomlGroupType::KeyValueGroup => GroupType::Standalone,
        }
    }

    fn to_embedding_text(text: &str, _group: &TomlGroup) -> String {
        text.to_string()
    }

    fn bm25_title_for_group(group: &TomlGroup, file_path: &str) -> Option<String> {
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
    fn test_toml_chunker_group_types() {
        let config = ChunkingConfig::default();
        let _chunker = TomlChunker::new(config);

        // Test root table
        let root_group = TomlGroup::new(
            "group1".to_string(),
            TomlGroupType::RootTable,
            String::new(),
        );
        assert_eq!(
            TomlChunker::to_group_type(&root_group),
            GroupType::ModuleWithContents
        );

        // Test named table
        let table_group = TomlGroup::new(
            "group2".to_string(),
            TomlGroupType::NamedTable,
            "database".to_string(),
        );
        assert_eq!(
            TomlChunker::to_group_type(&table_group),
            GroupType::ClassWithMethods
        );

        // Test array table element
        let array_elem_group = TomlGroup::new(
            "group3".to_string(),
            TomlGroupType::ArrayTableElement,
            "items".to_string(),
        );
        assert_eq!(
            TomlChunker::to_group_type(&array_elem_group),
            GroupType::RelatedFunctions
        );

        // Test key-value group
        let kv_group = TomlGroup::new(
            "group4".to_string(),
            TomlGroupType::KeyValueGroup,
            "config".to_string(),
        );
        assert_eq!(TomlChunker::to_group_type(&kv_group), GroupType::Standalone);
    }
}
